use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT_TEST_DIR: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone, Copy)]
struct ExpectedDiffSummary {
    inserted: u64,
    deleted: u64,
    changes: usize,
}

#[derive(Debug, Clone, Copy)]
struct RealSamplePair {
    slug: &'static str,
    old_name: &'static str,
    new_name: &'static str,
    expected: Option<ExpectedDiffSummary>,
}

const REAL_SAMPLE_PDFS: &[&str] = &[
    "annotations_base_v1.pdf",
    "annotations_visual_markup_v2.pdf",
    "complex_semantic_diff_v1.pdf",
    "complex_semantic_diff_v2.pdf",
    "document_outline_v1.pdf",
    "document_outline_v2.pdf",
    "document_v1.pdf",
    "document_v2.pdf",
    "headers_footers_v1.pdf",
    "headers_footers_v2.pdf",
    "inline_formatting_v1.pdf",
    "inline_formatting_v2.pdf",
    "interactive_forms_v1.pdf",
    "interactive_forms_v2.pdf",
    "interactive_links_v1.pdf",
    "interactive_links_v2.pdf",
    "multicolumn_layout_v1.pdf",
    "multicolumn_layout_v2.pdf",
    "multipage_table_v1.pdf",
    "multipage_table_v2.pdf",
    "report_with_images_v1.pdf",
    "report_with_images_v2.pdf",
    "scanned_document_v1.pdf",
    "scanned_document_v2.pdf",
    "semantic_contract_v1.pdf",
    "semantic_contract_v2.pdf",
    "semantic_images_v1.pdf",
    "semantic_images_v2.pdf",
    "ultimate_semantic_diff_v1.pdf",
    "ultimate_semantic_diff_v2.pdf",
    "vector_paths_graphic_v1.pdf",
    "vector_paths_graphic_v2.pdf",
    "watermark_overlay_v1.pdf",
    "watermark_overlay_v2.pdf",
];

const REAL_SAMPLE_PAIRS: &[RealSamplePair] = &[
    RealSamplePair {
        slug: "document",
        old_name: "document_v1.pdf",
        new_name: "document_v2.pdf",
        expected: Some(ExpectedDiffSummary {
            inserted: 0,
            deleted: 0,
            changes: 1,
        }),
    },
    RealSamplePair {
        slug: "report-with-images",
        old_name: "report_with_images_v1.pdf",
        new_name: "report_with_images_v2.pdf",
        expected: Some(ExpectedDiffSummary {
            inserted: 2,
            deleted: 2,
            changes: 4,
        }),
    },
    RealSamplePair {
        slug: "semantic-contract",
        old_name: "semantic_contract_v1.pdf",
        new_name: "semantic_contract_v2.pdf",
        expected: Some(ExpectedDiffSummary {
            inserted: 1,
            deleted: 1,
            changes: 2,
        }),
    },
    RealSamplePair {
        slug: "semantic-images",
        old_name: "semantic_images_v1.pdf",
        new_name: "semantic_images_v2.pdf",
        expected: Some(ExpectedDiffSummary {
            inserted: 1,
            deleted: 2,
            changes: 5,
        }),
    },
    RealSamplePair {
        slug: "complex-semantic-diff",
        old_name: "complex_semantic_diff_v1.pdf",
        new_name: "complex_semantic_diff_v2.pdf",
        expected: None,
    },
    RealSamplePair {
        slug: "ultimate-semantic-diff",
        old_name: "ultimate_semantic_diff_v1.pdf",
        new_name: "ultimate_semantic_diff_v2.pdf",
        expected: None,
    },
    RealSamplePair {
        slug: "interactive-links",
        old_name: "interactive_links_v1.pdf",
        new_name: "interactive_links_v2.pdf",
        expected: None,
    },
    RealSamplePair {
        slug: "multicolumn-layout",
        old_name: "multicolumn_layout_v1.pdf",
        new_name: "multicolumn_layout_v2.pdf",
        expected: None,
    },
    RealSamplePair {
        slug: "headers-footers",
        old_name: "headers_footers_v1.pdf",
        new_name: "headers_footers_v2.pdf",
        expected: None,
    },
    RealSamplePair {
        slug: "inline-formatting",
        old_name: "inline_formatting_v1.pdf",
        new_name: "inline_formatting_v2.pdf",
        expected: None,
    },
    RealSamplePair {
        slug: "watermark-overlay",
        old_name: "watermark_overlay_v1.pdf",
        new_name: "watermark_overlay_v2.pdf",
        expected: None,
    },
    RealSamplePair {
        slug: "multipage-table",
        old_name: "multipage_table_v1.pdf",
        new_name: "multipage_table_v2.pdf",
        expected: None,
    },
    RealSamplePair {
        slug: "interactive-forms",
        old_name: "interactive_forms_v1.pdf",
        new_name: "interactive_forms_v2.pdf",
        expected: None,
    },
    RealSamplePair {
        slug: "document-outline",
        old_name: "document_outline_v1.pdf",
        new_name: "document_outline_v2.pdf",
        expected: None,
    },
    RealSamplePair {
        slug: "annotations",
        old_name: "annotations_base_v1.pdf",
        new_name: "annotations_visual_markup_v2.pdf",
        expected: None,
    },
    RealSamplePair {
        slug: "vector-paths-graphic",
        old_name: "vector_paths_graphic_v1.pdf",
        new_name: "vector_paths_graphic_v2.pdf",
        expected: None,
    },
    RealSamplePair {
        slug: "scanned-document",
        old_name: "scanned_document_v1.pdf",
        new_name: "scanned_document_v2.pdf",
        expected: None,
    },
];

