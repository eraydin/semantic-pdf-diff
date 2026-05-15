use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT_TEST_DIR: AtomicUsize = AtomicUsize::new(0);

#[test]
fn diff_command_reports_text_changes_in_stdout_and_output_file() {
    let fixture = TestFixture::new("diff_command_reports_text_changes");
    let old_pdf = fixture.write_pdf("old.pdf", "Annual revenue was 10 million.");
    let new_pdf = fixture.write_pdf("new.pdf", "Annual revenue was 12 million.");

    let json_output = run_spdfdiff([
        "diff",
        path_arg(&old_pdf).as_str(),
        path_arg(&new_pdf).as_str(),
    ]);
    assert_success(&json_output);
    let json: Value =
        serde_json::from_slice(&json_output.stdout).expect("diff stdout should be valid JSON");

    assert_eq!(json["schema_version"], "0.1.0");
    assert_eq!(json["summary"]["modified"], 1);
    assert_eq!(json["changes"][0]["id"], "change-0000");
    assert_eq!(json["changes"][0]["kind"], "Modified");
    assert_eq!(
        json["changes"][0]["old_node"]["text"],
        "Annual revenue was 10 million."
    );
    assert_eq!(
        json["changes"][0]["new_node"]["text"],
        "Annual revenue was 12 million."
    );

    let markdown_path = fixture.path("diff.md");
    let markdown_output = run_spdfdiff([
        "diff",
        path_arg(&old_pdf).as_str(),
        path_arg(&new_pdf).as_str(),
        "--format",
        "md",
        "--output",
        path_arg(&markdown_path).as_str(),
    ]);
    assert_success(&markdown_output);
    assert!(
        markdown_output.stdout.is_empty(),
        "--output should not duplicate the report on stdout"
    );

    let markdown = fs::read_to_string(markdown_path).expect("Markdown report should be written");
    assert!(markdown.contains("# Semantic PDF Diff"));
    assert!(markdown.contains("| Modified | 1 |"));
    assert!(markdown.contains("`change-0000` Modified"));
}

#[test]
fn inspect_command_reports_object_graph_for_supported_formats() {
    let fixture = TestFixture::new("inspect_command_reports_object_graph");
    let pdf = fixture.write_pdf("inspect.pdf", "Inspection target");

    let json_output = run_spdfdiff(["inspect", path_arg(&pdf).as_str()]);
    assert_success(&json_output);
    let json: Value =
        serde_json::from_slice(&json_output.stdout).expect("inspect stdout should be valid JSON");

    assert!(json["object_count"].as_u64().unwrap_or_default() >= 4);
    assert_eq!(json["first_page_streams"], 1);

    let html_output = run_spdfdiff(["inspect", path_arg(&pdf).as_str(), "--format", "html"]);
    assert_success(&html_output);
    let html = String::from_utf8(html_output.stdout).expect("inspect HTML should be UTF-8");

    assert!(html.starts_with("<!doctype html>"));
    assert!(html.contains("# PDF Inspect"));
    assert!(html.contains("- First-page streams: 1"));
}

#[test]
fn extract_command_reports_positioned_text_and_writes_json() {
    let fixture = TestFixture::new("extract_command_reports_positioned_text");
    let pdf = fixture.write_pdf("extract.pdf", "First paragraph.");

    let markdown_output = run_spdfdiff(["extract", path_arg(&pdf).as_str(), "--format", "md"]);
    assert_success(&markdown_output);
    let markdown =
        String::from_utf8(markdown_output.stdout).expect("extract Markdown should be UTF-8");

    assert!(markdown.contains("# Extracted Text"));
    assert!(markdown.contains("- First paragraph."));

    let json_path = fixture.path("extract.json");
    let json_output = run_spdfdiff([
        "extract",
        path_arg(&pdf).as_str(),
        "--output",
        path_arg(&json_path).as_str(),
    ]);
    assert_success(&json_output);
    assert!(
        json_output.stdout.is_empty(),
        "--output should not duplicate the report on stdout"
    );

    let json: Value = serde_json::from_str(
        &fs::read_to_string(json_path).expect("extract JSON report should be written"),
    )
    .expect("extract output file should be valid JSON");
    assert_eq!(json["paragraphs"], 1);
    assert!(
        json["diagnostic_count"].as_u64().unwrap_or_default() >= 1,
        "literal-string fallback extraction should remain visibly diagnostic"
    );
}

