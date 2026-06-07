mod support;

use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};
use support::pdf_fixture::MinimalPdf;

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
    "Bean_and_Leaf_Menu_new.pdf",
    "Bean_and_Leaf_Menu_old.pdf",
    "annotations_base_v1.pdf",
    "annotations_visual_markup_v2.pdf",
    "attachment_link_bundle_v1.pdf",
    "attachment_link_bundle_v2.pdf",
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
    "layered_redaction_v1.pdf",
    "layered_redaction_v2.pdf",
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
    "tagged_table_reflow_v1.pdf",
    "tagged_table_reflow_v2.pdf",
    "ultimate_semantic_diff_v1.pdf",
    "ultimate_semantic_diff_v2.pdf",
    "vector_paths_graphic_v1.pdf",
    "vector_paths_graphic_v2.pdf",
    "visual_diff_image_content_new.pdf",
    "visual_diff_image_content_old.pdf",
    "watermark_overlay_v1.pdf",
    "watermark_overlay_v2.pdf",
];

const REAL_SAMPLE_PAIRS: &[RealSamplePair] = &[
    RealSamplePair {
        slug: "bean-and-leaf-menu",
        old_name: "Bean_and_Leaf_Menu_old.pdf",
        new_name: "Bean_and_Leaf_Menu_new.pdf",
        expected: None,
    },
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
            changes: 5,
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
            changes: 8,
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
        slug: "attachment-link-bundle",
        old_name: "attachment_link_bundle_v1.pdf",
        new_name: "attachment_link_bundle_v2.pdf",
        expected: None,
    },
    RealSamplePair {
        slug: "layered-redaction",
        old_name: "layered_redaction_v1.pdf",
        new_name: "layered_redaction_v2.pdf",
        expected: None,
    },
    RealSamplePair {
        slug: "tagged-table-reflow",
        old_name: "tagged_table_reflow_v1.pdf",
        new_name: "tagged_table_reflow_v2.pdf",
        expected: None,
    },
    RealSamplePair {
        slug: "vector-paths-graphic",
        old_name: "vector_paths_graphic_v1.pdf",
        new_name: "vector_paths_graphic_v2.pdf",
        expected: None,
    },
    RealSamplePair {
        slug: "visual-diff-image-content",
        old_name: "visual_diff_image_content_old.pdf",
        new_name: "visual_diff_image_content_new.pdf",
        expected: None,
    },
    RealSamplePair {
        slug: "scanned-document",
        old_name: "scanned_document_v1.pdf",
        new_name: "scanned_document_v2.pdf",
        expected: None,
    },
];

const SAMPLE_SCENARIO_MARKDOWN: &[&str] = &["semantic_diff_test_cases.md"];

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
    assert_eq!(json["changes"][0]["old_node"]["semantic_role"], "Paragraph");
    assert_eq!(
        json["changes"][0]["new_node"]["text"],
        "Annual revenue was 12 million."
    );
    assert_eq!(json["changes"][0]["new_node"]["semantic_role"], "Paragraph");
    assert_eq!(json["changes"][0]["text_hunks"][1]["kind"], "Replaced");
    assert_eq!(json["changes"][0]["text_hunks"][1]["old_text"], "10");
    assert_eq!(json["changes"][0]["text_hunks"][1]["new_text"], "12");

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
fn diff_command_respects_layout_tolerance_option() {
    let fixture = TestFixture::new("diff_command_layout_tolerance");
    let old_pdf = fixture.write_pdf_at("old.pdf", "Stable paragraph", 72.0, 720.0);
    let new_pdf = fixture.write_pdf_at("new.pdf", "Stable paragraph", 75.0, 720.0);

    let default_output = run_spdfdiff([
        "diff",
        path_arg(&old_pdf).as_str(),
        path_arg(&new_pdf).as_str(),
    ]);
    assert_success(&default_output);
    let default_json: Value =
        serde_json::from_slice(&default_output.stdout).expect("diff stdout should be valid JSON");
    assert_eq!(default_json["summary"]["layout_changed"], 1);
    assert_eq!(
        default_json["changes"][0]["layout_diff"]["delta_x"],
        serde_json::json!(3.0)
    );

    let tolerant_output = run_spdfdiff([
        "diff",
        path_arg(&old_pdf).as_str(),
        path_arg(&new_pdf).as_str(),
        "--layout-tolerance-pt",
        "4.0",
    ]);
    assert_success(&tolerant_output);
    let tolerant_json: Value =
        serde_json::from_slice(&tolerant_output.stdout).expect("diff stdout should be valid JSON");
    assert_eq!(tolerant_json["summary"]["layout_changed"], 0);
    assert_eq!(tolerant_json["changes"].as_array().unwrap().len(), 0);
}

