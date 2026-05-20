use clap::{Parser, Subcommand, ValueEnum};
use diff_core::{DiffConfig, diff_semantic_documents};
use pdf_content::{ContentOp, ContentProgram};
use serde::{Deserialize, Serialize};
use spdfdiff_types::{
    AiReviewReport, ByteRange, ChangeKind, ChangeSeverity, Diagnostic, DiffDocument, FileRole,
    ObjectId, ParseConfig, PdfDiffError, Provenance, Rect, SemanticChange, SemanticNodeEvidence,
};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::time::{Duration, Instant};

#[derive(Debug, Parser)]
#[command(name = "spdfdiff", version, about = "Semantic PDF diff CLI")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Diff {
        old_pdf: PathBuf,
        new_pdf: PathBuf,
        #[arg(long, value_enum, default_value_t = DiffReportFormat::Json)]
        format: DiffReportFormat,
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long, default_value_t = 2.0)]
        layout_tolerance_pt: f32,
        #[arg(long)]
        fail_on_changes: bool,
    },
    Inspect {
        file: PathBuf,
        #[arg(long, value_enum, default_value_t = ReportFormat::Json)]
        format: ReportFormat,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    Extract {
        file: PathBuf,
        #[arg(long, value_enum, default_value_t = ReportFormat::Json)]
        format: ReportFormat,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    Corpus {
        folder: PathBuf,
        #[arg(long)]
        manifest: Option<PathBuf>,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        fail_on_gate: bool,
    },
    Benchmark {
        #[arg(long, default_value_t = 50)]
        pages: usize,
        #[arg(long)]
        output: PathBuf,
    },
    Review {
        ai_json: PathBuf,
        #[arg(long, default_value = "http://127.0.0.1:8080/v1")]
        endpoint: String,
        #[arg(long, default_value = "local-model")]
        model: String,
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long)]
        api_key: Option<String>,
        #[arg(long, default_value_t = 120)]
        timeout_seconds: u64,
        #[arg(long, default_value_t = 20)]
        max_review_items: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum DiffReportFormat {
    Json,
    AiJson,
    Md,
    Html,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum ReportFormat {
    Json,
    Md,
    Html,
}

fn main() {
    let cli = Cli::parse();

    match run(cli) {
        Ok(exit_code) => {
            if exit_code != 0 {
                std::process::exit(exit_code);
            }
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(exit_code_for_error(&error));
        }
    }
}

fn exit_code_for_error(error: &PdfDiffError) -> i32 {
    match error {
        PdfDiffError::UnsupportedPdf(message) if message.contains("UNSUPPORTED_ENCRYPTION") => 3,
        PdfDiffError::InternalInvariant(_) => 4,
        PdfDiffError::ResourceLimitExceeded(_)
        | PdfDiffError::UnsupportedPdf(_)
        | PdfDiffError::InvalidInput(_) => 2,
    }
}

fn run(cli: Cli) -> Result<i32, PdfDiffError> {
    match cli.command {
        Command::Diff {
            old_pdf,
            new_pdf,
            format,
            output,
            layout_tolerance_pt,
            fail_on_changes,
        } => {
            let old_bytes = std::fs::read(&old_pdf)
                .map_err(|error| PdfDiffError::InvalidInput(error.to_string()))?;
            let new_bytes = std::fs::read(&new_pdf)
                .map_err(|error| PdfDiffError::InvalidInput(error.to_string()))?;
            let document = diff_pdf_bytes(
                &display_file_name(&old_pdf),
                &old_bytes,
                &display_file_name(&new_pdf),
                &new_bytes,
                DiffConfig {
                    layout_tolerance_pt,
                    ..DiffConfig::default()
                },
            )?;
            let rendered = render_diff(&document, format);
            write_or_print(rendered, output)?;
            return Ok(if fail_on_changes && !document.changes.is_empty() {
                1
            } else {
                0
            });
        }
        Command::Inspect {
            file,
            format,
            output,
        } => {
            let bytes = std::fs::read(&file)
                .map_err(|error| PdfDiffError::InvalidInput(error.to_string()))?;
            let parsed = pdf_core::PdfDocument::parse_with_config(&bytes, ParseConfig::default())?;
            let rendered = render_inspect_report(&display_file_name(&file), &parsed, format);
            write_or_print(rendered, output)?;
        }
        Command::Extract {
            file,
            format,
            output,
        } => {
            let bytes = std::fs::read(&file)
                .map_err(|error| PdfDiffError::InvalidInput(error.to_string()))?;
            let semantic = semantic_document_from_pdf(
                &display_file_name(&file),
                &bytes,
                ParseConfig::default(),
            )?;
            let rendered = render_extract_report(&semantic, format);
            write_or_print(rendered, output)?;
        }
        Command::Corpus {
            folder,
            manifest,
            output,
            fail_on_gate,
        } => {
            let manifest = manifest.as_deref().map(load_corpus_manifest).transpose()?;
            let report =
                build_corpus_report_model(&folder, ParseConfig::default(), manifest.as_ref())?;
            let gate_failed = report.gate.as_ref().is_some_and(|gate| !gate.passed);
            let rendered = to_json_pretty(&report)?;
            std::fs::write(&output, rendered)
                .map_err(|error| PdfDiffError::InvalidInput(error.to_string()))?;
            if fail_on_gate && gate_failed {
                return Ok(1);
            }
        }
        Command::Benchmark { pages, output } => {
            let report = run_synthetic_benchmark(pages)?;
            let rendered = to_json_pretty(&report)?;
            std::fs::write(&output, rendered)
                .map_err(|error| PdfDiffError::InvalidInput(error.to_string()))?;
        }
        Command::Review {
            ai_json,
            endpoint,
            model,
            output,
            api_key,
            timeout_seconds,
            max_review_items,
        } => {
            let report = review_ai_json_with_openai_compatible(
                &ai_json,
                &endpoint,
                &model,
                api_key.as_deref(),
                Duration::from_secs(timeout_seconds),
                max_review_items,
            )?;
            write_or_print(to_json_pretty(&report)?, output)?;
        }
    }
    Ok(0)
}

#[derive(Debug, Serialize)]
struct CorpusReport {
    folder: String,
    total: usize,
    parsed: usize,
    partial: usize,
    failed: usize,
    diagnostic_counts: BTreeMap<String, usize>,
    diff_diagnostic_counts: BTreeMap<String, usize>,
    diff_pairs: Vec<CorpusDiffPairReport>,
    gate: Option<CorpusGateReport>,
    files: Vec<CorpusFileReport>,
}

#[derive(Debug, Deserialize)]
struct CorpusManifest {
    schema_version: String,
    #[serde(default)]
    required_files: Vec<String>,
    #[serde(default)]
    diff_pairs: Vec<CorpusManifestDiffPair>,
    #[serde(default)]
    thresholds: CorpusGateThresholds,
}

#[derive(Debug, Deserialize)]
struct CorpusManifestDiffPair {
    name: String,
    old_file: String,
    new_file: String,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize)]
struct CorpusGateThresholds {
    #[serde(default)]
    min_parsed_files: Option<usize>,
    #[serde(default)]
    max_missing_required_files: usize,
    #[serde(default)]
    max_failed_files: usize,
    #[serde(default)]
    max_failed_diff_pairs: usize,
}

#[derive(Debug, Serialize)]
struct CorpusDiffPairReport {
    name: String,
    old_file: String,
    new_file: String,
    status: CorpusDiffPairStatus,
    changes: usize,
    diagnostics: Vec<String>,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum CorpusDiffPairStatus {
    Diffed,
    Failed,
}

#[derive(Debug, Serialize)]
struct CorpusGateReport {
    manifest_schema_version: String,
    passed: bool,
    thresholds: CorpusGateThresholds,
    missing_required_files: Vec<String>,
    failures: Vec<String>,
}

#[derive(Debug, Serialize)]
struct CorpusFileReport {
    file: String,
    status: CorpusFileStatus,
    extracted_nodes: usize,
    diagnostics: Vec<String>,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum CorpusFileStatus {
    Parsed,
    Partial,
    Failed,
}

#[derive(Debug, Serialize)]
struct InspectReport<'a> {
    file: &'a str,
    object_count: usize,
    diagnostic_count: usize,
    first_page_streams: usize,
    incremental_update: IncrementalUpdateReport,
    tagged_structure: TaggedStructureReport,
}

#[derive(Debug, Serialize)]
struct ExtractReport<'a> {
    file: &'a str,
    paragraphs: usize,
    header_candidates: usize,
    footer_candidates: usize,
    page_template_candidates: usize,
    table_candidates: usize,
    table_cells: usize,
    tables: Vec<ExtractTableReport>,
    diagnostic_count: usize,
    tagged_structure: Option<TaggedStructureReport>,
}

#[derive(Debug, Serialize)]
struct ExtractTableReport {
    node_id: String,
    page: usize,
    rows: usize,
    columns: usize,
    cells: Vec<Vec<String>>,
    cell_spans: Vec<Vec<ExtractTableCellSpanReport>>,
    border_hints: usize,
    border_boxes: Vec<Rect>,
    confidence: f32,
}

#[derive(Debug, Serialize)]
struct ExtractTableCellSpanReport {
    row_span: usize,
    column_span: usize,
    placeholder: bool,
}

#[derive(Debug, Clone, Serialize)]
struct TaggedStructureReport {
    detected: bool,
    root_object: Option<String>,
    element_count: usize,
    mcid_count: usize,
    parent_tree_entries: usize,
    structure_types: Vec<String>,
    diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct IncrementalUpdateReport {
    detected: bool,
    revision_count: usize,
    selected_startxref_offset: Option<usize>,
    prior_startxref_offsets: Vec<usize>,
    trailer_prev_offsets: Vec<usize>,
}

#[derive(Debug, Serialize)]
struct BenchmarkReport {
    pages: usize,
    target_total_ms: u128,
    under_target: bool,
    timings_ms: BenchmarkTimings,
    peak_memory_bytes: Option<u64>,
    memory_note: String,
    summary: spdfdiff_types::DiffSummary,
    diagnostics: Vec<String>,
}

#[derive(Debug, Serialize)]
struct BenchmarkTimings {
    parse: u128,
    extract: u128,
    semantic: u128,
    diff: u128,
    report: u128,
    total: u128,
}

#[derive(Debug, Serialize)]
struct ExternalLlmReviewReport {
    schema_version: String,
    provider: String,
    endpoint: String,
    model: String,
    source: ExternalLlmReviewSource,
    request: OpenAiChatRequest,
    response: ExternalLlmReviewResponse,
}

#[derive(Debug, Serialize)]
struct ExternalLlmReviewSource {
    ai_review_schema_version: String,
    source_schema_version: String,
    old_fingerprint: String,
    new_fingerprint: String,
    total_changes: usize,
    review_items_sent: usize,
    diagnostic_count: usize,
}

#[derive(Debug, Serialize)]
struct ExternalLlmReviewResponse {
    status_code: u16,
    model: Option<String>,
    finish_reason: Option<String>,
    content: String,
}

#[derive(Debug, Serialize)]
struct LlmReviewPromptInput<'a> {
    task: &'a str,
    old_fingerprint: &'a str,
    new_fingerprint: &'a str,
    summary: &'a spdfdiff_types::AiReviewSummary,
    question_hints: &'a [spdfdiff_types::AiReviewQuestionHint],
    review_items: &'a [spdfdiff_types::AiReviewItem],
    diagnostic_summary: &'a [spdfdiff_types::AiDiagnosticCount],
}

#[derive(Debug, Serialize)]
struct OpenAiChatRequest {
    model: String,
    messages: Vec<OpenAiChatMessage>,
    temperature: f32,
    stream: bool,
}

#[derive(Debug, Serialize)]
struct OpenAiChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct OpenAiChatResponse {
    model: Option<String>,
    choices: Vec<OpenAiChatChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChatChoice {
    message: OpenAiChatChoiceMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChatChoiceMessage {
    content: String,
}

#[derive(Debug)]
struct HttpEndpoint {
    host: String,
    port: u16,
    authority: String,
    path: String,
}

#[derive(Debug)]
struct HttpResponseParts {
    status_code: u16,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
}

fn load_corpus_manifest(path: &Path) -> Result<CorpusManifest, PdfDiffError> {
    let bytes =
        std::fs::read(path).map_err(|error| PdfDiffError::InvalidInput(error.to_string()))?;
    serde_json::from_slice(&bytes).map_err(|error| {
        PdfDiffError::InvalidInput(format!(
            "failed to parse corpus manifest {}: {error}",
            path.display()
        ))
    })
}

#[cfg(test)]
fn build_corpus_report(folder: &Path, config: ParseConfig) -> Result<String, PdfDiffError> {
    to_json_pretty(&build_corpus_report_model(folder, config, None)?)
}

fn build_corpus_report_model(
    folder: &Path,
    config: ParseConfig,
    manifest: Option<&CorpusManifest>,
) -> Result<CorpusReport, PdfDiffError> {
    let mut paths = Vec::new();
    for entry in
        std::fs::read_dir(folder).map_err(|error| PdfDiffError::InvalidInput(error.to_string()))?
    {
        let entry = entry.map_err(|error| PdfDiffError::InvalidInput(error.to_string()))?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("pdf") {
            paths.push(path);
        }
    }
    paths.sort();

    let total = paths.len();
    let mut parsed = 0usize;
    let mut failed = 0usize;
    let mut partial = 0usize;
    let mut files = Vec::new();
    let mut diagnostic_counts = BTreeMap::new();
    let discovered_files = paths
        .iter()
        .map(|path| display_file_name(path))
        .collect::<Vec<_>>();
    for path in paths {
        let file = display_file_name(&path);
        match std::fs::read(&path) {
            Ok(bytes) => match semantic_document_from_pdf(&file, &bytes, config) {
                Ok(document) => {
                    let diagnostics = document
                        .diagnostics
                        .iter()
                        .map(|diagnostic| diagnostic.code.clone())
                        .collect::<Vec<_>>();
                    for code in &diagnostics {
                        *diagnostic_counts.entry(code.clone()).or_insert(0) += 1;
                    }
                    parsed += 1;
                    let status = if diagnostics.is_empty() {
                        CorpusFileStatus::Parsed
                    } else {
                        partial += 1;
                        CorpusFileStatus::Partial
                    };
                    files.push(CorpusFileReport {
                        file,
                        status,
                        extracted_nodes: document.nodes.len(),
                        diagnostics,
                        error: None,
                    });
                }
                Err(error) => {
                    failed += 1;
                    files.push(CorpusFileReport {
                        file,
                        status: CorpusFileStatus::Failed,
                        extracted_nodes: 0,
                        diagnostics: Vec::new(),
                        error: Some(error.to_string()),
                    });
                }
            },
            Err(error) => {
                failed += 1;
                files.push(CorpusFileReport {
                    file,
                    status: CorpusFileStatus::Failed,
                    extracted_nodes: 0,
                    diagnostics: Vec::new(),
                    error: Some(error.to_string()),
                });
            }
        }
    }

    let (diff_pairs, diff_diagnostic_counts) = build_corpus_diff_pair_reports(folder, manifest)?;
    let gate = manifest.map(|manifest| {
        build_corpus_gate_report(manifest, &discovered_files, parsed, failed, &diff_pairs)
    });

    Ok(CorpusReport {
        folder: display_file_name(folder),
        total,
        parsed,
        partial,
        failed,
        diagnostic_counts,
        diff_diagnostic_counts,
        diff_pairs,
        gate,
        files,
    })
}

fn build_corpus_diff_pair_reports(
    folder: &Path,
    manifest: Option<&CorpusManifest>,
) -> Result<(Vec<CorpusDiffPairReport>, BTreeMap<String, usize>), PdfDiffError> {
    let Some(manifest) = manifest else {
        return Ok((Vec::new(), BTreeMap::new()));
    };
    let mut reports = Vec::new();
    let mut diagnostic_counts = BTreeMap::new();
    for pair in &manifest.diff_pairs {
        let old_path = folder.join(&pair.old_file);
        let new_path = folder.join(&pair.new_file);
        let report = match (std::fs::read(&old_path), std::fs::read(&new_path)) {
            (Ok(old_bytes), Ok(new_bytes)) => match diff_pdf_bytes(
                &pair.old_file,
                &old_bytes,
                &pair.new_file,
                &new_bytes,
                DiffConfig::default(),
            ) {
                Ok(document) => {
                    let diagnostics = document
                        .diagnostics
                        .iter()
                        .map(|diagnostic| diagnostic.code.clone())
                        .collect::<Vec<_>>();
                    for code in &diagnostics {
                        *diagnostic_counts.entry(code.clone()).or_insert(0) += 1;
                    }
                    CorpusDiffPairReport {
                        name: pair.name.clone(),
                        old_file: pair.old_file.clone(),
                        new_file: pair.new_file.clone(),
                        status: CorpusDiffPairStatus::Diffed,
                        changes: document.changes.len(),
                        diagnostics,
                        error: None,
                    }
                }
                Err(error) => CorpusDiffPairReport {
                    name: pair.name.clone(),
                    old_file: pair.old_file.clone(),
                    new_file: pair.new_file.clone(),
                    status: CorpusDiffPairStatus::Failed,
                    changes: 0,
                    diagnostics: Vec::new(),
                    error: Some(error.to_string()),
                },
            },
            (Err(error), _) => CorpusDiffPairReport {
                name: pair.name.clone(),
                old_file: pair.old_file.clone(),
                new_file: pair.new_file.clone(),
                status: CorpusDiffPairStatus::Failed,
                changes: 0,
                diagnostics: Vec::new(),
                error: Some(format!("failed to read {}: {error}", pair.old_file)),
            },
            (_, Err(error)) => CorpusDiffPairReport {
                name: pair.name.clone(),
                old_file: pair.old_file.clone(),
                new_file: pair.new_file.clone(),
                status: CorpusDiffPairStatus::Failed,
                changes: 0,
                diagnostics: Vec::new(),
                error: Some(format!("failed to read {}: {error}", pair.new_file)),
            },
        };
        reports.push(report);
    }
    Ok((reports, diagnostic_counts))
}

fn build_corpus_gate_report(
    manifest: &CorpusManifest,
    discovered_files: &[String],
    parsed: usize,
    failed: usize,
    diff_pairs: &[CorpusDiffPairReport],
) -> CorpusGateReport {
    let mut missing_required_files = manifest
        .required_files
        .iter()
        .filter(|file| !discovered_files.contains(file))
        .cloned()
        .collect::<Vec<_>>();
    missing_required_files.sort();
    let failed_diff_pairs = diff_pairs
        .iter()
        .filter(|pair| matches!(pair.status, CorpusDiffPairStatus::Failed))
        .count();

    let mut failures = Vec::new();
    if let Some(minimum) = manifest.thresholds.min_parsed_files {
        if parsed < minimum {
            failures.push(format!(
                "parsed file count {parsed} is below minimum {minimum}"
            ));
        }
    }
    if missing_required_files.len() > manifest.thresholds.max_missing_required_files {
        failures.push(format!(
            "missing required file count {} exceeds maximum {}",
            missing_required_files.len(),
            manifest.thresholds.max_missing_required_files
        ));
    }
    if failed > manifest.thresholds.max_failed_files {
        failures.push(format!(
            "failed file count {failed} exceeds maximum {}",
            manifest.thresholds.max_failed_files
        ));
    }
    if failed_diff_pairs > manifest.thresholds.max_failed_diff_pairs {
        failures.push(format!(
            "failed diff pair count {failed_diff_pairs} exceeds maximum {}",
            manifest.thresholds.max_failed_diff_pairs
        ));
    }

    CorpusGateReport {
        manifest_schema_version: manifest.schema_version.clone(),
        passed: failures.is_empty(),
        thresholds: manifest.thresholds,
        missing_required_files,
        failures,
    }
}

fn display_file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|file_name| file_name.to_str())
        .unwrap_or(".")
        .to_owned()
}

fn to_json_pretty(value: &impl Serialize) -> Result<String, PdfDiffError> {
    serde_json::to_string_pretty(value)
        .map_err(|error| PdfDiffError::InternalInvariant(error.to_string()))
}

fn write_or_print(rendered: String, output: Option<PathBuf>) -> Result<(), PdfDiffError> {
    if let Some(output) = output {
        std::fs::write(output, rendered)
            .map_err(|error| PdfDiffError::InvalidInput(error.to_string()))
    } else {
        println!("{rendered}");
        Ok(())
    }
}

fn review_ai_json_with_openai_compatible(
    ai_json: &Path,
    endpoint: &str,
    model: &str,
    api_key: Option<&str>,
    timeout: Duration,
    max_review_items: usize,
) -> Result<ExternalLlmReviewReport, PdfDiffError> {
    let bytes =
        std::fs::read(ai_json).map_err(|error| PdfDiffError::InvalidInput(error.to_string()))?;
    let ai_review: AiReviewReport = serde_json::from_slice(&bytes).map_err(|error| {
        PdfDiffError::InvalidInput(format!(
            "failed to parse AI review JSON {}: {error}",
            ai_json.display()
        ))
    })?;
    let request = build_openai_review_request(&ai_review, model, max_review_items)?;
    let (status_code, response) =
        post_openai_chat_completion(endpoint, &request, api_key, timeout)?;
    let choice =
        response.choices.into_iter().next().ok_or_else(|| {
            PdfDiffError::InvalidInput("LLM response did not include choices".into())
        })?;
    let review_items_sent = ai_review.review_items.len().min(max_review_items);
    Ok(ExternalLlmReviewReport {
        schema_version: "0.1.0".into(),
        provider: "openai-compatible".into(),
        endpoint: endpoint.to_owned(),
        model: model.to_owned(),
        source: ExternalLlmReviewSource {
            ai_review_schema_version: ai_review.schema_version,
            source_schema_version: ai_review.source_schema_version,
            old_fingerprint: ai_review.old_fingerprint,
            new_fingerprint: ai_review.new_fingerprint,
            total_changes: ai_review.summary.total_changes,
            review_items_sent,
            diagnostic_count: ai_review.summary.diagnostic_count,
        },
        request,
        response: ExternalLlmReviewResponse {
            status_code,
            model: response.model,
            finish_reason: choice.finish_reason,
            content: choice.message.content,
        },
    })
}

fn build_openai_review_request(
    ai_review: &AiReviewReport,
    model: &str,
    max_review_items: usize,
) -> Result<OpenAiChatRequest, PdfDiffError> {
    let review_items = ai_review
        .review_items
        .iter()
        .take(max_review_items)
        .cloned()
        .collect::<Vec<_>>();
    let prompt_input = LlmReviewPromptInput {
        task: "Review this deterministic semantic PDF diff evidence. Return concise JSON with keys summary, risks, follow_up_questions, and cited_change_ids. Do not make legal or business conclusions. Cite only change_id values from the evidence.",
        old_fingerprint: &ai_review.old_fingerprint,
        new_fingerprint: &ai_review.new_fingerprint,
        summary: &ai_review.summary,
        question_hints: &ai_review.question_hints,
        review_items: &review_items,
        diagnostic_summary: &ai_review.diagnostic_summary,
    };
    let user_prompt = serde_json::to_string_pretty(&prompt_input)
        .map_err(|error| PdfDiffError::InternalInvariant(error.to_string()))?;
    Ok(OpenAiChatRequest {
        model: model.to_owned(),
        messages: vec![
            OpenAiChatMessage {
                role: "system".into(),
                content: "You are a local PDF diff review assistant. Use only the provided evidence. Keep uncertainty explicit. Never provide legal advice or classify business criticality beyond the evidence.".into(),
            },
            OpenAiChatMessage {
                role: "user".into(),
                content: user_prompt,
            },
        ],
        temperature: 0.0,
        stream: false,
    })
}

fn post_openai_chat_completion(
    endpoint: &str,
    request: &OpenAiChatRequest,
    api_key: Option<&str>,
    timeout: Duration,
) -> Result<(u16, OpenAiChatResponse), PdfDiffError> {
    let endpoint = parse_http_endpoint(endpoint)?;
    let body = serde_json::to_vec(request)
        .map_err(|error| PdfDiffError::InternalInvariant(error.to_string()))?;
    let mut headers = format!(
        "POST {} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nAccept: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
        endpoint.path,
        endpoint.authority,
        body.len()
    );
    if let Some(api_key) = api_key {
        headers.push_str(&format!("Authorization: Bearer {api_key}\r\n"));
    }
    headers.push_str("\r\n");

    let mut stream =
        TcpStream::connect((endpoint.host.as_str(), endpoint.port)).map_err(|error| {
            PdfDiffError::InvalidInput(format!("failed to connect to LLM endpoint: {error}"))
        })?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|error| PdfDiffError::InvalidInput(error.to_string()))?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|error| PdfDiffError::InvalidInput(error.to_string()))?;
    stream
        .write_all(headers.as_bytes())
        .and_then(|()| stream.write_all(&body))
        .map_err(|error| {
            PdfDiffError::InvalidInput(format!("failed to write LLM request: {error}"))
        })?;

    let mut response_bytes = Vec::new();
    stream.read_to_end(&mut response_bytes).map_err(|error| {
        PdfDiffError::InvalidInput(format!("failed to read LLM response: {error}"))
    })?;
    let response_parts = parse_http_response(&response_bytes)?;
    if !(200..300).contains(&response_parts.status_code) {
        return Err(PdfDiffError::InvalidInput(format!(
            "LLM endpoint returned HTTP {}: {}",
            response_parts.status_code,
            String::from_utf8_lossy(&response_parts.body)
        )));
    }
    let body = if header_contains(&response_parts.headers, "transfer-encoding", "chunked") {
        decode_chunked_body(&response_parts.body)?
    } else {
        response_parts.body
    };
    let response: OpenAiChatResponse = serde_json::from_slice(&body).map_err(|error| {
        PdfDiffError::InvalidInput(format!("failed to parse LLM response JSON: {error}"))
    })?;
    Ok((response_parts.status_code, response))
}

