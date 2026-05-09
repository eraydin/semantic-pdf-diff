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
    },
    Extract {
        file: PathBuf,
        #[arg(long, value_enum, default_value_t = ReportFormat::Json)]
        format: ReportFormat,
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
            let rendered = render_placeholder(&document, format);
            if let Some(output) = output {
                std::fs::write(&output, rendered)
                    .map_err(|error| PdfDiffError::InvalidInput(error.to_string()))?;
            } else {
                println!("{rendered}");
            }
        }
        Command::Inspect { file, format } | Command::Extract { file, format } => {
            let document = DiffDocument::empty(file.to_string_lossy().into_owned(), "");
            let rendered = render_placeholder(&document, format);
            println!("{rendered}");
        }
        Command::Corpus { folder, output } => {
            let report = format!(
                "{{\n  \"folder\": \"{}\",\n  \"total\": 0,\n  \"parsed\": 0,\n  \"partial\": 0,\n  \"failed\": 0\n}}\n",
                folder.display()
            );
            if let Err(error) = std::fs::write(&output, report) {
                eprintln!("failed to write {}: {error}", output.display());
                std::process::exit(2);
            }
        }
    }
    Ok(())
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
    let Some(content) = document.first_page_content() else {
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
    let program = pdf_content::parse_content_stream_with_limits(
        content.bytes,
        content.page_index,
        Some(content.stream_object_id),
        config.limits,
    );
    let extraction = pdf_text::extract_text_runs(&program, content.page_index);
    let mut diagnostics = document.diagnostics;
    diagnostics.extend(extraction.diagnostics);
    Ok(pdf_semantic::build_semantic_document(
        fingerprint,
        &extraction.runs,
        diagnostics,
    ))
}

fn render_placeholder(document: &DiffDocument, format: ReportFormat) -> String {
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
        assert_eq!(
            diff.changes[0].old_node.as_ref().unwrap().text.as_deref(),
            Some("Hello")
        );
        assert_eq!(
            diff.changes[0].new_node.as_ref().unwrap().text.as_deref(),
            Some("Hello world")
        );
        assert!(
            diff.diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "MISSING_TOUNICODE")
        );
    }

    fn minimal_pdf(text: &str) -> Vec<u8> {
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
BT /F1 12 Tf 72 720 Td ({text}) Tj ET
endstream
endobj
",
            text.len() + 32
        )
        .into_bytes()
    }
}