const SAMPLE_SCENARIO_MARKDOWN: &[&str] = &[
    "semantic_diff_test_cases.md",
    "semantic_diff_detailed_test_cases.md",
    "semantic_diff_detailed_test_cases_v3.md",
    "semantic_diff_detailed_test_cases_v4.md",
    "semantic_diff_detailed_test_cases_v5.md",
];

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
    for pair in REAL_SAMPLE_PAIRS {
        assert_real_sample_diff(&fixture, *pair);
    }
}

fn assert_real_sample_diff(fixture: &TestFixture, pair: RealSamplePair) {
    let old_pdf = real_sample_pdf(pair.old_name);
    let new_pdf = real_sample_pdf(pair.new_name);
    let output_path = fixture.path(&format!("{}-diff.json", pair.slug));

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
    if let Some(expected) = pair.expected {
        assert_eq!(report["summary"]["inserted"], expected.inserted);
        assert_eq!(report["summary"]["deleted"], expected.deleted);
        assert!(
            report["summary"]["modified"].as_u64().unwrap_or_default()
                + report["summary"]["layout_changed"]
                    .as_u64()
                    .unwrap_or_default()
                <= expected.changes as u64
        );
        assert_eq!(
            report["changes"]
                .as_array()
                .expect("changes should be an array")
                .len(),
            expected.changes
        );
    }
    assert_diagnostic_code_absent(&report, "MISSING_TOUNICODE");
    assert_diagnostic_code_absent(&report, "UNSUPPORTED_STREAM_FILTER");
    assert_diagnostic_code_absent(&report, "UNSUPPORTED_OBJECT_STREAM");
    assert_diagnostic_code_absent(&report, "MISSING_PAGE_CONTENT");
    assert!(
        !fs::read_to_string(output_path)
            .expect("diff JSON should be readable")
            .contains("\\u0000")
    );
}

#[test]
fn inspect_command_completes_against_real_sample_pdf() {
    for sample in real_sample_pdf_names().iter().copied() {
        let pdf = real_sample_pdf(sample);

        let output = run_spdfdiff(["inspect", path_arg(&pdf).as_str(), "--format", "json"]);
        assert_success(&output);
        let report: Value =
            serde_json::from_slice(&output.stdout).expect("inspect stdout should be valid JSON");

        assert_eq!(report["file"], path_arg(&pdf));
        assert!(
            report["object_count"].as_u64().unwrap_or_default() >= 1,
            "inspect should parse a non-empty object graph for {sample}"
        );
        assert!(report["diagnostic_count"].as_u64().unwrap_or_default() <= 2);
        assert!(report["first_page_streams"].as_u64().unwrap_or_default() >= 1);
    }
}

#[test]
fn extract_command_completes_against_real_sample_pdf_with_readable_content() {
    for sample in real_sample_pdf_names().iter().copied() {
        let pdf = real_sample_pdf(sample);

        let output = run_spdfdiff(["extract", path_arg(&pdf).as_str(), "--format", "json"]);
        assert_success(&output);
        let report: Value =
            serde_json::from_slice(&output.stdout).expect("extract stdout should be valid JSON");

        assert_eq!(report["file"], path_arg(&pdf));
        let paragraphs = report["paragraphs"].as_u64().unwrap_or_default();
        if sample.starts_with("scanned_document_") {
            assert_eq!(
                paragraphs, 0,
                "image-only scanned samples should not invent text"
            );
        } else {
            assert!(paragraphs >= 1, "expected extractable text in {sample}");
        }
        assert!(report["diagnostic_count"].as_u64().unwrap_or_default() <= 2);
    }
}