#[test]
fn diff_command_emits_ai_review_json() {
    let fixture = TestFixture::new("diff_command_emits_ai_review_json");
    let old_pdf = fixture.write_pdf("old.pdf", "Payment is due within 30 days.");
    let new_pdf = fixture.write_pdf("new.pdf", "Payment is due within 15 days.");

    let output = run_spdfdiff([
        "diff",
        path_arg(&old_pdf).as_str(),
        path_arg(&new_pdf).as_str(),
        "--format",
        "ai-json",
    ]);
    assert_success(&output);
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("ai-json stdout should be valid JSON");

    assert_eq!(json["schema_version"], "0.1.0");
    assert_eq!(json["source_schema_version"], "0.1.0");
    assert_eq!(json["summary"]["total_changes"], 1);
    assert_eq!(
        json["review_items"][0]["change_id"],
        json["question_hints"][0]["supporting_change_ids"][0]
    );
    let tags = json["review_items"][0]["tags"]
        .as_array()
        .expect("tags should be an array")
        .iter()
        .map(|value| value.as_str().unwrap_or_default())
        .collect::<Vec<_>>();
    assert!(tags.contains(&"PaymentTermsCandidate"));
    assert!(tags.contains(&"NumericValueChanged"));
    assert_eq!(
        json["review_items"][0]["evidence"]["old_text"],
        "Payment is due within 30 days."
    );
    assert_eq!(
        json["review_items"][0]["evidence"]["old_semantic_role"],
        "Paragraph"
    );
    assert_eq!(
        json["review_items"][0]["evidence"]["new_text"],
        "Payment is due within 15 days."
    );
    assert_eq!(
        json["review_items"][0]["evidence"]["new_semantic_role"],
        "Paragraph"
    );
}

#[test]
fn diff_fail_on_changes_exits_one_only_when_changes_exist() {
    let fixture = TestFixture::new("diff_fail_on_changes");
    let old_pdf = fixture.write_pdf("old.pdf", "Annual revenue was 10 million.");
    let new_pdf = fixture.write_pdf("new.pdf", "Annual revenue was 12 million.");

    let changed = run_spdfdiff([
        "diff",
        path_arg(&old_pdf).as_str(),
        path_arg(&new_pdf).as_str(),
        "--fail-on-changes",
    ]);
    assert_eq!(changed.status.code(), Some(1));
    assert!(changed.stderr.is_empty());

    let unchanged = run_spdfdiff([
        "diff",
        path_arg(&old_pdf).as_str(),
        path_arg(&old_pdf).as_str(),
        "--fail-on-changes",
    ]);
    assert_success(&unchanged);
}

#[test]
fn visual_diff_command_uses_external_renderer_and_writes_heatmap() {
    let fixture = TestFixture::new("visual_diff_command");
    let old_pdf = fixture.write_pdf("old.pdf", "Visual old");
    let new_pdf = fixture.write_pdf("new.pdf", "Visual new");
    let renderer = fixture.write_mock_visual_renderer_command();
    let output_path = fixture.path("visual-diff.json");
    let artifacts_dir = fixture.path("visual-artifacts");
    let renderer_command = quoted_command_path(&renderer);

    let output = run_spdfdiff([
        "visual-diff",
        path_arg(&old_pdf).as_str(),
        path_arg(&new_pdf).as_str(),
        "--renderer-command",
        renderer_command.as_str(),
        "--output",
        path_arg(&output_path).as_str(),
        "--artifacts-dir",
        path_arg(&artifacts_dir).as_str(),
    ]);
    assert_success(&output);
    assert!(
        output.stdout.is_empty(),
        "--output should not duplicate the visual diff report on stdout"
    );

    let report = read_json(&output_path);
    assert_eq!(report["schema_version"], "1");
    assert_eq!(report["renderer"]["output_format"], "ppm-rgb");
    assert_eq!(report["summary"]["compared_pages"], 1);
    assert_eq!(report["summary"]["changed_pages"], 1);
    assert_eq!(report["summary"]["changed_pixels"], 1);
    assert_eq!(report["summary"]["total_pixels"], 2);
    assert_eq!(report["pages"][0]["status"], "changed");
    assert_eq!(report["pages"][0]["changed_pixel_ratio"], 0.5);
    assert_eq!(
        report["pages"][0]["heatmap"],
        "heatmaps/page-0001-heatmap.ppm"
    );
    assert!(artifacts_dir.join("old-rendered/page-0001.ppm").is_file());
    assert!(artifacts_dir.join("new-rendered/page-0001.ppm").is_file());
    assert!(
        artifacts_dir
            .join("heatmaps/page-0001-heatmap.ppm")
            .is_file()
    );
}

#[test]
fn check_command_writes_artifacts_and_fails_on_unsuppressed_changes() {
    let fixture = TestFixture::new("check_command_fails_on_changes");
    fixture.write_pdf("old.pdf", "Annual revenue was 10 million.");
    fixture.write_pdf("new.pdf", "Annual revenue was 12 million.");
    let config = fixture.path(".spdfdiff.toml");
    fs::write(
        &config,
        r#"
schema_version = "1"
output_dir = "artifacts"
formats = ["json", "html"]
fail_on_changes = true

[[pairs]]
name = "contract"
old = "old.pdf"
new = "new.pdf"
max_diagnostics = 10
"#,
    )
    .expect("check config should be written");

    let output = run_spdfdiff(["check", "--config", path_arg(&config).as_str()]);

    assert_eq!(output.status.code(), Some(1));
    let report: Value =
        serde_json::from_slice(&output.stdout).expect("check stdout should be valid JSON");
    assert_eq!(report["schema_version"], "1");
    assert_eq!(report["passed"], false);
    assert_eq!(report["total_pairs"], 1);
    assert_eq!(report["changed_pairs"], 1);
    assert_eq!(report["total_unsuppressed_changes"], 1);
    assert_eq!(report["pairs"][0]["name"], "contract");
    assert_eq!(report["pairs"][0]["status"], "failed");
    assert_eq!(report["pairs"][0]["changes"], 1);
    assert_eq!(report["pairs"][0]["unsuppressed_changes"], 1);
    assert_eq!(
        report["pairs"][0]["outputs"]["json"],
        "artifacts/contract.json"
    );
    assert_eq!(
        report["pairs"][0]["outputs"]["html"],
        "artifacts/contract.html"
    );
    assert!(fixture.path("artifacts/contract.json").exists());
    assert!(fixture.path("artifacts/contract.html").exists());
}

