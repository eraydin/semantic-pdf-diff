use std::collections::{BTreeMap, BTreeSet};

use spdfdiff_types::{
    AiConfidenceBucket, AiDiagnosticCount, AiEvidenceBundle, AiReviewAnswer, AiReviewItem,
    AiReviewQuestionHint, AiReviewReport, AiReviewSummary, AiReviewTag, ChangeKind, DiffDocument,
    LayoutDiff, PdfDiffError, Rect, SemanticChange,
};

pub fn to_json(document: &DiffDocument) -> Result<String, PdfDiffError> {
    serde_json::to_string_pretty(document)
        .map_err(|error| PdfDiffError::InternalInvariant(error.to_string()))
}

pub fn to_ai_review_json(document: &DiffDocument) -> Result<String, PdfDiffError> {
    serde_json::to_string_pretty(&build_ai_review_report(document))
        .map_err(|error| PdfDiffError::InternalInvariant(error.to_string()))
}

#[must_use]
pub fn build_ai_review_report(document: &DiffDocument) -> AiReviewReport {
    let review_items = document
        .changes
        .iter()
        .map(build_ai_review_item)
        .collect::<Vec<_>>();
    let unsupported_surface_count = document
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code.starts_with("UNSUPPORTED_"))
        .count();
    let low_confidence_change_count = review_items
        .iter()
        .filter(|item| item.confidence_bucket == AiConfidenceBucket::Low)
        .count();

    AiReviewReport {
        schema_version: "0.1.0".into(),
        source_schema_version: document.schema_version.clone(),
        old_fingerprint: document.old_fingerprint.clone(),
        new_fingerprint: document.new_fingerprint.clone(),
        summary: AiReviewSummary {
            total_changes: document.changes.len(),
            inserted: document.summary.inserted,
            deleted: document.summary.deleted,
            modified: document.summary.modified,
            moved: document.summary.moved,
            layout_changed: document.summary.layout_changed,
            diagnostic_count: document.diagnostics.len(),
            low_confidence_change_count,
            unsupported_surface_count,
        },
        question_hints: build_question_hints(&review_items, unsupported_surface_count),
        review_items,
        diagnostic_summary: diagnostic_summary(document),
    }
}

#[must_use]
pub fn to_html(document: &DiffDocument) -> String {
    let mut output = String::from(
        "<!doctype html><html><head><meta charset=\"utf-8\"><style>\
body{font-family:system-ui,-apple-system,Segoe UI,sans-serif;margin:24px;color:#1f2933;background:#fff}\
table{border-collapse:collapse;width:100%;margin:12px 0}th,td{border:1px solid #d9e2ec;padding:8px;vertical-align:top;text-align:left}\
th{background:#f0f4f8}.change{margin:16px 0;border:1px solid #d9e2ec}.change h3{margin:0;padding:10px;background:#f8fafc}\
.meta{color:#52606d;font-size:0.9rem}.hunks code{display:inline-block;margin:2px 4px 2px 0;padding:2px 4px;background:#f0f4f8}\
.overlay-grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(280px,1fr));gap:12px;margin:12px 0}.overlay{border:1px solid #d9e2ec;padding:8px;background:#fbfdff}.overlay svg{width:100%;height:auto;max-height:240px;background:#fff}.overlay rect{fill:rgba(37,99,235,.12);stroke:#2563eb;stroke-width:1.5}.overlay text{font-size:10px;fill:#102a43}\
.diagnostic{margin:4px 0}</style><title>Semantic PDF Diff</title></head><body>",
    );
    output.push_str("<h1>Semantic PDF Diff</h1>");
    output.push_str("<table><thead><tr><th>Metric</th><th>Count</th></tr></thead><tbody>");
    for (label, count) in [
        ("Inserted", document.summary.inserted),
        ("Deleted", document.summary.deleted),
        ("Modified", document.summary.modified),
        ("Moved", document.summary.moved),
        ("Layout changed", document.summary.layout_changed),
    ] {
        output.push_str(&format!("<tr><td>{label}</td><td>{count}</td></tr>",));
    }
    output.push_str("</tbody></table>");
    push_html_overlays(&mut output, document);

    output.push_str("<h2>Changes</h2>");
    if document.changes.is_empty() {
        output.push_str("<p>No semantic changes detected.</p>");
    } else {
        for change in &document.changes {
            output.push_str(&format!(
                "<section class=\"change\"><h3>{} {:?} {:?}</h3><p class=\"meta\">confidence {:.3}: {}</p>",
                escape_html(&change.id),
                change.kind,
                change.severity,
                change.confidence,
                escape_html(&change.reason)
            ));
            output.push_str("<table><thead><tr><th>Old</th><th>New</th></tr></thead><tbody><tr>");
            output.push_str("<td>");
            push_html_evidence(&mut output, change.old_node.as_ref());
            output.push_str("</td><td>");
            push_html_evidence(&mut output, change.new_node.as_ref());
            output.push_str("</td></tr></tbody></table>");
            if !change.text_hunks.is_empty() {
                output.push_str("<div class=\"hunks\"><strong>Text hunks</strong><br>");
                for hunk in &change.text_hunks {
                    output.push_str(&format!(
                        "<code>{}: {} -> {}</code>",
                        escape_html(&hunk_label(hunk)),
                        escape_html(hunk.old_text.as_deref().unwrap_or("")),
                        escape_html(hunk.new_text.as_deref().unwrap_or(""))
                    ));
                }
                output.push_str("</div>");
            }
            if let Some(layout_diff) = &change.layout_diff {
                output.push_str(&format!(
                    "<div class=\"meta\"><strong>Layout diff</strong>: {}</div>",
                    escape_html(&layout_diff_summary(layout_diff))
                ));
            }
            output.push_str("</section>");
        }
    }

    output.push_str("<h2>Diagnostics</h2>");
    if document.diagnostics.is_empty() {
        output.push_str("<p>No diagnostics.</p>");
    } else {
        for diagnostic in &document.diagnostics {
            output.push_str(&format!(
                "<div class=\"diagnostic\"><code>{:?}</code> <code>{}</code> {}</div>",
                diagnostic.severity,
                escape_html(&diagnostic.code),
                escape_html(&diagnostic.message)
            ));
        }
    }
    output.push_str("</body></html>");
    output
}