#[test]
fn corpus_command_sorts_files_and_summarizes_partial_and_failed_inputs() {
    let fixture = TestFixture::new("corpus_command_sorts_files");
    let corpus = fixture.path("corpus");
    fs::create_dir_all(&corpus).expect("corpus directory should be created");
    fs::write(
        corpus.join("b.pdf"),
        minimal_pdf("Corpus parseable document."),
    )
    .expect("valid PDF should be written");
    fs::write(corpus.join("a.pdf"), b"this is not a pdf").expect("invalid PDF should be written");
    fs::write(corpus.join("ignored.txt"), b"not part of the corpus")
        .expect("ignored file should be written");

    let report_path = fixture.path("corpus_report.json");
    let output = run_spdfdiff([
        "corpus",
        path_arg(&corpus).as_str(),
        "--output",
        path_arg(&report_path).as_str(),
    ]);
    assert_success(&output);
    assert!(
        output.stdout.is_empty(),
        "corpus writes only to the requested output file"
    );

    let report: Value = serde_json::from_str(
        &fs::read_to_string(report_path).expect("corpus report should be written"),
    )
    .expect("corpus report should be valid JSON");

    assert_eq!(report["folder"], "corpus");
    assert_eq!(report["total"], 2);
    assert_eq!(report["parsed"], 1);
    assert_eq!(report["partial"], 1);
    assert_eq!(report["failed"], 1);
    assert_eq!(report["files"][0]["file"], "a.pdf");
    assert_eq!(report["files"][0]["status"], "failed");
    assert_eq!(report["files"][1]["file"], "b.pdf");
    assert_eq!(report["files"][1]["status"], "partial");
    assert_eq!(report["diagnostic_counts"]["MISSING_TOUNICODE"], 1);
}

#[test]
fn diff_command_completes_against_real_sample_pdfs() {
    let fixture = TestFixture::new("diff_command_real_samples");
    assert_real_sample_diff(
        &fixture,
        "document_v1.pdf",
        "document_v2.pdf",
        "document-diff.json",
        1,
        1,
        2,
    );
    assert_real_sample_diff(
        &fixture,
        "report_with_images_v1.pdf",
        "report_with_images_v2.pdf",
        "image-report-diff.json",
        2,
        2,
        4,
    );
}

fn assert_real_sample_diff(
    fixture: &TestFixture,
    old_name: &str,
    new_name: &str,
    output_name: &str,
    expected_inserted: u64,
    expected_deleted: u64,
    expected_changes: usize,
) {
    let old_pdf = real_sample_pdf(old_name);
    let new_pdf = real_sample_pdf(new_name);
    let output_path = fixture.path(output_name);

    let output = run_spdfdiff([
        "diff",
        path_arg(&old_pdf).as_str(),
        path_arg(&new_pdf).as_str(),
        "--format",
        "json",
        "--output",
        path_arg(&output_path).as_str(),
    ]);
    assert_success(&output);
    assert!(
        output.stdout.is_empty(),
        "--output should not duplicate the report on stdout"
    );

    let report = read_json(&output_path);
    assert_eq!(report["schema_version"], "0.1.0");
    assert_eq!(report["old_fingerprint"], path_arg(&old_pdf));
    assert_eq!(report["new_fingerprint"], path_arg(&new_pdf));
    assert!(report["summary"].is_object());
    assert!(report["changes"].is_array());
    assert_eq!(report["summary"]["inserted"], expected_inserted);
    assert_eq!(report["summary"]["deleted"], expected_deleted);
    assert_eq!(
        report["changes"]
            .as_array()
            .expect("changes should be an array")
            .len(),
        expected_changes
    );
    assert_diagnostic_code_present(&report, "MISSING_TOUNICODE");
    assert_diagnostic_code_absent(&report, "UNSUPPORTED_STREAM_FILTER");
    assert_diagnostic_code_absent(&report, "UNSUPPORTED_OBJECT_STREAM");
    assert_diagnostic_code_absent(&report, "MISSING_PAGE_CONTENT");
}

#[test]
fn inspect_command_completes_against_real_sample_pdf() {
    for sample in real_sample_pdf_names() {
        let pdf = real_sample_pdf(sample);

        let output = run_spdfdiff(["inspect", path_arg(&pdf).as_str(), "--format", "json"]);
        assert_success(&output);
        let report: Value =
            serde_json::from_slice(&output.stdout).expect("inspect stdout should be valid JSON");

        assert_eq!(report["file"], path_arg(&pdf));
        assert!(report["object_count"].as_u64().unwrap_or_default() >= 20);
        assert!(report["diagnostic_count"].as_u64().unwrap_or_default() <= 1);
        assert_eq!(report["first_page_streams"], 1);
    }
}

#[test]
fn extract_command_completes_against_real_sample_pdf_with_degraded_diagnostics() {
    for sample in real_sample_pdf_names() {
        let pdf = real_sample_pdf(sample);

        let output = run_spdfdiff(["extract", path_arg(&pdf).as_str(), "--format", "json"]);
        assert_success(&output);
        let report: Value =
            serde_json::from_slice(&output.stdout).expect("extract stdout should be valid JSON");

        assert_eq!(report["file"], path_arg(&pdf));
        assert!(report["paragraphs"].as_u64().unwrap_or_default() >= 1);
        assert!(report["diagnostic_count"].as_u64().unwrap_or_default() >= 1);
    }
}

