use spdfdiff_types::{DiffDocument, PdfDiffError};

pub fn to_json(document: &DiffDocument) -> Result<String, PdfDiffError> {
    serde_json::to_string_pretty(document)
        .map_err(|error| PdfDiffError::InternalInvariant(error.to_string()))
}

#[must_use]
pub fn to_html(document: &DiffDocument) -> String {
    let mut output = String::from(
        "<!doctype html><html><head><meta charset=\"utf-8\"><style>\
body{font-family:system-ui,-apple-system,Segoe UI,sans-serif;margin:24px;color:#1f2933;background:#fff}\
table{border-collapse:collapse;width:100%;margin:12px 0}th,td{border:1px solid #d9e2ec;padding:8px;vertical-align:top;text-align:left}\
th{background:#f0f4f8}.change{margin:16px 0;border:1px solid #d9e2ec}.change h3{margin:0;padding:10px;background:#f8fafc}\
.meta{color:#52606d;font-size:0.9rem}.hunks code{display:inline-block;margin:2px 4px 2px 0;padding:2px 4px;background:#f0f4f8}\
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
        ChangeKind, ChangeSeverity, Provenance, SemanticChange, SemanticNodeEvidence, TextHunk,
        TextHunkKind,
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
                bbox: None,
                text: Some("Annual revenue was 10 million.".into()),
                source: vec![Provenance::unknown()],
            }),
            new_node: Some(SemanticNodeEvidence {
                node_id: "new-node".into(),
                page: 0,
                bbox: None,
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
            confidence: 0.9,
            reason: "paragraph text differs".into(),
        });

        let markdown = to_markdown(&document);

        assert!(markdown.contains("| Modified | 1 |"));
        assert!(markdown.contains("`change-0000` Modified Major"));
        assert!(markdown.contains("Old page 1 `old-node`: Annual revenue was 10 million."));
        assert!(markdown.contains("New page 1 `new-node`: Annual revenue was 12 million."));
        assert!(markdown.contains("`Replaced` \"10\" -> \"12\""));
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
                bbox: None,
                text: Some("Annual revenue was 10 million.".into()),
                source: vec![Provenance::unknown()],
            }),
            new_node: Some(SemanticNodeEvidence {
                node_id: "new-node".into(),
                page: 0,
                bbox: None,
                text: Some("Annual revenue was 12 million.".into()),
                source: vec![Provenance::unknown()],
            }),
            text_hunks: Vec::new(),
            confidence: 0.9,
            reason: "paragraph text differs".into(),
        });

        let html = to_html(&document);

        assert!(html.contains("<!doctype html>"));
        assert!(html.contains("<th>Old</th><th>New</th>"));
        assert!(html.contains("Annual revenue was 10 million."));
        assert!(html.contains("Annual revenue was 12 million."));
        assert!(!html.contains("http://"));
        assert!(!html.contains("https://"));
    }
}
