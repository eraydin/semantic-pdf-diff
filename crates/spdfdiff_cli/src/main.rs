use clap::{Parser, Subcommand, ValueEnum};
use diff_core::{DiffConfig, diff_semantic_documents};
use pdf_content::{ContentOp, ContentProgram};
use serde::Serialize;
use spdfdiff_types::{
    ByteRange, ChangeKind, ChangeSeverity, Diagnostic, DiffDocument, FileRole, ObjectId,
    ParseConfig, PdfDiffError, Provenance, SemanticChange, SemanticNodeEvidence,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

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
        #[arg(long, value_enum, default_value_t = ReportFormat::Json)]
        format: ReportFormat,
        #[arg(long)]
        output: Option<PathBuf>,
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
        output: PathBuf,
    },
    Benchmark {
        #[arg(long, default_value_t = 50)]
        pages: usize,
        #[arg(long)]
        output: PathBuf,
    },
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
        Command::Corpus { folder, output } => {
            let report = build_corpus_report(&folder, ParseConfig::default())?;
            std::fs::write(&output, report)
                .map_err(|error| PdfDiffError::InvalidInput(error.to_string()))?;
        }
        Command::Benchmark { pages, output } => {
            let report = run_synthetic_benchmark(pages)?;
            let rendered = to_json_pretty(&report)?;
            std::fs::write(&output, rendered)
                .map_err(|error| PdfDiffError::InvalidInput(error.to_string()))?;
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
    files: Vec<CorpusFileReport>,
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
    tagged_structure: TaggedStructureReport,
}

#[derive(Debug, Serialize)]
struct ExtractReport<'a> {
    file: &'a str,
    paragraphs: usize,
    diagnostic_count: usize,
    tagged_structure: Option<TaggedStructureReport>,
}