#[test]
fn corpus_command_completes_against_real_sample_pdfs() {
    let fixture = TestFixture::new("corpus_command_real_samples");
    let corpus = fixture.path("real_corpus");
    fs::create_dir_all(&corpus).expect("real-sample corpus directory should be created");
    for sample in real_sample_pdf_names() {
        fs::copy(real_sample_pdf(sample), corpus.join(sample))
            .expect("real sample should be copied");
    }
    let output_path = fixture.path("real-corpus.json");

    let output = run_spdfdiff([
        "corpus",
        path_arg(&corpus).as_str(),
        "--output",
        path_arg(&output_path).as_str(),
    ]);
    assert_success(&output);
    assert!(
        output.stdout.is_empty(),
        "corpus writes only to the requested output file"
    );

    let report = read_json(&output_path);
    assert_eq!(report["folder"], "real_corpus");
    assert_eq!(report["total"], 4);
    assert_eq!(report["parsed"], 4);
    assert_eq!(report["partial"], 4);
    assert_eq!(report["failed"], 0);
    assert_eq!(report["files"][0]["file"], "document_v1.pdf");
    assert_eq!(report["files"][1]["file"], "document_v2.pdf");
    assert_eq!(report["files"][2]["file"], "report_with_images_v1.pdf");
    assert_eq!(report["files"][3]["file"], "report_with_images_v2.pdf");
    assert_eq!(report["diagnostic_counts"]["MISSING_TOUNICODE"], 4);
    assert!(
        report["diagnostic_counts"]["CONTENT_OPERATOR_UNKNOWN"]
            .as_u64()
            .unwrap_or_default()
            >= 1
    );
    assert!(report["diagnostic_counts"]["UNSUPPORTED_STREAM_FILTER"].is_null());
    assert!(report["diagnostic_counts"]["UNSUPPORTED_OBJECT_STREAM"].is_null());
    assert!(report["diagnostic_counts"]["MISSING_PAGE_CONTENT"].is_null());
}

fn run_spdfdiff<const N: usize>(args: [&str; N]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_spdfdiff"))
        .args(args)
        .output()
        .expect("spdfdiff process should start")
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "expected spdfdiff to exit successfully\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn path_arg(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn real_sample_pdf(name: &str) -> PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("samples")
        .join(name);
    assert!(
        path.is_file(),
        "expected real PDF sample at {}",
        path.display()
    );
    path
}

fn real_sample_pdf_names() -> [&'static str; 4] {
    [
        "document_v1.pdf",
        "document_v2.pdf",
        "report_with_images_v1.pdf",
        "report_with_images_v2.pdf",
    ]
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).expect("JSON report should be written"))
        .expect("report should be valid JSON")
}

fn assert_diagnostic_code_present(report: &Value, code: &str) {
    let diagnostics = report["diagnostics"]
        .as_array()
        .expect("diagnostics should be an array");
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic["code"] == code),
        "expected diagnostic code {code} in {diagnostics:?}"
    );
}

fn assert_diagnostic_code_absent(report: &Value, code: &str) {
    let diagnostics = report["diagnostics"]
        .as_array()
        .expect("diagnostics should be an array");
    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic["code"] != code),
        "did not expect diagnostic code {code} in {diagnostics:?}"
    );
}

struct TestFixture {
    root: PathBuf,
}

impl TestFixture {
    fn new(name: &str) -> Self {
        let index = NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir()
            .join("spdfdiff_cli_integration")
            .join(format!("{}-{}-{index}", std::process::id(), name));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("test fixture directory should be created");
        Self { root }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    fn write_pdf(&self, name: &str, text: &str) -> PathBuf {
        let path = self.path(name);
        fs::write(&path, minimal_pdf(text)).expect("PDF fixture should be written");
        path
    }
}

impl Drop for TestFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn minimal_pdf(text: &str) -> Vec<u8> {
    let content = format!("BT /F1 12 Tf 72 720 Td ({text}) Tj ET\n");
    format!(
        "%PDF-1.7\n\
         1 0 obj\n\
         << /Type /Catalog /Pages 2 0 R >>\n\
         endobj\n\
         2 0 obj\n\
         << /Type /Pages /Kids [3 0 R] /Count 1 >>\n\
         endobj\n\
         3 0 obj\n\
         << /Type /Page /Parent 2 0 R /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>\n\
         endobj\n\
         4 0 obj\n\
         << /Length {} >>\n\
         stream\n\
         {content}\
         endstream\n\
         endobj\n\
         5 0 obj\n\
         << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\n\
         endobj\n",
        content.len()
    )
    .into_bytes()
}