#[derive(Debug, Clone)]
struct OverlayRect {
    change_id: String,
    node_id: String,
    bbox: Rect,
}

fn push_html_overlays(output: &mut String, document: &DiffDocument) {
    let mut overlays: BTreeMap<(&'static str, usize), Vec<OverlayRect>> = BTreeMap::new();
    for change in &document.changes {
        if let Some(evidence) = &change.old_node
            && let Some(bbox) = evidence.bbox
            && is_reportable_rect(bbox)
        {
            overlays
                .entry(("Old", evidence.page))
                .or_default()
                .push(OverlayRect {
                    change_id: change.id.clone(),
                    node_id: evidence.node_id.clone(),
                    bbox,
                });
        }
        if let Some(evidence) = &change.new_node
            && let Some(bbox) = evidence.bbox
            && is_reportable_rect(bbox)
        {
            overlays
                .entry(("New", evidence.page))
                .or_default()
                .push(OverlayRect {
                    change_id: change.id.clone(),
                    node_id: evidence.node_id.clone(),
                    bbox,
                });
        }
    }
    if overlays.is_empty() {
        return;
    }

    output.push_str("<h2>Page Evidence Overlays</h2>");
    output.push_str(
        "<p class=\"meta\">Inline SVG rectangles use PDF user-space coordinates from extracted node bounding boxes.</p>",
    );
    output.push_str("<div class=\"overlay-grid\">");
    for ((role, page), mut rects) in overlays {
        rects.sort_by(|left, right| {
            left.change_id
                .cmp(&right.change_id)
                .then_with(|| left.node_id.cmp(&right.node_id))
        });
        output.push_str(&format!(
            "<section class=\"overlay\"><h3>{} page {}</h3>",
            role,
            page + 1
        ));
        push_svg_overlay(output, &rects);
        output.push_str("</section>");
    }
    output.push_str("</div>");
}

fn push_svg_overlay(output: &mut String, rects: &[OverlayRect]) {
    let Some((x0, y0, x1, y1)) = overlay_bounds(rects) else {
        return;
    };
    let margin = 8.0;
    let view_x = x0 - margin;
    let view_y = y0 - margin;
    let view_width = (x1 - x0 + margin * 2.0).max(1.0);
    let view_height = (y1 - y0 + margin * 2.0).max(1.0);
    output.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{view_x:.2} {view_y:.2} {view_width:.2} {view_height:.2}\" role=\"img\" aria-label=\"PDF user-space evidence overlay\">"
    ));
    for rect in rects {
        let (x, y, width, height) = normalized_rect(rect.bbox);
        output.push_str(&format!(
            "<rect x=\"{x:.2}\" y=\"{y:.2}\" width=\"{width:.2}\" height=\"{height:.2}\" data-change=\"{}\" data-node=\"{}\"><title>{} {}</title></rect>",
            escape_html(&rect.change_id),
            escape_html(&rect.node_id),
            escape_html(&rect.change_id),
            escape_html(&rect.node_id)
        ));
        output.push_str(&format!(
            "<text x=\"{:.2}\" y=\"{:.2}\">{}</text>",
            x,
            y - 2.0,
            escape_html(&rect.change_id)
        ));
    }
    output.push_str("</svg>");
}