fn parse_http_endpoint(endpoint: &str) -> Result<HttpEndpoint, PdfDiffError> {
    let endpoint = endpoint.trim();
    let Some(without_scheme) = endpoint.strip_prefix("http://") else {
        return Err(PdfDiffError::InvalidInput(
            "only http:// OpenAI-compatible endpoints are supported; run llama-server on localhost or use a local HTTP proxy".into(),
        ));
    };
    let (authority, base_path) = without_scheme
        .split_once('/')
        .map_or((without_scheme, ""), |(authority, path)| (authority, path));
    if authority.is_empty() {
        return Err(PdfDiffError::InvalidInput(
            "LLM endpoint must include a host".into(),
        ));
    }
    let (host, port) = authority
        .rsplit_once(':')
        .map_or((authority, 80), |(host, port)| {
            (host, port.parse::<u16>().unwrap_or(0))
        });
    if host.is_empty() || port == 0 {
        return Err(PdfDiffError::InvalidInput(format!(
            "invalid LLM endpoint authority: {authority}"
        )));
    }
    let base_path = format!("/{}", base_path.trim_matches('/'));
    let path = if base_path == "/" {
        "/v1/chat/completions".into()
    } else if base_path.ends_with("/chat/completions") {
        base_path
    } else {
        format!("{}/chat/completions", base_path.trim_end_matches('/'))
    };
    Ok(HttpEndpoint {
        host: host.to_owned(),
        port,
        authority: authority.to_owned(),
        path,
    })
}

fn parse_http_response(bytes: &[u8]) -> Result<HttpResponseParts, PdfDiffError> {
    let Some(header_end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") else {
        return Err(PdfDiffError::InvalidInput(
            "LLM endpoint returned an invalid HTTP response".into(),
        ));
    };
    let headers_text = String::from_utf8_lossy(&bytes[..header_end]);
    let mut lines = headers_text.lines();
    let status_line = lines
        .next()
        .ok_or_else(|| PdfDiffError::InvalidInput("missing HTTP status line".into()))?;
    let status_code = status_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| PdfDiffError::InvalidInput("missing HTTP status code".into()))?
        .parse::<u16>()
        .map_err(|error| {
            PdfDiffError::InvalidInput(format!("invalid HTTP status code: {error}"))
        })?;
    let mut headers = BTreeMap::new();
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_owned());
        }
    }
    Ok(HttpResponseParts {
        status_code,
        headers,
        body: bytes[header_end + 4..].to_vec(),
    })
}

fn header_contains(headers: &BTreeMap<String, String>, name: &str, value: &str) -> bool {
    headers
        .get(&name.to_ascii_lowercase())
        .is_some_and(|header| header.to_ascii_lowercase().contains(value))
}

fn decode_chunked_body(body: &[u8]) -> Result<Vec<u8>, PdfDiffError> {
    let mut decoded = Vec::new();
    let mut offset = 0usize;
    loop {
        let Some(line_end) = body[offset..]
            .windows(2)
            .position(|window| window == b"\r\n")
            .map(|position| offset + position)
        else {
            return Err(PdfDiffError::InvalidInput(
                "invalid chunked LLM response".into(),
            ));
        };
        let size_line = String::from_utf8_lossy(&body[offset..line_end]);
        let size_hex = size_line.split(';').next().unwrap_or("").trim();
        let size = usize::from_str_radix(size_hex, 16).map_err(|error| {
            PdfDiffError::InvalidInput(format!("invalid chunk size in LLM response: {error}"))
        })?;
        offset = line_end + 2;
        if size == 0 {
            break;
        }
        let chunk_end = offset + size;
        if chunk_end + 2 > body.len() {
            return Err(PdfDiffError::InvalidInput(
                "truncated chunked LLM response".into(),
            ));
        }
        decoded.extend_from_slice(&body[offset..chunk_end]);
        offset = chunk_end + 2;
    }
    Ok(decoded)
}

fn diff_pdf_bytes(
    old_fingerprint: &str,
    old_bytes: &[u8],
    new_fingerprint: &str,
    new_bytes: &[u8],
    diff_config: DiffConfig,
) -> Result<DiffDocument, PdfDiffError> {
    let config = ParseConfig::default();
    let old_document = pdf_core::PdfDocument::parse_with_config(old_bytes, config)?;
    let new_document = pdf_core::PdfDocument::parse_with_config(new_bytes, config)?;
    let old = semantic_document_from_document(old_fingerprint, &old_document, config);
    let new = semantic_document_from_document(new_fingerprint, &new_document, config);
    let mut diff = diff_semantic_documents(&old, &new, diff_config);
    append_image_payload_changes(&mut diff, &old_document, &new_document);
    append_graphic_surface_changes(&mut diff, &old_document, &new_document, config);
    append_document_surface_changes(&mut diff, &old_document, &new_document);
    Ok(diff)
}

fn semantic_document_from_pdf(
    fingerprint: &str,
    bytes: &[u8],
    config: ParseConfig,
) -> Result<pdf_semantic::SemanticDocument, PdfDiffError> {
    let document = pdf_core::PdfDocument::parse_with_config(bytes, config)?;
    Ok(semantic_document_from_document(
        fingerprint,
        &document,
        config,
    ))
}

fn semantic_document_from_document(
    fingerprint: &str,
    document: &pdf_core::PdfDocument,
    config: ParseConfig,
) -> pdf_semantic::SemanticDocument {
    let extraction = extract_text_runs_from_document(document, config);
    let tagged_structure = document.tagged_structure(config);
    let tagged_summary = tagged_structure
        .root_object_id
        .is_some()
        .then(|| semantic_tagged_structure_summary(&tagged_structure));
    pdf_semantic::build_semantic_document_with_tagged_structure_and_table_hints(
        fingerprint,
        &extraction.runs,
        extraction.diagnostics,
        tagged_summary,
        extraction.table_border_hints,
    )
}

