use clap::{Parser, Subcommand, ValueEnum};
use spdfdiff_types::DiffDocument;
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

    match cli.command {
        Command::Diff {
            old_pdf,
            new_pdf,
            format,
            output,
        } => {
            let document = DiffDocument::empty(
                old_pdf.to_string_lossy().into_owned(),
                new_pdf.to_string_lossy().into_owned(),
            );
            let rendered = render_placeholder(&document, format);
            if let Some(output) = output {
                if let Err(error) = std::fs::write(&output, rendered) {
                    eprintln!("failed to write {}: {error}", output.display());
                    std::process::exit(2);
                }
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
