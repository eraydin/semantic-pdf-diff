use std::collections::{BTreeMap, BTreeSet};

use pdf_semantic::{SemanticDocument, SemanticNode};
use spdfdiff_types::{
    ChangeKind, ChangeSeverity, Diagnostic, DiffDocument, LayoutDiff, Rect, SemanticChange,
    SemanticNodeEvidence, TextHunk, TextHunkGranularity, TextHunkKind, TextRange,
};

const DEFAULT_MAX_MATCH_MATRIX_CELLS: usize = 1_000_000;
const DEFAULT_MAX_GREEDY_MATCH_CANDIDATES: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DiffConfig {
    pub ignore_whitespace: bool,
    pub ignore_case: bool,
    pub detect_moves: bool,
    pub layout_tolerance_pt: f32,
    pub min_match_score: f32,
    pub max_match_matrix_cells: usize,
    pub max_greedy_match_candidates: usize,
}

impl Default for DiffConfig {
    fn default() -> Self {
        Self {
            ignore_whitespace: true,
            ignore_case: false,
            detect_moves: true,
            layout_tolerance_pt: 2.0,
            min_match_score: 0.8,
            max_match_matrix_cells: DEFAULT_MAX_MATCH_MATRIX_CELLS,
            max_greedy_match_candidates: DEFAULT_MAX_GREEDY_MATCH_CANDIDATES,
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
    let exact_matches = exact_text_matches(&old_texts, &new_texts, config);
    if let Some(diagnostic) = exact_matches.diagnostic {
        document.diagnostics.push(diagnostic);
    }
    let mut matches = exact_matches.matches;
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
        if old_end < old.nodes.len() && new_end < new.nodes.len() {
            emit_layout_change_if_needed(
                &old.nodes[old_end],
                &new.nodes[new_end],
                old_end,
                new_end,
                config,
                &mut document,
                classifier,
            );
        }
        old_start = old_end + 1;
        new_start = new_end + 1;
    }

    if config.detect_moves {
        relabel_insert_delete_pairs_as_moves(&mut document, config, classifier);
    }
    document
}

#[derive(Debug, Clone, PartialEq)]
struct ExactMatchResult {
    matches: Vec<(usize, usize)>,
    diagnostic: Option<Diagnostic>,
}

fn exact_text_matches(
    old_texts: &[String],
    new_texts: &[String],
    config: DiffConfig,
) -> ExactMatchResult {
    let old_len = old_texts.len();
    let new_len = new_texts.len();
    if matrix_cell_count_exceeds_limit(old_len, new_len, config.max_match_matrix_cells) {
        return ExactMatchResult {
            matches: greedy_exact_text_matches(old_texts, new_texts),
            diagnostic: Some(match_limit_diagnostic(
                "EXACT_MATCH_LIMIT_EXCEEDED",
                old_len,
                new_len,
                config.max_match_matrix_cells,
                "exact text anchor matrix",
                "greedy exact-anchor fallback",
            )),
        };
    }

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
    ExactMatchResult {
        matches,
        diagnostic: None,
    }
}

fn greedy_exact_text_matches(old_texts: &[String], new_texts: &[String]) -> Vec<(usize, usize)> {
    let mut new_indices_by_text = BTreeMap::<&str, Vec<usize>>::new();
    for (new_index, text) in new_texts.iter().enumerate() {
        if !text.is_empty() {
            new_indices_by_text
                .entry(text.as_str())
                .or_default()
                .push(new_index);
        }
    }

    let mut matches = Vec::new();
    let mut last_new_index = None;
    for (old_index, text) in old_texts.iter().enumerate() {
        let Some(new_indices) = new_indices_by_text.get(text.as_str()) else {
            continue;
        };
        let lower_bound = last_new_index.map_or(0, |index| index + 1);
        let position = new_indices.partition_point(|index| *index < lower_bound);
        let Some(new_index) = new_indices.get(position).copied() else {
            continue;
        };
        matches.push((old_index, new_index));
        last_new_index = Some(new_index);
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
    let matches = fuzzy_node_matches(old, new, old_range.clone(), new_range.clone(), config);
    if let Some(diagnostic) = matches.diagnostic {
        document.diagnostics.push(diagnostic);
    }
    let mut old_cursor = old_range.start;
    let mut new_cursor = new_range.start;
    for fuzzy_match in matches.matches {
        emit_unpaired_changes(
            old,
            new,
            old_cursor..fuzzy_match.old_index,
            new_cursor..fuzzy_match.new_index,
            document,
            classifier,
        );

        let old_node = &old.nodes[fuzzy_match.old_index];
        let new_node = &new.nodes[fuzzy_match.new_index];
        let text_hunks = text_hunks_for_nodes(old_node, new_node, config);
        let reason = modified_reason_with_hunks(old_node, new_node, config, &text_hunks);
        push_change_with_confidence(
            document,
            ChangeKind::Modified,
            Some(old_node),
            Some(new_node),
            ChangeDetails {
                reason: format!("{reason}; fuzzy_match_score={:.3}", fuzzy_match.score),
                confidence: fuzzy_match.score,
                text_hunks,
                layout_diff: layout_diff_if_bbox_changed(
                    old_node,
                    new_node,
                    fuzzy_match.old_index,
                    fuzzy_match.new_index,
                    config.layout_tolerance_pt,
                ),
            },
            classifier,
        );
        old_cursor = fuzzy_match.old_index + 1;
        new_cursor = fuzzy_match.new_index + 1;
    }

    emit_unpaired_changes(
        old,
        new,
        old_cursor..old_range.end,
        new_cursor..new_range.end,
        document,
        classifier,
    );
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct FuzzyMatch {
    old_index: usize,
    new_index: usize,
    score: f32,
}

#[derive(Debug, Clone, PartialEq)]
struct FuzzyMatchResult {
    matches: Vec<FuzzyMatch>,
    diagnostic: Option<Diagnostic>,
}

fn fuzzy_node_matches(
    old: &SemanticDocument,
    new: &SemanticDocument,
    old_range: std::ops::Range<usize>,
    new_range: std::ops::Range<usize>,
    config: DiffConfig,
) -> FuzzyMatchResult {
    let old_len = old_range.len();
    let new_len = new_range.len();
    if matrix_cell_count_exceeds_limit(old_len, new_len, config.max_match_matrix_cells) {
        return FuzzyMatchResult {
            matches: greedy_fuzzy_node_matches(old, new, old_range, new_range, config),
            diagnostic: Some(match_limit_diagnostic(
                "FUZZY_MATCH_LIMIT_EXCEEDED",
                old_len,
                new_len,
                config.max_match_matrix_cells,
                "fuzzy node match matrix",
                "bounded greedy fuzzy-match fallback",
            )),
        };
    }

    let mut scores = vec![vec![0.0f32; new_len]; old_len];
    for (old_offset, row) in scores.iter_mut().enumerate() {
        for (new_offset, score) in row.iter_mut().enumerate() {
            *score = fuzzy_match_score(
                &old.nodes[old_range.start + old_offset],
                &new.nodes[new_range.start + new_offset],
                config,
            );
        }
    }

    let mut best = vec![vec![0.0f32; new_len + 1]; old_len + 1];
    for old_offset in (0..old_len).rev() {
        for new_offset in (0..new_len).rev() {
            let match_score = if scores[old_offset][new_offset] >= config.min_match_score {
                scores[old_offset][new_offset] + best[old_offset + 1][new_offset + 1]
            } else {
                -1.0
            };
            best[old_offset][new_offset] = match_score
                .max(best[old_offset + 1][new_offset])
                .max(best[old_offset][new_offset + 1]);
        }
    }

    let mut matches = Vec::new();
    let mut old_offset = 0;
    let mut new_offset = 0;
    while old_offset < old_len && new_offset < new_len {
        let score = scores[old_offset][new_offset];
        let match_score = score + best[old_offset + 1][new_offset + 1];
        if score >= config.min_match_score
            && approximately_equal(best[old_offset][new_offset], match_score)
        {
            matches.push(FuzzyMatch {
                old_index: old_range.start + old_offset,
                new_index: new_range.start + new_offset,
                score,
            });
            old_offset += 1;
            new_offset += 1;
        } else if best[old_offset + 1][new_offset] >= best[old_offset][new_offset + 1] {
            old_offset += 1;
        } else {
            new_offset += 1;
        }
    }
    FuzzyMatchResult {
        matches,
        diagnostic: None,
    }
}

fn greedy_fuzzy_node_matches(
    old: &SemanticDocument,
    new: &SemanticDocument,
    old_range: std::ops::Range<usize>,
    new_range: std::ops::Range<usize>,
    config: DiffConfig,
) -> Vec<FuzzyMatch> {
    let mut matches = Vec::new();
    let mut new_cursor = new_range.start;
    let candidate_window = config.max_greedy_match_candidates.max(1);

    for old_index in old_range {
        if new_cursor >= new_range.end {
            break;
        }
        let scan_end = new_cursor
            .saturating_add(candidate_window)
            .min(new_range.end);
        let mut best_match = None;
        for new_index in new_cursor..scan_end {
            let score = fuzzy_match_score(&old.nodes[old_index], &new.nodes[new_index], config);
            if score < config.min_match_score {
                continue;
            }
            let should_replace = best_match
                .as_ref()
                .is_none_or(|current: &FuzzyMatch| score > current.score);
            if should_replace {
                best_match = Some(FuzzyMatch {
                    old_index,
                    new_index,
                    score,
                });
                if approximately_equal(score, 1.0) {
                    break;
                }
            }
        }
        if let Some(fuzzy_match) = best_match {
            new_cursor = fuzzy_match.new_index + 1;
            matches.push(fuzzy_match);
        }
    }
    matches
}

fn matrix_cell_count_exceeds_limit(old_len: usize, new_len: usize, limit: usize) -> bool {
    old_len
        .checked_mul(new_len)
        .is_none_or(|cell_count| cell_count > limit)
}

fn match_limit_diagnostic(
    code: &'static str,
    old_len: usize,
    new_len: usize,
    limit: usize,
    matrix_name: &'static str,
    fallback_name: &'static str,
) -> Diagnostic {
    let cell_count = old_len
        .checked_mul(new_len)
        .map_or("overflow".to_owned(), |count| count.to_string());
    Diagnostic::warning(
        code,
        format!(
            "{matrix_name} requires {cell_count} cells for {old_len} old nodes and {new_len} new nodes, exceeding limit {limit}; using {fallback_name}"
        ),
    )
}

fn approximately_equal(left: f32, right: f32) -> bool {
    (left - right).abs() <= f32::EPSILON * 16.0
}

fn fuzzy_match_score(old_node: &SemanticNode, new_node: &SemanticNode, config: DiffConfig) -> f32 {
    if old_node.page_index != new_node.page_index {
        return 0.0;
    }
    let old_text = comparable_text(old_node, config);
    let new_text = comparable_text(new_node, config);
    let old_tokens = normalized_tokens(
        old_node.normalized_text.as_deref().unwrap_or_default(),
        config,
    );
    let new_tokens = normalized_tokens(
        new_node.normalized_text.as_deref().unwrap_or_default(),
        config,
    );
    let score = token_similarity(&token_texts(&old_tokens), &token_texts(&new_tokens));
    if is_text_extension(&old_text, &new_text) {
        score.max(0.85)
    } else {
        score
    }
}

fn is_text_extension(left: &str, right: &str) -> bool {
    if left.is_empty() || right.is_empty() || left == right {
        return false;
    }
    left.starts_with(right) || right.starts_with(left)
}

fn token_similarity(old_tokens: &[String], new_tokens: &[String]) -> f32 {
    if old_tokens.is_empty() && new_tokens.is_empty() {
        return 1.0;
    }
    if old_tokens.is_empty() || new_tokens.is_empty() {
        return 0.0;
    }
    let common = token_lcs_len(old_tokens, new_tokens);
    let score = (2.0 * common as f32) / (old_tokens.len() + new_tokens.len()) as f32;
    if old_tokens.len() == new_tokens.len() && common + 1 == old_tokens.len() {
        score.max(0.8)
    } else {
        score
    }
}

fn token_lcs_len(left: &[String], right: &[String]) -> usize {
    let mut lengths = vec![vec![0usize; right.len() + 1]; left.len() + 1];
    for left_index in (0..left.len()).rev() {
        for right_index in (0..right.len()).rev() {
            lengths[left_index][right_index] = if left[left_index] == right[right_index] {
                lengths[left_index + 1][right_index + 1] + 1
            } else {
                lengths[left_index + 1][right_index].max(lengths[left_index][right_index + 1])
            };
        }
    }
    lengths[0][0]
}

fn emit_unpaired_changes(
    old: &SemanticDocument,
    new: &SemanticDocument,
    old_range: std::ops::Range<usize>,
    new_range: std::ops::Range<usize>,
    document: &mut DiffDocument,
    classifier: &impl SeverityClassifier,
) {
    for old_index in old_range {
        push_change(
            document,
            ChangeKind::Deleted,
            Some(&old.nodes[old_index]),
            None,
            "paragraph exists only in old document",
            classifier,
        );
    }

    for new_index in new_range {
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
    push_change_with_confidence(
        document,
        kind,
        old_node,
        new_node,
        ChangeDetails {
            reason: reason.to_owned(),
            confidence: 0.9,
            text_hunks: Vec::new(),
            layout_diff: None,
        },
        classifier,
    );
}

struct ChangeDetails {
    reason: String,
    confidence: f32,
    text_hunks: Vec<TextHunk>,
    layout_diff: Option<LayoutDiff>,
}

fn push_change_with_confidence(
    document: &mut DiffDocument,
    kind: ChangeKind,
    old_node: Option<&SemanticNode>,
    new_node: Option<&SemanticNode>,
    details: ChangeDetails,
    classifier: &impl SeverityClassifier,
) {
    let kind_for_summary = kind.clone();
    let mut change = SemanticChange {
        id: format!("change-{:04}", document.changes.len()),
        kind,
        severity: ChangeSeverity::Info,
        old_node: old_node.map(to_evidence),
        new_node: new_node.map(to_evidence),
        text_hunks: details.text_hunks,
        layout_diff: details.layout_diff,
        confidence: details.confidence,
        reason: details.reason,
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

fn emit_layout_change_if_needed(
    old_node: &SemanticNode,
    new_node: &SemanticNode,
    old_order: usize,
    new_order: usize,
    config: DiffConfig,
    document: &mut DiffDocument,
    classifier: &impl SeverityClassifier,
) {
    if !layout_changed(old_node, new_node, config.layout_tolerance_pt) {
        return;
    }
    push_change_with_confidence(
        document,
        ChangeKind::LayoutChanged,
        Some(old_node),
        Some(new_node),
        ChangeDetails {
            reason: "paragraph text is unchanged but page or bounding box moved beyond tolerance"
                .to_owned(),
            confidence: 0.9,
            text_hunks: Vec::new(),
            layout_diff: Some(layout_diff_for_nodes(
                old_node, new_node, old_order, new_order,
            )),
        },
        classifier,
    );
}

fn layout_changed(old_node: &SemanticNode, new_node: &SemanticNode, tolerance: f32) -> bool {
    if old_node.page_index != new_node.page_index {
        return true;
    }
    match (old_node.bbox, new_node.bbox) {
        (Some(old_bbox), Some(new_bbox)) => {
            (old_bbox.x0 - new_bbox.x0).abs() > tolerance
                || (old_bbox.y0 - new_bbox.y0).abs() > tolerance
                || (old_bbox.x1 - new_bbox.x1).abs() > tolerance
                || (old_bbox.y1 - new_bbox.y1).abs() > tolerance
        }
        (Some(_), None) | (None, Some(_)) => true,
        (None, None) => false,
    }
}

fn layout_diff_if_bbox_changed(
    old_node: &SemanticNode,
    new_node: &SemanticNode,
    old_order: usize,
    new_order: usize,
    tolerance: f32,
) -> Option<LayoutDiff> {
    layout_changed(old_node, new_node, tolerance)
        .then(|| layout_diff_for_nodes(old_node, new_node, old_order, new_order))
}

fn layout_diff_for_nodes(
    old_node: &SemanticNode,
    new_node: &SemanticNode,
    old_order: usize,
    new_order: usize,
) -> LayoutDiff {
    layout_diff_from_parts(
        old_node.bbox,
        new_node.bbox,
        old_node.page_index,
        new_node.page_index,
        old_order != new_order,
    )
}

fn layout_diff_for_evidence(
    old_node: Option<&SemanticNodeEvidence>,
    new_node: Option<&SemanticNodeEvidence>,
    reading_order_changed: bool,
) -> LayoutDiff {
    layout_diff_from_parts(
        old_node.and_then(|node| node.bbox),
        new_node.and_then(|node| node.bbox),
        old_node.map_or(0, |node| node.page),
        new_node.map_or(0, |node| node.page),
        reading_order_changed,
    )
}

fn layout_diff_from_parts(
    old_bbox: Option<Rect>,
    new_bbox: Option<Rect>,
    old_page: usize,
    new_page: usize,
    reading_order_changed: bool,
) -> LayoutDiff {
    let (delta_x, delta_y, delta_width, delta_height) =
        if let (Some(old_bbox), Some(new_bbox)) = (old_bbox, new_bbox) {
            (
                Some(new_bbox.x0 - old_bbox.x0),
                Some(new_bbox.y0 - old_bbox.y0),
                Some(new_bbox.width() - old_bbox.width()),
                Some(new_bbox.height() - old_bbox.height()),
            )
        } else {
            (None, None, None, None)
        };
    LayoutDiff {
        old_bbox,
        new_bbox,
        delta_x,
        delta_y,
        delta_width,
        delta_height,
        page_changed: old_page != new_page,
        reading_order_changed,
    }
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

fn modified_reason_with_hunks(
    old_node: &SemanticNode,
    new_node: &SemanticNode,
    config: DiffConfig,
    text_hunks: &[TextHunk],
) -> String {
    let old_text = old_node.normalized_text.as_deref().unwrap_or_default();
    let new_text = new_node.normalized_text.as_deref().unwrap_or_default();
    let old_tokens = normalized_tokens(old_text, config);
    let new_tokens = normalized_tokens(new_text, config);

    if token_texts(&old_tokens) == token_texts(&new_tokens) {
        return "paragraph text differs between exact-match anchors".into();
    }

    let first_change = text_hunks
        .iter()
        .find(|hunk| hunk.kind != TextHunkKind::Equal);
    if let Some(hunk) = first_change {
        return format!(
            "paragraph text differs between exact-match anchors (old: \"{}\" -> new: \"{}\")",
            hunk.old_text.as_deref().unwrap_or_default(),
            hunk.new_text.as_deref().unwrap_or_default()
        );
    }

    "paragraph text differs between exact-match anchors".into()
}

fn text_hunks_for_nodes(
    old_node: &SemanticNode,
    new_node: &SemanticNode,
    config: DiffConfig,
) -> Vec<TextHunk> {
    let old_tokens = normalized_tokens(
        old_node.normalized_text.as_deref().unwrap_or_default(),
        config,
    );
    let new_tokens = normalized_tokens(
        new_node.normalized_text.as_deref().unwrap_or_default(),
        config,
    );
    text_hunks_from_tokens(&old_tokens, &new_tokens)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TextToken {
    text: String,
    range: TextRange,
}

fn text_hunks_from_tokens(old_tokens: &[TextToken], new_tokens: &[TextToken]) -> Vec<TextHunk> {
    let mut lengths = vec![vec![0usize; new_tokens.len() + 1]; old_tokens.len() + 1];
    for old_index in (0..old_tokens.len()).rev() {
        for new_index in (0..new_tokens.len()).rev() {
            lengths[old_index][new_index] =
                if old_tokens[old_index].text == new_tokens[new_index].text {
                    lengths[old_index + 1][new_index + 1] + 1
                } else {
                    lengths[old_index + 1][new_index].max(lengths[old_index][new_index + 1])
                };
        }
    }

    let mut hunks = Vec::new();
    let mut deleted = Vec::new();
    let mut inserted = Vec::new();
    let mut old_index = 0;
    let mut new_index = 0;
    while old_index < old_tokens.len() || new_index < new_tokens.len() {
        if old_index < old_tokens.len()
            && new_index < new_tokens.len()
            && old_tokens[old_index].text == new_tokens[new_index].text
        {
            flush_change_hunk(&mut hunks, &mut deleted, &mut inserted);
            push_or_merge_hunk(
                &mut hunks,
                TextHunkKind::Equal,
                TextHunkGranularity::Token,
                Some(old_tokens[old_index].range),
                Some(new_tokens[new_index].range),
                Some(old_tokens[old_index].text.clone()),
                Some(new_tokens[new_index].text.clone()),
            );
            old_index += 1;
            new_index += 1;
        } else if new_index >= new_tokens.len()
            || (old_index < old_tokens.len()
                && lengths[old_index + 1][new_index] >= lengths[old_index][new_index + 1])
        {
            deleted.push(old_tokens[old_index].clone());
            old_index += 1;
        } else {
            inserted.push(new_tokens[new_index].clone());
            new_index += 1;
        }
    }
    flush_change_hunk(&mut hunks, &mut deleted, &mut inserted);
    hunks
}

fn token_texts(tokens: &[TextToken]) -> Vec<String> {
    tokens.iter().map(|token| token.text.clone()).collect()
}

fn flush_change_hunk(
    hunks: &mut Vec<TextHunk>,
    deleted: &mut Vec<TextToken>,
    inserted: &mut Vec<TextToken>,
) {
    if deleted.is_empty() && inserted.is_empty() {
        return;
    }
    if let Some(character_hunks) = character_hunks_for_small_replacement(deleted, inserted) {
        for hunk in character_hunks {
            push_or_merge_existing_hunk(hunks, hunk);
        }
        deleted.clear();
        inserted.clear();
        return;
    }

    let kind = match (deleted.is_empty(), inserted.is_empty()) {
        (false, false) => TextHunkKind::Replaced,
        (false, true) => TextHunkKind::Deleted,
        (true, false) => TextHunkKind::Inserted,
        (true, true) => unreachable!(),
    };
    push_or_merge_hunk(
        hunks,
        kind,
        TextHunkGranularity::Token,
        range_for_tokens(deleted),
        range_for_tokens(inserted),
        (!deleted.is_empty()).then(|| tokens_to_text(deleted, TextHunkGranularity::Token)),
        (!inserted.is_empty()).then(|| tokens_to_text(inserted, TextHunkGranularity::Token)),
    );
    deleted.clear();
    inserted.clear();
}

fn push_or_merge_hunk(
    hunks: &mut Vec<TextHunk>,
    kind: TextHunkKind,
    granularity: TextHunkGranularity,
    old_range: Option<TextRange>,
    new_range: Option<TextRange>,
    old_text: Option<String>,
    new_text: Option<String>,
) {
    let hunk = TextHunk {
        kind,
        granularity: Some(granularity),
        old_range,
        new_range,
        old_text,
        new_text,
    };
    push_or_merge_existing_hunk(hunks, hunk);
}

fn push_or_merge_existing_hunk(hunks: &mut Vec<TextHunk>, hunk: TextHunk) {
    if let Some(last) = hunks.last_mut() {
        if last.kind == hunk.kind && last.granularity == hunk.granularity {
            last.old_range = merge_ranges(last.old_range, hunk.old_range);
            last.new_range = merge_ranges(last.new_range, hunk.new_range);
            let granularity = last
                .granularity
                .as_ref()
                .unwrap_or(&TextHunkGranularity::Token);
            append_optional_text(&mut last.old_text, hunk.old_text, granularity);
            append_optional_text(&mut last.new_text, hunk.new_text, granularity);
            return;
        }
    }
    hunks.push(hunk);
}

fn append_optional_text(
    target: &mut Option<String>,
    addition: Option<String>,
    granularity: &TextHunkGranularity,
) {
    let Some(addition) = addition else {
        return;
    };
    match target {
        Some(target) if !target.is_empty() => {
            append_text_fragment(target, &addition, granularity);
        }
        Some(target) => target.push_str(&addition),
        None => *target = Some(addition),
    }
}

fn normalized_tokens(text: &str, config: DiffConfig) -> Vec<TextToken> {
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
    tokenize_normalized_text(&normalized)
}

fn character_hunks_for_small_replacement(
    deleted: &[TextToken],
    inserted: &[TextToken],
) -> Option<Vec<TextHunk>> {
    let old = deleted.first()?;
    let new = inserted.first()?;
    if deleted.len() != 1
        || inserted.len() != 1
        || old.text.chars().count() > 32
        || new.text.chars().count() > 32
        || !is_character_fallback_candidate(&old.text, &new.text)
    {
        return None;
    }

    let old_chars = old.text.chars().collect::<Vec<_>>();
    let new_chars = new.text.chars().collect::<Vec<_>>();
    let mut lengths = vec![vec![0usize; new_chars.len() + 1]; old_chars.len() + 1];
    for old_index in (0..old_chars.len()).rev() {
        for new_index in (0..new_chars.len()).rev() {
            lengths[old_index][new_index] = if old_chars[old_index] == new_chars[new_index] {
                lengths[old_index + 1][new_index + 1] + 1
            } else {
                lengths[old_index + 1][new_index].max(lengths[old_index][new_index + 1])
            };
        }
    }
    if lengths[0][0] == 0 {
        return None;
    }

    let mut hunks = Vec::new();
    let mut deleted_chars = Vec::new();
    let mut inserted_chars = Vec::new();
    let mut old_index = 0;
    let mut new_index = 0;
    while old_index < old_chars.len() || new_index < new_chars.len() {
        if old_index < old_chars.len()
            && new_index < new_chars.len()
            && old_chars[old_index] == new_chars[new_index]
        {
            flush_character_change_hunk(
                &mut hunks,
                &mut deleted_chars,
                &mut inserted_chars,
                old.range.start,
                new.range.start,
            );
            push_or_merge_hunk(
                &mut hunks,
                TextHunkKind::Equal,
                TextHunkGranularity::Character,
                Some(TextRange::new(
                    old.range.start + old_index,
                    old.range.start + old_index + 1,
                )),
                Some(TextRange::new(
                    new.range.start + new_index,
                    new.range.start + new_index + 1,
                )),
                Some(old_chars[old_index].to_string()),
                Some(new_chars[new_index].to_string()),
            );
            old_index += 1;
            new_index += 1;
        } else if new_index >= new_chars.len()
            || (old_index < old_chars.len()
                && lengths[old_index + 1][new_index] >= lengths[old_index][new_index + 1])
        {
            deleted_chars.push((old_index, old_chars[old_index]));
            old_index += 1;
        } else {
            inserted_chars.push((new_index, new_chars[new_index]));
            new_index += 1;
        }
    }
    flush_character_change_hunk(
        &mut hunks,
        &mut deleted_chars,
        &mut inserted_chars,
        old.range.start,
        new.range.start,
    );
    Some(hunks)
}

fn is_character_fallback_candidate(old: &str, new: &str) -> bool {
    let old_is_number = old.chars().all(|character| character.is_ascii_digit());
    let new_is_number = new.chars().all(|character| character.is_ascii_digit());
    !old_is_number
        && !new_is_number
        && old.chars().any(char::is_alphabetic)
        && new.chars().any(char::is_alphabetic)
}

fn flush_character_change_hunk(
    hunks: &mut Vec<TextHunk>,
    deleted: &mut Vec<(usize, char)>,
    inserted: &mut Vec<(usize, char)>,
    old_base: usize,
    new_base: usize,
) {
    if deleted.is_empty() && inserted.is_empty() {
        return;
    }
    let kind = match (deleted.is_empty(), inserted.is_empty()) {
        (false, false) => TextHunkKind::Replaced,
        (false, true) => TextHunkKind::Deleted,
        (true, false) => TextHunkKind::Inserted,
        (true, true) => unreachable!(),
    };
    let old_text = (!deleted.is_empty()).then(|| deleted.iter().map(|(_, c)| c).collect());
    let new_text = (!inserted.is_empty()).then(|| inserted.iter().map(|(_, c)| c).collect());
    push_or_merge_hunk(
        hunks,
        kind,
        TextHunkGranularity::Character,
        character_range(deleted, old_base),
        character_range(inserted, new_base),
        old_text,
        new_text,
    );
    deleted.clear();
    inserted.clear();
}

fn character_range(characters: &[(usize, char)], base: usize) -> Option<TextRange> {
    let start = characters.first()?.0;
    let end = characters.last()?.0 + 1;
    Some(TextRange::new(base + start, base + end))
}

fn append_text_fragment(target: &mut String, addition: &str, granularity: &TextHunkGranularity) {
    if *granularity == TextHunkGranularity::Character || is_punctuation_fragment(addition) {
        target.push_str(addition);
    } else {
        target.push(' ');
        target.push_str(addition);
    }
}

fn merge_ranges(left: Option<TextRange>, right: Option<TextRange>) -> Option<TextRange> {
    match (left, right) {
        (Some(left), Some(right)) => Some(TextRange::new(
            left.start.min(right.start),
            left.end.max(right.end),
        )),
        (Some(range), None) | (None, Some(range)) => Some(range),
        (None, None) => None,
    }
}

fn range_for_tokens(tokens: &[TextToken]) -> Option<TextRange> {
    Some(TextRange::new(
        tokens.first()?.range.start,
        tokens.last()?.range.end,
    ))
}

fn tokens_to_text(tokens: &[TextToken], granularity: TextHunkGranularity) -> String {
    let mut text = String::new();
    for token in tokens {
        if text.is_empty() {
            text.push_str(&token.text);
        } else {
            append_text_fragment(&mut text, &token.text, &granularity);
        }
    }
    text
}

fn is_punctuation_fragment(fragment: &str) -> bool {
    let mut chars = fragment.chars();
    matches!(
        chars.next(),
        Some(character) if chars.next().is_none() && character.is_ascii_punctuation()
    )
}

fn tokenize_normalized_text(text: &str) -> Vec<TextToken> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut current_kind = None;
    let mut current_start = 0usize;
    let mut char_index = 0usize;
    for character in text.chars() {
        if character.is_whitespace() {
            flush_token(
                &mut tokens,
                &mut current,
                &mut current_kind,
                current_start,
                char_index,
            );
            char_index += 1;
            continue;
        }

        let kind = token_kind(character);
        if current_kind == Some(kind) {
            current.push(character);
        } else {
            flush_token(
                &mut tokens,
                &mut current,
                &mut current_kind,
                current_start,
                char_index,
            );
            current_start = char_index;
            current_kind = Some(kind);
            current.push(character);
        }
        char_index += 1;
    }
    flush_token(
        &mut tokens,
        &mut current,
        &mut current_kind,
        current_start,
        char_index,
    );
    tokens
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenKind {
    Word,
    Punctuation,
}

fn token_kind(character: char) -> TokenKind {
    if character.is_alphanumeric() {
        TokenKind::Word
    } else {
        TokenKind::Punctuation
    }
}

fn flush_token(
    tokens: &mut Vec<TextToken>,
    current: &mut String,
    current_kind: &mut Option<TokenKind>,
    start: usize,
    end: usize,
) {
    if !current.is_empty() {
        tokens.push(TextToken {
            text: std::mem::take(current),
            range: TextRange::new(start, end),
        });
    }
    *current_kind = None;
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
        change.layout_diff = Some(layout_diff_for_evidence(
            change.old_node.as_ref(),
            change.new_node.as_ref(),
            true,
        ));
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
    use pdf_semantic::{SemanticAnchor, SemanticNode, SemanticNodeKind};
    use spdfdiff_types::Provenance;

    #[test]
    fn detects_modified_paragraph() {
        let old = document_with_text("old", "Hello");
        let new = document_with_text("new", "Hello world");

        let diff = diff_semantic_documents(&old, &new, DiffConfig::default());

        assert_eq!(diff.summary.modified, 1);
        assert_eq!(diff.changes[0].id, "change-0000");
        assert_eq!(diff.changes[0].kind, ChangeKind::Modified);
        assert!(
            diff.changes[0]
                .reason
                .contains("old: \"\" -> new: \"world\"")
        );
        assert_eq!(diff.changes[0].text_hunks.len(), 2);
        assert_eq!(diff.changes[0].text_hunks[1].kind, TextHunkKind::Inserted);
        assert_eq!(
            diff.changes[0].text_hunks[1].new_text.as_deref(),
            Some("world")
        );
        assert!(diff.changes[0].reason.contains("fuzzy_match_score=0.850"));
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
    fn fuzzy_matches_edited_paragraph_between_exact_anchors() {
        let old = document_with_texts("old", &["Alpha", "Payment is due in 30 days", "Omega"]);
        let new = document_with_texts("new", &["Alpha", "Payment is due in 15 days", "Omega"]);

        let diff = diff_semantic_documents(&old, &new, DiffConfig::default());

        assert_eq!(diff.summary.modified, 1);
        assert_eq!(diff.summary.inserted, 0);
        assert_eq!(diff.summary.deleted, 0);
        assert_eq!(diff.changes[0].kind, ChangeKind::Modified);
        assert!((diff.changes[0].confidence - 0.833).abs() < 0.001);
        assert!(diff.changes[0].reason.contains("fuzzy_match_score=0.833"));
        assert!(
            diff.changes[0]
                .text_hunks
                .iter()
                .any(|hunk| hunk.kind == TextHunkKind::Replaced
                    && hunk.old_text.as_deref() == Some("30")
                    && hunk.new_text.as_deref() == Some("15"))
        );
        assert_eq!(
            diff.changes[0].old_node.as_ref().unwrap().text.as_deref(),
            Some("Payment is due in 30 days")
        );
        assert_eq!(
            diff.changes[0].new_node.as_ref().unwrap().text.as_deref(),
            Some("Payment is due in 15 days")
        );
    }

    #[test]
    fn small_alphabetic_replacements_include_character_hunks() {
        let old = document_with_texts("old", &["Alpha", "The color changed", "Omega"]);
        let new = document_with_texts("new", &["Alpha", "The colour changed", "Omega"]);

        let diff = diff_semantic_documents(&old, &new, DiffConfig::default());

        assert_eq!(diff.summary.modified, 1);
        assert!(
            diff.changes[0].text_hunks.iter().any(|hunk| {
                hunk.kind == TextHunkKind::Inserted
                    && hunk.granularity == Some(TextHunkGranularity::Character)
                    && hunk.old_range.is_none()
                    && hunk.new_range == Some(TextRange::new(8, 9))
                    && hunk.new_text.as_deref() == Some("u")
            }),
            "expected inserted character hunk in {:?}",
            diff.changes[0].text_hunks
        );
    }

    #[test]
    fn numeric_replacements_remain_token_hunks() {
        let old = document_with_texts("old", &["Alpha", "Payment is due in 30 days", "Omega"]);
        let new = document_with_texts("new", &["Alpha", "Payment is due in 15 days", "Omega"]);

        let diff = diff_semantic_documents(&old, &new, DiffConfig::default());

        assert!(
            diff.changes[0].text_hunks.iter().any(|hunk| {
                hunk.kind == TextHunkKind::Replaced
                    && hunk.granularity == Some(TextHunkGranularity::Token)
                    && hunk.old_text.as_deref() == Some("30")
                    && hunk.new_text.as_deref() == Some("15")
            }),
            "expected numeric token replacement hunk in {:?}",
            diff.changes[0].text_hunks
        );
    }

    #[test]
    fn leaves_low_confidence_unmatched_blocks_as_delete_insert() {
        let old = document_with_texts("old", &["Alpha", "Cat dog", "Omega"]);
        let new = document_with_texts("new", &["Alpha", "Invoice total", "Omega"]);

        let diff = diff_semantic_documents(&old, &new, DiffConfig::default());

        assert_eq!(diff.summary.modified, 0);
        assert_eq!(diff.summary.deleted, 1);
        assert_eq!(diff.summary.inserted, 1);
        assert_eq!(diff.changes[0].kind, ChangeKind::Deleted);
        assert_eq!(diff.changes[1].kind, ChangeKind::Inserted);
    }

    #[test]
    fn fuzzy_matching_respects_minimum_score() {
        let old = document_with_texts("old", &["Alpha", "Payment is due in 30 days", "Omega"]);
        let new = document_with_texts("new", &["Alpha", "Payment is due in 15 days", "Omega"]);
        let config = DiffConfig {
            min_match_score: 0.95,
            ..DiffConfig::default()
        };

        let diff = diff_semantic_documents(&old, &new, config);

        assert_eq!(diff.summary.modified, 0);
        assert_eq!(diff.summary.deleted, 1);
        assert_eq!(diff.summary.inserted, 1);
    }

    #[test]
    fn exact_matching_falls_back_when_matrix_limit_is_exceeded() {
        let old = document_with_texts("old", &["Alpha", "Beta", "Gamma"]);
        let new = document_with_texts("new", &["Inserted before", "Alpha", "Beta", "Gamma"]);
        let config = DiffConfig {
            max_match_matrix_cells: 1,
            ..DiffConfig::default()
        };

        let diff = diff_semantic_documents(&old, &new, config);

        assert_eq!(diff.summary.inserted, 1);
        assert_eq!(diff.summary.deleted, 0);
        assert_eq!(diff.summary.modified, 0);
        assert!(
            diff.diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "EXACT_MATCH_LIMIT_EXCEEDED")
        );
    }

    #[test]
    fn fuzzy_matching_falls_back_when_matrix_limit_is_exceeded() {
        let old = document_with_texts("old", &["Alpha", "Payment is due in 30 days", "Omega"]);
        let new = document_with_texts("new", &["Alpha", "Payment is due in 15 days", "Omega"]);
        let config = DiffConfig {
            max_match_matrix_cells: 0,
            ..DiffConfig::default()
        };

        let diff = diff_semantic_documents(&old, &new, config);

        assert_eq!(diff.summary.modified, 1);
        assert_eq!(diff.summary.inserted, 0);
        assert_eq!(diff.summary.deleted, 0);
        assert!(
            diff.diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "FUZZY_MATCH_LIMIT_EXCEEDED")
        );
    }

    #[test]
    fn bounded_matching_handles_many_unmatched_nodes_without_large_matrices() {
        let old_texts = (0..600)
            .map(|index| format!("Old paragraph {index} alpha beta"))
            .collect::<Vec<_>>();
        let new_texts = (0..600)
            .map(|index| format!("New paragraph {index} gamma delta"))
            .collect::<Vec<_>>();
        let old = document_with_owned_texts("old", old_texts);
        let new = document_with_owned_texts("new", new_texts);
        let config = DiffConfig {
            max_match_matrix_cells: 64,
            max_greedy_match_candidates: 8,
            ..DiffConfig::default()
        };

        let diff = diff_semantic_documents(&old, &new, config);

        assert_eq!(diff.summary.modified, 0);
        assert_eq!(diff.summary.deleted, 600);
        assert_eq!(diff.summary.inserted, 600);
        assert!(
            diff.diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "EXACT_MATCH_LIMIT_EXCEEDED")
        );
        assert!(
            diff.diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "FUZZY_MATCH_LIMIT_EXCEEDED")
        );
    }

    #[test]
    fn detects_moved_paragraph_from_insert_delete_pair() {
        let old = document_with_positioned_texts(
            "old",
            &[
                ("Alpha", 0.0, 10.0),
                ("Beta", 0.0, 20.0),
                ("Gamma", 0.0, 0.0),
            ],
        );
        let new = document_with_positioned_texts(
            "new",
            &[
                ("Beta", 0.0, 20.0),
                ("Alpha", 40.0, 10.0),
                ("Gamma", 0.0, 0.0),
            ],
        );

        let diff = diff_semantic_documents(&old, &new, DiffConfig::default());

        assert_eq!(diff.summary.moved, 1);
        assert_eq!(diff.summary.inserted, 0);
        assert_eq!(diff.summary.deleted, 0);
        assert_eq!(diff.changes.len(), 1);
        assert_eq!(diff.changes[0].kind, ChangeKind::Moved);
        let layout_diff = diff.changes[0]
            .layout_diff
            .as_ref()
            .expect("moved content should keep layout evidence");
        assert_eq!(layout_diff.delta_x, Some(40.0));
        assert_eq!(layout_diff.delta_y, Some(0.0));
        assert!(layout_diff.reading_order_changed);
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

    #[test]
    fn detects_layout_change_for_exact_text_match() {
        let old = document_with_positioned_text("old", "Alpha", 0.0, 0.0);
        let new = document_with_positioned_text("new", "Alpha", 12.0, 0.0);

        let diff = diff_semantic_documents(&old, &new, DiffConfig::default());

        assert_eq!(diff.summary.layout_changed, 1);
        assert_eq!(diff.summary.modified, 0);
        assert_eq!(diff.changes[0].kind, ChangeKind::LayoutChanged);
        assert_eq!(diff.changes[0].severity, ChangeSeverity::Minor);
        let layout_diff = diff.changes[0]
            .layout_diff
            .as_ref()
            .expect("layout-only change should include structured evidence");
        assert_eq!(layout_diff.delta_x, Some(12.0));
        assert_eq!(layout_diff.delta_y, Some(0.0));
        assert_eq!(layout_diff.delta_width, Some(0.0));
        assert_eq!(layout_diff.delta_height, Some(0.0));
        assert!(!layout_diff.page_changed);
        assert!(!layout_diff.reading_order_changed);
    }

    #[test]
    fn ignores_layout_change_inside_tolerance() {
        let old = document_with_positioned_text("old", "Alpha", 0.0, 0.0);
        let new = document_with_positioned_text("new", "Alpha", 1.0, 0.0);

        let diff = diff_semantic_documents(&old, &new, DiffConfig::default());

        assert_eq!(diff.summary.layout_changed, 0);
        assert_eq!(diff.changes, Vec::new());
    }

    fn document_with_text(fingerprint: &str, text: &str) -> SemanticDocument {
        document_with_texts(fingerprint, &[text])
    }

    fn document_with_texts(fingerprint: &str, texts: &[&str]) -> SemanticDocument {
        document_with_owned_texts(fingerprint, texts.iter().map(|text| (*text).to_owned()))
    }

    fn document_with_owned_texts(
        fingerprint: &str,
        texts: impl IntoIterator<Item = String>,
    ) -> SemanticDocument {
        SemanticDocument {
            fingerprint: fingerprint.into(),
            nodes: texts
                .into_iter()
                .enumerate()
                .map(|(index, text)| SemanticNode {
                    id: format!("n{index:04}"),
                    kind: SemanticNodeKind::Paragraph,
                    page_index: 0,
                    bbox: None,
                    normalized_text: Some(text),
                    table: None,
                    anchor: SemanticAnchor::unknown(),
                    source: vec![Provenance::unknown()],
                    confidence: 1.0,
                })
                .collect(),
            diagnostics: Vec::new(),
            tagged_structure: None,
        }
    }

    fn document_with_positioned_text(
        fingerprint: &str,
        text: &str,
        x: f32,
        y: f32,
    ) -> SemanticDocument {
        let mut document = document_with_text(fingerprint, text);
        document.nodes[0].bbox = Some(spdfdiff_types::Rect {
            x0: x,
            y0: y,
            x1: x + 10.0,
            y1: y + 10.0,
        });
        document
    }

    fn document_with_positioned_texts(
        fingerprint: &str,
        texts: &[(&str, f32, f32)],
    ) -> SemanticDocument {
        let mut document = document_with_owned_texts(
            fingerprint,
            texts.iter().map(|(text, _, _)| *text).map(str::to_owned),
        );
        for (node, (_, x, y)) in document.nodes.iter_mut().zip(texts) {
            node.bbox = Some(spdfdiff_types::Rect {
                x0: *x,
                y0: *y,
                x1: *x + 10.0,
                y1: *y + 10.0,
            });
        }
        document
    }
}