fn overlay_bounds(rects: &[OverlayRect]) -> Option<(f32, f32, f32, f32)> {
    let mut iter = rects.iter().map(|rect| normalized_rect(rect.bbox));
    let (mut x0, mut y0, width, height) = iter.next()?;
    let mut x1 = x0 + width;
    let mut y1 = y0 + height;
    for (x, y, width, height) in iter {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x + width);
        y1 = y1.max(y + height);
    }
    Some((x0, y0, x1, y1))
}

fn normalized_rect(rect: Rect) -> (f32, f32, f32, f32) {
    let x0 = rect.x0.min(rect.x1);
    let y0 = rect.y0.min(rect.y1);
    let x1 = rect.x0.max(rect.x1);
    let y1 = rect.y0.max(rect.y1);
    (x0, y0, (x1 - x0).max(0.1), (y1 - y0).max(0.1))
}

fn is_reportable_rect(rect: Rect) -> bool {
    rect.x0.is_finite()
        && rect.y0.is_finite()
        && rect.x1.is_finite()
        && rect.y1.is_finite()
        && (rect.x1 - rect.x0).abs() > 0.0
        && (rect.y1 - rect.y0).abs() > 0.0
}

fn build_ai_review_item(change: &SemanticChange) -> AiReviewItem {
    let tags = review_tags(change);
    AiReviewItem {
        change_id: change.id.clone(),
        kind: change.kind.clone(),
        severity: change.severity,
        confidence: change.confidence,
        confidence_bucket: confidence_bucket(change.confidence),
        explanation: review_explanation(change, &tags),
        evidence: evidence_bundle(change),
        tags,
    }
}

fn confidence_bucket(confidence: f32) -> AiConfidenceBucket {
    if confidence >= 0.9 {
        AiConfidenceBucket::High
    } else if confidence >= 0.75 {
        AiConfidenceBucket::Medium
    } else {
        AiConfidenceBucket::Low
    }
}

fn review_tags(change: &SemanticChange) -> Vec<AiReviewTag> {
    let mut tags = BTreeSet::new();
    match change.kind {
        ChangeKind::Inserted => {
            tags.insert(AiReviewTag::ContentInserted);
        }
        ChangeKind::Deleted => {
            tags.insert(AiReviewTag::ContentDeleted);
        }
        ChangeKind::Modified => {
            tags.insert(AiReviewTag::TextChanged);
        }
        ChangeKind::Moved => {
            tags.insert(AiReviewTag::ContentMoved);
        }
        ChangeKind::LayoutChanged => {
            tags.insert(AiReviewTag::LayoutOnly);
        }
        ChangeKind::AnnotationChanged => {
            tags.insert(AiReviewTag::AnnotationOrLinkChanged);
        }
        ChangeKind::FormFieldChanged => {
            tags.insert(AiReviewTag::FormFieldChanged);
        }
        ChangeKind::MetadataChanged => {
            tags.insert(AiReviewTag::MetadataChanged);
        }
        ChangeKind::ObjectChanged | ChangeKind::StyleChanged => {
            tags.insert(AiReviewTag::VisualSurfaceChanged);
        }
        ChangeKind::Unknown => {}
    }

    let text = change_text(change);
    let lower_text = text.to_lowercase();
    if has_any(
        &lower_text,
        &[
            "payment",
            "invoice",
            "amount",
            "fee",
            "price",
            "revenue",
            "total",
            "usd",
            "$",
            "maintenance",
            "schedule",
        ],
    ) {
        tags.insert(AiReviewTag::PaymentTermsCandidate);
    }
    if has_any(
        &lower_text,
        &[
            "day", "days", "date", "term", "notice", "year", "annual", "month", "weekly",
        ],
    ) {
        tags.insert(AiReviewTag::DateOrDurationCandidate);
    }
    if has_any(
        &lower_text,
        &[
            "corp",
            "llc",
            "inc",
            "client",
            "vendor",
            "party",
            "contractor",
        ],
    ) {
        tags.insert(AiReviewTag::PartyNameCandidate);
    }
    if change.text_hunks.iter().any(hunk_has_digit_change) {
        tags.insert(AiReviewTag::NumericValueChanged);
    }
    if change.confidence < 0.75 {
        tags.insert(AiReviewTag::LowConfidence);
    }
    if change.reason.contains("UNSUPPORTED_") {
        tags.insert(AiReviewTag::UnsupportedSurface);
    }

    tags.into_iter().collect()
}

