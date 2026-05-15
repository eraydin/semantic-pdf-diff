use spdfdiff_types::{DiffDocument, PdfDiffError};

pub fn to_json(document: &DiffDocument) -> Result<String, PdfDiffError> {
    serde_json::to_string_pretty(document)
        .map_err(|error| PdfDiffError::InternalInvariant(error.to_string()))
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

#[cfg(test)]
mod tests {
    use super::*;
    use spdfdiff_types::{
        ChangeKind, ChangeSeverity, Provenance, SemanticChange, SemanticNodeEvidence,
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
            confidence: 0.9,
            reason: "paragraph text differs".into(),
        });

        let markdown = to_markdown(&document);

        assert!(markdown.contains("| Modified | 1 |"));
        assert!(markdown.contains("`change-0000` Modified Major"));
        assert!(markdown.contains("Old page 1 `old-node`: Annual revenue was 10 million."));
        assert!(markdown.contains("New page 1 `new-node`: Annual revenue was 12 million."));
    }
}