struct ExtractedTextRuns {
    runs: Vec<pdf_text::TextRun>,
    diagnostics: Vec<Diagnostic>,
    table_border_hints: Vec<pdf_semantic::TableBorderHint>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OcrConfig {
    command: String,
    mode: OcrCommandMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OcrCommandMode {
    Tesseract,
    Plain,
}

impl OcrConfig {
    fn from_environment() -> Self {
        if let Some(command) = std::env::var_os("SPDFDIFF_OCR_COMMAND")
            .and_then(|value| value.into_string().ok())
            .filter(|value| !value.trim().is_empty())
        {
            return Self {
                mode: if command.to_ascii_lowercase().contains("tesseract") {
                    OcrCommandMode::Tesseract
                } else {
                    OcrCommandMode::Plain
                },
                command,
            };
        }

        Self {
            command: "tesseract".into(),
            mode: OcrCommandMode::Tesseract,
        }
    }
}

fn extract_text_runs_from_document(
    document: &pdf_core::PdfDocument,
    config: ParseConfig,
) -> ExtractedTextRuns {
    let contents = document.page_contents();
    let font_resources = pdf_text::font_resources_from_document(document);
    if contents.is_empty() {
        let mut diagnostics = document.diagnostics.clone();
        diagnostics.extend(font_resources.diagnostics.clone());
        diagnostics.push(spdfdiff_types::Diagnostic::warning(
            "MISSING_PAGE_CONTENT",
            "no page content stream was available for extraction",
        ));
        append_unsupported_feature_diagnostics(document, &font_resources, true, &mut diagnostics);
        return ExtractedTextRuns {
            runs: Vec::new(),
            diagnostics,
            table_border_hints: Vec::new(),
        };
    }
    let mut programs: Vec<(usize, ContentProgram)> = Vec::new();
    for content in &contents {
        let mut stream_program = pdf_content::parse_content_stream_with_limits(
            content.bytes,
            content.page_index,
            Some(content.stream_object_id),
            config.limits,
        );
        if let Some((_, page_program)) = programs
            .iter_mut()
            .find(|(page_index, _)| *page_index == content.page_index)
        {
            page_program
                .operations
                .append(&mut stream_program.operations);
            page_program
                .diagnostics
                .append(&mut stream_program.diagnostics);
        } else {
            programs.push((content.page_index, stream_program));
        }
    }
    let mut runs = Vec::new();
    let mut diagnostics = document.diagnostics.clone();
    diagnostics.extend(font_resources.diagnostics.clone());
    let mut table_border_hints = Vec::new();
    for (page_index, mut program) in programs {
        table_border_hints.extend(table_border_hints_from_program(&program));
        let tounicode_result = apply_tounicode_maps(&mut program, document, &font_resources);
        let applied_tounicode = tounicode_result.applied;
        diagnostics.extend(tounicode_result.diagnostics);
        let extraction = pdf_text::extract_text_runs(&program, page_index);
        diagnostics.extend(
            extraction
                .diagnostics
                .into_iter()
                .filter(|diagnostic| !applied_tounicode || diagnostic.code != "MISSING_TOUNICODE"),
        );
        runs.extend(extraction.runs);
    }
    if runs.is_empty() && document_has_token(document, "/Subtype /Image") {
        let ocr = extract_ocr_text_runs_from_document(document, OcrConfig::from_environment());
        diagnostics.extend(ocr.diagnostics);
        runs.extend(ocr.runs);
    }
    append_unsupported_feature_diagnostics(
        document,
        &font_resources,
        runs.is_empty(),
        &mut diagnostics,
    );
    ExtractedTextRuns {
        runs,
        diagnostics,
        table_border_hints,
    }
}

fn append_unsupported_feature_diagnostics(
    document: &pdf_core::PdfDocument,
    font_resources: &pdf_text::FontResourceSet,
    has_no_text_runs: bool,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if document_has_token(document, "/Subtype /Image") && has_no_text_runs {
        diagnostics.push(Diagnostic::warning(
            "MISSING_TEXT_LAYER",
            "no extractable text layer was found and OCR did not produce text",
        ));
    }

    if document_has_any_token(document, &["/AcroForm", "/Widget"]) {
        diagnostics.push(Diagnostic::warning(
            "UNSUPPORTED_FORM_FIELD_DIFF",
            "interactive form field comparison is not implemented",
        ));
    }

    append_font_diagnostics(font_resources, diagnostics);
    append_tagged_pdf_diagnostics(document, diagnostics);
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OcrImage {
    index: usize,
    object_id: ObjectId,
    byte_range: ByteRange,
    width: usize,
    height: usize,
    pixels_rgb: Vec<u8>,
    hash: String,
}

fn extract_ocr_text_runs_from_document(
    document: &pdf_core::PdfDocument,
    config: OcrConfig,
) -> ExtractedTextRuns {
    let mut runs = Vec::new();
    let mut diagnostics = Vec::new();
    let images = ocr_images(document, &mut diagnostics);

    for image in images {
        match run_ocr_for_image(&image, &config) {
            Ok(text) => {
                let normalized = normalize_ocr_text(&text);
                if normalized.is_empty() {
                    continue;
                }
                diagnostics.push(Diagnostic::info(
                    "OCR_TEXT_EXTRACTED",
                    format!(
                        "extracted OCR text from image XObject {} 0 R",
                        image.object_id.number
                    ),
                ));
                runs.push(ocr_text_run(&image, normalized));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                break;
            }
            Err(error) => diagnostics.push(Diagnostic::warning(
                "OCR_ENGINE_FAILED",
                format!(
                    "OCR engine failed for image XObject {} 0 R: {error}",
                    image.object_id.number
                ),
            )),
        }
    }

    ExtractedTextRuns {
        runs,
        diagnostics,
        table_border_hints: Vec::new(),
    }
}

fn ocr_images(
    document: &pdf_core::PdfDocument,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<OcrImage> {
    let soft_masks = soft_mask_object_ids(document);
    let mut images = Vec::new();
    for object in document
        .objects
        .iter()
        .filter(|object| object.body.contains("/Subtype /Image"))
    {
        if soft_masks.contains(&object.id) {
            continue;
        }
        let index = images.len();
        match ocr_image_from_object(index, object) {
            Ok(Some(image)) => images.push(image),
            Ok(None) => {}
            Err(message) => diagnostics.push(Diagnostic::warning(
                "OCR_IMAGE_UNSUPPORTED",
                format!(
                    "image XObject {} 0 R is not supported for OCR extraction: {message}",
                    object.id.number
                ),
            )),
        }
    }
    images
}

fn soft_mask_object_ids(document: &pdf_core::PdfDocument) -> Vec<ObjectId> {
    document
        .objects
        .iter()
        .filter_map(|object| reference_after_key(&object.body, "SMask"))
        .collect()
}

fn ocr_image_from_object(
    index: usize,
    object: &pdf_core::PdfObject,
) -> Result<Option<OcrImage>, String> {
    let Some(stream) = &object.stream else {
        return Ok(None);
    };
    if !stream.decoded {
        return Err("stream bytes were not decoded".into());
    }

    let width =
        pdf_usize_after_name(&object.body, "Width").ok_or_else(|| "missing /Width".to_owned())?;
    let height =
        pdf_usize_after_name(&object.body, "Height").ok_or_else(|| "missing /Height".to_owned())?;
    let bits_per_component = pdf_usize_after_name(&object.body, "BitsPerComponent")
        .ok_or_else(|| "missing /BitsPerComponent".to_owned())?;
    if bits_per_component != 8 {
        return Err(format!(
            "BitsPerComponent {bits_per_component} is not supported"
        ));
    }

    let color_space = value_after_pdf_name(&object.body, "ColorSpace")
        .ok_or_else(|| "missing /ColorSpace".to_owned())?;
    let components = match color_space.as_str() {
        "DeviceRGB" => 3,
        "DeviceGray" => 1,
        other => return Err(format!("ColorSpace /{other} is not supported")),
    };

    let columns = pdf_usize_after_name(&object.body, "Columns").unwrap_or(width);
    let colors = pdf_usize_after_name(&object.body, "Colors").unwrap_or(components);
    if columns != width || colors != components {
        return Err("DecodeParms columns/colors do not match image dimensions".into());
    }

    let predictor = pdf_usize_after_name(&object.body, "Predictor").unwrap_or(1);
    let samples = decode_image_samples(&stream.bytes, width, height, components, predictor)?;
    let pixels_rgb = if components == 3 {
        samples
    } else {
        samples
            .into_iter()
            .flat_map(|sample| [sample, sample, sample])
            .collect()
    };
    let hash = stable_hash(&pixels_rgb);

    Ok(Some(OcrImage {
        index,
        object_id: object.id,
        byte_range: stream.byte_range,
        width,
        height,
        pixels_rgb,
        hash,
    }))
}

fn decode_image_samples(
    bytes: &[u8],
    width: usize,
    height: usize,
    components: usize,
    predictor: usize,
) -> Result<Vec<u8>, String> {
    let row_len = width
        .checked_mul(components)
        .ok_or_else(|| "image row size overflowed".to_owned())?;
    let expected = row_len
        .checked_mul(height)
        .ok_or_else(|| "image size overflowed".to_owned())?;

    if predictor == 1 {
        if bytes.len() < expected {
            return Err(format!(
                "decoded stream has {} bytes but expected at least {expected}",
                bytes.len()
            ));
        }
        return Ok(bytes[..expected].to_vec());
    }

    if !(10..=15).contains(&predictor) {
        return Err(format!("Predictor {predictor} is not supported"));
    }

    let encoded_row_len = row_len
        .checked_add(1)
        .ok_or_else(|| "PNG predictor row size overflowed".to_owned())?;
    let expected_encoded = encoded_row_len
        .checked_mul(height)
        .ok_or_else(|| "PNG predictor image size overflowed".to_owned())?;
    if bytes.len() < expected_encoded {
        return Err(format!(
            "decoded stream has {} bytes but expected at least {expected_encoded}",
            bytes.len()
        ));
    }

    let mut output = vec![0; expected];
    for y in 0..height {
        let input_start = y * encoded_row_len;
        let filter = bytes[input_start];
        let input = &bytes[input_start + 1..input_start + 1 + row_len];
        let row_start = y * row_len;
        for x in 0..row_len {
            let left = if x >= components {
                output[row_start + x - components]
            } else {
                0
            };
            let up = if y > 0 {
                output[row_start + x - row_len]
            } else {
                0
            };
            let up_left = if y > 0 && x >= components {
                output[row_start + x - row_len - components]
            } else {
                0
            };
            let predictor_byte = match filter {
                0 => 0,
                1 => left,
                2 => up,
                3 => ((u16::from(left) + u16::from(up)) / 2) as u8,
                4 => paeth_predictor(left, up, up_left),
                other => return Err(format!("PNG row filter {other} is not supported")),
            };
            output[row_start + x] = input[x].wrapping_add(predictor_byte);
        }
    }

    Ok(output)
}

fn paeth_predictor(left: u8, up: u8, up_left: u8) -> u8 {
    let left = i32::from(left);
    let up = i32::from(up);
    let up_left = i32::from(up_left);
    let estimate = left + up - up_left;
    let left_distance = (estimate - left).abs();
    let up_distance = (estimate - up).abs();
    let up_left_distance = (estimate - up_left).abs();
    if left_distance <= up_distance && left_distance <= up_left_distance {
        left as u8
    } else if up_distance <= up_left_distance {
        up as u8
    } else {
        up_left as u8
    }
}

fn run_ocr_for_image(image: &OcrImage, config: &OcrConfig) -> std::io::Result<String> {
    let path = write_temp_ppm(image)?;
    let mut command = ProcessCommand::new(&config.command);
    match config.mode {
        OcrCommandMode::Tesseract => {
            command.arg(&path).arg("stdout").arg("--psm").arg("6");
        }
        OcrCommandMode::Plain => {
            command.arg(&path);
        }
    }
    command
        .env("SPDFDIFF_OCR_OBJECT_ID", image.object_id.number.to_string())
        .env("SPDFDIFF_OCR_IMAGE_INDEX", image.index.to_string())
        .env("SPDFDIFF_OCR_IMAGE_HASH", &image.hash);
    let output = command.output();
    let _ = std::fs::remove_file(&path);
    let output = output?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(std::io::Error::other(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ))
    }
}

fn write_temp_ppm(image: &OcrImage) -> std::io::Result<PathBuf> {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "spdfdiff-ocr-{}-{}-{}.ppm",
        std::process::id(),
        image.object_id.number,
        image.hash
    ));
    let mut bytes = format!("P6\n{} {}\n255\n", image.width, image.height).into_bytes();
    bytes.extend_from_slice(&image.pixels_rgb);
    std::fs::write(&path, bytes)?;
    Ok(path)
}

fn ocr_text_run(image: &OcrImage, text: String) -> pdf_text::TextRun {
    pdf_text::TextRun {
        id: format!("ocr-image-{:04}", image.index),
        text: text.clone(),
        normalized_text: text,
        glyphs: Vec::new(),
        bbox: Rect {
            x0: 0.0,
            y0: 0.0,
            x1: image.width as f32,
            y1: image.height as f32,
        },
        source: Provenance {
            file_role: None,
            object_id: Some(image.object_id),
            page_index: Some(0),
            stream_object_id: Some(image.object_id),
            content_op_index: None,
            byte_range: Some(image.byte_range),
        },
        marked_content: None,
    }
}

fn normalize_ocr_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn pdf_usize_after_name(body: &str, key: &str) -> Option<usize> {
    value_after_pdf_name(body, key)?.parse().ok()
}

fn append_font_diagnostics(
    font_resources: &pdf_text::FontResourceSet,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let cid_missing_count = font_resources
        .fonts
        .values()
        .filter(|font| font.is_cid_or_type0() && font.to_unicode.is_none())
        .count();
    if cid_missing_count > 0 {
        diagnostics.push(Diagnostic::warning(
            "MISSING_TOUNICODE_CID_FONT",
            format!(
                "{cid_missing_count} CID/Type0 font objects have no ToUnicode map; extraction falls back to literal bytes with lower confidence"
            ),
        ));
    }
}

fn append_tagged_pdf_diagnostics(
    document: &pdf_core::PdfDocument,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let tagged_structure = document.tagged_structure(ParseConfig::default());
    if tagged_structure.root_object_id.is_some() {
        diagnostics.push(Diagnostic::info(
            "TAGGED_PDF_STRUCTURE_DETECTED",
            format!(
                "parsed tagged PDF structure with {} elements and {} MCID references; untagged layout heuristics remain the fallback when MCID mapping is incomplete",
                tagged_element_count(&tagged_structure.roots),
                tagged_mcid_count(&tagged_structure.roots)
            ),
        ));
    }
    diagnostics.extend(tagged_structure.diagnostics);
    let mcid_count = document
        .objects
        .iter()
        .filter_map(|object| object.stream.as_ref())
        .map(|stream| byte_pattern_count(&stream.bytes, b"/MCID"))
        .sum::<usize>();
    if mcid_count > 0 {
        diagnostics.push(Diagnostic::info(
            "TAGGED_MCID_DETECTED",
            format!("detected {mcid_count} marked-content IDs available for semantic node mapping"),
        ));
    }
}

fn semantic_tagged_structure_summary(
    structure: &pdf_core::TaggedStructure,
) -> pdf_semantic::TaggedStructureSummary {
    let mut structure_types = Vec::new();
    collect_tagged_structure_types(&structure.roots, &mut structure_types);
    structure_types.sort();
    structure_types.dedup();
    pdf_semantic::TaggedStructureSummary {
        root_object_id: structure.root_object_id,
        element_count: tagged_element_count(&structure.roots),
        mcid_count: tagged_mcid_count(&structure.roots),
        parent_tree_entries: structure.parent_tree.len(),
        structure_types,
        elements: semantic_tagged_elements(&structure.roots),
        confidence: if structure.diagnostics.is_empty() {
            0.8
        } else {
            0.5
        },
    }
}

fn semantic_tagged_elements(
    elements: &[pdf_core::TaggedStructureElement],
) -> Vec<pdf_semantic::TaggedStructureElementSummary> {
    elements
        .iter()
        .map(|element| pdf_semantic::TaggedStructureElementSummary {
            structure_type: element.structure_type.clone(),
            mcids: element.mcids.clone(),
            children: semantic_tagged_elements(&element.children),
        })
        .collect()
}

fn tagged_structure_report(structure: &pdf_core::TaggedStructure) -> TaggedStructureReport {
    let mut structure_types = Vec::new();
    collect_tagged_structure_types(&structure.roots, &mut structure_types);
    structure_types.sort();
    structure_types.dedup();
    TaggedStructureReport {
        detected: structure.root_object_id.is_some(),
        root_object: structure
            .root_object_id
            .map(|object_id| format!("{} {} R", object_id.number, object_id.generation)),
        element_count: tagged_element_count(&structure.roots),
        mcid_count: tagged_mcid_count(&structure.roots),
        parent_tree_entries: structure.parent_tree.len(),
        structure_types,
        diagnostics: structure
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.clone())
            .collect(),
    }
}

fn tagged_structure_report_from_semantic(
    summary: &pdf_semantic::TaggedStructureSummary,
) -> TaggedStructureReport {
    TaggedStructureReport {
        detected: summary.root_object_id.is_some(),
        root_object: summary
            .root_object_id
            .map(|object_id| format!("{} {} R", object_id.number, object_id.generation)),
        element_count: summary.element_count,
        mcid_count: summary.mcid_count,
        parent_tree_entries: summary.parent_tree_entries,
        structure_types: summary.structure_types.clone(),
        diagnostics: Vec::new(),
    }
}

fn incremental_update_report(
    info: Option<&pdf_core::IncrementalUpdateInfo>,
) -> IncrementalUpdateReport {
    match info {
        Some(info) => IncrementalUpdateReport {
            detected: true,
            revision_count: info.revision_count,
            selected_startxref_offset: info.selected_startxref_offset,
            prior_startxref_offsets: info.prior_startxref_offsets.clone(),
            trailer_prev_offsets: info.trailer_prev_offsets.clone(),
        },
        None => IncrementalUpdateReport {
            detected: false,
            revision_count: 1,
            selected_startxref_offset: None,
            prior_startxref_offsets: Vec::new(),
            trailer_prev_offsets: Vec::new(),
        },
    }
}

