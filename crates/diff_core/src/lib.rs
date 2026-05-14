use std::collections::BTreeSet;

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

    let old_texts = old
        .nodes
        .iter()
        .map(|node| comparable_text(node, config))
        .collect::<Vec<_>>();
    let new_texts = new
        .nodes
        .iter()
        .map(|node| comparable_text(node, config))
        .collect::<Vec<_>>();
    let mut matches = exact_text_matches(&old_texts, &new_texts);
    matches.push((old.nodes.len(), new.nodes.len()));

    let mut old_start = 0;
    let mut new_start = 0;
    for (old_end, new_end) in matches {
        emit_unmatched_range(
            old,
            new,
            old_start..old_end,
            new_start..new_end,
            config,
            &mut document,
            classifier,
        );
        old_start = old_end + 1;
        new_start = new_end + 1;
    }

    if config.detect_moves {
        relabel_insert_delete_pairs_as_moves(&mut document, config, classifier);
    }
    document
}

fn exact_text_matches(old_texts: &[String], new_texts: &[String]) -> Vec<(usize, usize)> {
    let mut lengths = vec![vec![0usize; new_texts.len() + 1]; old_texts.len() + 1];
    for old_index in (0..old_texts.len()).rev() {
        for new_index in (0..new_texts.len()).rev() {
            lengths[old_index][new_index] = if old_texts[old_index] == new_texts[new_index] {
                lengths[old_index + 1][new_index + 1] + 1
            } else {
                lengths[old_index + 1][new_index].max(lengths[old_index][new_index + 1])
            };
        }
    }

    let mut matches = Vec::new();
    let mut old_index = 0;
    let mut new_index = 0;
    while old_index < old_texts.len() && new_index < new_texts.len() {
        if old_texts[old_index] == new_texts[new_index] {
            matches.push((old_index, new_index));
            old_index += 1;
            new_index += 1;
        } else if lengths[old_index + 1][new_index] >= lengths[old_index][new_index + 1] {
            old_index += 1;
        } else {
            new_index += 1;
        }
    }
    matches
}

fn emit_unmatched_range(
    old: &SemanticDocument,
    new: &SemanticDocument,
    old_range: std::ops::Range<usize>,
    new_range: std::ops::Range<usize>,
    config: DiffConfig,
    document: &mut DiffDocument,
    classifier: &impl SeverityClassifier,
) {
    let paired = old_range.len().min(new_range.len());
    for offset in 0..paired {
        let old_node = &old.nodes[old_range.start + offset];
        let new_node = &new.nodes[new_range.start + offset];
        let reason = modified_reason_with_hunk(old_node, new_node, config);
        push_change(
            document,
            ChangeKind::Modified,
            Some(old_node),
            Some(new_node),
            &reason,
            classifier,
        );
    }

    for old_index in old_range.start + paired..old_range.end {
        push_change(
            document,
            ChangeKind::Deleted,
            Some(&old.nodes[old_index]),
            None,
            "paragraph exists only in old document",
            classifier,
        );
    }

    for new_index in new_range.start + paired..new_range.end {
        push_change(
            document,
            ChangeKind::Inserted,
            None,
            Some(&new.nodes[new_index]),
            "paragraph exists only in new document",
            classifier,
        );
    }
}