#[test]
fn check_command_disambiguates_colliding_artifact_names() {
    let fixture = TestFixture::new("check_command_artifact_name_collisions");
    fixture.write_pdf("old-a.pdf", "Annual revenue was 10 million.");
    fixture.write_pdf("new-a.pdf", "Annual revenue was 12 million.");
    fixture.write_pdf("old-b.pdf", "Payment is due within 30 days.");
    fixture.write_pdf("new-b.pdf", "Payment is due within 15 days.");
    let config = fixture.path(".spdfdiff.toml");
    fs::write(
        &config,
        r#"
schema_version = "1"
output_dir = "artifacts"
formats = ["json"]
fail_on_changes = false

[[pairs]]
name = "contract/a"
old = "old-a.pdf"
new = "new-a.pdf"

[[pairs]]
name = "contract?a"
old = "old-b.pdf"
new = "new-b.pdf"
"#,
    )
    .expect("check config should be written");

    let output = run_spdfdiff(["check", "--config", path_arg(&config).as_str()]);

    assert_success(&output);
    let report: Value =
        serde_json::from_slice(&output.stdout).expect("check stdout should be valid JSON");
    assert_eq!(report["passed"], true);
    assert_eq!(
        report["pairs"][0]["outputs"]["json"],
        "artifacts/contract-a.json"
    );
    assert_eq!(
        report["pairs"][1]["outputs"]["json"],
        "artifacts/contract-a-0002.json"
    );
    assert!(fixture.path("artifacts/contract-a.json").exists());
    assert!(fixture.path("artifacts/contract-a-0002.json").exists());
}

#[test]
fn check_command_rejects_unsupported_config_schema_version() {
    let fixture = TestFixture::new("check_command_rejects_schema_version");
    fixture.write_pdf("old.pdf", "Annual revenue was 10 million.");
    fixture.write_pdf("new.pdf", "Annual revenue was 12 million.");
    let config = fixture.path(".spdfdiff.toml");
    fs::write(
        &config,
        r#"
schema_version = "2"
output_dir = "artifacts"
formats = ["json"]

[[pairs]]
old = "old.pdf"
new = "new.pdf"
"#,
    )
    .expect("check config should be written");

    let output = run_spdfdiff(["check", "--config", path_arg(&config).as_str()]);

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("unsupported check config schema_version 2; expected 1"));
}

#[test]
fn check_command_suppresses_changes_from_baseline_report() {
    let fixture = TestFixture::new("check_command_baseline");
    let old_pdf = fixture.write_pdf("old.pdf", "Payment is due within 30 days.");
    let new_pdf = fixture.write_pdf("new.pdf", "Payment is due within 15 days.");
    let baseline = fixture.path("baseline.json");
    assert_success(&run_spdfdiff([
        "diff",
        path_arg(&old_pdf).as_str(),
        path_arg(&new_pdf).as_str(),
        "--output",
        path_arg(&baseline).as_str(),
    ]));
    let mut baseline_json: Value =
        serde_json::from_slice(&fs::read(&baseline).expect("baseline should be readable"))
            .expect("baseline should be JSON");
    baseline_json["changes"][0]["reason"] =
        Value::String("approved baseline reason wording changed".to_owned());
    fs::write(
        &baseline,
        serde_json::to_vec_pretty(&baseline_json).expect("baseline should render"),
    )
    .expect("baseline should be rewritten");
    let config = fixture.path(".spdfdiff.toml");
    fs::write(
        &config,
        r#"
schema_version = "1"
output_dir = "artifacts"
formats = ["json"]
fail_on_changes = true

[[pairs]]
name = "payment-terms"
old = "old.pdf"
new = "new.pdf"
baseline = "baseline.json"
"#,
    )
    .expect("check config should be written");

    let output = run_spdfdiff(["check", "--config", path_arg(&config).as_str()]);

    assert_success(&output);
    let report: Value =
        serde_json::from_slice(&output.stdout).expect("check stdout should be valid JSON");
    assert_eq!(report["passed"], true);
    assert_eq!(report["total_changes"], 1);
    assert_eq!(report["total_suppressed_changes"], 1);
    assert_eq!(report["total_unsuppressed_changes"], 0);
    assert_eq!(report["pairs"][0]["status"], "passed");
    assert_eq!(report["pairs"][0]["suppressed_changes"], 1);
    assert_eq!(report["pairs"][0]["unsuppressed_changes"], 0);
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
    assert_eq!(json["diagnostic_count"], 0);
}

#[test]
fn minimal_pdf_fixture_writer_is_deterministic_and_parseable() {
    let first = multi_page_minimal_pdf("First page", "Second page");
    let second = multi_page_minimal_pdf("First page", "Second page");

    assert_eq!(first, second);
    let text = String::from_utf8(first.clone()).expect("fixture should be ASCII PDF syntax");
    assert!(text.contains("xref\n0 8\n"));
    assert!(text.contains("trailer\n<< /Size 8 /Root 1 0 R >>"));
    assert!(text.contains("startxref\n"));
    assert!(text.ends_with("%%EOF\n"));

    let document =
        pdf_core::PdfDocument::parse(&first).expect("fixture writer output should parse");
    assert_eq!(document.objects.len(), 7);
    assert_eq!(document.page_contents().len(), 2);
}