fn hunk_has_digit_change(hunk: &spdfdiff_types::TextHunk) -> bool {
    hunk.old_text
        .as_deref()
        .is_some_and(|text| text.chars().any(|character| character.is_ascii_digit()))
        || hunk
            .new_text
            .as_deref()
            .is_some_and(|text| text.chars().any(|character| character.is_ascii_digit()))
}

fn has_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

fn change_text(change: &SemanticChange) -> String {
    [
        change
            .old_node
            .as_ref()
            .and_then(|node| node.text.as_deref())
            .unwrap_or_default(),
        change
            .new_node
            .as_ref()
            .and_then(|node| node.text.as_deref())
            .unwrap_or_default(),
    ]
    .join(" ")
}

fn review_explanation(change: &SemanticChange, tags: &[AiReviewTag]) -> String {
    let mut parts = vec![match change.kind {
        ChangeKind::Inserted => "Content was inserted.".to_owned(),
        ChangeKind::Deleted => "Content was deleted.".to_owned(),
        ChangeKind::Modified => "Text changed between matched semantic nodes.".to_owned(),
        ChangeKind::Moved => {
            "Content appears to have moved without a primary text change.".to_owned()
        }
        ChangeKind::LayoutChanged => {
            "Layout changed while text evidence stayed comparable.".to_owned()
        }
        ChangeKind::StyleChanged => "A style-facing surface changed.".to_owned(),
        ChangeKind::MetadataChanged => "A metadata-facing surface changed.".to_owned(),
        ChangeKind::AnnotationChanged => "An annotation or link surface changed.".to_owned(),
        ChangeKind::FormFieldChanged => "A form-field surface changed.".to_owned(),
        ChangeKind::ObjectChanged => "A report-facing PDF object surface changed.".to_owned(),
        ChangeKind::Unknown => "A change was detected but not classified further.".to_owned(),
    }];

    if tags.contains(&AiReviewTag::PaymentTermsCandidate) {
        parts.push("Payment or amount terms are mentioned; treat this as a review candidate, not a legal conclusion.".into());
    }
    if tags.contains(&AiReviewTag::DateOrDurationCandidate) {
        parts.push("Date, duration, or notice language is mentioned.".into());
    }
    if tags.contains(&AiReviewTag::LowConfidence) {
        parts.push("Confidence is low; inspect extraction diagnostics and source evidence.".into());
    }
    parts.push(change.reason.clone());
    parts.join(" ")
}

fn evidence_bundle(change: &SemanticChange) -> AiEvidenceBundle {
    let mut provenance = Vec::new();
    if let Some(old_node) = &change.old_node {
        provenance.extend(old_node.source.clone());
    }
    if let Some(new_node) = &change.new_node {
        provenance.extend(new_node.source.clone());
    }

    AiEvidenceBundle {
        old_node_id: change.old_node.as_ref().map(|node| node.node_id.clone()),
        new_node_id: change.new_node.as_ref().map(|node| node.node_id.clone()),
        section_hint: section_hint(change),
        old_page: change.old_node.as_ref().map(|node| node.page),
        new_page: change.new_node.as_ref().map(|node| node.page),
        old_bbox: change.old_node.as_ref().and_then(|node| node.bbox),
        new_bbox: change.new_node.as_ref().and_then(|node| node.bbox),
        old_text: change.old_node.as_ref().and_then(|node| node.text.clone()),
        new_text: change.new_node.as_ref().and_then(|node| node.text.clone()),
        text_hunks: change.text_hunks.clone(),
        layout_diff: change.layout_diff.clone(),
        provenance,
    }
}