fn tagged_element_count(elements: &[pdf_core::TaggedStructureElement]) -> usize {
    elements
        .iter()
        .map(|element| 1 + tagged_element_count(&element.children))
        .sum()
}

fn tagged_mcid_count(elements: &[pdf_core::TaggedStructureElement]) -> usize {
    elements
        .iter()
        .map(|element| element.mcids.len() + tagged_mcid_count(&element.children))
        .sum()
}

fn collect_tagged_structure_types(
    elements: &[pdf_core::TaggedStructureElement],
    structure_types: &mut Vec<String>,
) {
    for element in elements {
        structure_types.push(element.structure_type.clone());
        collect_tagged_structure_types(&element.children, structure_types);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ImagePayload {
    index: usize,
    object_id: ObjectId,
    byte_range: ByteRange,
    byte_len: usize,
    hash: String,
}

fn append_image_payload_changes(
    document: &mut DiffDocument,
    old_document: &pdf_core::PdfDocument,
    new_document: &pdf_core::PdfDocument,
) {
    let old_images = image_payloads(old_document);
    let new_images = image_payloads(new_document);
    for index in 0..old_images.len().max(new_images.len()) {
        match (old_images.get(index), new_images.get(index)) {
            (Some(old_image), Some(new_image)) if old_image.hash == new_image.hash => {}
            (Some(old_image), Some(new_image)) => push_image_payload_change(
                document,
                Some(old_image),
                Some(new_image),
                format!(
                    "image payload differs at image index {index} (old hash {} -> new hash {})",
                    old_image.hash, new_image.hash
                ),
            ),
            (Some(old_image), None) => push_image_payload_change(
                document,
                Some(old_image),
                None,
                format!("image payload at index {index} exists only in old document"),
            ),
            (None, Some(new_image)) => push_image_payload_change(
                document,
                None,
                Some(new_image),
                format!("image payload at index {index} exists only in new document"),
            ),
            (None, None) => {}
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct DocumentSurface {
    category: SurfaceCategory,
    index: usize,
    object_id: ObjectId,
    summary: String,
    hash: String,
    bbox: Option<Rect>,
    byte_range: Option<ByteRange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum SurfaceCategory {
    Annotation,
    FormField,
    Outline,
    Metadata,
    Attachment,
}

fn append_document_surface_changes(
    document: &mut DiffDocument,
    old_document: &pdf_core::PdfDocument,
    new_document: &pdf_core::PdfDocument,
) {
    for category in [
        SurfaceCategory::Annotation,
        SurfaceCategory::FormField,
        SurfaceCategory::Outline,
        SurfaceCategory::Metadata,
        SurfaceCategory::Attachment,
    ] {
        let old_surfaces = document_surfaces(old_document, category);
        let new_surfaces = document_surfaces(new_document, category);
        for index in 0..old_surfaces.len().max(new_surfaces.len()) {
            match (old_surfaces.get(index), new_surfaces.get(index)) {
                (Some(old_surface), Some(new_surface)) if old_surface.hash == new_surface.hash => {}
                (Some(old_surface), Some(new_surface)) => push_surface_change(
                    document,
                    Some(old_surface),
                    Some(new_surface),
                    format!(
                        "{} differs at index {index}",
                        old_surface.category.report_label()
                    ),
                ),
                (Some(old_surface), None) => push_surface_change(
                    document,
                    Some(old_surface),
                    None,
                    format!(
                        "{} exists only in old document at index {index}",
                        old_surface.category.report_label()
                    ),
                ),
                (None, Some(new_surface)) => push_surface_change(
                    document,
                    None,
                    Some(new_surface),
                    format!(
                        "{} exists only in new document at index {index}",
                        new_surface.category.report_label()
                    ),
                ),
                (None, None) => {}
            }
        }
    }
}

impl SurfaceCategory {
    fn change_kind(self) -> ChangeKind {
        match self {
            SurfaceCategory::Annotation | SurfaceCategory::Attachment => {
                ChangeKind::AnnotationChanged
            }
            SurfaceCategory::FormField => ChangeKind::FormFieldChanged,
            SurfaceCategory::Outline | SurfaceCategory::Metadata => ChangeKind::MetadataChanged,
        }
    }

    fn report_label(self) -> &'static str {
        match self {
            SurfaceCategory::Annotation => "annotation/link surface",
            SurfaceCategory::FormField => "form field surface",
            SurfaceCategory::Outline => "outline/bookmark surface",
            SurfaceCategory::Metadata => "metadata/XMP surface",
            SurfaceCategory::Attachment => "embedded attachment surface",
        }
    }

    fn node_prefix(self) -> &'static str {
        match self {
            SurfaceCategory::Annotation => "annotation",
            SurfaceCategory::FormField => "form",
            SurfaceCategory::Outline => "outline",
            SurfaceCategory::Metadata => "metadata",
            SurfaceCategory::Attachment => "attachment",
        }
    }
}

fn document_surfaces(
    document: &pdf_core::PdfDocument,
    category: SurfaceCategory,
) -> Vec<DocumentSurface> {
    let mut surfaces = document
        .objects
        .iter()
        .filter(|object| surface_matches_category(&object.body, category))
        .enumerate()
        .map(|(index, object)| {
            let (summary, hash, bbox) = summarize_and_hash_surface(object, category);
            DocumentSurface {
                category,
                index,
                object_id: object.id,
                summary,
                hash,
                bbox,
                byte_range: object.stream.as_ref().map(|stream| stream.byte_range),
            }
        })
        .collect::<Vec<_>>();
    surfaces.sort_by_key(|surface| (surface.category, surface.index, surface.object_id));
    surfaces
}

fn surface_matches_category(body: &str, category: SurfaceCategory) -> bool {
    match category {
        SurfaceCategory::Annotation => {
            body.contains("/Subtype /Link")
                || (body.contains("/Type /Annot") && !body.contains("/Subtype /Widget"))
        }
        SurfaceCategory::FormField => {
            body.contains("/AcroForm") || body.contains("/Subtype /Widget")
        }
        SurfaceCategory::Outline => {
            body.contains("/Outlines")
                || (body.contains("/Title")
                    && (body.contains("/Dest")
                        || body.contains("/Parent")
                        || body.contains("/First")
                        || body.contains("/Next")))
        }
        SurfaceCategory::Metadata => {
            body.contains("/Type /Metadata") || body.contains("/Metadata") || body.contains("/Info")
        }
        SurfaceCategory::Attachment => {
            body.contains("/EmbeddedFiles")
                || body.contains("/Filespec")
                || body.contains("/Subtype /FileAttachment")
        }
    }
}

fn summarize_surface(body: &str, category: SurfaceCategory) -> String {
    if category == SurfaceCategory::Annotation {
        return annotation_signature(body);
    }
    let mut parts = vec![category.report_label().to_owned()];
    for key in ["Subtype", "URI", "Title", "F", "Desc", "Contents", "T", "V"] {
        if let Some(value) = value_after_pdf_name(body, key) {
            parts.push(format!("{key}={value}"));
        }
    }
    parts.join(" ")
}

fn summarize_and_hash_surface(
    object: &pdf_core::PdfObject,
    category: SurfaceCategory,
) -> (String, String, Option<Rect>) {
    let summary = summarize_surface(&object.body, category);
    let hash = if category == SurfaceCategory::Annotation {
        stable_hash(summary.as_bytes())
    } else {
        let bytes = object
            .stream
            .as_ref()
            .map_or_else(|| object.body.as_bytes(), |stream| stream.bytes.as_slice());
        stable_hash(bytes)
    };
    let bbox = if category == SurfaceCategory::Annotation {
        rect_after_pdf_name(&object.body, "Rect")
    } else {
        None
    };
    (summary, hash, bbox)
}

fn annotation_signature(body: &str) -> String {
    let mut parts = vec!["annotation/link semantic surface".to_owned()];
    push_pdf_name_value(&mut parts, body, "Subtype", "subtype");
    push_pdf_name_value(&mut parts, body, "S", "action");
    push_pdf_name_value(&mut parts, body, "URI", "uri");
    push_pdf_name_value(&mut parts, body, "Dest", "dest");
    push_pdf_name_value(&mut parts, body, "Contents", "contents");
    push_pdf_name_value(&mut parts, body, "T", "title");
    push_pdf_name_value(&mut parts, body, "NM", "name");
    push_pdf_name_value(&mut parts, body, "M", "modified");
    if let Some(rect) = rect_after_pdf_name(body, "Rect") {
        parts.push(format!("rect={}", canonical_rect(rect)));
    }
    push_pdf_array_value(&mut parts, body, "C", "color");
    push_pdf_array_value(&mut parts, body, "QuadPoints", "quad_points");
    push_pdf_array_value(&mut parts, body, "Border", "border");
    parts.join(" ")
}

fn push_pdf_name_value(parts: &mut Vec<String>, body: &str, key: &str, label: &str) {
    if let Some(value) = pdf_value_after_name(body, key) {
        parts.push(format!("{label}={value}"));
    }
}

fn push_pdf_array_value(parts: &mut Vec<String>, body: &str, key: &str, label: &str) {
    if let Some(value) = array_after_pdf_name(body, key) {
        parts.push(format!("{label}=[{}]", canonical_pdf_array(value)));
    }
}

fn value_after_pdf_name(body: &str, key: &str) -> Option<String> {
    pdf_value_after_name(body, key)
}

fn pdf_value_after_name(body: &str, key: &str) -> Option<String> {
    let start = pdf_name_value_starts(body, key).last().copied()?;
    let remaining = body[start..].trim_start();
    if let Some(array) = remaining.strip_prefix('[') {
        return array
            .split_once(']')
            .map(|(value, _)| format!("[{}]", canonical_pdf_array(value)));
    }
    if let Some(value) = remaining.strip_prefix('(') {
        return value
            .split_once(')')
            .map(|(value, _)| value.chars().take(120).collect());
    }
    if let Some(value) = remaining.strip_prefix('/') {
        return value
            .split_whitespace()
            .next()
            .map(|value| value.chars().take(120).collect());
    }
    remaining
        .split_whitespace()
        .next()
        .map(|value| value.chars().take(120).collect())
}

fn pdf_name_value_start(body: &str, key: &str) -> Option<usize> {
    pdf_name_value_starts(body, key).into_iter().next()
}

fn pdf_name_value_starts(body: &str, key: &str) -> Vec<usize> {
    let needle = format!("/{key}");
    let mut search_start = 0usize;
    let mut starts = Vec::new();
    while let Some(relative) = body[search_start..].find(&needle) {
        let name_start = search_start + relative;
        let value_start = name_start + needle.len();
        let is_delimited = body[value_start..]
            .chars()
            .next()
            .is_none_or(is_pdf_delimiter);
        if is_delimited {
            starts.push(value_start);
        }
        search_start = value_start;
    }
    starts
}

fn is_pdf_delimiter(value: char) -> bool {
    value.is_ascii_whitespace() || matches!(value, '[' | ']' | '(' | ')' | '<' | '>' | '/')
}

fn array_after_pdf_name<'a>(body: &'a str, key: &str) -> Option<&'a str> {
    let start = pdf_name_value_starts(body, key).last().copied()?;
    let remaining = body[start..].trim_start();
    let array = remaining.strip_prefix('[')?;
    array.split_once(']').map(|(value, _)| value)
}

fn canonical_pdf_array(value: &str) -> String {
    value
        .split_whitespace()
        .take(32)
        .collect::<Vec<_>>()
        .join(" ")
}

fn rect_after_pdf_name(body: &str, key: &str) -> Option<Rect> {
    let numbers = number_array_after_pdf_name(body, key);
    if numbers.len() < 4 {
        return None;
    }
    Some(Rect {
        x0: numbers[0].min(numbers[2]),
        y0: numbers[1].min(numbers[3]),
        x1: numbers[0].max(numbers[2]),
        y1: numbers[1].max(numbers[3]),
    })
}

fn number_array_after_pdf_name(body: &str, key: &str) -> Vec<f32> {
    array_after_pdf_name(body, key)
        .map(|value| {
            value
                .split_whitespace()
                .filter_map(|token| token.parse::<f32>().ok())
                .collect()
        })
        .unwrap_or_default()
}

fn canonical_rect(rect: Rect) -> String {
    format!(
        "[{} {} {} {}]",
        canonical_report_number(rect.x0),
        canonical_report_number(rect.y0),
        canonical_report_number(rect.x1),
        canonical_report_number(rect.y1)
    )
}

fn push_surface_change(
    document: &mut DiffDocument,
    old_surface: Option<&DocumentSurface>,
    new_surface: Option<&DocumentSurface>,
    reason: String,
) {
    let category = old_surface
        .map(|surface| surface.category)
        .or_else(|| new_surface.map(|surface| surface.category))
        .unwrap_or(SurfaceCategory::Metadata);
    let change = SemanticChange {
        id: format!("change-{:04}", document.changes.len()),
        kind: category.change_kind(),
        severity: ChangeSeverity::Info,
        old_node: old_surface.map(|surface| surface_evidence(FileRole::Old, surface)),
        new_node: new_surface.map(|surface| surface_evidence(FileRole::New, surface)),
        text_hunks: Vec::new(),
        layout_diff: None,
        confidence: 0.8,
        reason,
    };
    document.changes.push(change);
}

fn surface_evidence(file_role: FileRole, surface: &DocumentSurface) -> SemanticNodeEvidence {
    SemanticNodeEvidence {
        node_id: format!("{}-{:04}", surface.category.node_prefix(), surface.index),
        page: 0,
        bbox: surface.bbox,
        text: Some(format!(
            "{} object {} 0 R hash={} {}",
            surface.category.report_label(),
            surface.object_id.number,
            surface.hash,
            surface.summary
        )),
        source: vec![Provenance {
            file_role: Some(file_role),
            object_id: Some(surface.object_id),
            page_index: None,
            stream_object_id: Some(surface.object_id),
            content_op_index: None,
            byte_range: surface.byte_range,
        }],
    }
}

fn image_payloads(document: &pdf_core::PdfDocument) -> Vec<ImagePayload> {
    document
        .objects
        .iter()
        .filter(|object| object.body.contains("/Subtype /Image"))
        .filter_map(|object| {
            let stream = object.stream.as_ref()?;
            Some((object.id, stream.byte_range, stream.bytes.as_slice()))
        })
        .enumerate()
        .map(|(index, (object_id, byte_range, bytes))| ImagePayload {
            index,
            object_id,
            byte_range,
            byte_len: bytes.len(),
            hash: stable_hash(bytes),
        })
        .collect()
}

fn push_image_payload_change(
    document: &mut DiffDocument,
    old_image: Option<&ImagePayload>,
    new_image: Option<&ImagePayload>,
    reason: String,
) {
    let change = SemanticChange {
        id: format!("change-{:04}", document.changes.len()),
        kind: ChangeKind::ObjectChanged,
        severity: ChangeSeverity::Info,
        old_node: old_image.map(|image| image_payload_evidence(FileRole::Old, image)),
        new_node: new_image.map(|image| image_payload_evidence(FileRole::New, image)),
        text_hunks: Vec::new(),
        layout_diff: None,
        confidence: 1.0,
        reason,
    };
    document.changes.push(change);
}

fn image_payload_evidence(file_role: FileRole, image: &ImagePayload) -> SemanticNodeEvidence {
    SemanticNodeEvidence {
        node_id: format!("image-{:04}", image.index),
        page: 0,
        bbox: None,
        text: Some(format!(
            "image XObject {} 0 R bytes={} hash={}",
            image.object_id.number, image.byte_len, image.hash
        )),
        source: vec![Provenance {
            file_role: Some(file_role),
            object_id: Some(image.object_id),
            page_index: None,
            stream_object_id: Some(image.object_id),
            content_op_index: None,
            byte_range: Some(image.byte_range),
        }],
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum GraphicSurfaceCategory {
    VectorPath,
    Style,
    TextStyle,
}

#[derive(Debug, Clone, PartialEq)]
struct GraphicSurface {
    category: GraphicSurfaceCategory,
    index: usize,
    page_index: usize,
    stream_object_id: Option<ObjectId>,
    content_op_index: Option<usize>,
    bbox: Option<Rect>,
    signature: String,
    hash: String,
    match_key: Option<String>,
    source: Provenance,
}

fn append_graphic_surface_changes(
    document: &mut DiffDocument,
    old_document: &pdf_core::PdfDocument,
    new_document: &pdf_core::PdfDocument,
    config: ParseConfig,
) {
    let old_surfaces = graphic_surfaces(old_document, config);
    let new_surfaces = graphic_surfaces(new_document, config);
    for index in 0..old_surfaces.len().max(new_surfaces.len()) {
        match (old_surfaces.get(index), new_surfaces.get(index)) {
            (Some(old_surface), Some(new_surface))
                if old_surface.hash == new_surface.hash
                    && old_surface.category == new_surface.category => {}
            (Some(old_surface), Some(new_surface))
                if old_surface.category == GraphicSurfaceCategory::TextStyle
                    && new_surface.category == GraphicSurfaceCategory::TextStyle
                    && old_surface.match_key != new_surface.match_key => {}
            (Some(old_surface), None)
                if old_surface.category == GraphicSurfaceCategory::TextStyle => {}
            (None, Some(new_surface))
                if new_surface.category == GraphicSurfaceCategory::TextStyle => {}
            (Some(old_surface), Some(new_surface)) => push_graphic_surface_change(
                document,
                Some(old_surface),
                Some(new_surface),
                format!(
                    "{} differs at operation index {index} (old hash {} -> new hash {})",
                    old_surface.category.report_label(),
                    old_surface.hash,
                    new_surface.hash
                ),
            ),
            (Some(old_surface), None) => push_graphic_surface_change(
                document,
                Some(old_surface),
                None,
                format!(
                    "{} exists only in old document at operation index {index}",
                    old_surface.category.report_label()
                ),
            ),
            (None, Some(new_surface)) => push_graphic_surface_change(
                document,
                None,
                Some(new_surface),
                format!(
                    "{} exists only in new document at operation index {index}",
                    new_surface.category.report_label()
                ),
            ),
            (None, None) => {}
        }
    }
}

fn graphic_surfaces(document: &pdf_core::PdfDocument, config: ParseConfig) -> Vec<GraphicSurface> {
    let mut surfaces = Vec::new();
    for content in document.page_contents() {
        let program = pdf_content::parse_content_stream_with_limits(
            content.bytes,
            content.page_index,
            Some(content.stream_object_id),
            config.limits,
        );
        let mut current_font: Option<String> = None;
        let mut current_font_size = 12.0f32;
        for operation in &program.operations {
            if let ContentOp::SetFont { name, size, .. } = operation {
                current_font = Some(name.clone());
                current_font_size = *size;
            }
            if let Some(mut surface) = graphic_surface_from_operation(surfaces.len(), operation) {
                surface.page_index = content.page_index;
                if surface.stream_object_id.is_none() {
                    surface.stream_object_id = Some(content.stream_object_id);
                }
                surfaces.push(surface);
            }
            if let Some(mut surface) = text_style_surface_from_operation(
                surfaces.len(),
                operation,
                current_font.as_deref(),
                current_font_size,
            ) {
                surface.page_index = content.page_index;
                if surface.stream_object_id.is_none() {
                    surface.stream_object_id = Some(content.stream_object_id);
                }
                surfaces.push(surface);
            }
        }
    }
    surfaces.sort_by_key(|surface| {
        (
            surface.page_index,
            surface.stream_object_id,
            surface.content_op_index,
            surface.category,
            surface.hash.clone(),
        )
    });
    for (index, surface) in surfaces.iter_mut().enumerate() {
        surface.index = index;
    }
    surfaces
}

fn graphic_surface_from_operation(index: usize, operation: &ContentOp) -> Option<GraphicSurface> {
    match operation {
        ContentOp::AppendRectangle { rect, source } => {
            let signature = format!(
                "vector re {} {} {} {}",
                canonical_report_number(rect.x0),
                canonical_report_number(rect.y0),
                canonical_report_number(rect.x1),
                canonical_report_number(rect.y1)
            );
            Some(graphic_surface(
                GraphicSurfaceCategory::VectorPath,
                index,
                Some(*rect),
                signature,
                source,
            ))
        }
        ContentOp::RecognizedNonText {
            operator,
            operands,
            source,
        } if is_vector_graphics_operator(operator) => {
            let signature = if operands.is_empty() {
                format!("vector {operator}")
            } else {
                format!("vector {operator} {}", operands.join(" "))
            };
            Some(graphic_surface(
                GraphicSurfaceCategory::VectorPath,
                index,
                None,
                signature,
                source,
            ))
        }
        ContentOp::RecognizedNonText {
            operator,
            operands,
            source,
        } if is_graphic_style_operator(operator) => {
            let signature = if operands.is_empty() {
                format!("style {operator}")
            } else {
                format!("style {operator} {}", operands.join(" "))
            };
            Some(graphic_surface(
                GraphicSurfaceCategory::Style,
                index,
                None,
                signature,
                source,
            ))
        }
        ContentOp::BeginText { .. }
        | ContentOp::EndText { .. }
        | ContentOp::SetFont { .. }
        | ContentOp::MoveTextPosition { .. }
        | ContentOp::MoveToNextLine { .. }
        | ContentOp::SetTextLeading { .. }
        | ContentOp::SetCharacterSpacing { .. }
        | ContentOp::SetWordSpacing { .. }
        | ContentOp::SetHorizontalScaling { .. }
        | ContentOp::SetTextMatrix { .. }
        | ContentOp::ShowText { .. }
        | ContentOp::ShowAdjustedText { .. }
        | ContentOp::SaveGraphicsState { .. }
        | ContentOp::RestoreGraphicsState { .. }
        | ContentOp::ConcatMatrix { .. }
        | ContentOp::BeginMarkedContent { .. }
        | ContentOp::EndMarkedContent { .. }
        | ContentOp::RecognizedNonText { .. }
        | ContentOp::Unknown { .. } => None,
    }
}

fn text_style_surface_from_operation(
    index: usize,
    operation: &ContentOp,
    current_font: Option<&str>,
    current_font_size: f32,
) -> Option<GraphicSurface> {
    let (text, source) = match operation {
        ContentOp::ShowText { text, source, .. }
        | ContentOp::ShowAdjustedText { text, source, .. } => (text, source),
        ContentOp::BeginText { .. }
        | ContentOp::EndText { .. }
        | ContentOp::SetFont { .. }
        | ContentOp::MoveTextPosition { .. }
        | ContentOp::MoveToNextLine { .. }
        | ContentOp::SetTextLeading { .. }
        | ContentOp::SetCharacterSpacing { .. }
        | ContentOp::SetWordSpacing { .. }
        | ContentOp::SetHorizontalScaling { .. }
        | ContentOp::SetTextMatrix { .. }
        | ContentOp::SaveGraphicsState { .. }
        | ContentOp::RestoreGraphicsState { .. }
        | ContentOp::ConcatMatrix { .. }
        | ContentOp::AppendRectangle { .. }
        | ContentOp::BeginMarkedContent { .. }
        | ContentOp::EndMarkedContent { .. }
        | ContentOp::RecognizedNonText { .. }
        | ContentOp::Unknown { .. } => return None,
    };
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return None;
    }
    let signature = format!(
        "text-style text_hash={} font={} size={}",
        stable_hash(normalized.as_bytes()),
        current_font.unwrap_or("unknown"),
        canonical_report_number(current_font_size)
    );
    let mut surface = graphic_surface(
        GraphicSurfaceCategory::TextStyle,
        index,
        None,
        signature,
        source,
    );
    surface.match_key = Some(stable_hash(normalized.as_bytes()));
    Some(surface)
}

fn graphic_surface(
    category: GraphicSurfaceCategory,
    index: usize,
    bbox: Option<Rect>,
    signature: String,
    source: &Provenance,
) -> GraphicSurface {
    GraphicSurface {
        category,
        index,
        page_index: source.page_index.unwrap_or(0),
        stream_object_id: source.stream_object_id,
        content_op_index: source.content_op_index,
        bbox,
        hash: stable_hash(signature.as_bytes()),
        signature,
        match_key: None,
        source: source.clone(),
    }
}

impl GraphicSurfaceCategory {
    fn change_kind(self) -> ChangeKind {
        match self {
            GraphicSurfaceCategory::VectorPath => ChangeKind::ObjectChanged,
            GraphicSurfaceCategory::Style | GraphicSurfaceCategory::TextStyle => {
                ChangeKind::StyleChanged
            }
        }
    }

    fn report_label(self) -> &'static str {
        match self {
            GraphicSurfaceCategory::VectorPath => "native vector graphic surface",
            GraphicSurfaceCategory::Style => "graphic style surface",
            GraphicSurfaceCategory::TextStyle => "text style surface",
        }
    }

    fn node_prefix(self) -> &'static str {
        match self {
            GraphicSurfaceCategory::VectorPath => "vector",
            GraphicSurfaceCategory::Style => "style",
            GraphicSurfaceCategory::TextStyle => "text-style",
        }
    }
}

fn push_graphic_surface_change(
    document: &mut DiffDocument,
    old_surface: Option<&GraphicSurface>,
    new_surface: Option<&GraphicSurface>,
    reason: String,
) {
    let category = old_surface
        .map(|surface| surface.category)
        .or_else(|| new_surface.map(|surface| surface.category))
        .unwrap_or(GraphicSurfaceCategory::VectorPath);
    let change = SemanticChange {
        id: format!("change-{:04}", document.changes.len()),
        kind: category.change_kind(),
        severity: ChangeSeverity::Info,
        old_node: old_surface.map(|surface| graphic_surface_evidence(FileRole::Old, surface)),
        new_node: new_surface.map(|surface| graphic_surface_evidence(FileRole::New, surface)),
        text_hunks: Vec::new(),
        layout_diff: None,
        confidence: 0.75,
        reason,
    };
    document.changes.push(change);
}

fn graphic_surface_evidence(file_role: FileRole, surface: &GraphicSurface) -> SemanticNodeEvidence {
    let mut source = surface.source.clone();
    source.file_role = Some(file_role);
    SemanticNodeEvidence {
        node_id: format!("{}-{:04}", surface.category.node_prefix(), surface.index),
        page: surface.page_index,
        bbox: surface.bbox,
        text: Some(format!(
            "{} page={} op={} hash={} {}",
            surface.category.report_label(),
            surface.page_index,
            surface.content_op_index.unwrap_or_default(),
            surface.hash,
            surface.signature
        )),
        source: vec![source],
    }
}

fn stable_hash(bytes: &[u8]) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn canonical_report_number(value: f32) -> String {
    if value == 0.0 {
        return "0".to_owned();
    }
    let mut text = format!("{value:.4}");
    while text.contains('.') && text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    text
}

fn document_has_any_token(document: &pdf_core::PdfDocument, tokens: &[&str]) -> bool {
    tokens
        .iter()
        .any(|token| document_has_token(document, token))
}

fn document_has_token(document: &pdf_core::PdfDocument, token: &str) -> bool {
    document
        .objects
        .iter()
        .any(|object| document_has_object_token(object, token))
}

fn document_has_object_token(object: &pdf_core::PdfObject, token: &str) -> bool {
    object.body.contains(token)
}

fn byte_pattern_count(bytes: &[u8], pattern: &[u8]) -> usize {
    bytes
        .windows(pattern.len())
        .filter(|window| *window == pattern)
        .count()
}

fn table_border_hints_from_program(program: &ContentProgram) -> Vec<pdf_semantic::TableBorderHint> {
    program
        .operations
        .iter()
        .filter_map(|operation| {
            let ContentOp::AppendRectangle { rect, source } = operation else {
                return None;
            };
            Some(pdf_semantic::TableBorderHint {
                page_index: source.page_index.unwrap_or(0),
                bbox: *rect,
                source: vec![source.clone()],
            })
        })
        .collect()
}

fn is_vector_graphics_operator(operator: &str) -> bool {
    matches!(
        operator,
        "m" | "l"
            | "c"
            | "v"
            | "y"
            | "h"
            | "re"
            | "S"
            | "s"
            | "f"
            | "F"
            | "f*"
            | "B"
            | "B*"
            | "b"
            | "b*"
            | "n"
            | "W"
            | "W*"
            | "sh"
    )
}

fn is_graphic_style_operator(operator: &str) -> bool {
    matches!(
        operator,
        "w" | "J"
            | "j"
            | "M"
            | "d"
            | "ri"
            | "i"
            | "gs"
            | "CS"
            | "cs"
            | "SC"
            | "SCN"
            | "sc"
            | "scn"
            | "G"
            | "g"
            | "RG"
            | "rg"
            | "K"
            | "k"
    )
}

struct ToUnicodeApplyResult {
    applied: bool,
    diagnostics: Vec<Diagnostic>,
}

fn apply_tounicode_maps(
    program: &mut ContentProgram,
    document: &pdf_core::PdfDocument,
    font_resources: &pdf_text::FontResourceSet,
) -> ToUnicodeApplyResult {
    let maps = font_tounicode_maps(document, font_resources);
    if maps.maps.is_empty() {
        return ToUnicodeApplyResult {
            applied: false,
            diagnostics: maps.diagnostics,
        };
    }

    let mut current_font: Option<String> = None;
    let mut applied = false;
    for operation in &mut program.operations {
        match operation {
            ContentOp::SetFont { name, .. } => {
                current_font = Some(name.clone());
            }
            ContentOp::ShowText {
                text, raw_bytes, ..
            }
            | ContentOp::ShowAdjustedText {
                text, raw_bytes, ..
            } => {
                let Some(font_name) = current_font.as_deref() else {
                    continue;
                };
                let Some(map) = maps.maps.get(font_name) else {
                    continue;
                };
                if let Some(decoded) = decode_with_tounicode(raw_bytes, map) {
                    *text = decoded;
                    applied = true;
                }
            }
            ContentOp::BeginText { .. }
            | ContentOp::EndText { .. }
            | ContentOp::MoveTextPosition { .. }
            | ContentOp::MoveToNextLine { .. }
            | ContentOp::SetTextLeading { .. }
            | ContentOp::SetCharacterSpacing { .. }
            | ContentOp::SetWordSpacing { .. }
            | ContentOp::SetHorizontalScaling { .. }
            | ContentOp::SetTextMatrix { .. }
            | ContentOp::SaveGraphicsState { .. }
            | ContentOp::RestoreGraphicsState { .. }
            | ContentOp::ConcatMatrix { .. }
            | ContentOp::AppendRectangle { .. }
            | ContentOp::BeginMarkedContent { .. }
            | ContentOp::EndMarkedContent { .. }
            | ContentOp::RecognizedNonText { .. }
            | ContentOp::Unknown { .. } => {}
        }
    }

    ToUnicodeApplyResult {
        applied,
        diagnostics: maps.diagnostics,
    }
}

struct ToUnicodeMaps {
    maps: BTreeMap<String, BTreeMap<Vec<u8>, String>>,
    diagnostics: Vec<Diagnostic>,
}

fn font_tounicode_maps(
    document: &pdf_core::PdfDocument,
    font_resources: &pdf_text::FontResourceSet,
) -> ToUnicodeMaps {
    let objects_by_id = document
        .objects
        .iter()
        .map(|object| (object.id, object))
        .collect::<BTreeMap<_, _>>();

    let mut maps = BTreeMap::new();
    let mut diagnostics = Vec::new();
    for (font_name, font_resource) in &font_resources.fonts {
        if let Some(cmap_object_id) = font_resource.to_unicode {
            let Some(cmap_stream) = objects_by_id
                .get(&cmap_object_id)
                .and_then(|object| object.stream.as_ref())
            else {
                continue;
            };
            let cmap = parse_tounicode_cmap_with_diagnostics(&cmap_stream.bytes);
            diagnostics.extend(cmap.diagnostics);
            if !cmap.map.is_empty() {
                maps.insert(font_name.clone(), cmap.map);
            }
        }
    }
    ToUnicodeMaps { maps, diagnostics }
}

fn reference_after_key(body: &str, key: &str) -> Option<ObjectId> {
    let start = pdf_name_value_start(body, key)?;
    parse_reference_at(&body[start..])
}

fn parse_reference_at(body: &str) -> Option<ObjectId> {
    let tokens = body_tokens(body);
    let number = tokens.first()?.parse().ok()?;
    let generation = tokens.get(1)?.parse().ok()?;
    if tokens.get(2)? != "R" {
        return None;
    }
    Some(ObjectId { number, generation })
}

fn body_tokens(body: &str) -> Vec<String> {
    body.replace("<<", " ")
        .replace(">>", " ")
        .replace('/', " /")
        .replace(['[', ']'], " ")
        .split_whitespace()
        .map(ToOwned::to_owned)
        .collect()
}

struct ToUnicodeCMap {
    map: BTreeMap<Vec<u8>, String>,
    diagnostics: Vec<Diagnostic>,
}

fn parse_tounicode_cmap_with_diagnostics(bytes: &[u8]) -> ToUnicodeCMap {
    let text = String::from_utf8_lossy(bytes);
    let mut map = BTreeMap::new();
    let mut diagnostics = Vec::new();
    let mut in_bfrange = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.ends_with("beginbfrange") {
            in_bfrange = true;
            continue;
        }
        if trimmed == "endbfrange" {
            in_bfrange = false;
            continue;
        }
        if trimmed.ends_with("beginbfchar") || trimmed == "endbfchar" {
            continue;
        }
        let hex_tokens = hex_tokens_in_line(line);
        if in_bfrange && hex_tokens.len() >= 3 {
            if let Err(message) = insert_bfrange(&mut map, &hex_tokens, trimmed.contains('[')) {
                diagnostics.push(Diagnostic::warning("CMAP_UNSUPPORTED_RANGE", message));
            }
        } else if hex_tokens.len() == 2 {
            if let Some(decoded) = unicode_hex_to_string(&hex_tokens[1]) {
                map.insert(hex_tokens[0].clone(), decoded);
            }
        } else if trimmed.contains("begin")
            || trimmed.contains("end")
            || trimmed.is_empty()
            || trimmed.starts_with('%')
        {
            continue;
        }
    }
    ToUnicodeCMap { map, diagnostics }
}

