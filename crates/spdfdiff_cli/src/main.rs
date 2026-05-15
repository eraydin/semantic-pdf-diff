use clap::{Parser, Subcommand, ValueEnum};
use diff_core::{DiffConfig, diff_semantic_documents};
use pdf_content::{ContentOp, ContentProgram};
use serde::Serialize;
use spdfdiff_types::{DiffDocument, ObjectId, ParseConfig, PdfDiffError};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum ReportFormat {
    Json,
    Md,
    Html,
}

fn main() {
    let cli = Cli::parse();

    if let Err(error) = run(cli) {
        eprintln!("{error}");
        std::process::exit(2);
    }
}

fn run(cli: Cli) -> Result<(), PdfDiffError> {
    match cli.command {
        Command::Diff {
            old_pdf,
            new_pdf,
            format,
            output,
        } => {
            let old_bytes = std::fs::read(&old_pdf)
                .map_err(|error| PdfDiffError::InvalidInput(error.to_string()))?;
            let new_bytes = std::fs::read(&new_pdf)
                .map_err(|error| PdfDiffError::InvalidInput(error.to_string()))?;
            let document = diff_pdf_bytes(
                &old_pdf.to_string_lossy(),
                &old_bytes,
                &new_pdf.to_string_lossy(),
                &new_bytes,
            )?;
            let rendered = render_diff(&document, format);
            write_or_print(rendered, output)?;
        }
        Command::Inspect {
            file,
            format,
            output,
        } => {
            let bytes = std::fs::read(&file)
                .map_err(|error| PdfDiffError::InvalidInput(error.to_string()))?;
            let parsed = pdf_core::PdfDocument::parse_with_config(&bytes, ParseConfig::default())?;
            let rendered = render_inspect_report(&file.to_string_lossy(), &parsed, format);
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
                &file.to_string_lossy(),
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
    }
    Ok(())
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
}

#[derive(Debug, Serialize)]
struct ExtractReport<'a> {
    file: &'a str,
    paragraphs: usize,
    diagnostic_count: usize,
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
    let old = semantic_document_from_pdf(old_fingerprint, old_bytes, config)?;
    let new = semantic_document_from_pdf(new_fingerprint, new_bytes, config)?;
    Ok(diff_semantic_documents(&old, &new, DiffConfig::default()))
}

fn semantic_document_from_pdf(
    fingerprint: &str,
    bytes: &[u8],
    config: ParseConfig,
) -> Result<pdf_semantic::SemanticDocument, PdfDiffError> {
    let document = pdf_core::PdfDocument::parse_with_config(bytes, config)?;
    let Some(contents) = document.first_page_contents() else {
        let mut semantic =
            pdf_semantic::build_semantic_document(fingerprint, &[], document.diagnostics);
        semantic
            .diagnostics
            .push(spdfdiff_types::Diagnostic::warning(
                "MISSING_PAGE_CONTENT",
                "no page content stream was available for extraction",
            ));
        return Ok(semantic);
    };
    let page_index = contents[0].page_index;
    let mut program = ContentProgram {
        operations: Vec::new(),
        diagnostics: Vec::new(),
    };
    for content in contents {
        let mut stream_program = pdf_content::parse_content_stream_with_limits(
            content.bytes,
            content.page_index,
            Some(content.stream_object_id),
            config.limits,
        );
        program.operations.append(&mut stream_program.operations);
        program.diagnostics.append(&mut stream_program.diagnostics);
    }
    let applied_tounicode = apply_tounicode_maps(&mut program, &document);
    let extraction = pdf_text::extract_text_runs(&program, page_index);
    let mut diagnostics = document.diagnostics;
    diagnostics.extend(
        extraction
            .diagnostics
            .into_iter()
            .filter(|diagnostic| !applied_tounicode || diagnostic.code != "MISSING_TOUNICODE"),
    );
    Ok(pdf_semantic::build_semantic_document(
        fingerprint,
        &extraction.runs,
        diagnostics,
    ))
}

fn apply_tounicode_maps(program: &mut ContentProgram, document: &pdf_core::PdfDocument) -> bool {
    let maps = font_tounicode_maps(document);
    if maps.is_empty() {
        return false;
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
                let Some(map) = maps.get(font_name) else {
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

    applied
}

fn font_tounicode_maps(
    document: &pdf_core::PdfDocument,
) -> BTreeMap<String, BTreeMap<Vec<u8>, String>> {
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
            let cmap = parse_tounicode_cmap(&cmap_stream.bytes);
            if !cmap.is_empty() {
                maps.insert(font_name, cmap);
            }
        }
    }
    maps
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

fn parse_tounicode_cmap(bytes: &[u8]) -> BTreeMap<Vec<u8>, String> {
    let text = String::from_utf8_lossy(bytes);
    let mut map = BTreeMap::new();
    for line in text.lines() {
        let hex_tokens = line
            .split_whitespace()
            .filter_map(hex_token_bytes)
            .collect::<Vec<_>>();
        if hex_tokens.len() == 2 {
            if let Some(decoded) = unicode_hex_to_string(&hex_tokens[1]) {
                map.insert(hex_tokens[0].clone(), decoded);
            }
        }
    }
    map
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

fn render_diff(document: &DiffDocument, format: ReportFormat) -> String {
    match format {
        ReportFormat::Json => diff_report::to_json(document)
            .unwrap_or_else(|error| format!("{{\"error\":\"{error}\"}}")),
        ReportFormat::Md => diff_report::to_markdown(document),
        ReportFormat::Html => {
            let markdown = diff_report::to_markdown(document);
            format!(
                "<!doctype html><meta charset=\"utf-8\"><pre>{}</pre>",
                escape_html(&markdown)
            )
        }
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
    let report = InspectReport {
        file: fingerprint,
        object_count,
        diagnostic_count,
        first_page_streams,
    };
    match format {
        ReportFormat::Json => {
            to_json_pretty(&report).unwrap_or_else(|error| format!("{{\"error\":\"{error}\"}}"))
        }
        ReportFormat::Md => format!(
            "# PDF Inspect\n\n- File: `{}`\n- Objects: {}\n- Diagnostics: {}\n- First-page streams: {}\n",
            fingerprint, object_count, diagnostic_count, first_page_streams
        ),
        ReportFormat::Html => format!(
            "<!doctype html><meta charset=\"utf-8\"><pre># PDF Inspect\n\n- File: `{}`\n- Objects: {}\n- Diagnostics: {}\n- First-page streams: {}\n</pre>",
            escape_html(fingerprint),
            object_count,
            diagnostic_count,
            first_page_streams
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
            };
            to_json_pretty(&report).unwrap_or_else(|error| format!("{{\"error\":\"{error}\"}}"))
        }
        ReportFormat::Md => {
            let mut out = format!("# Extracted Text\n\nFile: `{}`\n\n", document.fingerprint);
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
    fn inspect_report_includes_object_count() {
        let parsed = pdf_core::PdfDocument::parse(minimal_pdf("Hello").as_slice()).unwrap();
        let json = render_inspect_report("sample.pdf", &parsed, ReportFormat::Json);
        assert!(json.contains("\"object_count\""));
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
        let cmap =
            parse_tounicode_cmap(b"2 beginbfchar\n<0026> <0043>\n<004f> <006c>\nendbfchar\n");

        assert_eq!(
            decode_with_tounicode(&[0x00, 0x26, 0x00, 0x4f], &cmap).as_deref(),
            Some("Cl")
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
}