fn section_hint(change: &SemanticChange) -> Option<String> {
    change
        .new_node
        .as_ref()
        .and_then(|node| node.text.as_deref())
        .and_then(section_hint_from_text)
        .or_else(|| {
            change
                .old_node
                .as_ref()
                .and_then(|node| node.text.as_deref())
                .and_then(section_hint_from_text)
        })
}

fn section_hint_from_text(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }

    let lower = trimmed.to_lowercase();
    if lower.starts_with("section ") || lower.starts_with("clause ") {
        return Some(first_words(trimmed, 10));
    }

    let first_token = trimmed.split_whitespace().next().unwrap_or_default();
    let looks_numbered = first_token
        .chars()
        .any(|character| character.is_ascii_digit())
        && (first_token.ends_with('.') || first_token.ends_with(')') || first_token.contains('.'));
    if looks_numbered {
        Some(first_words(trimmed, 10))
    } else {
        None
    }
}

fn first_words(text: &str, limit: usize) -> String {
    let mut value = text
        .split_whitespace()
        .take(limit)
        .collect::<Vec<_>>()
        .join(" ");
    if value.len() > 96 {
        value.truncate(96);
        value = value.trim_end().to_owned();
    }
    value
}

fn build_question_hints(
    review_items: &[AiReviewItem],
    unsupported_surface_count: usize,
) -> Vec<AiReviewQuestionHint> {
    vec![
        question_hint(
            "Which contractual obligations changed?",
            review_items,
            |item| {
                item.tags.iter().any(|tag| {
                    matches!(
                        tag,
                        AiReviewTag::TextChanged
                            | AiReviewTag::ContentInserted
                            | AiReviewTag::ContentDeleted
                            | AiReviewTag::ContentMoved
                    )
                }) && change_text_mentions_obligation(&item.evidence)
            },
            "Candidate obligation changes are based on obligation-like keywords and semantic change evidence.",
        ),
        question_hint(
            "Were payment terms modified?",
            review_items,
            |item| item.tags.contains(&AiReviewTag::PaymentTermsCandidate),
            "Payment-term candidates are based on payment, invoice, amount, or currency language in changed evidence.",
        ),
        question_hint(
            "Did layout change without text changing?",
            review_items,
            |item| item.tags.contains(&AiReviewTag::LayoutOnly),
            "Layout-only answers use changes classified separately from text modifications.",
        ),
        question_hint(
            "Which changes are low-confidence because extraction was incomplete?",
            review_items,
            |item| item.tags.contains(&AiReviewTag::LowConfidence),
            "Low-confidence answers use the engine confidence bucket and should be cross-checked with diagnostics.",
        ),
        AiReviewQuestionHint {
            question: "Were unsupported PDF surfaces encountered?".into(),
            answer: if unsupported_surface_count > 0 {
                AiReviewAnswer::Yes
            } else {
                AiReviewAnswer::No
            },
            supporting_change_ids: Vec::new(),
            rationale: "Unsupported surfaces are counted from stable diagnostic codes that start with UNSUPPORTED_.".into(),
        },
    ]
}

fn question_hint(
    question: &str,
    review_items: &[AiReviewItem],
    predicate: impl Fn(&AiReviewItem) -> bool,
    rationale: &str,
) -> AiReviewQuestionHint {
    let supporting_change_ids = review_items
        .iter()
        .filter(|item| predicate(item))
        .map(|item| item.change_id.clone())
        .collect::<Vec<_>>();
    AiReviewQuestionHint {
        question: question.into(),
        answer: if supporting_change_ids.is_empty() {
            AiReviewAnswer::No
        } else {
            AiReviewAnswer::Yes
        },
        supporting_change_ids,
        rationale: rationale.into(),
    }
}