#[derive(Debug, Clone, Serialize)]
struct TaggedStructureReport {
    detected: bool,
    root_object: Option<String>,
    element_count: usize,
    mcid_count: usize,
    structure_types: Vec<String>,
    diagnostics: Vec<String>,
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

fn build_corpus_report(folder: &Path, config: ParseConfig) -> Result<String, PdfDiffError> {
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

    to_json_pretty(&CorpusReport {
        folder: display_file_name(folder),
        total,
        parsed,
        partial,
        failed,
        diagnostic_counts,
        files,
    })
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

fn diff_pdf_bytes(
    old_fingerprint: &str,
    old_bytes: &[u8],
    new_fingerprint: &str,
    new_bytes: &[u8],
) -> Result<DiffDocument, PdfDiffError> {
    let config = ParseConfig::default();
    let old_document = pdf_core::PdfDocument::parse_with_config(old_bytes, config)?;
    let new_document = pdf_core::PdfDocument::parse_with_config(new_bytes, config)?;
    let old = semantic_document_from_document(old_fingerprint, &old_document, config);
    let new = semantic_document_from_document(new_fingerprint, &new_document, config);
    let mut diff = diff_semantic_documents(&old, &new, DiffConfig::default());
    append_image_payload_changes(&mut diff, &old_document, &new_document);
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
    let semantic = pdf_semantic::build_semantic_document(
        fingerprint,
        &extraction.runs,
        extraction.diagnostics,
    );
    if tagged_structure.root_object_id.is_some() {
        semantic.with_tagged_structure(semantic_tagged_structure_summary(&tagged_structure))
    } else {
        semantic
    }
}

struct ExtractedTextRuns {
    runs: Vec<pdf_text::TextRun>,
    diagnostics: Vec<Diagnostic>,
}

fn extract_text_runs_from_document(
    document: &pdf_core::PdfDocument,
    config: ParseConfig,
) -> ExtractedTextRuns {
    let contents = document.page_contents();
    if contents.is_empty() {
        let mut diagnostics = document.diagnostics.clone();
        diagnostics.push(spdfdiff_types::Diagnostic::warning(
            "MISSING_PAGE_CONTENT",
            "no page content stream was available for extraction",
        ));
        append_unsupported_feature_diagnostics(document, true, false, &mut diagnostics);
        return ExtractedTextRuns {
            runs: Vec::new(),
            diagnostics,
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
    let mut has_vector_graphics = false;
    for (page_index, mut program) in programs {
        has_vector_graphics |= program_has_vector_graphics(&program);
        let tounicode_result = apply_tounicode_maps(&mut program, document);
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
    append_unsupported_feature_diagnostics(
        document,
        runs.is_empty(),
        has_vector_graphics,
        &mut diagnostics,
    );
    ExtractedTextRuns { runs, diagnostics }
}

fn append_unsupported_feature_diagnostics(
    document: &pdf_core::PdfDocument,
    has_no_text_runs: bool,
    has_vector_graphics: bool,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if has_vector_graphics {
        diagnostics.push(Diagnostic::warning(
            "UNSUPPORTED_VECTOR_GRAPHIC_DIFF",
            "native vector path comparison is not implemented",
        ));
    }

    if document_has_token(document, "/Subtype /Image") && has_no_text_runs {
        diagnostics.push(Diagnostic::warning(
            "MISSING_TEXT_LAYER",
            "no extractable text layer was found; OCR is not implemented",
        ));
    }

    if document_has_any_token(document, &["/Annots", "/Subtype /Link", "/Annot"]) {
        diagnostics.push(Diagnostic::warning(
            "UNSUPPORTED_ANNOTATION_DIFF",
            "annotation and link target comparison is not implemented",
        ));
    }

    if document_has_any_token(document, &["/AcroForm", "/Widget"]) {
        diagnostics.push(Diagnostic::warning(
            "UNSUPPORTED_FORM_FIELD_DIFF",
            "interactive form field comparison is not implemented",
        ));
    }

    append_font_diagnostics(document, diagnostics);
    append_tagged_pdf_diagnostics(document, diagnostics);
}

fn append_font_diagnostics(document: &pdf_core::PdfDocument, diagnostics: &mut Vec<Diagnostic>) {
    let cid_missing_count = document
        .objects
        .iter()
        .filter(|object| {
            (document_has_object_token(object, "/Subtype /Type0")
                || document_has_object_token(object, "/CIDFontType"))
                && !document_has_object_token(object, "/ToUnicode")
        })
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
        structure_types,
        confidence: if structure.diagnostics.is_empty() {
            0.8
        } else {
            0.5
        },
    }
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
        structure_types: summary.structure_types.clone(),
        diagnostics: Vec::new(),
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct DocumentSurface {
    category: SurfaceCategory,
    index: usize,
    object_id: ObjectId,
    summary: String,
    hash: String,
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
            let bytes = object
                .stream
                .as_ref()
                .map_or_else(|| object.body.as_bytes(), |stream| stream.bytes.as_slice());
            let summary = summarize_surface(&object.body, category);
            DocumentSurface {
                category,
                index,
                object_id: object.id,
                summary,
                hash: stable_hash(bytes),
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
    let mut parts = vec![category.report_label().to_owned()];
    for key in ["Subtype", "URI", "Title", "F", "Desc", "Contents", "T", "V"] {
        if let Some(value) = value_after_pdf_name(body, key) {
            parts.push(format!("{key}={value}"));
        }
    }
    parts.join(" ")
}

fn value_after_pdf_name(body: &str, key: &str) -> Option<String> {
    let start = body.find(&format!("/{key}"))? + key.len() + 1;
    let remaining = body[start..].trim_start();
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
        confidence: 0.8,
        reason,
    };
    document.changes.push(change);
}

fn surface_evidence(file_role: FileRole, surface: &DocumentSurface) -> SemanticNodeEvidence {
    SemanticNodeEvidence {
        node_id: format!("{}-{:04}", surface.category.node_prefix(), surface.index),
        page: 0,
        bbox: None,
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

fn stable_hash(bytes: &[u8]) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
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

fn program_has_vector_graphics(program: &ContentProgram) -> bool {
    program.operations.iter().any(|operation| {
        matches!(
            operation,
            ContentOp::RecognizedNonText { operator, .. }
                if is_vector_graphics_operator(operator)
        )
    })
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

struct ToUnicodeApplyResult {
    applied: bool,
    diagnostics: Vec<Diagnostic>,
}

fn apply_tounicode_maps(
    program: &mut ContentProgram,
    document: &pdf_core::PdfDocument,
) -> ToUnicodeApplyResult {
    let maps = font_tounicode_maps(document);
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

fn font_tounicode_maps(document: &pdf_core::PdfDocument) -> ToUnicodeMaps {
    let objects_by_id = document
        .objects
        .iter()
        .map(|object| (object.id, object))
        .collect::<BTreeMap<_, _>>();
    let mut font_to_cmap = BTreeMap::new();
    for object in &document.objects {
        if let Some(cmap_id) = reference_after_key(&object.body, "ToUnicode") {
            font_to_cmap.insert(object.id, cmap_id);
        }
    }

    let mut maps = BTreeMap::new();
    let mut diagnostics = Vec::new();
    for object in &document.objects {
        for (font_name, font_object_id) in named_references(&object.body) {
            let Some(cmap_object_id) = font_to_cmap.get(&font_object_id) else {
                continue;
            };
            let Some(cmap_stream) = objects_by_id
                .get(cmap_object_id)
                .and_then(|object| object.stream.as_ref())
            else {
                continue;
            };
            let cmap = parse_tounicode_cmap_with_diagnostics(&cmap_stream.bytes);
            diagnostics.extend(cmap.diagnostics);
            if !cmap.map.is_empty() {
                maps.insert(font_name, cmap.map);
            }
        }
    }
    ToUnicodeMaps { maps, diagnostics }
}

fn reference_after_key(body: &str, key: &str) -> Option<ObjectId> {
    let start = body.find(&format!("/{key}"))? + key.len() + 1;
    parse_reference_at(&body[start..])
}

fn named_references(body: &str) -> Vec<(String, ObjectId)> {
    let tokens = body_tokens(body);
    let mut references = Vec::new();
    for index in 0..tokens.len().saturating_sub(3) {
        let Some(name) = tokens[index].strip_prefix('/') else {
            continue;
        };
        let Ok(number) = tokens[index + 1].parse::<u32>() else {
            continue;
        };
        let Ok(generation) = tokens[index + 2].parse::<u16>() else {
            continue;
        };
        if tokens[index + 3] == "R" {
            references.push((name.to_owned(), ObjectId { number, generation }));
        }
    }
    references
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
    let _rendered = render_diff(&diff, ReportFormat::Json);
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

fn render_diff(document: &DiffDocument, format: ReportFormat) -> String {
    match format {
        ReportFormat::Json => diff_report::to_json(document)
            .unwrap_or_else(|error| format!("{{\"error\":\"{error}\"}}")),
        ReportFormat::Md => diff_report::to_markdown(document),
        ReportFormat::Html => diff_report::to_html(document),
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
    let report = InspectReport {
        file: fingerprint,
        object_count,
        diagnostic_count,
        first_page_streams,
        tagged_structure: tagged_structure.clone(),
    };
    match format {
        ReportFormat::Json => {
            to_json_pretty(&report).unwrap_or_else(|error| format!("{{\"error\":\"{error}\"}}"))
        }
        ReportFormat::Md => format!(
            "# PDF Inspect\n\n- File: `{}`\n- Objects: {}\n- Diagnostics: {}\n- First-page streams: {}\n- Tagged structure: {} elements, {} MCIDs\n",
            fingerprint,
            object_count,
            diagnostic_count,
            first_page_streams,
            tagged_structure.element_count,
            tagged_structure.mcid_count
        ),
        ReportFormat::Html => format!(
            "<!doctype html><meta charset=\"utf-8\"><pre># PDF Inspect\n\n- File: `{}`\n- Objects: {}\n- Diagnostics: {}\n- First-page streams: {}\n- Tagged structure: {} elements, {} MCIDs\n</pre>",
            escape_html(fingerprint),
            object_count,
            diagnostic_count,
            first_page_streams,
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
            let report = ExtractReport {
                file: &document.fingerprint,
                paragraphs: document.nodes.len(),
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
        let diff = diff_pdf_bytes("old", &old_pdf, "new", &new_pdf)
            .expect("minimal vertical slice should diff");

        assert_eq!(diff.summary.modified, 1);
    }

    #[test]
    fn diffs_text_across_multiple_content_streams() {
        let old_pdf = multi_stream_pdf("world");
        let new_pdf = multi_stream_pdf("there");
        let diff = diff_pdf_bytes("old", &old_pdf, "new", &new_pdf)
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
    fn reports_vector_graphics_as_unsupported_diff_surface() {
        let semantic =
            semantic_document_from_pdf("vector", &vector_graphics_pdf(), ParseConfig::default())
                .expect("vector extraction should complete with diagnostics");

        assert!(
            semantic
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "UNSUPPORTED_VECTOR_GRAPHIC_DIFF")
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
    fn finds_font_resource_references_for_tounicode_maps() {
        let refs = named_references("<</LQYSYM 18 0 R/KFDXKX 22 0 R>>");

        assert_eq!(
            refs,
            vec![
                (
                    "LQYSYM".into(),
                    ObjectId {
                        number: 18,
                        generation: 0
                    }
                ),
                (
                    "KFDXKX".into(),
                    ObjectId {
                        number: 22,
                        generation: 0
                    }
                )
            ]
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

    fn minimal_pdf(text: &str) -> Vec<u8> {
        format!("%PDF-1.7\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n3 0 obj\n<< /Type /Page /Parent 2 0 R /Contents 4 0 R >>\nendobj\n4 0 obj\n<< /Length {} >>\nstream\nBT /F1 12 Tf 72 720 Td ({text}) Tj ET\nendstream\nendobj\n", text.len() + 32).into_bytes()
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

    fn vector_graphics_pdf() -> Vec<u8> {
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
BT /F1 12 Tf 72 720 Td (Chart) Tj ET 0 0 m 10 10 l S
endstream
endobj
"
        .as_bytes()
        .to_vec()
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