fn push_change(
    document: &mut DiffDocument,
    kind: ChangeKind,
    old_node: Option<&SemanticNode>,
    new_node: Option<&SemanticNode>,
    reason: &str,
    classifier: &impl SeverityClassifier,
) {
    let kind_for_summary = kind.clone();
    let mut change = SemanticChange {
        id: format!("change-{:04}", document.changes.len()),
        kind,
        severity: ChangeSeverity::Info,
        old_node: old_node.map(to_evidence),
        new_node: new_node.map(to_evidence),
        confidence: 0.9,
        reason: reason.into(),
    };
    change.severity = classifier.classify(&change);
    match kind_for_summary {
        ChangeKind::Inserted => document.summary.inserted += 1,
        ChangeKind::Deleted => document.summary.deleted += 1,
        ChangeKind::Modified => document.summary.modified += 1,
        ChangeKind::Moved => document.summary.moved += 1,
        ChangeKind::LayoutChanged => document.summary.layout_changed += 1,
        ChangeKind::StyleChanged
        | ChangeKind::MetadataChanged
        | ChangeKind::AnnotationChanged
        | ChangeKind::FormFieldChanged
        | ChangeKind::ObjectChanged
        | ChangeKind::Unknown => {}
    }
    document.changes.push(change);
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

fn modified_reason_with_hunk(
    old_node: &SemanticNode,
    new_node: &SemanticNode,
    config: DiffConfig,
) -> String {
    let old_text = old_node.normalized_text.as_deref().unwrap_or_default();
    let new_text = new_node.normalized_text.as_deref().unwrap_or_default();
    let old_tokens = normalized_tokens(old_text, config);
    let new_tokens = normalized_tokens(new_text, config);

    if old_tokens == new_tokens {
        return "paragraph text differs between exact-match anchors".into();
    }

    let prefix_len = shared_prefix_len(&old_tokens, &new_tokens);
    let old_suffix_start = shared_suffix_start(&old_tokens, &new_tokens, prefix_len);
    let new_suffix_start = shared_suffix_start(&new_tokens, &old_tokens, prefix_len);

    let old_hunk = old_tokens[prefix_len..old_suffix_start].join(" ");
    let new_hunk = new_tokens[prefix_len..new_suffix_start].join(" ");
    format!(
        "paragraph text differs between exact-match anchors (old: \"{}\" -> new: \"{}\")",
        old_hunk, new_hunk
    )
}

fn normalized_tokens(text: &str, config: DiffConfig) -> Vec<String> {
    let base = if config.ignore_whitespace {
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    } else {
        text.to_string()
    };
    let normalized = if config.ignore_case {
        base.to_lowercase()
    } else {
        base
    };
    normalized
        .split_whitespace()
        .map(ToOwned::to_owned)
        .collect()
}

fn shared_prefix_len(left: &[String], right: &[String]) -> usize {
    let mut index = 0;
    while index < left.len() && index < right.len() && left[index] == right[index] {
        index += 1;
    }
    index
}

fn shared_suffix_start(target: &[String], other: &[String], prefix_len: usize) -> usize {
    let mut suffix_len = 0;
    while target.len() > prefix_len + suffix_len
        && other.len() > prefix_len + suffix_len
        && target[target.len() - 1 - suffix_len] == other[other.len() - 1 - suffix_len]
    {
        suffix_len += 1;
    }
    target.len() - suffix_len
}

fn relabel_insert_delete_pairs_as_moves(
    document: &mut DiffDocument,
    config: DiffConfig,
    classifier: &impl SeverityClassifier,
) {
    let mut deleted_candidates = document
        .changes
        .iter()
        .enumerate()
        .filter_map(|(index, change)| (change.kind == ChangeKind::Deleted).then_some(index))
        .collect::<Vec<_>>();
    let mut consumed_deleted = BTreeSet::new();

    for insert_index in 0..document.changes.len() {
        if document.changes[insert_index].kind != ChangeKind::Inserted {
            continue;
        }
        let Some(insert_text) = document.changes[insert_index]
            .new_node
            .as_ref()
            .and_then(|node| node.text.as_ref())
        else {
            continue;
        };
        let insert_text = normalize_text_for_match(insert_text, config);

        let Some((position, deleted_index)) =
            deleted_candidates
                .iter()
                .enumerate()
                .find_map(|(position, deleted_index)| {
                    let deleted_text = document.changes[*deleted_index]
                        .old_node
                        .as_ref()
                        .and_then(|node| node.text.as_ref())?;
                    (normalize_text_for_match(deleted_text, config) == insert_text)
                        .then_some((position, *deleted_index))
                })
        else {
            continue;
        };

        let old_evidence = document.changes[deleted_index].old_node.clone();
        let change = &mut document.changes[insert_index];
        change.kind = ChangeKind::Moved;
        change.old_node = old_evidence;
        change.reason = "paragraph text moved to a different reading-order position".into();
        change.severity = classifier.classify(change);
        consumed_deleted.insert(deleted_index);
        deleted_candidates.remove(position);
    }

    if consumed_deleted.is_empty() {
        return;
    }
    let retained = document
        .changes
        .iter()
        .enumerate()
        .filter_map(|(index, change)| {
            (!consumed_deleted.contains(&index)).then_some(change.clone())
        })
        .collect::<Vec<_>>();
    document.changes = retained;
    renumber_and_recompute_summary(document);
}

fn normalize_text_for_match(text: &str, config: DiffConfig) -> String {
    let text = if config.ignore_whitespace {
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    } else {
        text.to_string()
    };
    if config.ignore_case {
        text.to_lowercase()
    } else {
        text
    }
}

fn renumber_and_recompute_summary(document: &mut DiffDocument) {
    document.summary.inserted = 0;
    document.summary.deleted = 0;
    document.summary.modified = 0;
    document.summary.moved = 0;
    document.summary.layout_changed = 0;
    for (index, change) in document.changes.iter_mut().enumerate() {
        change.id = format!("change-{index:04}");
        match change.kind {
            ChangeKind::Inserted => document.summary.inserted += 1,
            ChangeKind::Deleted => document.summary.deleted += 1,
            ChangeKind::Modified => document.summary.modified += 1,
            ChangeKind::Moved => document.summary.moved += 1,
            ChangeKind::LayoutChanged => document.summary.layout_changed += 1,
            _ => {}
        }
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
        assert_eq!(
            diff.changes[0].reason,
            "paragraph text differs between exact-match anchors (old: \"\" -> new: \"world\")"
        );
        assert_ne!(diff.changes[0].severity, ChangeSeverity::Critical);
    }

    #[test]
    fn anchors_unchanged_text_around_inserted_paragraph() {
        let old = document_with_texts("old", &["Alpha", "Beta"]);
        let new = document_with_texts("new", &["Alpha", "Inserted", "Beta"]);

        let diff = diff_semantic_documents(&old, &new, DiffConfig::default());

        assert_eq!(diff.summary.inserted, 1);
        assert_eq!(diff.summary.modified, 0);
        assert_eq!(diff.changes.len(), 1);
        assert_eq!(diff.changes[0].id, "change-0000");
        assert_eq!(diff.changes[0].kind, ChangeKind::Inserted);
        assert_eq!(
            diff.changes[0].new_node.as_ref().unwrap().text.as_deref(),
            Some("Inserted")
        );
    }

    #[test]
    fn anchors_unchanged_text_around_deleted_paragraph() {
        let old = document_with_texts("old", &["Alpha", "Deleted", "Beta"]);
        let new = document_with_texts("new", &["Alpha", "Beta"]);

        let diff = diff_semantic_documents(&old, &new, DiffConfig::default());

        assert_eq!(diff.summary.deleted, 1);
        assert_eq!(diff.summary.modified, 0);
        assert_eq!(diff.changes.len(), 1);
        assert_eq!(diff.changes[0].kind, ChangeKind::Deleted);
        assert_eq!(
            diff.changes[0].old_node.as_ref().unwrap().text.as_deref(),
            Some("Deleted")
        );
    }

    #[test]
    fn detects_moved_paragraph_from_insert_delete_pair() {
        let old = document_with_texts("old", &["Alpha", "Beta", "Gamma"]);
        let new = document_with_texts("new", &["Beta", "Alpha", "Gamma"]);

        let diff = diff_semantic_documents(&old, &new, DiffConfig::default());

        assert_eq!(diff.summary.moved, 1);
        assert_eq!(diff.summary.inserted, 0);
        assert_eq!(diff.summary.deleted, 0);
        assert_eq!(diff.changes.len(), 1);
        assert_eq!(diff.changes[0].kind, ChangeKind::Moved);
    }

    struct MajorClassifier;

    impl SeverityClassifier for MajorClassifier {
        fn classify(&self, _change: &SemanticChange) -> ChangeSeverity {
            ChangeSeverity::Major
        }
    }

    #[test]
    fn move_relabeling_respects_custom_classifier() {
        let old = document_with_texts("old", &["Alpha", "Beta", "Gamma"]);
        let new = document_with_texts("new", &["Beta", "Alpha", "Gamma"]);

        let diff = diff_semantic_documents_with_classifier(
            &old,
            &new,
            DiffConfig::default(),
            &MajorClassifier,
        );

        assert_eq!(diff.changes[0].kind, ChangeKind::Moved);
        assert_eq!(diff.changes[0].severity, ChangeSeverity::Major);
    }

    fn document_with_text(fingerprint: &str, text: &str) -> SemanticDocument {
        document_with_texts(fingerprint, &[text])
    }

    fn document_with_texts(fingerprint: &str, texts: &[&str]) -> SemanticDocument {
        SemanticDocument {
            fingerprint: fingerprint.into(),
            nodes: texts
                .iter()
                .enumerate()
                .map(|(index, text)| SemanticNode {
                    id: format!("n{index:04}"),
                    kind: SemanticNodeKind::Paragraph,
                    page_index: 0,
                    bbox: None,
                    normalized_text: Some((*text).into()),
                    source: vec![Provenance::unknown()],
                    confidence: 1.0,
                })
                .collect(),
            diagnostics: Vec::new(),
        }
    }
}