fn insert_bfrange(
    map: &mut BTreeMap<Vec<u8>, String>,
    hex_tokens: &[Vec<u8>],
    array_mode: bool,
) -> Result<(), String> {
    if hex_tokens.len() < 3 {
        return Err("bfrange entry has fewer than three hex operands".into());
    }
    let start = bytes_to_u32(&hex_tokens[0]).ok_or("bfrange start is too wide")?;
    let end = bytes_to_u32(&hex_tokens[1]).ok_or("bfrange end is too wide")?;
    if end < start {
        return Err("bfrange end is before start".into());
    }
    let count = usize::try_from(end - start + 1).map_err(|_| "bfrange is too large")?;
    if !array_mode {
        let destination_start =
            bytes_to_u32(&hex_tokens[2]).ok_or("bfrange destination is too wide")?;
        for offset in 0..count {
            let source = int_to_be_bytes(start + offset as u32, hex_tokens[0].len());
            let destination =
                int_to_be_bytes(destination_start + offset as u32, hex_tokens[2].len());
            let Some(decoded) = unicode_hex_to_string(&destination) else {
                return Err("bfrange destination is not valid UTF-16BE".into());
            };
            map.insert(source, decoded);
        }
    } else {
        if hex_tokens.len() - 2 < count {
            return Err("bfrange destination array is shorter than source range".into());
        }
        for (offset, destination) in hex_tokens[2..2 + count].iter().enumerate() {
            let source = int_to_be_bytes(start + offset as u32, hex_tokens[0].len());
            let Some(decoded) = unicode_hex_to_string(destination) else {
                return Err("bfrange array destination is not valid UTF-16BE".into());
            };
            map.insert(source, decoded);
        }
    }
    Ok(())
}