#[test]
fn generated_diff_pair_matrix_matches_golden_snapshots() {
    let fixture = TestFixture::new("generated_diff_pair_matrix");
    let cases = [
        (
            "identical",
            MinimalPdf::single_page("Stable paragraph").to_bytes(),
            MinimalPdf::single_page("Stable paragraph").to_bytes(),
        ),
        (
            "inserted-paragraph",
            MinimalPdf::single_page("Stable paragraph").to_bytes(),
            MinimalPdf::two_pages("Stable paragraph", "Inserted paragraph").to_bytes(),
        ),
        (
            "deleted-paragraph",
            MinimalPdf::two_pages("Stable paragraph", "Deleted paragraph").to_bytes(),
            MinimalPdf::single_page("Stable paragraph").to_bytes(),
        ),
        (
            "modified-paragraph",
            MinimalPdf::single_page("Payment is due within 30 days.").to_bytes(),
            MinimalPdf::single_page("Payment is due within 15 days.").to_bytes(),
        ),
        (
            "moved-paragraph",
            MinimalPdf::two_pages("First paragraph", "Second paragraph").to_bytes(),
            MinimalPdf::two_pages("Second paragraph", "First paragraph").to_bytes(),
        ),
        (
            "layout-only-movement",
            MinimalPdf::single_page_at("Stable paragraph", 72.0, 720.0).to_bytes(),
            MinimalPdf::single_page_at("Stable paragraph", 96.0, 720.0).to_bytes(),
        ),
        (
            "changed-page-count",
            MinimalPdf::single_page("Only page").to_bytes(),
            MinimalPdf::two_pages("Only page", "Additional page").to_bytes(),
        ),
    ];

    for (name, old_bytes, new_bytes) in cases {
        let old_pdf = fixture.write_bytes(&format!("{name}-old.pdf"), &old_bytes);
        let new_pdf = fixture.write_bytes(&format!("{name}-new.pdf"), &new_bytes);
        let output_path = fixture.path(&format!("{name}.json"));
        assert_success(&run_spdfdiff([
            "diff",
            path_arg(&old_pdf).as_str(),
            path_arg(&new_pdf).as_str(),
            "--format",
            "json",
            "--output",
            path_arg(&output_path).as_str(),
        ]));
        let report = read_json(&output_path);
        assert_eq!(
            stable_diff_pair_snapshot(&report),
            read_json(&golden_diff_pair_snapshot(name)),
            "generated diff-pair snapshot changed for {name}"
        );
    }
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
    assert_eq!(report["partial"], 0);
    assert_eq!(report["failed"], 1);
    assert_eq!(report["files"][0]["file"], "a.pdf");
    assert_eq!(report["files"][0]["status"], "failed");
    assert_eq!(report["files"][1]["file"], "b.pdf");
    assert_eq!(report["files"][1]["status"], "parsed");
    assert!(report["diagnostic_counts"]["MISSING_TOUNICODE"].is_null());
}

#[test]
fn benchmark_command_reports_m8_t5_phase_metrics() {
    let fixture = TestFixture::new("benchmark_command_phase_metrics");
    let output_path = fixture.path("reports/benchmark.json");

    let output = run_spdfdiff([
        "benchmark",
        "--pages",
        "50",
        "--output",
        path_arg(&output_path).as_str(),
    ]);
    assert_success(&output);

    let json: Value = serde_json::from_slice(
        &fs::read(&output_path).expect("benchmark report should be written"),
    )
    .expect("benchmark output should be valid JSON");

    assert_eq!(json["pages"], 50);
    assert_eq!(json["target_total_ms"], 5000);
    assert_eq!(json["under_target"], true);
    for phase in ["parse", "extract", "semantic", "diff", "report", "total"] {
        assert!(json["timings_ms"][phase].is_number());
    }
    assert!(json["summary"]["modified"].as_u64().unwrap_or_default() >= 1);
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
    assert_eq!(report["old_fingerprint"], pair.old_name);
    assert_eq!(report["new_fingerprint"], pair.new_name);
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
        assert!(
            report["changes"]
                .as_array()
                .expect("changes should be an array")
                .len()
                >= expected.changes,
            "expected at least the documented semantic changes; object-level surfaces may add evidence changes"
        );
    }
    assert_diagnostic_code_absent(&report, "MISSING_TOUNICODE");
    if pair.slug == "bean-and-leaf-menu" {
        assert!(
            report["diagnostics"]
                .as_array()
                .expect("diagnostics should be an array")
                .iter()
                .any(|diagnostic| diagnostic["code"] == "UNSUPPORTED_STREAM_FILTER"),
            "Bean and Leaf sample should preserve unsupported-filter diagnostics"
        );
    } else {
        assert_diagnostic_code_absent(&report, "UNSUPPORTED_STREAM_FILTER");
    }
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

        assert_eq!(report["file"], sample);
        assert!(
            report["object_count"].as_u64().unwrap_or_default() >= 1,
            "inspect should parse a non-empty object graph for {sample}"
        );
        assert!(report["diagnostic_count"].as_u64().unwrap_or_default() <= 8);
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

        assert_eq!(report["file"], sample);
        let paragraphs = report["paragraphs"].as_u64().unwrap_or_default();
        if sample.starts_with("scanned_document_") {
            assert_eq!(
                paragraphs, 0,
                "image-only scanned samples should not invent text"
            );
        } else {
            assert!(paragraphs >= 1, "expected extractable text in {sample}");
        }
        assert!(report["diagnostic_count"].as_u64().unwrap_or_default() <= 4);
    }
}