fn change_text_mentions_obligation(evidence: &AiEvidenceBundle) -> bool {
    let text = [
        evidence.old_text.as_deref().unwrap_or_default(),
        evidence.new_text.as_deref().unwrap_or_default(),
    ]
    .join(" ")
    .to_lowercase();
    has_any(
        &text,
        &[
            "shall",
            "must",
            "required",
            "obligation",
            "liable",
            "liability",
            "indemnification",
            "termination",
            "notice",
            "payment",
        ],
    )
}

fn diagnostic_summary(document: &DiffDocument) -> Vec<AiDiagnosticCount> {
    let mut counts = BTreeMap::new();
    for diagnostic in &document.diagnostics {
        *counts.entry(diagnostic.code.clone()).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .map(|(code, count)| AiDiagnosticCount { code, count })
        .collect()
}

#[must_use]
pub fn to_markdown(document: &DiffDocument) -> String {
    let mut output = format!(
        "# Semantic PDF Diff\n\n| Metric | Count |\n| --- | ---: |\n| Inserted | {} |\n| Deleted | {} |\n| Modified | {} |\n| Moved | {} |\n| Layout changed | {} |\n\n",
        document.summary.inserted,
        document.summary.deleted,
        document.summary.modified,
        document.summary.moved,
        document.summary.layout_changed
    );

    output.push_str("## Changes\n\n");
    if document.changes.is_empty() {
        output.push_str("No semantic changes detected.\n\n");
    } else {
        for change in &document.changes {
            output.push_str(&format!(
                "- `{}` {:?} {:?}: {}\n",
                change.id, change.kind, change.severity, change.reason
            ));
            push_evidence_line(&mut output, "Old", change.old_node.as_ref());
            push_evidence_line(&mut output, "New", change.new_node.as_ref());
            if !change.text_hunks.is_empty() {
                output.push_str("  - Text hunks:");
                for hunk in &change.text_hunks {
                    output.push_str(&format!(
                        " `{}` \"{}\" -> \"{}\"",
                        hunk_label(hunk),
                        hunk.old_text.as_deref().unwrap_or_default(),
                        hunk.new_text.as_deref().unwrap_or_default()
                    ));
                }
                output.push('\n');
            }
            if let Some(layout_diff) = &change.layout_diff {
                output.push_str(&format!(
                    "  - Layout diff: {}\n",
                    layout_diff_summary(layout_diff)
                ));
            }
        }
        output.push('\n');
    }

    output.push_str("## Diagnostics\n\n");
    if document.diagnostics.is_empty() {
        output.push_str("No diagnostics.\n");
    } else {
        for diagnostic in &document.diagnostics {
            output.push_str(&format!(
                "- `{:?}` `{}` {}\n",
                diagnostic.severity, diagnostic.code, diagnostic.message
            ));
        }
    }

    output
}

fn push_html_evidence(
    output: &mut String,
    evidence: Option<&spdfdiff_types::SemanticNodeEvidence>,
) {
    let Some(evidence) = evidence else {
        output.push_str("<em>None</em>");
        return;
    };
    output.push_str(&format!(
        "<div class=\"meta\">page {} <code>{}</code></div>",
        evidence.page + 1,
        escape_html(&evidence.node_id)
    ));
    if let Some(bbox) = evidence.bbox {
        output.push_str(&format!(
            "<div class=\"meta\">bbox [{:.2}, {:.2}, {:.2}, {:.2}] in PDF user space</div>",
            bbox.x0, bbox.y0, bbox.x1, bbox.y1
        ));
    }
    if let Some(text) = &evidence.text {
        output.push_str(&format!("<div>{}</div>", escape_html(text)));
    }
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn push_evidence_line(
    output: &mut String,
    label: &str,
    evidence: Option<&spdfdiff_types::SemanticNodeEvidence>,
) {
    let Some(evidence) = evidence else {
        return;
    };
    output.push_str(&format!(
        "  - {label} page {} `{}`",
        evidence.page + 1,
        evidence.node_id
    ));
    if let Some(text) = &evidence.text {
        output.push_str(&format!(": {text}"));
    }
    output.push('\n');
}

fn layout_diff_summary(layout_diff: &LayoutDiff) -> String {
    let mut parts = Vec::new();
    if let Some(delta_x) = layout_diff.delta_x {
        parts.push(format!("dx={delta_x:.2}"));
    }
    if let Some(delta_y) = layout_diff.delta_y {
        parts.push(format!("dy={delta_y:.2}"));
    }
    if let Some(delta_width) = layout_diff.delta_width {
        parts.push(format!("dw={delta_width:.2}"));
    }
    if let Some(delta_height) = layout_diff.delta_height {
        parts.push(format!("dh={delta_height:.2}"));
    }
    if layout_diff.page_changed {
        parts.push("page_changed=true".to_owned());
    }
    if layout_diff.reading_order_changed {
        parts.push("reading_order_changed=true".to_owned());
    }
    if parts.is_empty() {
        "bbox changed without numeric delta".to_owned()
    } else {
        parts.join(", ")
    }
}

fn hunk_label(hunk: &spdfdiff_types::TextHunk) -> String {
    match &hunk.granularity {
        Some(granularity) => format!("{:?}/{:?}", hunk.kind, granularity),
        None => format!("{:?}", hunk.kind),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spdfdiff_types::{
        ChangeKind, ChangeSeverity, Provenance, Rect, SemanticChange, SemanticNodeEvidence,
        TextHunk, TextHunkKind,
    };

    #[test]
    fn markdown_includes_summary_and_change_list() {
        let mut document = DiffDocument::empty("old", "new");
        document.summary.modified = 1;
        document.changes.push(SemanticChange {
            id: "change-0000".into(),
            kind: ChangeKind::Modified,
            severity: ChangeSeverity::Major,
            old_node: Some(SemanticNodeEvidence {
                node_id: "old-node".into(),
                page: 0,
                bbox: Some(Rect {
                    x0: 72.0,
                    y0: 700.0,
                    x1: 240.0,
                    y1: 716.0,
                }),
                text: Some("Annual revenue was 10 million.".into()),
                source: vec![Provenance::unknown()],
            }),
            new_node: Some(SemanticNodeEvidence {
                node_id: "new-node".into(),
                page: 0,
                bbox: Some(Rect {
                    x0: 72.0,
                    y0: 682.0,
                    x1: 246.0,
                    y1: 698.0,
                }),
                text: Some("Annual revenue was 12 million.".into()),
                source: vec![Provenance::unknown()],
            }),
            text_hunks: vec![TextHunk {
                kind: TextHunkKind::Replaced,
                granularity: None,
                old_range: None,
                new_range: None,
                old_text: Some("10".into()),
                new_text: Some("12".into()),
            }],
            layout_diff: Some(LayoutDiff {
                old_bbox: Some(Rect {
                    x0: 72.0,
                    y0: 700.0,
                    x1: 240.0,
                    y1: 716.0,
                }),
                new_bbox: Some(Rect {
                    x0: 72.0,
                    y0: 682.0,
                    x1: 246.0,
                    y1: 698.0,
                }),
                delta_x: Some(0.0),
                delta_y: Some(-18.0),
                delta_width: Some(6.0),
                delta_height: Some(0.0),
                page_changed: false,
                reading_order_changed: false,
            }),
            confidence: 0.9,
            reason: "paragraph text differs".into(),
        });

        let markdown = to_markdown(&document);

        assert!(markdown.contains("| Modified | 1 |"));
        assert!(markdown.contains("`change-0000` Modified Major"));
        assert!(markdown.contains("Old page 1 `old-node`: Annual revenue was 10 million."));
        assert!(markdown.contains("New page 1 `new-node`: Annual revenue was 12 million."));
        assert!(markdown.contains("`Replaced` \"10\" -> \"12\""));
        assert!(markdown.contains("Layout diff: dx=0.00, dy=-18.00, dw=6.00, dh=0.00"));
    }

    #[test]
    fn html_is_self_contained_side_by_side_report() {
        let mut document = DiffDocument::empty("old", "new");
        document.summary.modified = 1;
        document.changes.push(SemanticChange {
            id: "change-0000".into(),
            kind: ChangeKind::Modified,
            severity: ChangeSeverity::Major,
            old_node: Some(SemanticNodeEvidence {
                node_id: "old-node".into(),
                page: 0,
                bbox: Some(Rect {
                    x0: 72.0,
                    y0: 700.0,
                    x1: 240.0,
                    y1: 716.0,
                }),
                text: Some("Annual revenue was 10 million.".into()),
                source: vec![Provenance::unknown()],
            }),
            new_node: Some(SemanticNodeEvidence {
                node_id: "new-node".into(),
                page: 0,
                bbox: Some(Rect {
                    x0: 72.0,
                    y0: 682.0,
                    x1: 246.0,
                    y1: 698.0,
                }),
                text: Some("Annual revenue was 12 million.".into()),
                source: vec![Provenance::unknown()],
            }),
            text_hunks: Vec::new(),
            layout_diff: Some(LayoutDiff {
                old_bbox: Some(Rect {
                    x0: 72.0,
                    y0: 700.0,
                    x1: 240.0,
                    y1: 716.0,
                }),
                new_bbox: Some(Rect {
                    x0: 72.0,
                    y0: 682.0,
                    x1: 246.0,
                    y1: 698.0,
                }),
                delta_x: Some(0.0),
                delta_y: Some(-18.0),
                delta_width: Some(6.0),
                delta_height: Some(0.0),
                page_changed: false,
                reading_order_changed: false,
            }),
            confidence: 0.9,
            reason: "paragraph text differs".into(),
        });

        let html = to_html(&document);

        assert!(html.contains("<!doctype html>"));
        assert!(html.contains("<th>Old</th><th>New</th>"));
        assert!(html.contains("<h2>Page Evidence Overlays</h2>"));
        assert!(html.contains("<svg xmlns=\"http://www.w3.org/2000/svg\""));
        assert!(html.contains("data-change=\"change-0000\""));
        assert!(html.contains("bbox [72.00, 700.00, 240.00, 716.00] in PDF user space"));
        assert!(html.contains("Layout diff"));
        assert!(html.contains("dx=0.00, dy=-18.00, dw=6.00, dh=0.00"));
        assert!(html.contains("Annual revenue was 10 million."));
        assert!(html.contains("Annual revenue was 12 million."));
        assert!(!html.contains("src=\"http"));
        assert!(!html.contains("href=\"http"));
    }

    #[test]
    fn ai_review_report_summarizes_questions_tags_and_evidence() {
        let mut document = DiffDocument::empty("old.pdf", "new.pdf");
        document.summary.modified = 1;
        document.changes.push(SemanticChange {
            id: "change-0000".into(),
            kind: ChangeKind::Modified,
            severity: ChangeSeverity::Major,
            old_node: Some(SemanticNodeEvidence {
                node_id: "old-node".into(),
                page: 0,
                bbox: None,
                text: Some("Payment is due within 30 days.".into()),
                source: vec![Provenance::unknown()],
            }),
            new_node: Some(SemanticNodeEvidence {
                node_id: "new-node".into(),
                page: 0,
                bbox: None,
                text: Some("Payment is due within 15 days.".into()),
                source: vec![Provenance::unknown()],
            }),
            text_hunks: vec![TextHunk {
                kind: TextHunkKind::Replaced,
                granularity: None,
                old_range: None,
                new_range: None,
                old_text: Some("30".into()),
                new_text: Some("15".into()),
            }],
            layout_diff: None,
            confidence: 0.91,
            reason: "paragraph text differs".into(),
        });

        let report = build_ai_review_report(&document);

        assert_eq!(report.summary.total_changes, 1);
        assert_eq!(
            report.review_items[0].confidence_bucket,
            AiConfidenceBucket::High
        );
        assert!(
            report.review_items[0]
                .tags
                .contains(&AiReviewTag::PaymentTermsCandidate)
        );
        assert!(
            report.review_items[0]
                .tags
                .contains(&AiReviewTag::NumericValueChanged)
        );
        assert_eq!(
            report.review_items[0].evidence.old_node_id.as_deref(),
            Some("old-node")
        );
        assert_eq!(
            report.review_items[0].evidence.new_node_id.as_deref(),
            Some("new-node")
        );
        assert_eq!(
            report.review_items[0].evidence.old_text.as_deref(),
            Some("Payment is due within 30 days.")
        );
        let payment_hint = report
            .question_hints
            .iter()
            .find(|hint| hint.question == "Were payment terms modified?")
            .expect("payment question hint should be present");
        assert_eq!(payment_hint.answer, AiReviewAnswer::Yes);
        assert_eq!(payment_hint.supporting_change_ids, vec!["change-0000"]);
    }
}