#[test]
fn html_outputs_complete_against_real_sample_pdfs() {
    let fixture = TestFixture::new("real_sample_html_outputs");

    for pair in real_sample_pdf_pairs().iter().copied() {
        let output_path = fixture.path(&format!("{}-diff.html", pair.slug));
        assert_success(&run_spdfdiff([
            "diff",
            path_arg(&real_sample_pdf(pair.old_name)).as_str(),
            path_arg(&real_sample_pdf(pair.new_name)).as_str(),
            "--format",
            "html",
            "--output",
            path_arg(&output_path).as_str(),
        ]));
        let html = fs::read_to_string(output_path).expect("diff HTML should be written");
        assert_self_contained_html(&html);
        assert!(html.contains("# Semantic PDF Diff"));
        if pair.slug == "semantic-contract" {
            assert_readable_output_contains_all(
                &html,
                &["TechCorp LLC", "$6,000.00", "Annual Maintenance"],
            );
        }
        if pair.slug == "semantic-images" {
            assert_readable_output_contains_all(&html, &["upgraded, reinforced", "24V"]);
        }
    }

    for sample in real_sample_pdf_names().iter().copied() {
        let pdf = real_sample_pdf(sample);
        let inspect_path = fixture.path(&format!("inspect-{sample}.html"));
        assert_success(&run_spdfdiff([
            "inspect",
            path_arg(&pdf).as_str(),
            "--format",
            "html",
            "--output",
            path_arg(&inspect_path).as_str(),
        ]));
        let inspect_html =
            fs::read_to_string(inspect_path).expect("inspect HTML should be written");
        assert_self_contained_html(&inspect_html);
        assert!(inspect_html.contains("# PDF Inspect"));

        let extract_path = fixture.path(&format!("extract-{sample}.html"));
        assert_success(&run_spdfdiff([
            "extract",
            path_arg(&pdf).as_str(),
            "--format",
            "html",
            "--output",
            path_arg(&extract_path).as_str(),
        ]));
        let extract_html =
            fs::read_to_string(extract_path).expect("extract HTML should be written");
        assert_self_contained_html(&extract_html);
        assert!(extract_html.contains("# Extracted Text"));
        if sample == "semantic_contract_v2.pdf" {
            assert_readable_output_contains_all(
                &extract_html,
                &["TechCorp LLC", "Annual Maintenance", "50% of the total"],
            );
        }
        if sample == "semantic_images_v2.pdf" {
            assert_readable_output_contains_all(
                &extract_html,
                &[
                    "Product Specification: The Widget",
                    "upgraded, reinforced",
                    "24V",
                ],
            );
        }
    }
}

#[test]
fn generated_output_files_include_expected_semantic_sample_content() {
    let fixture = TestFixture::new("semantic_sample_output_content");
    let contract_v2 = real_sample_pdf("semantic_contract_v2.pdf");
    let images_v2 = real_sample_pdf("semantic_images_v2.pdf");
    let contract_extract = fixture.path("contract-v2.md");
    let images_extract = fixture.path("images-v2.md");
    let contract_diff = fixture.path("contract-diff.json");
    let images_diff = fixture.path("images-diff.json");

    assert_success(&run_spdfdiff([
        "extract",
        path_arg(&contract_v2).as_str(),
        "--format",
        "md",
        "--output",
        path_arg(&contract_extract).as_str(),
    ]));
    assert_success(&run_spdfdiff([
        "extract",
        path_arg(&images_v2).as_str(),
        "--format",
        "md",
        "--output",
        path_arg(&images_extract).as_str(),
    ]));
    assert_success(&run_spdfdiff([
        "diff",
        path_arg(&real_sample_pdf("semantic_contract_v1.pdf")).as_str(),
        path_arg(&contract_v2).as_str(),
        "--format",
        "json",
        "--output",
        path_arg(&contract_diff).as_str(),
    ]));
    assert_success(&run_spdfdiff([
        "diff",
        path_arg(&real_sample_pdf("semantic_images_v1.pdf")).as_str(),
        path_arg(&images_v2).as_str(),
        "--format",
        "json",
        "--output",
        path_arg(&images_diff).as_str(),
    ]));

    let contract_text =
        fs::read_to_string(contract_extract).expect("contract extract should be written");
    assert_readable_output_contains_all(
        &contract_text,
        &[
            "TechCorp LLC",
            "30 days written notice",
            "15 days of invoice receipt",
            "Annual Maintenance",
            "50% of the total",
        ],
    );

    let images_text = fs::read_to_string(images_extract).expect("image extract should be written");
    assert_readable_output_contains_all(
        &images_text,
        &[
            "Product Specification: The Widget",
            "upgraded, reinforced",
            "60Hz",
            "24V",
            "Internal Wiring",
        ],
    );

    let contract_diff_text =
        fs::read_to_string(contract_diff).expect("contract diff should be written");
    assert_readable_output_contains_all(
        &contract_diff_text,
        &["TechCorp LLC", "$6,000.00", "Annual Maintenance"],
    );

    let images_diff_text = fs::read_to_string(images_diff).expect("image diff should be written");
    assert_readable_output_contains_all(&images_diff_text, &["upgraded, reinforced", "24V"]);
}

