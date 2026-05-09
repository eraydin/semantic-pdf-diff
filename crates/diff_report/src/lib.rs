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

#[cfg(test)]
mod tests {
    use super::*;
    use spdfdiff_types::{ChangeKind, ChangeSeverity, SemanticChange};

    #[test]
    fn markdown_includes_summary_and_change_list() {
        let mut document = DiffDocument::empty("old", "new");
        document.summary.modified = 1;
        document.changes.push(SemanticChange {
            id: "change-0000".into(),
            kind: ChangeKind::Modified,
            severity: ChangeSeverity::Major,
            old_node: None,
            new_node: None,
            confidence: 0.9,
            reason: "paragraph text differs".into(),
        });

        let markdown = to_markdown(&document);

        assert!(markdown.contains("| Modified | 1 |"));
        assert!(markdown.contains("`change-0000` Modified Major"));
    }
}
