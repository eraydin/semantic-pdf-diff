use pdf_semantic::{SemanticDocument, SemanticNode};
use spdfdiff_types::{
    ChangeKind, ChangeSeverity, DiffDocument, SemanticChange, SemanticNodeEvidence,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DiffConfig {
    pub ignore_whitespace: bool,
    pub ignore_case: bool,
    pub detect_moves: bool,
    pub layout_tolerance_pt: f32,
    pub min_match_score: f32,
}

impl Default for DiffConfig {
    fn default() -> Self {
        Self {
            ignore_whitespace: true,
            ignore_case: false,
            detect_moves: true,
            layout_tolerance_pt: 2.0,
            min_match_score: 0.8,
        }
    }
}

pub trait SeverityClassifier {
    fn classify(&self, change: &SemanticChange) -> ChangeSeverity;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DefaultSeverityClassifier;

impl SeverityClassifier for DefaultSeverityClassifier {
    fn classify(&self, change: &SemanticChange) -> ChangeSeverity {
        match change.kind {
            ChangeKind::Inserted | ChangeKind::Deleted | ChangeKind::Modified => {
                ChangeSeverity::Major
            }
            ChangeKind::Moved | ChangeKind::LayoutChanged | ChangeKind::StyleChanged => {
                ChangeSeverity::Minor
            }
            ChangeKind::MetadataChanged | ChangeKind::ObjectChanged => ChangeSeverity::Info,
            ChangeKind::AnnotationChanged | ChangeKind::FormFieldChanged | ChangeKind::Unknown => {
                ChangeSeverity::Major
            }
        }
    }
}

#[must_use]
pub fn diff_semantic_documents(
    old: &SemanticDocument,
    new: &SemanticDocument,
    config: DiffConfig,
) -> DiffDocument {
    let classifier = DefaultSeverityClassifier;
    diff_semantic_documents_with_classifier(old, new, config, &classifier)
}

#[must_use]
pub fn diff_semantic_documents_with_classifier(
    old: &SemanticDocument,
    new: &SemanticDocument,
    config: DiffConfig,
    classifier: &impl SeverityClassifier,
) -> DiffDocument {
    let mut document = DiffDocument::empty(&old.fingerprint, &new.fingerprint);
    document.diagnostics.extend(old.diagnostics.clone());
    document.diagnostics.extend(new.diagnostics.clone());

    let max_len = old.nodes.len().max(new.nodes.len());
    for index in 0..max_len {
        match (old.nodes.get(index), new.nodes.get(index)) {
            (Some(old_node), Some(new_node)) => {
                if comparable_text(old_node, config) != comparable_text(new_node, config) {
                    let mut change = SemanticChange {
                        id: format!("change-{index:04}"),
                        kind: ChangeKind::Modified,
                        severity: ChangeSeverity::Info,
                        old_node: Some(to_evidence(old_node)),
                        new_node: Some(to_evidence(new_node)),
                        confidence: 0.9,
                        reason: "paragraph text differs at the same reading-order position".into(),
                    };
                    change.severity = classifier.classify(&change);
                    document.summary.modified += 1;
                    document.changes.push(change);
                }
            }
            (Some(old_node), None) => {
                let mut change = SemanticChange {
                    id: format!("change-{index:04}"),
                    kind: ChangeKind::Deleted,
                    severity: ChangeSeverity::Info,
                    old_node: Some(to_evidence(old_node)),
                    new_node: None,
                    confidence: 0.9,
                    reason: "paragraph exists only in old document".into(),
                };
                change.severity = classifier.classify(&change);
                document.summary.deleted += 1;
                document.changes.push(change);
            }
            (None, Some(new_node)) => {
                let mut change = SemanticChange {
                    id: format!("change-{index:04}"),
                    kind: ChangeKind::Inserted,
                    severity: ChangeSeverity::Info,
                    old_node: None,
                    new_node: Some(to_evidence(new_node)),
                    confidence: 0.9,
                    reason: "paragraph exists only in new document".into(),
                };
                change.severity = classifier.classify(&change);
                document.summary.inserted += 1;
                document.changes.push(change);
            }
            (None, None) => {}
        }
    }

    document
}

fn comparable_text(node: &SemanticNode, config: DiffConfig) -> String {
    let text = node.normalized_text.clone().unwrap_or_default();
    let text = if config.ignore_whitespace {
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    } else {
        text
    };
    if config.ignore_case {
        text.to_lowercase()
    } else {
        text
    }
}

fn to_evidence(node: &SemanticNode) -> SemanticNodeEvidence {
    SemanticNodeEvidence {
        node_id: node.id.clone(),
        page: node.page_index,
        bbox: node.bbox,
        text: node.normalized_text.clone(),
        source: node.source.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_semantic::{SemanticNode, SemanticNodeKind};
    use spdfdiff_types::Provenance;

    #[test]
    fn detects_modified_paragraph() {
        let old = document_with_text("old", "Hello");
        let new = document_with_text("new", "Hello world");

        let diff = diff_semantic_documents(&old, &new, DiffConfig::default());

        assert_eq!(diff.summary.modified, 1);
        assert_eq!(diff.changes[0].id, "change-0000");
        assert_eq!(diff.changes[0].kind, ChangeKind::Modified);
        assert_ne!(diff.changes[0].severity, ChangeSeverity::Critical);
    }

    fn document_with_text(fingerprint: &str, text: &str) -> SemanticDocument {
        SemanticDocument {
            fingerprint: fingerprint.into(),
            nodes: vec![SemanticNode {
                id: "n0000".into(),
                kind: SemanticNodeKind::Paragraph,
                page_index: 0,
                bbox: None,
                normalized_text: Some(text.into()),
                source: vec![Provenance::unknown()],
                confidence: 1.0,
            }],
            diagnostics: Vec::new(),
        }
    }
}