fn bytes_to_u32(bytes: &[u8]) -> Option<u32> {
    if bytes.len() > 4 {
        return None;
    }
    let mut value = 0u32;
    for byte in bytes {
        value = (value << 8) | u32::from(*byte);
    }
    Some(value)
}

fn int_to_be_bytes(value: u32, width: usize) -> Vec<u8> {
    let bytes = value.to_be_bytes();
    bytes[bytes.len().saturating_sub(width)..].to_vec()
}

fn hex_token_bytes(token: &str) -> Option<Vec<u8>> {
    let token = token.strip_prefix('<')?.strip_suffix('>')?;
    if token.is_empty() || token.len() % 2 != 0 {
        return None;
    }
    (0..token.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&token[index..index + 2], 16).ok())
        .collect()
}

fn hex_tokens_in_line(line: &str) -> Vec<Vec<u8>> {
    let bytes = line.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'<' || bytes.get(index + 1) == Some(&b'<') {
            index += 1;
            continue;
        }
        let Some(relative_end) = bytes[index + 1..].iter().position(|byte| *byte == b'>') else {
            break;
        };
        let end = index + 1 + relative_end;
        if let Some(token) = line.get(index..=end).and_then(hex_token_bytes) {
            tokens.push(token);
        }
        index = end + 1;
    }
    tokens
}

fn unicode_hex_to_string(bytes: &[u8]) -> Option<String> {
    if bytes.len() % 2 != 0 {
        return None;
    }
    let mut out = String::new();
    for chunk in bytes.chunks_exact(2) {
        let code_unit = u16::from_be_bytes([chunk[0], chunk[1]]);
        let character = char::from_u32(u32::from(code_unit))?;
        out.push(character);
    }
    Some(out)
}

fn decode_with_tounicode(raw_bytes: &[u8], map: &BTreeMap<Vec<u8>, String>) -> Option<String> {
    let mut decoded = String::new();
    let mut index = 0;
    while index < raw_bytes.len() {
        let mut matched = None;
        for width in (1..=4).rev() {
            let end = index + width;
            if end <= raw_bytes.len() {
                if let Some(value) = map.get(&raw_bytes[index..end]) {
                    matched = Some((width, value));
                    break;
                }
            }
        }
        let (width, value) = matched?;
        decoded.push_str(value);
        index += width;
    }
    Some(decoded)
}

fn run_synthetic_benchmark(pages: usize) -> Result<BenchmarkReport, PdfDiffError> {
    const TARGET_TOTAL_MS: u128 = 5_000;
    let total_start = Instant::now();
    let old_bytes = synthetic_text_pdf(pages, None);
    let new_bytes = synthetic_text_pdf(pages, Some((pages / 2, "revised")));
    let config = ParseConfig::default();

    let parse_start = Instant::now();
    let old_document = pdf_core::PdfDocument::parse_with_config(&old_bytes, config)?;
    let new_document = pdf_core::PdfDocument::parse_with_config(&new_bytes, config)?;
    let parse = parse_start.elapsed().as_millis();

    let extract_start = Instant::now();
    let old_extraction = extract_text_runs_from_document(&old_document, config);
    let new_extraction = extract_text_runs_from_document(&new_document, config);
    let extract = extract_start.elapsed().as_millis();

    let semantic_start = Instant::now();
    let old_semantic = pdf_semantic::build_semantic_document(
        "benchmark-old",
        &old_extraction.runs,
        old_extraction.diagnostics,
    );
    let new_semantic = pdf_semantic::build_semantic_document(
        "benchmark-new",
        &new_extraction.runs,
        new_extraction.diagnostics,
    );
    let semantic = semantic_start.elapsed().as_millis();

    let diff_start = Instant::now();
    let diff = diff_semantic_documents(&old_semantic, &new_semantic, DiffConfig::default());
    let diff_ms = diff_start.elapsed().as_millis();

    let report_start = Instant::now();
    let _rendered = render_diff(&diff, DiffReportFormat::Json);
    let report = report_start.elapsed().as_millis();
    let total = total_start.elapsed().as_millis();

    Ok(BenchmarkReport {
        pages,
        target_total_ms: TARGET_TOTAL_MS,
        under_target: total <= TARGET_TOTAL_MS,
        timings_ms: BenchmarkTimings {
            parse,
            extract,
            semantic,
            diff: diff_ms,
            report,
            total,
        },
        peak_memory_bytes: current_process_memory_bytes(),
        memory_note: if current_process_memory_bytes().is_some() {
            "current process memory sample".into()
        } else {
            "memory usage is unavailable without a platform-specific safe probe".into()
        },
        summary: diff.summary,
        diagnostics: diff
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.clone())
            .collect(),
    })
}

fn current_process_memory_bytes() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let status = std::fs::read_to_string("/proc/self/status").ok()?;
        let rss_kb = status
            .lines()
            .find_map(|line| line.strip_prefix("VmHWM:"))
            .or_else(|| status.lines().find_map(|line| line.strip_prefix("VmRSS:")))?
            .split_whitespace()
            .next()?
            .parse::<u64>()
            .ok()?;
        Some(rss_kb * 1024)
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

fn synthetic_text_pdf(pages: usize, replacement: Option<(usize, &str)>) -> Vec<u8> {
    let page_count = pages.max(1);
    let mut objects = Vec::<String>::new();
    let page_ids = (0..page_count)
        .map(|index| 3 + (index * 2))
        .collect::<Vec<_>>();
    let kids = page_ids
        .iter()
        .map(|object_id| format!("{object_id} 0 R"))
        .collect::<Vec<_>>()
        .join(" ");

    objects.push("1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n".into());
    objects.push(format!(
        "2 0 obj\n<< /Type /Pages /Kids [{kids}] /Count {page_count} >>\nendobj\n"
    ));

    for page_index in 0..page_count {
        let page_object_id = 3 + (page_index * 2);
        let content_object_id = page_object_id + 1;
        let text = replacement
            .filter(|(target_page, _)| *target_page == page_index)
            .map_or_else(
                || format!("Benchmark page {page_index} stable paragraph"),
                |(_, replacement_text)| {
                    format!("Benchmark page {page_index} stable paragraph {replacement_text}")
                },
            );
        let content = format!("BT /F1 12 Tf 72 720 Td ({text}) Tj ET");
        objects.push(format!(
            "{page_object_id} 0 obj\n<< /Type /Page /Parent 2 0 R /Contents {content_object_id} 0 R >>\nendobj\n"
        ));
        objects.push(format!(
            "{content_object_id} 0 obj\n<< /Length {} >>\nstream\n{content}\nendstream\nendobj\n",
            content.len()
        ));
    }

    let mut pdf = b"%PDF-1.7\n".to_vec();
    for object in objects {
        pdf.extend_from_slice(object.as_bytes());
    }
    pdf
}

fn render_diff(document: &DiffDocument, format: DiffReportFormat) -> String {
    match format {
        DiffReportFormat::Json => diff_report::to_json(document)
            .unwrap_or_else(|error| format!("{{\"error\":\"{error}\"}}")),
        DiffReportFormat::AiJson => diff_report::to_ai_review_json(document)
            .unwrap_or_else(|error| format!("{{\"error\":\"{error}\"}}")),
        DiffReportFormat::Md => diff_report::to_markdown(document),
        DiffReportFormat::Html => diff_report::to_html(document),
    }
}

fn render_inspect_report(
    fingerprint: &str,
    document: &pdf_core::PdfDocument,
    format: ReportFormat,
) -> String {
    let object_count = document.objects.len();
    let diagnostic_count = document.diagnostics.len();
    let first_page_streams = document
        .first_page_contents()
        .map_or(0, |contents| contents.len());
    let tagged_structure =
        tagged_structure_report(&document.tagged_structure(ParseConfig::default()));
    let incremental_update = incremental_update_report(document.incremental_update.as_ref());
    let report = InspectReport {
        file: fingerprint,
        object_count,
        diagnostic_count,
        first_page_streams,
        incremental_update: incremental_update.clone(),
        tagged_structure: tagged_structure.clone(),
    };
    match format {
        ReportFormat::Json => {
            to_json_pretty(&report).unwrap_or_else(|error| format!("{{\"error\":\"{error}\"}}"))
        }
        ReportFormat::Md => format!(
            "# PDF Inspect\n\n- File: `{}`\n- Objects: {}\n- Diagnostics: {}\n- First-page streams: {}\n- Incremental update: {} revisions\n- Tagged structure: {} elements, {} MCIDs\n",
            fingerprint,
            object_count,
            diagnostic_count,
            first_page_streams,
            incremental_update.revision_count,
            tagged_structure.element_count,
            tagged_structure.mcid_count
        ),
        ReportFormat::Html => format!(
            "<!doctype html><meta charset=\"utf-8\"><pre># PDF Inspect\n\n- File: `{}`\n- Objects: {}\n- Diagnostics: {}\n- First-page streams: {}\n- Incremental update: {} revisions\n- Tagged structure: {} elements, {} MCIDs\n</pre>",
            escape_html(fingerprint),
            object_count,
            diagnostic_count,
            first_page_streams,
            incremental_update.revision_count,
            tagged_structure.element_count,
            tagged_structure.mcid_count
        ),
    }
}

fn render_extract_report(
    document: &pdf_semantic::SemanticDocument,
    format: ReportFormat,
) -> String {
    match format {
        ReportFormat::Json => {
            let tables = extract_table_reports(document);
            let table_cells = tables
                .iter()
                .map(|table| table.cells.iter().map(Vec::len).sum::<usize>())
                .sum();
            let report = ExtractReport {
                file: &document.fingerprint,
                paragraphs: document.nodes.len(),
                header_candidates: semantic_kind_count(
                    document,
                    pdf_semantic::SemanticNodeKind::HeaderCandidate,
                ),
                footer_candidates: semantic_kind_count(
                    document,
                    pdf_semantic::SemanticNodeKind::FooterCandidate,
                ),
                page_template_candidates: semantic_kind_count(
                    document,
                    pdf_semantic::SemanticNodeKind::PageTemplateCandidate,
                ),
                table_candidates: tables.len(),
                table_cells,
                tables,
                diagnostic_count: document.diagnostics.len(),
                tagged_structure: document
                    .tagged_structure
                    .as_ref()
                    .map(tagged_structure_report_from_semantic),
            };
            to_json_pretty(&report).unwrap_or_else(|error| format!("{{\"error\":\"{error}\"}}"))
        }
        ReportFormat::Md => {
            let mut out = format!("# Extracted Text\n\nFile: `{}`\n\n", document.fingerprint);
            if let Some(tagged_structure) = &document.tagged_structure {
                out.push_str(&format!(
                    "Tagged structure: {} elements, {} MCIDs\n\n",
                    tagged_structure.element_count, tagged_structure.mcid_count
                ));
            }
            for node in &document.nodes {
                if let Some(text) = &node.normalized_text {
                    out.push_str(&format!("- {}\n", text));
                    if let Some(table) = &node.table {
                        out.push_str(&format!(
                            "  table: {} rows x {} columns, {} border hints, confidence {:.2}\n",
                            table.rows.len(),
                            table.column_x_positions.len(),
                            table.border_hints.len(),
                            table.confidence
                        ));
                    }
                }
            }
            out
        }
        ReportFormat::Html => {
            let markdown = render_extract_report(document, ReportFormat::Md);
            format!(
                "<!doctype html><meta charset=\"utf-8\"><pre>{}</pre>",
                escape_html(&markdown)
            )
        }
    }
}

fn semantic_kind_count(
    document: &pdf_semantic::SemanticDocument,
    kind: pdf_semantic::SemanticNodeKind,
) -> usize {
    document
        .nodes
        .iter()
        .filter(|node| node.kind == kind)
        .count()
}