#[test]
fn extract_command_uses_configured_ocr_for_scanned_sample_pdf() {
    let fixture = TestFixture::new("ocr_scanned_sample");
    let ocr_command = fixture.write_mock_ocr_command();
    let output = run_spdfdiff_with_env(
        [
            "extract",
            path_arg(&real_sample_pdf("scanned_document_v1.pdf")).as_str(),
            "--format",
            "md",
        ],
        &[("SPDFDIFF_OCR_COMMAND", path_arg(&ocr_command).as_str())],
    );

    assert_success(&output);
    let markdown =
        String::from_utf8(output.stdout).expect("extract markdown should be valid UTF-8");
    assert_readable_output_contains_all(&markdown, &["Mock OCR text for image"]);
}

#[test]
fn diff_command_uses_configured_ocr_for_scanned_sample_pdf() {
    let fixture = TestFixture::new("ocr_scanned_diff");
    let ocr_command = fixture.write_mock_ocr_command();
    let output_path = fixture.path("scanned-ocr-diff.json");

    let output = run_spdfdiff_with_env(
        [
            "diff",
            path_arg(&real_sample_pdf("scanned_document_v1.pdf")).as_str(),
            path_arg(&real_sample_pdf("scanned_document_v2.pdf")).as_str(),
            "--format",
            "json",
            "--output",
            path_arg(&output_path).as_str(),
        ],
        &[("SPDFDIFF_OCR_COMMAND", path_arg(&ocr_command).as_str())],
    );

    assert_success(&output);
    let report = read_json(&output_path);
    assert!(report["summary"]["modified"].as_u64().unwrap_or_default() >= 1);
    assert_diagnostic_code_absent(&report, "MISSING_TEXT_LAYER");
    assert!(
        report["diagnostics"]
            .as_array()
            .expect("diagnostics should be an array")
            .iter()
            .any(|diagnostic| diagnostic["code"] == "OCR_TEXT_EXTRACTED")
    );
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
        assert!(html.contains("<h1>Semantic PDF Diff</h1>"));
        assert!(html.contains("<th>Old</th><th>New</th>"));
        if html.contains("bbox [") {
            assert!(html.contains("<svg xmlns=\"http://www.w3.org/2000/svg\""));
        }
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
fn ai_json_outputs_complete_against_real_sample_pdfs() {
    let fixture = TestFixture::new("real_sample_ai_json_outputs");

    for pair in real_sample_pdf_pairs().iter().copied() {
        let output_path = fixture.path(&format!("{}-ai-review.json", pair.slug));
        assert_success(&run_spdfdiff([
            "diff",
            path_arg(&real_sample_pdf(pair.old_name)).as_str(),
            path_arg(&real_sample_pdf(pair.new_name)).as_str(),
            "--format",
            "ai-json",
            "--output",
            path_arg(&output_path).as_str(),
        ]));

        let report = read_json(&output_path);
        assert_eq!(report["schema_version"], "0.1.0");
        assert_eq!(report["source_schema_version"], "0.1.0");
        assert!(report["summary"]["total_changes"].is_u64());
        assert_eq!(
            report["review_items"]
                .as_array()
                .expect("ai-json review_items should be an array")
                .len() as u64,
            report["summary"]["total_changes"]
                .as_u64()
                .expect("ai-json total_changes should be a number")
        );
        assert_eq!(
            report["question_hints"]
                .as_array()
                .expect("ai-json question_hints should be an array")
                .len(),
            6
        );
        assert!(report["diagnostic_summary"].is_array());

        if pair.slug == "semantic-contract" {
            assert!(
                ai_json_has_tag(&report, "PaymentTermsCandidate"),
                "semantic contract ai-json should flag payment-term candidate evidence"
            );
            assert!(
                ai_json_has_tag(&report, "DateOrDurationCandidate"),
                "semantic contract ai-json should flag date/duration candidate evidence"
            );
        }
        if pair.slug == "semantic-images" {
            assert_eq!(
                ai_json_question_answer(&report, "Were unsupported PDF surfaces encountered?"),
                Some("No")
            );
        }
        if pair.slug == "headers-footers" {
            assert!(
                ai_json_has_tag(&report, "RepeatedPageRegion"),
                "headers/footers ai-json should tag repeated page-region evidence"
            );
            assert_eq!(
                ai_json_question_answer(&report, "Did repeated page regions change?"),
                Some("Yes")
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
fn generated_reports_reflect_documented_scenario_expectations() {
    let fixture = TestFixture::new("documented_scenario_expectations");

    assert_diff_contains_all(
        &fixture,
        "document",
        "document_v1.pdf",
        "document_v2.pdf",
        &["Redis", "150ms", "Version 1.1", "scalable backend"],
    );
    assert_diff_contains_all(
        &fixture,
        "complex",
        "complex_semantic_diff_v1.pdf",
        "complex_semantic_diff_v2.pdf",
        &["γ", "Σ", "Δ"],
    );
    assert_diff_contains_all(
        &fixture,
        "headers-footers",
        "headers_footers_v1.pdf",
        "headers_footers_v2.pdf",
        &["994-B", "2026", "firewall logs"],
    );
    assert_diff_contains_all(
        &fixture,
        "multipage-table",
        "multipage_table_v1.pdf",
        "multipage_table_v2.pdf",
        &["User_15@example.com"],
    );
    assert_diff_contains_all(
        &fixture,
        "interactive-forms",
        "interactive_forms_v1.pdf",
        "interactive_forms_v2.pdf",
        &["Jane Doe", "Engineering", "Laptop"],
    );
    assert_diff_lacks_diagnostic(
        &fixture,
        "interactive-forms-supported",
        "interactive_forms_v1.pdf",
        "interactive_forms_v2.pdf",
        "UNSUPPORTED_FORM_FIELD_DIFF",
    );
    assert_diff_contains_all(
        &fixture,
        "document-outline",
        "document_outline_v1.pdf",
        "document_outline_v2.pdf",
        &["Caching Layer", "API Specifications"],
    );
    assert_diff_contains_all(
        &fixture,
        "layered-redaction",
        "layered_redaction_v1.pdf",
        "layered_redaction_v2.pdf",
        &["REDACTED", "hidden legacy text", "Privacy review"],
    );
    assert_diff_contains_all(
        &fixture,
        "tagged-table-reflow",
        "tagged_table_reflow_v1.pdf",
        "tagged_table_reflow_v2.pdf",
        &[
            "Tagged Control Matrix Q2",
            "MFA Required",
            "Evidence Export",
        ],
    );
    assert_diff_contains_all(
        &fixture,
        "attachment-link-bundle",
        "attachment_link_bundle_v1.pdf",
        "attachment_link_bundle_v2.pdf",
        &["control-evidence-v2.zip", "sha256: BBB222", "production"],
    );

    assert_diff_contains_all(
        &fixture,
        "image-report",
        "report_with_images_v1.pdf",
        "report_with_images_v2.pdf",
        &["ObjectChanged", "image payload differs"],
    );
    assert_diff_contains_all(
        &fixture,
        "semantic-images",
        "semantic_images_v1.pdf",
        "semantic_images_v2.pdf",
        &["ObjectChanged", "image payload"],
    );
    assert_diff_contains_all(
        &fixture,
        "interactive-links",
        "interactive_links_v1.pdf",
        "interactive_links_v2.pdf",
        &[
            "AnnotationChanged",
            "uri=https://monitor.example.com/db2_cluster",
        ],
    );
    assert_diff_contains_all(
        &fixture,
        "attachment-link-bundle-diagnostic",
        "attachment_link_bundle_v1.pdf",
        "attachment_link_bundle_v2.pdf",
        &[
            "AnnotationChanged",
            "uri=https://example.test/evidence/v2-final",
        ],
    );
    assert_diff_has_diagnostic(
        &fixture,
        "scanned-document",
        "scanned_document_v1.pdf",
        "scanned_document_v2.pdf",
        "MISSING_TEXT_LAYER",
    );
    assert_diff_contains_all(
        &fixture,
        "vector-paths",
        "vector_paths_graphic_v1.pdf",
        "vector_paths_graphic_v2.pdf",
        &["ObjectChanged", "native vector graphic surface"],
    );
    assert_diff_has_diagnostic(
        &fixture,
        "tagged-table-reflow-diagnostic",
        "tagged_table_reflow_v1.pdf",
        "tagged_table_reflow_v2.pdf",
        "TAGGED_MCID_DETECTED",
    );
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
    assert_eq!(report["total"], 44);
    assert_eq!(report["parsed"], 44);
    assert_eq!(report["partial"], 4);
    assert_eq!(report["failed"], 0);
    for (index, sample) in real_sample_pdf_names().iter().copied().enumerate() {
        assert_eq!(report["files"][index]["file"], sample);
    }
    assert!(report["diagnostic_counts"]["CONTENT_OPERATOR_UNKNOWN"].is_null());
    assert!(report["diagnostic_counts"]["STREAM_LENGTH_MISMATCH"].is_null());
    assert_eq!(report["diagnostic_counts"]["MISSING_TEXT_LAYER"], 2);
    assert!(report["diagnostic_counts"]["UNSUPPORTED_ANNOTATION_DIFF"].is_null());
    assert!(report["diagnostic_counts"]["UNSUPPORTED_VECTOR_GRAPHIC_DIFF"].is_null());
    assert!(report["diagnostic_counts"]["MISSING_TOUNICODE_CID_FONT"].is_null());
    assert!(report["diagnostic_counts"]["MISSING_TOUNICODE"].is_null());
    assert_eq!(report["diagnostic_counts"]["TAGGED_MCID_DETECTED"], 2);
    assert_eq!(
        report["diagnostic_counts"]["TAGGED_PDF_STRUCTURE_DETECTED"],
        2
    );
    assert!(report["diagnostic_counts"]["UNSUPPORTED_IMAGE_DIFF"].is_null());
    assert_eq!(report["diagnostic_counts"]["UNSUPPORTED_STREAM_FILTER"], 7);
    assert!(report["diagnostic_counts"]["UNSUPPORTED_OBJECT_STREAM"].is_null());
    assert!(report["diagnostic_counts"]["MISSING_PAGE_CONTENT"].is_null());
}

#[test]
fn corpus_command_evaluates_committed_sample_manifest_gate() {
    let fixture = TestFixture::new("corpus_command_manifest_gate");
    let corpus = fixture.path("real_corpus");
    fs::create_dir_all(&corpus).expect("real-sample corpus directory should be created");
    for sample in real_sample_pdf_names().iter().copied() {
        fs::copy(real_sample_pdf(sample), corpus.join(sample))
            .expect("real sample should be copied");
    }
    let output_path = fixture.path("manifest-corpus.json");
    let manifest = sample_file("compatibility_corpus_manifest.json");

    let output = run_spdfdiff([
        "corpus",
        path_arg(&corpus).as_str(),
        "--manifest",
        path_arg(&manifest).as_str(),
        "--output",
        path_arg(&output_path).as_str(),
        "--fail-on-gate",
    ]);
    assert_success(&output);

    let report = read_json(&output_path);
    assert_eq!(report["gate"]["passed"], true);
    assert_eq!(report["gate"]["manifest_schema_version"], "1");
    assert_eq!(
        report["gate"]["missing_required_files"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    assert_eq!(report["diff_pairs"].as_array().unwrap().len(), 22);
    assert_eq!(report["diff_pairs"][0]["name"], "bean-and-leaf-menu");
    assert_eq!(report["diff_pairs"][0]["status"], "diffed");
    assert!(report["diff_diagnostic_counts"].is_object());
}

fn run_spdfdiff<const N: usize>(args: [&str; N]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_spdfdiff"))
        .args(args)
        .output()
        .expect("spdfdiff process should start")
}

fn run_spdfdiff_with_env<const N: usize>(args: [&str; N], envs: &[(&str, &str)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_spdfdiff"));
    command.args(args);
    for (key, value) in envs {
        command.env(key, value);
    }
    command.output().expect("spdfdiff process should start")
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

fn quoted_command_path(path: &Path) -> String {
    path_arg(path)
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

fn golden_diff_pair_snapshot(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join("diff_pairs")
        .join(format!("{name}.json"))
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

fn stable_diff_pair_snapshot(report: &Value) -> Value {
    let changes = report["changes"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|change| {
            serde_json::json!({
                "kind": change["kind"],
                "severity": change["severity"],
                "old_page": change["old_node"]["page"],
                "new_page": change["new_node"]["page"],
                "old_text": change["old_node"]["text"],
                "new_text": change["new_node"]["text"],
                "text_hunks": change["text_hunks"].as_array().map(|hunks| {
                    hunks
                        .iter()
                        .map(|hunk| {
                            serde_json::json!({
                                "kind": hunk["kind"],
                                "granularity": hunk["granularity"],
                                "old_text": hunk["old_text"],
                                "new_text": hunk["new_text"],
                            })
                        })
                        .collect::<Vec<_>>()
                }).unwrap_or_default(),
                "layout_diff": change["layout_diff"].as_object().map(|layout| {
                    serde_json::json!({
                        "delta_x": layout.get("delta_x").unwrap_or(&Value::Null),
                        "delta_y": layout.get("delta_y").unwrap_or(&Value::Null),
                        "page_changed": layout.get("page_changed").unwrap_or(&Value::Null),
                        "reading_order_changed": layout.get("reading_order_changed").unwrap_or(&Value::Null),
                    })
                }),
            })
        })
        .collect::<Vec<_>>();

    serde_json::json!({
        "schema_version": report["schema_version"],
        "summary": report["summary"],
        "changes": changes,
        "diagnostic_codes": report["diagnostics"].as_array().map(|diagnostics| {
            diagnostics
                .iter()
                .filter_map(|diagnostic| diagnostic["code"].as_str())
                .collect::<Vec<_>>()
        }).unwrap_or_default(),
    })
}

fn ai_json_has_tag(report: &Value, expected_tag: &str) -> bool {
    report["review_items"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|item| {
            item["tags"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|tag| tag.as_str() == Some(expected_tag))
        })
}

fn ai_json_question_answer<'a>(report: &'a Value, question: &str) -> Option<&'a str> {
    report["question_hints"]
        .as_array()?
        .iter()
        .find(|hint| hint["question"].as_str() == Some(question))
        .and_then(|hint| hint["answer"].as_str())
}

fn assert_diff_contains_all(
    fixture: &TestFixture,
    slug: &str,
    old_name: &str,
    new_name: &str,
    expected_terms: &[&str],
) {
    let output_path = fixture.path(&format!("{slug}.json"));
    assert_success(&run_spdfdiff([
        "diff",
        path_arg(&real_sample_pdf(old_name)).as_str(),
        path_arg(&real_sample_pdf(new_name)).as_str(),
        "--format",
        "json",
        "--output",
        path_arg(&output_path).as_str(),
    ]));
    let output = fs::read_to_string(output_path).expect("diff JSON should be written");
    assert_readable_output_contains_all(&output, expected_terms);
}

fn assert_diff_has_diagnostic(
    fixture: &TestFixture,
    slug: &str,
    old_name: &str,
    new_name: &str,
    expected_code: &str,
) {
    let output_path = fixture.path(&format!("{slug}.json"));
    assert_success(&run_spdfdiff([
        "diff",
        path_arg(&real_sample_pdf(old_name)).as_str(),
        path_arg(&real_sample_pdf(new_name)).as_str(),
        "--format",
        "json",
        "--output",
        path_arg(&output_path).as_str(),
    ]));
    let report = read_json(&output_path);
    let diagnostics = report["diagnostics"]
        .as_array()
        .expect("diagnostics should be an array");
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic["code"] == expected_code),
        "expected diagnostic code {expected_code} in {diagnostics:?}"
    );
}

fn assert_diff_lacks_diagnostic(
    fixture: &TestFixture,
    slug: &str,
    old_name: &str,
    new_name: &str,
    absent_code: &str,
) {
    let output_path = fixture.path(&format!("{slug}.json"));
    assert_success(&run_spdfdiff([
        "diff",
        path_arg(&real_sample_pdf(old_name)).as_str(),
        path_arg(&real_sample_pdf(new_name)).as_str(),
        "--format",
        "json",
        "--output",
        path_arg(&output_path).as_str(),
    ]));
    let report = read_json(&output_path);
    assert_diagnostic_code_absent(&report, absent_code);
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
        !output.contains("src=\"http") && !output.contains("href=\"http"),
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
        fs::write(&path, MinimalPdf::single_page(text).to_bytes())
            .expect("PDF fixture should be written");
        path
    }

    fn write_pdf_at(&self, name: &str, text: &str, x: f32, y: f32) -> PathBuf {
        let path = self.path(name);
        fs::write(&path, MinimalPdf::single_page_at(text, x, y).to_bytes())
            .expect("PDF fixture should be written");
        path
    }

    fn write_bytes(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.path(name);
        fs::write(&path, bytes).expect("fixture bytes should be written");
        path
    }

    fn write_mock_ocr_command(&self) -> PathBuf {
        #[cfg(windows)]
        {
            let path = self.path("mock-ocr.cmd");
            fs::write(
                &path,
                "@echo off\r\necho Mock OCR text for image %SPDFDIFF_OCR_IMAGE_INDEX% %SPDFDIFF_OCR_IMAGE_HASH%\r\n",
            )
            .expect("mock OCR command should be written");
            path
        }
        #[cfg(not(windows))]
        {
            use std::os::unix::fs::PermissionsExt;

            let path = self.path("mock-ocr.sh");
            fs::write(
                &path,
                "#!/bin/sh\nprintf 'Mock OCR text for image %s %s\\n' \"$SPDFDIFF_OCR_IMAGE_INDEX\" \"$SPDFDIFF_OCR_IMAGE_HASH\"\n",
            )
            .expect("mock OCR command should be written");
            let mut permissions = fs::metadata(&path)
                .expect("mock OCR command metadata should be readable")
                .permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&path, permissions).expect("mock OCR command should be executable");
            path
        }
    }

    fn write_mock_visual_renderer_command(&self) -> PathBuf {
        #[cfg(windows)]
        {
            let path = self.path("mock-visual-renderer.cmd");
            fs::write(
                &path,
                "@echo off\r\nif \"%SPDFDIFF_RENDER_ROLE%\"==\"old\" (\r\n  > \"%SPDFDIFF_RENDER_OUTPUT_DIR%\\page-0001.ppm\" echo P3\r\n  >> \"%SPDFDIFF_RENDER_OUTPUT_DIR%\\page-0001.ppm\" echo 2 1\r\n  >> \"%SPDFDIFF_RENDER_OUTPUT_DIR%\\page-0001.ppm\" echo 255\r\n  >> \"%SPDFDIFF_RENDER_OUTPUT_DIR%\\page-0001.ppm\" echo 0 0 0 255 255 255\r\n) else (\r\n  > \"%SPDFDIFF_RENDER_OUTPUT_DIR%\\page-0001.ppm\" echo P3\r\n  >> \"%SPDFDIFF_RENDER_OUTPUT_DIR%\\page-0001.ppm\" echo 2 1\r\n  >> \"%SPDFDIFF_RENDER_OUTPUT_DIR%\\page-0001.ppm\" echo 255\r\n  >> \"%SPDFDIFF_RENDER_OUTPUT_DIR%\\page-0001.ppm\" echo 0 0 0 255 0 0\r\n)\r\n",
            )
            .expect("mock visual renderer command should be written");
            path
        }
        #[cfg(not(windows))]
        {
            use std::os::unix::fs::PermissionsExt;

            let path = self.path("mock-visual-renderer.sh");
            fs::write(
                &path,
                "#!/bin/sh\nif [ \"$SPDFDIFF_RENDER_ROLE\" = \"old\" ]; then\n  pixels='0 0 0 255 255 255'\nelse\n  pixels='0 0 0 255 0 0'\nfi\ncat > \"$SPDFDIFF_RENDER_OUTPUT_DIR/page-0001.ppm\" <<EOF\nP3\n2 1\n255\n$pixels\nEOF\n",
            )
            .expect("mock visual renderer command should be written");
            let mut permissions = fs::metadata(&path)
                .expect("mock visual renderer command metadata should be readable")
                .permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&path, permissions)
                .expect("mock visual renderer command should be executable");
            path
        }
    }
}

impl Drop for TestFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn minimal_pdf(text: &str) -> Vec<u8> {
    MinimalPdf::single_page(text).to_bytes()
}

fn multi_page_minimal_pdf(first_text: &str, second_text: &str) -> Vec<u8> {
    MinimalPdf::two_pages(first_text, second_text).to_bytes()
}