#[test]
fn scenario_markdown_files_document_all_real_sample_pairs() {
    let mut scenarios = String::new();
    for markdown in SAMPLE_SCENARIO_MARKDOWN {
        let path = sample_file(markdown);
        assert!(
            path.is_file(),
            "expected scenario markdown at {}",
            path.display()
        );
        scenarios.push_str(
            &fs::read_to_string(path).expect("scenario markdown should be readable as UTF-8"),
        );
        scenarios.push('\n');
    }

    for pair in real_sample_pdf_pairs() {
        assert!(
            scenarios.contains(pair.old_name),
            "scenario markdown should document {}",
            pair.old_name
        );
        assert!(
            scenarios.contains(pair.new_name),
            "scenario markdown should document {}",
            pair.new_name
        );
    }
}

#[test]
fn corpus_command_completes_against_real_sample_pdfs() {
    let fixture = TestFixture::new("corpus_command_real_samples");
    let corpus = fixture.path("real_corpus");
    fs::create_dir_all(&corpus).expect("real-sample corpus directory should be created");
    for sample in real_sample_pdf_names().iter().copied() {
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
    assert_eq!(report["total"], 34);
    assert_eq!(report["parsed"], 34);
    assert_eq!(report["partial"], 6);
    assert_eq!(report["failed"], 0);
    for (index, sample) in real_sample_pdf_names().iter().copied().enumerate() {
        assert_eq!(report["files"][index]["file"], sample);
    }
    assert!(report["diagnostic_counts"]["CONTENT_OPERATOR_UNKNOWN"].is_null());
    assert_eq!(report["diagnostic_counts"]["STREAM_LENGTH_MISMATCH"], 7);
    assert!(report["diagnostic_counts"]["MISSING_TOUNICODE"].is_null());
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

fn sample_file(name: &str) -> PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("samples")
        .join(name);
    assert!(path.is_file(), "expected sample file at {}", path.display());
    path
}

fn real_sample_pdf(name: &str) -> PathBuf {
    let path = sample_file(name);
    assert!(
        path.extension().and_then(|extension| extension.to_str()) == Some("pdf"),
        "expected real PDF sample at {}",
        path.display()
    );
    path
}

fn real_sample_pdf_names() -> &'static [&'static str] {
    REAL_SAMPLE_PDFS
}

fn real_sample_pdf_pairs() -> &'static [RealSamplePair] {
    REAL_SAMPLE_PAIRS
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).expect("JSON report should be written"))
        .expect("report should be valid JSON")
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

fn assert_readable_output_contains_all(output: &str, expected_terms: &[&str]) {
    assert!(
        !output.contains('\0') && !output.contains("\\u0000"),
        "output should not contain embedded NUL text: {output}"
    );
    for expected in expected_terms {
        assert!(
            output.contains(expected),
            "expected generated output to contain `{expected}` in:\n{output}"
        );
    }
}

fn assert_self_contained_html(output: &str) {
    assert!(output.starts_with("<!doctype html>"));
    assert!(
        !output.contains("http://") && !output.contains("https://"),
        "HTML output should not depend on external network resources: {output}"
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