fn extract_table_reports(document: &pdf_semantic::SemanticDocument) -> Vec<ExtractTableReport> {
    document
        .nodes
        .iter()
        .filter(|node| node.kind == pdf_semantic::SemanticNodeKind::TableCandidate)
        .filter_map(|node| {
            let table = node.table.as_ref()?;
            Some(ExtractTableReport {
                node_id: node.id.clone(),
                page: node.page_index,
                rows: table.rows.len(),
                columns: table.column_x_positions.len(),
                cells: table
                    .rows
                    .iter()
                    .map(|row| row.cells.iter().map(|cell| cell.text.clone()).collect())
                    .collect(),
                cell_spans: table
                    .rows
                    .iter()
                    .map(|row| {
                        row.cells
                            .iter()
                            .map(|cell| ExtractTableCellSpanReport {
                                row_span: cell.row_span,
                                column_span: cell.column_span,
                                placeholder: cell.is_placeholder,
                            })
                            .collect()
                    })
                    .collect(),
                border_hints: table.border_hints.len(),
                border_boxes: table.border_hints.iter().map(|hint| hint.bbox).collect(),
                confidence: table.confidence,
            })
        })
        .collect()
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diffs_minimal_pdf_text() {
        let old_pdf = minimal_pdf("Hello");
        let new_pdf = minimal_pdf("Hello world");
        let diff = diff_pdf_bytes("old", &old_pdf, "new", &new_pdf, DiffConfig::default())
            .expect("minimal vertical slice should diff");

        assert_eq!(diff.summary.modified, 1);
    }

    #[test]
    fn diffs_text_across_multiple_content_streams() {
        let old_pdf = multi_stream_pdf("world");
        let new_pdf = multi_stream_pdf("there");
        let diff = diff_pdf_bytes("old", &old_pdf, "new", &new_pdf, DiffConfig::default())
            .expect("multi-stream vertical slice should diff");

        assert_eq!(diff.summary.modified, 1);
        assert_eq!(
            diff.changes[0].old_node.as_ref().unwrap().text.as_deref(),
            Some("Hello world")
        );
        assert_eq!(
            diff.changes[0].new_node.as_ref().unwrap().text.as_deref(),
            Some("Hello there")
        );
    }

    #[test]
    fn extracts_text_across_multiple_pages() {
        let semantic =
            semantic_document_from_pdf("sample", &multi_page_pdf(), ParseConfig::default())
                .expect("multi-page extraction should succeed");

        let extracted = semantic
            .nodes
            .iter()
            .filter_map(|node| node.normalized_text.as_deref())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(extracted.contains("First page"));
        assert!(extracted.contains("Second page"));
    }

    #[test]
    fn reports_image_only_pdf_as_missing_text_layer() {
        let semantic =
            semantic_document_from_pdf("image-only", &image_only_pdf(), ParseConfig::default())
                .expect("image-only extraction should complete with diagnostics");

        assert!(semantic.nodes.is_empty());
        assert!(
            semantic
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "MISSING_TEXT_LAYER")
        );
    }

    #[test]
    fn reports_image_payload_changes() {
        let diff = diff_pdf_bytes(
            "old",
            &image_payload_pdf(b"x"),
            "new",
            &image_payload_pdf(b"y"),
            DiffConfig::default(),
        )
        .expect("image payload diff should complete");

        assert!(
            diff.changes
                .iter()
                .any(|change| change.kind == ChangeKind::ObjectChanged
                    && change.reason.contains("image payload differs"))
        );
        assert!(
            diff.diagnostics
                .iter()
                .all(|diagnostic| diagnostic.code != "UNSUPPORTED_IMAGE_DIFF")
        );
        assert!(
            diff.diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "MISSING_TEXT_LAYER")
        );
    }

    #[test]
    fn reports_native_vector_graphic_changes() {
        let diff = diff_pdf_bytes(
            "old",
            &vector_graphics_pdf("0 0 m 10 10 l S"),
            "new",
            &vector_graphics_pdf("0 0 m 15 10 l S"),
            DiffConfig::default(),
        )
        .expect("vector graphic diff should complete");

        assert!(
            diff.changes
                .iter()
                .any(|change| change.kind == ChangeKind::ObjectChanged
                    && change.reason.contains("native vector graphic surface"))
        );
        assert!(
            diff.diagnostics
                .iter()
                .all(|diagnostic| diagnostic.code != "UNSUPPORTED_VECTOR_GRAPHIC_DIFF")
        );
    }

    #[test]
    fn reports_graphic_style_changes() {
        let diff = diff_pdf_bytes(
            "old",
            &vector_graphics_pdf("0.1 0.2 0.3 rg 0 0 m 10 10 l S"),
            "new",
            &vector_graphics_pdf("0.7 0.2 0.3 rg 0 0 m 10 10 l S"),
            DiffConfig::default(),
        )
        .expect("graphic style diff should complete");

        assert!(
            diff.changes
                .iter()
                .any(|change| change.kind == ChangeKind::StyleChanged
                    && change.reason.contains("graphic style surface"))
        );
    }

    #[test]
    fn reports_text_style_changes_for_unchanged_text() {
        let diff = diff_pdf_bytes(
            "old",
            &styled_text_pdf("F1", 12.0, "Heading"),
            "new",
            &styled_text_pdf("F1", 18.0, "Heading"),
            DiffConfig::default(),
        )
        .expect("text style diff should complete");

        assert_eq!(diff.summary.modified, 0);
        assert!(
            diff.changes
                .iter()
                .any(|change| change.kind == ChangeKind::StyleChanged
                    && change.reason.contains("text style surface"))
        );
    }

    #[test]
    fn reports_link_annotation_semantic_changes() {
        let diff = diff_pdf_bytes(
            "old",
            &annotation_pdf(
                "/Type /Annot /Subtype /Link /Rect [72 700 180 720] /A << /S /URI /URI (https://example.test/v1) >>",
            ),
            "new",
            &annotation_pdf(
                "/Type /Annot /Subtype /Link /Rect [72 700 180 720] /A << /S /URI /URI (https://example.test/v2) >>",
            ),
            DiffConfig::default(),
        )
        .expect("link annotation diff should complete");

        let change = diff
            .changes
            .iter()
            .find(|change| change.kind == ChangeKind::AnnotationChanged)
            .expect("URI change should produce an annotation semantic change");
        assert!(change.reason.contains("annotation/link"));
        assert!(
            change
                .new_node
                .as_ref()
                .and_then(|node| node.text.as_deref())
                .is_some_and(|text| text.contains("uri=https://example.test/v2")
                    && text.contains("rect=[72 700 180 720]"))
        );
        assert!(
            diff.diagnostics
                .iter()
                .all(|diagnostic| diagnostic.code != "UNSUPPORTED_ANNOTATION_DIFF")
        );
    }

    #[test]
    fn ignores_annotation_dictionary_order_when_semantics_match() {
        let diff = diff_pdf_bytes(
            "old",
            &annotation_pdf(
                "/Type /Annot /Subtype /Text /Rect [10 20 30 40] /Contents (Review note) /C [1 1 0]",
            ),
            "new",
            &annotation_pdf(
                "/C [1 1 0] /Contents (Review note) /Rect [10 20 30 40] /Subtype /Text /Type /Annot",
            ),
            DiffConfig::default(),
        )
        .expect("annotation diff should complete");

        assert!(
            diff.changes
                .iter()
                .all(|change| change.kind != ChangeKind::AnnotationChanged)
        );
    }

    #[test]
    fn reports_cid_font_without_tounicode() {
        let semantic = semantic_document_from_pdf(
            "cid-font",
            &cid_font_without_tounicode_pdf(),
            ParseConfig::default(),
        )
        .expect("CID-font extraction should complete with diagnostics");

        assert!(
            semantic
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "MISSING_TOUNICODE_CID_FONT")
        );
    }

    #[test]
    fn does_not_report_cid_descendant_missing_tounicode_when_type0_parent_has_map() {
        let semantic = semantic_document_from_pdf(
            "cid-font-with-map",
            &cid_font_with_tounicode_pdf(),
            ParseConfig::default(),
        )
        .expect("CID-font extraction should use the Type0 ToUnicode map");

        let text = semantic
            .nodes
            .iter()
            .filter_map(|node| node.normalized_text.as_deref())
            .collect::<Vec<_>>()
            .join(" ");

        assert_eq!(text, "C");
        assert!(
            semantic
                .diagnostics
                .iter()
                .all(|diagnostic| diagnostic.code != "MISSING_TOUNICODE_CID_FONT"
                    && diagnostic.code != "MISSING_TOUNICODE")
        );
    }

    #[test]
    fn reports_tagged_pdf_structure_markers() {
        let semantic = semantic_document_from_pdf("tagged", &tagged_pdf(), ParseConfig::default())
            .expect("tagged PDF extraction should complete with diagnostics");

        assert!(
            semantic
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "TAGGED_PDF_STRUCTURE_DETECTED")
        );
        assert!(
            semantic
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "TAGGED_MCID_DETECTED")
        );
        let tagged_structure = semantic
            .tagged_structure
            .as_ref()
            .expect("simple structure tree should be parsed");
        assert_eq!(tagged_structure.element_count, 1);
        assert_eq!(tagged_structure.mcid_count, 1);
        assert_eq!(tagged_structure.structure_types, vec!["P".to_owned()]);
    }

    #[test]
    fn synthetic_benchmark_reports_all_m8_t5_phases() {
        let report = run_synthetic_benchmark(50).expect("benchmark should run");

        assert_eq!(report.pages, 50);
        assert!(report.under_target);
        assert!(report.timings_ms.total <= report.target_total_ms);
        assert!(report.summary.modified >= 1);
    }

    #[test]
    fn inspect_report_includes_object_count() {
        let parsed = pdf_core::PdfDocument::parse(minimal_pdf("Hello").as_slice()).unwrap();
        let json = render_inspect_report("sample.pdf", &parsed, ReportFormat::Json);
        assert!(json.contains("\"object_count\""));
    }

    #[test]
    fn inspect_report_includes_tagged_structure_summary() {
        let parsed = pdf_core::PdfDocument::parse(tagged_pdf().as_slice()).unwrap();
        let json = render_inspect_report("tagged.pdf", &parsed, ReportFormat::Json);
        let value: serde_json::Value = serde_json::from_str(&json).expect("JSON should parse");

        assert_eq!(value["tagged_structure"]["detected"], true);
        assert_eq!(value["tagged_structure"]["element_count"], 1);
        assert_eq!(value["tagged_structure"]["mcid_count"], 1);
        assert_eq!(value["tagged_structure"]["structure_types"][0], "P");
    }

    #[test]
    fn inspect_report_includes_incremental_update_offsets() {
        let parsed = pdf_core::PdfDocument::parse(incremental_update_pdf().as_slice()).unwrap();
        let json = render_inspect_report("incremental.pdf", &parsed, ReportFormat::Json);
        let value: serde_json::Value = serde_json::from_str(&json).expect("JSON should parse");

        assert_eq!(value["incremental_update"]["detected"], true);
        assert_eq!(value["incremental_update"]["revision_count"], 2);
        assert_eq!(
            value["incremental_update"]["selected_startxref_offset"],
            128
        );
        assert_eq!(value["incremental_update"]["prior_startxref_offsets"][0], 0);
        assert_eq!(value["incremental_update"]["trailer_prev_offsets"][0], 42);
    }

    #[test]
    fn inspect_json_escapes_file_name() {
        let parsed = pdf_core::PdfDocument::parse(minimal_pdf("Hello").as_slice()).unwrap();
        let json =
            render_inspect_report("sample \"quoted\" \\ file.pdf", &parsed, ReportFormat::Json);
        let value: serde_json::Value = serde_json::from_str(&json).expect("JSON should parse");

        assert_eq!(value["file"], "sample \"quoted\" \\ file.pdf");
    }

    #[test]
    fn extract_report_lists_text() {
        let semantic =
            semantic_document_from_pdf("sample", &minimal_pdf("Hello"), ParseConfig::default())
                .expect("extract should succeed");
        let markdown = render_extract_report(&semantic, ReportFormat::Md);
        assert!(markdown.contains("- Hello"));
    }

    #[test]
    fn extract_report_serializes_table_candidate_evidence() {
        let semantic = pdf_semantic::build_semantic_document(
            "table",
            &[
                text_run("a1", "A1", 10.0, 100.0),
                text_run("a2", "A2", 70.0, 100.0),
                text_run("b1", "B1", 10.0, 84.0),
                text_run("b2", "B2", 70.0, 84.0),
            ],
            Vec::new(),
        );
        let json = render_extract_report(&semantic, ReportFormat::Json);
        let value: serde_json::Value =
            serde_json::from_str(&json).expect("extract JSON should parse");

        assert_eq!(value["table_candidates"], 1);
        assert_eq!(value["table_cells"], 4);
        assert_eq!(value["tables"][0]["rows"], 2);
        assert_eq!(value["tables"][0]["columns"], 2);
        assert_eq!(value["tables"][0]["cells"][1][1], "B2");

        let markdown = render_extract_report(&semantic, ReportFormat::Md);
        assert!(markdown.contains("table: 2 rows x 2 columns"));
    }

    #[test]
    fn extract_report_serializes_sparse_table_blank_cells() {
        let semantic = pdf_semantic::build_semantic_document(
            "sparse-table",
            &[
                text_run("a1", "A1", 10.0, 100.0),
                text_run("a2", "A2", 70.0, 100.0),
                text_run("b2", "B2", 70.0, 84.0),
            ],
            Vec::new(),
        );
        let json = render_extract_report(&semantic, ReportFormat::Json);
        let value: serde_json::Value =
            serde_json::from_str(&json).expect("extract JSON should parse");

        assert_eq!(value["table_candidates"], 1);
        assert_eq!(value["table_cells"], 4);
        assert_eq!(value["tables"][0]["cells"][1][0], "");
        assert_eq!(value["tables"][0]["cells"][1][1], "B2");
    }

    #[test]
    fn extract_report_serializes_table_column_spans() {
        let semantic = pdf_semantic::build_semantic_document(
            "spanning-table",
            &[
                text_run_with_width("header", "Total", 10.0, 116.0, 72.0),
                text_run("a1", "A1", 10.0, 100.0),
                text_run("a2", "A2", 70.0, 100.0),
            ],
            Vec::new(),
        );
        let json = render_extract_report(&semantic, ReportFormat::Json);
        let value: serde_json::Value =
            serde_json::from_str(&json).expect("extract JSON should parse");

        assert_eq!(value["table_candidates"], 1);
        assert_eq!(value["tables"][0]["cells"][0][0], "Total");
        assert_eq!(value["tables"][0]["cell_spans"][0][0]["column_span"], 2);
        assert_eq!(value["tables"][0]["cell_spans"][0][1]["placeholder"], true);
        assert_eq!(value["tables"][0]["cell_spans"][0][1]["column_span"], 0);
        assert_eq!(value["tables"][0]["cell_spans"][1][0]["column_span"], 1);
        assert_eq!(value["tables"][0]["cell_spans"][1][1]["column_span"], 1);
    }

    #[test]
    fn extract_report_serializes_table_row_spans_and_merged_cells() {
        let semantic = pdf_semantic::build_semantic_document(
            "merged-table",
            &[
                text_run_with_box("merged", "Total", 10.0, 84.0, 72.0, 44.0),
                text_run("a3", "A3", 130.0, 116.0),
                text_run("b3", "B3", 130.0, 84.0),
                text_run("c1", "C1", 10.0, 68.0),
                text_run("c2", "C2", 70.0, 68.0),
                text_run("c3", "C3", 130.0, 68.0),
            ],
            Vec::new(),
        );
        let json = render_extract_report(&semantic, ReportFormat::Json);
        let value: serde_json::Value =
            serde_json::from_str(&json).expect("extract JSON should parse");

        assert_eq!(value["table_candidates"], 1);
        assert_eq!(value["tables"][0]["cells"][0][0], "Total");
        assert_eq!(value["tables"][0]["cell_spans"][0][0]["row_span"], 2);
        assert_eq!(value["tables"][0]["cell_spans"][0][0]["column_span"], 2);
        assert_eq!(value["tables"][0]["cell_spans"][0][1]["placeholder"], true);
        assert_eq!(value["tables"][0]["cell_spans"][0][1]["row_span"], 0);
        assert_eq!(value["tables"][0]["cell_spans"][1][0]["placeholder"], true);
        assert_eq!(value["tables"][0]["cell_spans"][1][0]["column_span"], 0);
        assert_eq!(value["tables"][0]["cells"][1][2], "B3");
        assert_eq!(value["tables"][0]["cells"][2][0], "C1");
    }

    #[test]
    fn extract_report_serializes_table_border_hint_evidence() {
        let semantic = pdf_semantic::build_semantic_document_with_table_hints(
            "table",
            &[
                text_run("a1", "A1", 10.0, 100.0),
                text_run("a2", "A2", 70.0, 100.0),
                text_run("b1", "B1", 10.0, 84.0),
                text_run("b2", "B2", 70.0, 84.0),
            ],
            Vec::new(),
            vec![pdf_semantic::TableBorderHint {
                page_index: 0,
                bbox: Rect {
                    x0: 8.0,
                    y0: 82.0,
                    x1: 82.0,
                    y1: 114.0,
                },
                source: vec![Provenance {
                    page_index: Some(0),
                    content_op_index: Some(2),
                    ..Provenance::unknown()
                }],
            }],
        );
        let json = render_extract_report(&semantic, ReportFormat::Json);
        let value: serde_json::Value =
            serde_json::from_str(&json).expect("extract JSON should parse");

        assert_eq!(value["tables"][0]["border_hints"], 1);
        assert_eq!(value["tables"][0]["border_boxes"][0]["x0"], 8.0);
        assert_eq!(value["tables"][0]["confidence"], 0.75);

        let markdown = render_extract_report(&semantic, ReportFormat::Md);
        assert!(markdown.contains("1 border hints"));
    }

    #[test]
    fn review_command_calls_openai_compatible_endpoint() {
        let folder = PathBuf::from("target/spdfdiff_cli_tests/llm_review");
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).expect("fixture folder should be created");
        let ai_json = folder.join("review.ai.json");
        let output = folder.join("review.llm.json");
        write_ai_review_fixture(&ai_json);
        let endpoint = spawn_fake_openai_server("{\"summary\":\"local review ok\"}");

        let exit_code = run(Cli {
            command: Command::Review {
                ai_json,
                endpoint,
                model: "local-test-model".into(),
                output: Some(output.clone()),
                api_key: None,
                timeout_seconds: 5,
                max_review_items: 5,
            },
        })
        .expect("review command should succeed");

        assert_eq!(exit_code, 0);
        let value: serde_json::Value = serde_json::from_slice(
            &std::fs::read(&output).expect("review output should be written"),
        )
        .expect("review output should parse");
        assert_eq!(value["schema_version"], "0.1.0");
        assert_eq!(value["provider"], "openai-compatible");
        assert_eq!(value["model"], "local-test-model");
        assert_eq!(value["source"]["total_changes"], 1);
        assert_eq!(value["response"]["status_code"], 200);
        assert_eq!(value["response"]["model"], "fake-local-model");
        assert_eq!(
            value["response"]["content"],
            "{\"summary\":\"local review ok\"}"
        );
        assert_eq!(value["request"]["stream"], false);
        assert_eq!(value["request"]["temperature"], 0.0);

        std::fs::remove_dir_all(&folder).expect("fixture folder should be removed");
    }

    fn write_ai_review_fixture(path: &Path) {
        let value = serde_json::json!({
            "schema_version": "0.1.0",
            "source_schema_version": "0.1.0",
            "old_fingerprint": "old.pdf",
            "new_fingerprint": "new.pdf",
            "summary": {
                "total_changes": 1,
                "inserted": 0,
                "deleted": 0,
                "modified": 1,
                "moved": 0,
                "layout_changed": 0,
                "diagnostic_count": 0,
                "low_confidence_change_count": 0,
                "unsupported_surface_count": 0
            },
            "question_hints": [],
            "review_items": [{
                "change_id": "c0001",
                "kind": "Modified",
                "severity": "Major",
                "confidence": 0.9,
                "confidence_bucket": "High",
                "tags": ["TextChanged"],
                "explanation": "Text changed.",
                "evidence": {
                    "old_node_id": "n0001",
                    "new_node_id": "n0001",
                    "section_hint": null,
                    "old_page": 0,
                    "new_page": 0,
                    "old_bbox": null,
                    "new_bbox": null,
                    "old_text": "Payment is due in 30 days.",
                    "new_text": "Payment is due in 15 days.",
                    "text_hunks": [],
                    "provenance": []
                }
            }],
            "diagnostic_summary": []
        });
        std::fs::write(path, serde_json::to_vec_pretty(&value).unwrap())
            .expect("AI review fixture should be written");
    }

    fn spawn_fake_openai_server(content: &'static str) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("fake server should bind");
        let address = listener.local_addr().expect("fake server addr");
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("fake server should accept");
            let request = read_fake_http_request(&mut stream);
            assert!(request.contains("POST /v1/chat/completions HTTP/1.1"));
            assert!(request.contains("\"model\":\"local-test-model\""));
            assert!(request.contains("c0001"));
            let response_body = serde_json::json!({
                "model": "fake-local-model",
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "content": content
                    },
                    "finish_reason": "stop"
                }]
            })
            .to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            stream
                .write_all(response.as_bytes())
                .expect("fake response should be written");
        });
        format!("http://{address}/v1")
    }

    fn read_fake_http_request(stream: &mut std::net::TcpStream) -> String {
        let mut buffer = Vec::new();
        let mut chunk = [0u8; 1024];
        loop {
            let read = stream.read(&mut chunk).expect("fake request should read");
            if read == 0 {
                break;
            }
            buffer.extend_from_slice(&chunk[..read]);
            if let Some(header_end) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&buffer[..header_end]);
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().ok())
                            .flatten()
                    })
                    .unwrap_or(0);
                if buffer.len() >= header_end + 4 + content_length {
                    break;
                }
            }
        }
        String::from_utf8(buffer).expect("fake request should be UTF-8")
    }

    fn text_run(id: &str, text: &str, x: f32, y: f32) -> pdf_text::TextRun {
        text_run_with_width(id, text, x, y, 10.0)
    }

    fn text_run_with_width(id: &str, text: &str, x: f32, y: f32, width: f32) -> pdf_text::TextRun {
        text_run_with_box(id, text, x, y, width, 12.0)
    }

    fn text_run_with_box(
        id: &str,
        text: &str,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    ) -> pdf_text::TextRun {
        pdf_text::TextRun {
            id: id.to_owned(),
            text: text.to_owned(),
            normalized_text: text.to_owned(),
            glyphs: Vec::new(),
            bbox: Rect {
                x0: x,
                y0: y,
                x1: x + width,
                y1: y + height,
            },
            source: Provenance {
                page_index: Some(0),
                ..Provenance::unknown()
            },
            marked_content: None,
        }
    }

    #[test]
    fn parses_and_applies_tounicode_cmap() {
        let cmap = parse_tounicode_cmap_with_diagnostics(
            b"2 beginbfchar\n<0026> <0043>\n<004f> <006c>\nendbfchar\n",
        );

        assert_eq!(
            decode_with_tounicode(&[0x00, 0x26, 0x00, 0x4f], &cmap.map).as_deref(),
            Some("Cl")
        );
    }

    #[test]
    fn parses_tounicode_bfrange_and_reports_unsupported_syntax() {
        let cmap = parse_tounicode_cmap_with_diagnostics(
            b"1 beginbfrange\n<0001> <0003> <0041>\n<0004> <0005> [<0058> <0059>]\n<0006> <0008> [<005a>]\nendbfrange\n",
        );

        assert_eq!(
            decode_with_tounicode(&[0, 1, 0, 2, 0, 3], &cmap.map).as_deref(),
            Some("ABC")
        );
        assert_eq!(
            decode_with_tounicode(&[0, 4, 0, 5], &cmap.map).as_deref(),
            Some("XY")
        );
        assert!(
            cmap.diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "CMAP_UNSUPPORTED_RANGE")
        );
    }

    #[test]
    fn corpus_report_lists_files_and_diagnostic_counts() {
        let folder = PathBuf::from("target/spdfdiff_cli_tests/corpus_report");
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).expect("fixture folder should be created");
        std::fs::write(folder.join("b.pdf"), minimal_pdf("Hello"))
            .expect("valid fixture should be written");
        std::fs::write(folder.join("a.pdf"), b"not a pdf")
            .expect("invalid fixture should be written");

        let report = build_corpus_report(&folder, ParseConfig::default())
            .expect("corpus report should render");
        let value: serde_json::Value =
            serde_json::from_str(&report).expect("corpus JSON should parse");

        assert_eq!(value["folder"], "corpus_report");
        assert_eq!(value["total"], 2);
        assert_eq!(value["parsed"], 1);
        assert_eq!(value["partial"], 1);
        assert_eq!(value["failed"], 1);
        assert_eq!(value["files"][0]["file"], "a.pdf");
        assert_eq!(value["files"][1]["file"], "b.pdf");
        assert_eq!(value["diagnostic_counts"]["MISSING_TOUNICODE"], 1);

        std::fs::remove_dir_all(&folder).expect("fixture folder should be removed");
    }

    #[test]
    fn corpus_report_evaluates_manifest_gate_and_diff_pairs() {
        let folder = PathBuf::from("target/spdfdiff_cli_tests/corpus_manifest");
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).expect("fixture folder should be created");
        std::fs::write(folder.join("old.pdf"), minimal_pdf("Hello"))
            .expect("old fixture should be written");
        std::fs::write(folder.join("new.pdf"), minimal_pdf("Hello world"))
            .expect("new fixture should be written");
        let manifest = CorpusManifest {
            schema_version: "1".to_owned(),
            required_files: vec![
                "old.pdf".to_owned(),
                "new.pdf".to_owned(),
                "missing.pdf".to_owned(),
            ],
            diff_pairs: vec![CorpusManifestDiffPair {
                name: "fixture".to_owned(),
                old_file: "old.pdf".to_owned(),
                new_file: "new.pdf".to_owned(),
            }],
            thresholds: CorpusGateThresholds {
                min_parsed_files: Some(3),
                max_missing_required_files: 0,
                max_failed_files: 0,
                max_failed_diff_pairs: 0,
            },
        };

        let report = build_corpus_report_model(&folder, ParseConfig::default(), Some(&manifest))
            .expect("manifest corpus report should render");
        assert_eq!(report.diff_pairs.len(), 1);
        assert_eq!(report.diff_pairs[0].changes, 1);
        let gate = report.gate.expect("manifest should produce gate report");
        assert!(!gate.passed);
        assert_eq!(gate.missing_required_files, vec!["missing.pdf"]);
        assert_eq!(gate.failures.len(), 2);

        std::fs::remove_dir_all(&folder).expect("fixture folder should be removed");
    }

    fn minimal_pdf(text: &str) -> Vec<u8> {
        format!("%PDF-1.7\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n3 0 obj\n<< /Type /Page /Parent 2 0 R /Contents 4 0 R >>\nendobj\n4 0 obj\n<< /Length {} >>\nstream\nBT /F1 12 Tf 72 720 Td ({text}) Tj ET\nendstream\nendobj\n", text.len() + 32).into_bytes()
    }

    fn styled_text_pdf(font: &str, size: f32, text: &str) -> Vec<u8> {
        let content = format!(
            "BT /{font} {} Tf 72 720 Td ({text}) Tj ET",
            canonical_report_number(size)
        );
        format!(
            "%PDF-1.7
1 0 obj
<< /Type /Catalog /Pages 2 0 R >>
endobj
2 0 obj
<< /Type /Pages /Kids [3 0 R] /Count 1 >>
endobj
3 0 obj
<< /Type /Page /Parent 2 0 R /Contents 4 0 R >>
endobj
4 0 obj
<< /Length {} >>
stream
{content}
endstream
endobj
",
            content.len()
        )
        .into_bytes()
    }

    fn incremental_update_pdf() -> Vec<u8> {
        let mut pdf = minimal_pdf("Hello");
        pdf.extend_from_slice(
            b"xref\n0 1\n0000000000 65535 f \ntrailer\n<< /Size 1 /Prev 42 >>\nstartxref\n0\n%%EOF\nxref\n0 1\n0000000000 65535 f \ntrailer\n<< /Size 1 /Prev 0 >>\nstartxref\n128\n%%EOF\n",
        );
        pdf
    }

    fn multi_stream_pdf(second_text: &str) -> Vec<u8> {
        format!(
            "%PDF-1.7
1 0 obj
<< /Type /Catalog /Pages 2 0 R >>
endobj
2 0 obj
<< /Type /Pages /Kids [3 0 R] /Count 1 >>
endobj
3 0 obj
<< /Type /Page /Parent 2 0 R /Contents [4 0 R 5 0 R] >>
endobj
4 0 obj
<< /Length 33 >>
stream
BT /F1 12 Tf 72 720 Td (Hello) Tj
endstream
endobj
5 0 obj
<< /Length {} >>
stream
({second_text}) Tj ET
endstream
endobj
",
            second_text.len() + 9
        )
        .into_bytes()
    }

    fn multi_page_pdf() -> Vec<u8> {
        "%PDF-1.7
1 0 obj
<< /Type /Catalog /Pages 2 0 R >>
endobj
2 0 obj
<< /Type /Pages /Kids [3 0 R 5 0 R] /Count 2 >>
endobj
3 0 obj
<< /Type /Page /Parent 2 0 R /Contents 4 0 R >>
endobj
4 0 obj
<< /Length 43 >>
stream
BT /F1 12 Tf 72 720 Td (First page) Tj ET
endstream
endobj
5 0 obj
<< /Type /Page /Parent 2 0 R /Contents 6 0 R >>
endobj
6 0 obj
<< /Length 44 >>
stream
BT /F1 12 Tf 72 720 Td (Second page) Tj ET
endstream
endobj
"
        .as_bytes()
        .to_vec()
    }

    fn image_only_pdf() -> Vec<u8> {
        image_payload_pdf(b"x")
    }

    fn image_payload_pdf(payload: &[u8]) -> Vec<u8> {
        let payload_text = String::from_utf8_lossy(payload);
        "%PDF-1.7
1 0 obj
<< /Type /Catalog /Pages 2 0 R >>
endobj
2 0 obj
<< /Type /Pages /Kids [3 0 R] /Count 1 >>
endobj
3 0 obj
<< /Type /Page /Parent 2 0 R /Resources << /XObject << /Im1 5 0 R >> >> /Contents 4 0 R >>
endobj
4 0 obj
<< /Length 21 >>
stream
q 10 0 0 10 0 0 cm /Im1 Do Q
endstream
endobj
5 0 obj
<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceGray /BitsPerComponent 8 /Length 1 >>
stream
x
endstream
endobj
"
        .replace("stream\nx\nendstream", &format!("stream\n{payload_text}\nendstream"))
        .into_bytes()
    }

    fn annotation_pdf(annotation_body: &str) -> Vec<u8> {
        let content = "BT /F1 12 Tf 72 720 Td (Link) Tj ET";
        format!(
            "%PDF-1.7
1 0 obj
<< /Type /Catalog /Pages 2 0 R >>
endobj
2 0 obj
<< /Type /Pages /Kids [3 0 R] /Count 1 >>
endobj
3 0 obj
<< /Type /Page /Parent 2 0 R /Contents 4 0 R /Annots [5 0 R] >>
endobj
4 0 obj
<< /Length {} >>
stream
{content}
endstream
endobj
5 0 obj
<< {annotation_body} >>
endobj
",
            content.len()
        )
        .into_bytes()
    }

    fn vector_graphics_pdf(path_ops: &str) -> Vec<u8> {
        format!(
            "%PDF-1.7
1 0 obj
<< /Type /Catalog /Pages 2 0 R >>
endobj
2 0 obj
<< /Type /Pages /Kids [3 0 R] /Count 1 >>
endobj
3 0 obj
<< /Type /Page /Parent 2 0 R /Contents 4 0 R >>
endobj
4 0 obj
<< /Length 44 >>
stream
BT /F1 12 Tf 72 720 Td (Chart) Tj ET {path_ops}
endstream
endobj
"
        )
        .into_bytes()
    }

    fn cid_font_without_tounicode_pdf() -> Vec<u8> {
        "%PDF-1.7
1 0 obj
<< /Type /Catalog /Pages 2 0 R >>
endobj
2 0 obj
<< /Type /Pages /Kids [3 0 R] /Count 1 >>
endobj
3 0 obj
<< /Type /Page /Parent 2 0 R /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>
endobj
4 0 obj
<< /Length 38 >>
stream
BT /F1 12 Tf 72 720 Td (Hello) Tj ET
endstream
endobj
5 0 obj
<< /Type /Font /Subtype /Type0 /BaseFont /CIDFont /DescendantFonts [6 0 R] >>
endobj
6 0 obj
<< /Type /Font /Subtype /CIDFontType2 /BaseFont /CIDFont >>
endobj
"
        .as_bytes()
        .to_vec()
    }

    fn cid_font_with_tounicode_pdf() -> Vec<u8> {
        "%PDF-1.7
1 0 obj
<< /Type /Catalog /Pages 2 0 R >>
endobj
2 0 obj
<< /Type /Pages /Kids [3 0 R] /Count 1 >>
endobj
3 0 obj
<< /Type /Page /Parent 2 0 R /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>
endobj
4 0 obj
<< /Length 38 >>
stream
BT /F1 12 Tf 72 720 Td <0026> Tj ET
endstream
endobj
5 0 obj
<< /Type /Font /Subtype /Type0 /BaseFont /CIDFont /Encoding /Identity-H /DescendantFonts [6 0 R] /ToUnicode 7 0 R >>
endobj
6 0 obj
<< /Type /Font /Subtype /CIDFontType2 /BaseFont /CIDFont >>
endobj
7 0 obj
<< /Length 39 >>
stream
1 beginbfchar
<0026> <0043>
endbfchar
endstream
endobj
"
        .as_bytes()
        .to_vec()
    }

    fn tagged_pdf() -> Vec<u8> {
        "%PDF-1.7
1 0 obj
<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 6 0 R >>
endobj
2 0 obj
<< /Type /Pages /Kids [3 0 R] /Count 1 >>
endobj
3 0 obj
<< /Type /Page /Parent 2 0 R /StructParents 0 /Contents 4 0 R >>
endobj
4 0 obj
<< /Length 61 >>
stream
BT /P << /MCID 0 >> BDC /F1 12 Tf 72 720 Td (Tagged) Tj EMC ET
endstream
endobj
6 0 obj
<< /Type /StructTreeRoot /K [7 0 R] >>
endobj
7 0 obj
<< /Type /StructElem /S /P /K 0 /Pg 3 0 R >>
endobj
"
        .as_bytes()
        .to_vec()
    }
}
