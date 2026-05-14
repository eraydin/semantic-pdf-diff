use clap::{Parser, Subcommand, ValueEnum};
use diff_core::{DiffConfig, diff_semantic_documents};
use spdfdiff_types::{DiffDocument, ParseConfig, PdfDiffError};
use std::path::PathBuf;

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

fn build_corpus_report(
    folder: &std::path::Path,
    config: ParseConfig,
) -> Result<String, PdfDiffError> {
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
    for path in paths {
        let Ok(bytes) = std::fs::read(&path) else {
            failed += 1;
            continue;
        };
        match pdf_core::PdfDocument::parse_with_config(&bytes, config) {
            Ok(document) => {
                if document.diagnostics.is_empty() {
                    parsed += 1;
                } else {
                    parsed += 1;
                    partial += 1;
                }
            }
            Err(_) => failed += 1,
        }
    }

    Ok(format!(
        "{{\n  \"folder\": \"{}\",\n  \"total\": {},\n  \"parsed\": {},\n  \"partial\": {},\n  \"failed\": {}\n}}\n",
        folder.display(),
        total,
        parsed,
        partial,
        failed
    ))
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
    let mut program = pdf_content::ContentProgram {
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
    let extraction = pdf_text::extract_text_runs(&program, page_index);
    let mut diagnostics = document.diagnostics;
    diagnostics.extend(extraction.diagnostics);
    Ok(pdf_semantic::build_semantic_document(
        fingerprint,
        &extraction.runs,
        diagnostics,
    ))
}

fn render_diff(document: &DiffDocument, format: ReportFormat) -> String {
    match format {
        ReportFormat::Json => diff_report::to_json(document)
            .unwrap_or_else(|error| format!("{{\"error\":\"{error}\"}}")),
        ReportFormat::Md => diff_report::to_markdown(document),
        ReportFormat::Html => {
            let markdown = diff_report::to_markdown(document);
            format!("<!doctype html><meta charset=\"utf-8\"><pre>{markdown}</pre>")
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
    match format {
        ReportFormat::Json => format!(
            "{{\n  \"file\": \"{}\",\n  \"object_count\": {},\n  \"diagnostic_count\": {}\n}}\n",
            fingerprint, object_count, diagnostic_count
        ),
        ReportFormat::Md => format!(
            "# PDF Inspect\n\n- File: `{}`\n- Objects: {}\n- Diagnostics: {}\n- First-page streams: {}\n",
            fingerprint, object_count, diagnostic_count, first_page_streams
        ),
        ReportFormat::Html => format!(
            "<!doctype html><meta charset=\"utf-8\"><pre># PDF Inspect\n\n- File: `{}`\n- Objects: {}\n- Diagnostics: {}\n- First-page streams: {}\n</pre>",
            fingerprint, object_count, diagnostic_count, first_page_streams
        ),
    }
}

fn render_extract_report(
    document: &pdf_semantic::SemanticDocument,
    format: ReportFormat,
) -> String {
    match format {
        ReportFormat::Json => {
            let paragraphs = document.nodes.len();
            let diagnostics = document.diagnostics.len();
            format!(
                "{{\n  \"file\": \"{}\",\n  \"paragraphs\": {},\n  \"diagnostic_count\": {}\n}}\n",
                document.fingerprint, paragraphs, diagnostics
            )
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
            format!("<!doctype html><meta charset=\"utf-8\"><pre>{markdown}</pre>")
        }
    }
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
    fn inspect_report_includes_object_count() {
        let parsed = pdf_core::PdfDocument::parse(minimal_pdf("Hello").as_slice()).unwrap();
        let json = render_inspect_report("sample.pdf", &parsed, ReportFormat::Json);
        assert!(json.contains("\"object_count\""));
    }

    #[test]
    fn extract_report_lists_text() {
        let semantic =
            semantic_document_from_pdf("sample", &minimal_pdf("Hello"), ParseConfig::default())
                .expect("extract should succeed");
        let markdown = render_extract_report(&semantic, ReportFormat::Md);
        assert!(markdown.contains("- Hello"));
    }

    fn minimal_pdf(text: &str) -> Vec<u8> {
        format!("%PDF-1.7\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n3 0 obj\n<< /Type /Page /Parent 2 0 R /Contents 4 0 R >>\nendobj\n4 0 obj\n<< /Length {} >>\nstream\nBT /F1 12 Tf 72 720 Td ({text}) Tj ET\nendstream\nendobj\n", text.len() + 32).into_bytes()
    }
}
