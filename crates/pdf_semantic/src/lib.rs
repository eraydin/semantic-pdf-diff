use pdf_text::TextRun;
use spdfdiff_types::{Diagnostic, Provenance, Rect};

const LINE_BASELINE_TOLERANCE: f32 = 3.0;
const PARAGRAPH_GAP_MULTIPLIER: f32 = 1.8;

#[derive(Debug, Clone, PartialEq)]
pub struct SemanticDocument {
    pub fingerprint: String,
    pub nodes: Vec<SemanticNode>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SemanticNode {
    pub id: String,
    pub kind: SemanticNodeKind,
    pub page_index: usize,
    pub bbox: Option<Rect>,
    pub normalized_text: Option<String>,
    pub anchor: SemanticAnchor,
    pub source: Vec<Provenance>,
    pub confidence: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticAnchor {
    pub strong_text_hash: String,
    pub weak_text_signature: String,
    pub geometry_bucket: String,
    pub heading_context: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticNodeKind {
    Page,
    HeadingCandidate,
    Paragraph,
    ListCandidate,
    TableCandidate,
    FigureCandidate,
    UnknownBlock,
}

#[derive(Debug, Clone)]
struct TextLine {
    page_index: usize,
    bbox: Rect,
    text: String,
    source: Vec<Provenance>,
}

impl TextLine {
    fn height(&self) -> f32 {
        self.bbox.height().max(1.0)
    }
}

#[must_use]
pub fn build_semantic_document(
    fingerprint: impl Into<String>,
    runs: &[TextRun],
    diagnostics: Vec<Diagnostic>,
) -> SemanticDocument {
    let lines = cluster_lines(runs);
    let mut nodes = cluster_paragraphs(&lines);
    classify_heading_candidates(&mut nodes);
    classify_table_candidates(&mut nodes);
    classify_list_candidates(&mut nodes);
    assign_semantic_anchors(&mut nodes);

    SemanticDocument {
        fingerprint: fingerprint.into(),
        nodes,
        diagnostics,
    }
}

fn cluster_lines(runs: &[TextRun]) -> Vec<TextLine> {
    let mut ordered_runs = runs
        .iter()
        .filter(|run| !run.normalized_text.is_empty())
        .collect::<Vec<_>>();
    ordered_runs.sort_by(|left, right| {
        let left_page = left.source.page_index.unwrap_or(0);
        let right_page = right.source.page_index.unwrap_or(0);
        left_page
            .cmp(&right_page)
            .then_with(|| {
                right
                    .bbox
                    .y0
                    .partial_cmp(&left.bbox.y0)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| {
                left.bbox
                    .x0
                    .partial_cmp(&right.bbox.x0)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });

    let mut lines: Vec<TextLine> = Vec::new();
    for run in ordered_runs {
        let page_index = run.source.page_index.unwrap_or(0);
        if let Some(line) = lines.iter_mut().find(|line| {
            line.page_index == page_index
                && (line.bbox.y0 - run.bbox.y0).abs() <= LINE_BASELINE_TOLERANCE
        }) {
            append_text(&mut line.text, &run.normalized_text);
            line.bbox = union_rect(line.bbox, run.bbox);
            line.source.push(run.source.clone());
        } else {
            lines.push(TextLine {
                page_index,
                bbox: run.bbox,
                text: run.normalized_text.clone(),
                source: vec![run.source.clone()],
            });
        }
    }
    lines
}

fn cluster_paragraphs(lines: &[TextLine]) -> Vec<SemanticNode> {
    let mut nodes = Vec::new();
    let mut current: Option<TextLine> = None;

    for line in lines {
        if let Some(paragraph) = &mut current {
            let vertical_gap = paragraph.bbox.y0 - line.bbox.y1;
            let gap_limit = paragraph.height().max(line.height()) * PARAGRAPH_GAP_MULTIPLIER;
            if paragraph.page_index == line.page_index
                && vertical_gap >= 0.0
                && vertical_gap <= gap_limit
            {
                append_text(&mut paragraph.text, &line.text);
                paragraph.bbox = union_rect(paragraph.bbox, line.bbox);
                paragraph.source.extend(line.source.clone());
                continue;
            }

            nodes.push(line_to_node(nodes.len(), paragraph));
        }
        current = Some(line.clone());
    }

    if let Some(paragraph) = &current {
        nodes.push(line_to_node(nodes.len(), paragraph));
    }
    nodes
}

fn line_to_node(index: usize, line: &TextLine) -> SemanticNode {
    SemanticNode {
        id: format!("n{index:04}"),
        kind: SemanticNodeKind::Paragraph,
        page_index: line.page_index,
        bbox: Some(line.bbox),
        normalized_text: Some(line.text.clone()),
        anchor: SemanticAnchor::unknown(),
        source: line.source.clone(),
        confidence: 0.7,
    }
}

impl SemanticAnchor {
    #[must_use]
    pub fn unknown() -> Self {
        Self {
            strong_text_hash: "text:0000000000000000".into(),
            weak_text_signature: "weak:0000000000000000".into(),
            geometry_bucket: "page-unknown:x-unknown:y-unknown".into(),
            heading_context: None,
        }
    }
}

fn classify_heading_candidates(nodes: &mut [SemanticNode]) {
    let mut heights = nodes
        .iter()
        .filter_map(|node| node.bbox.map(Rect::height))
        .filter(|height| *height > 0.0)
        .collect::<Vec<_>>();
    if heights.len() < 2 {
        return;
    }
    heights.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    let median_height = heights[(heights.len() - 1) / 2];

    for node in nodes {
        let Some(text) = node.normalized_text.as_deref() else {
            continue;
        };
        let Some(bbox) = node.bbox else {
            continue;
        };
        if is_heading_candidate(text, bbox.height(), median_height) {
            node.kind = SemanticNodeKind::HeadingCandidate;
            node.confidence = 0.65;
        }
    }
}

fn is_heading_candidate(text: &str, height: f32, median_height: f32) -> bool {
    let text = text.trim();
    if text.is_empty() || text.len() > 80 || text.ends_with('.') {
        return false;
    }
    let larger_than_body = height >= median_height * 1.2;
    let heading_shape = text
        .chars()
        .next()
        .is_some_and(|character| character.is_uppercase() || character.is_ascii_digit());
    larger_than_body && heading_shape
}

fn classify_table_candidates(nodes: &mut [SemanticNode]) {
    for node in nodes {
        if node.kind != SemanticNodeKind::Paragraph {
            continue;
        }
        if is_table_candidate(node) {
            node.kind = SemanticNodeKind::TableCandidate;
            node.confidence = 0.55;
        }
    }
}

fn is_table_candidate(node: &SemanticNode) -> bool {
    let Some(text) = node.normalized_text.as_deref() else {
        return false;
    };
    let tokens = text.split_whitespace().collect::<Vec<_>>();
    node.source.len() >= 4
        && tokens.len() >= 4
        && tokens.len() <= node.source.len() + 2
        && tokens.iter().all(|token| token.len() <= 16)
}

fn classify_list_candidates(nodes: &mut [SemanticNode]) {
    for node in nodes {
        if node.kind != SemanticNodeKind::Paragraph {
            continue;
        }
        let Some(text) = node.normalized_text.as_deref() else {
            continue;
        };
        if is_list_candidate(text) {
            node.kind = SemanticNodeKind::ListCandidate;
            node.confidence = 0.6;
        }
    }
}

fn is_list_candidate(text: &str) -> bool {
    let text = text.trim_start();
    is_bullet_list_marker(text) || is_numbered_list_marker(text)
}

fn is_bullet_list_marker(text: &str) -> bool {
    text.strip_prefix('-')
        .or_else(|| text.strip_prefix('*'))
        .or_else(|| text.strip_prefix('+'))
        .is_some_and(|remaining| remaining.starts_with(char::is_whitespace))
}

fn is_numbered_list_marker(text: &str) -> bool {
    let mut chars = text.char_indices();
    let mut digit_end = None;
    for (index, character) in &mut chars {
        if character.is_ascii_digit() {
            digit_end = Some(index + character.len_utf8());
        } else {
            break;
        }
    }
    let Some(digit_end) = digit_end else {
        return false;
    };
    if digit_end > 3 {
        return false;
    }
    let remaining = &text[digit_end..];
    let Some(after_marker) = remaining
        .strip_prefix('.')
        .or_else(|| remaining.strip_prefix(')'))
    else {
        return false;
    };
    after_marker.starts_with(char::is_whitespace)
}

fn assign_semantic_anchors(nodes: &mut [SemanticNode]) {
    let mut current_heading_context = None;
    for node in nodes {
        node.anchor = build_anchor(node, current_heading_context.as_deref());
        if node.kind == SemanticNodeKind::HeadingCandidate {
            current_heading_context = Some(node.anchor.strong_text_hash.clone());
        }
    }
}

fn build_anchor(node: &SemanticNode, heading_context: Option<&str>) -> SemanticAnchor {
    let text = node.normalized_text.as_deref().unwrap_or_default();
    SemanticAnchor {
        strong_text_hash: format!("text:{:016x}", stable_hash(normalize_anchor_text(text))),
        weak_text_signature: format!("weak:{:016x}", stable_hash(weak_signature_text(text))),
        geometry_bucket: geometry_bucket(node.page_index, node.bbox),
        heading_context: heading_context.map(ToOwned::to_owned),
    }
}

fn normalize_anchor_text(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn weak_signature_text(text: &str) -> String {
    let tokens = normalize_anchor_text(text)
        .split_whitespace()
        .filter(|token| token.chars().any(char::is_alphabetic))
        .take(8)
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        normalize_anchor_text(text)
    } else {
        tokens.join(" ")
    }
}

fn geometry_bucket(page_index: usize, bbox: Option<Rect>) -> String {
    let Some(bbox) = bbox else {
        return format!("page-{page_index}:x-unknown:y-unknown");
    };
    format!(
        "page-{page_index}:x-{:04}:y-{:04}",
        bucket_coordinate(bbox.x0),
        bucket_coordinate(bbox.y0)
    )
}

fn bucket_coordinate(value: f32) -> i32 {
    (value / 50.0).floor() as i32
}

fn stable_hash(text: String) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x00000100000001b3;
    let mut hash = FNV_OFFSET;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

fn append_text(target: &mut String, next: &str) {
    if target.is_empty()
        || target.ends_with(char::is_whitespace)
        || next.starts_with(char::is_whitespace)
    {
        target.push_str(next);
    } else {
        target.push(' ');
        target.push_str(next);
    }
}

fn union_rect(left: Rect, right: Rect) -> Rect {
    Rect {
        x0: left.x0.min(right.x0),
        y0: left.y0.min(right.y0),
        x1: left.x1.max(right.x1),
        y1: left.y1.max(right.y1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spdfdiff_types::{LineSegment, Point};

    #[test]
    fn turns_text_runs_into_paragraph_nodes() {
        let run = text_run("run1", "Hello", 0, rect(10.0, 20.0, 40.0, 32.0));
        let document = build_semantic_document("fixture", &[run], Vec::new());

        assert_eq!(document.nodes.len(), 1);
        assert_eq!(document.nodes[0].kind, SemanticNodeKind::Paragraph);
        assert_eq!(document.nodes[0].normalized_text.as_deref(), Some("Hello"));
        let _ = LineSegment {
            start: Point { x: 0.0, y: 0.0 },
            end: Point { x: 1.0, y: 1.0 },
        };
    }

    #[test]
    fn groups_same_line_runs_left_to_right() {
        let runs = vec![
            text_run("run2", "world", 0, rect(50.0, 20.0, 80.0, 32.0)),
            text_run("run1", "Hello", 0, rect(10.0, 20.0, 40.0, 32.0)),
        ];
        let document = build_semantic_document("fixture", &runs, Vec::new());

        assert_eq!(document.nodes.len(), 1);
        assert_eq!(
            document.nodes[0].normalized_text.as_deref(),
            Some("Hello world")
        );
        assert_eq!(document.nodes[0].source.len(), 2);
    }

    #[test]
    fn groups_multiline_paragraph_by_vertical_gap() {
        let runs = vec![
            text_run("run1", "First line", 0, rect(10.0, 100.0, 80.0, 112.0)),
            text_run("run2", "second line", 0, rect(10.0, 84.0, 90.0, 96.0)),
        ];
        let document = build_semantic_document("fixture", &runs, Vec::new());

        assert_eq!(document.nodes.len(), 1);
        assert_eq!(
            document.nodes[0].normalized_text.as_deref(),
            Some("First line second line")
        );
        assert_eq!(
            document.nodes[0].bbox.unwrap(),
            rect(10.0, 84.0, 90.0, 112.0)
        );
    }

    #[test]
    fn keeps_separate_paragraphs_when_gap_is_large() {
        let runs = vec![
            text_run(
                "run1",
                "First paragraph",
                0,
                rect(10.0, 100.0, 100.0, 112.0),
            ),
            text_run("run2", "Second paragraph", 0, rect(10.0, 40.0, 110.0, 52.0)),
        ];
        let document = build_semantic_document("fixture", &runs, Vec::new());

        assert_eq!(document.nodes.len(), 2);
        assert_eq!(
            document.nodes[0].normalized_text.as_deref(),
            Some("First paragraph")
        );
        assert_eq!(
            document.nodes[1].normalized_text.as_deref(),
            Some("Second paragraph")
        );
    }

    #[test]
    fn detects_controlled_heading_candidate() {
        let runs = vec![
            text_run("heading", "1. Scope", 0, rect(10.0, 120.0, 80.0, 140.0)),
            text_run(
                "body",
                "This paragraph explains the scope.",
                0,
                rect(10.0, 40.0, 180.0, 52.0),
            ),
        ];
        let document = build_semantic_document("fixture", &runs, Vec::new());

        assert_eq!(document.nodes.len(), 2);
        assert_eq!(document.nodes[0].kind, SemanticNodeKind::HeadingCandidate);
        assert_eq!(document.nodes[0].confidence, 0.65);
        assert_eq!(document.nodes[1].kind, SemanticNodeKind::Paragraph);
    }

    #[test]
    fn assigns_stable_semantic_anchors() {
        let runs = vec![text_run(
            "run1",
            "Payment is due in 30 days",
            0,
            rect(72.0, 120.0, 180.0, 132.0),
        )];
        let first = build_semantic_document("first", &runs, Vec::new());
        let second = build_semantic_document("second", &runs, Vec::new());

        assert_eq!(first.nodes[0].anchor, second.nodes[0].anchor);
        assert!(first.nodes[0].anchor.strong_text_hash.starts_with("text:"));
        assert!(
            first.nodes[0]
                .anchor
                .weak_text_signature
                .starts_with("weak:")
        );
        assert_eq!(
            first.nodes[0].anchor.geometry_bucket,
            "page-0:x-0001:y-0002"
        );
    }

    #[test]
    fn text_edit_changes_strong_hash_but_keeps_weak_signature() {
        let old = build_semantic_document(
            "old",
            &[text_run(
                "old",
                "Payment is due in 30 days",
                0,
                rect(72.0, 120.0, 180.0, 132.0),
            )],
            Vec::new(),
        );
        let new = build_semantic_document(
            "new",
            &[text_run(
                "new",
                "Payment is due in 15 days",
                0,
                rect(72.0, 120.0, 180.0, 132.0),
            )],
            Vec::new(),
        );

        assert_ne!(
            old.nodes[0].anchor.strong_text_hash,
            new.nodes[0].anchor.strong_text_hash
        );
        assert_eq!(
            old.nodes[0].anchor.weak_text_signature,
            new.nodes[0].anchor.weak_text_signature
        );
    }

    #[test]
    fn paragraph_anchor_keeps_heading_context() {
        let document = build_semantic_document(
            "fixture",
            &[
                text_run("heading", "1. Scope", 0, rect(10.0, 120.0, 80.0, 140.0)),
                text_run("body", "Body text", 0, rect(10.0, 40.0, 80.0, 52.0)),
            ],
            Vec::new(),
        );

        assert_eq!(document.nodes[0].anchor.heading_context, None);
        assert_eq!(
            document.nodes[1].anchor.heading_context,
            Some(document.nodes[0].anchor.strong_text_hash.clone())
        );
    }

    #[test]
    fn detects_basic_numbered_list_candidate() {
        let document = build_semantic_document(
            "fixture",
            &[
                text_run("item1", "1. First item", 0, rect(10.0, 120.0, 90.0, 132.0)),
                text_run("body", "Body paragraph.", 0, rect(10.0, 40.0, 100.0, 52.0)),
            ],
            Vec::new(),
        );

        assert_eq!(document.nodes[0].kind, SemanticNodeKind::ListCandidate);
        assert_eq!(document.nodes[0].confidence, 0.6);
        assert_eq!(document.nodes[1].kind, SemanticNodeKind::Paragraph);
    }

    #[test]
    fn detects_basic_bullet_list_candidate() {
        let document = build_semantic_document(
            "fixture",
            &[text_run(
                "item",
                "- Bullet item",
                0,
                rect(10.0, 120.0, 90.0, 132.0),
            )],
            Vec::new(),
        );

        assert_eq!(document.nodes[0].kind, SemanticNodeKind::ListCandidate);
    }

    #[test]
    fn detects_simple_text_table_candidate() {
        let runs = vec![
            text_run("a1", "A1", 0, rect(10.0, 100.0, 20.0, 112.0)),
            text_run("a2", "A2", 0, rect(70.0, 100.0, 80.0, 112.0)),
            text_run("b1", "B1", 0, rect(10.0, 84.0, 20.0, 96.0)),
            text_run("b2", "B2", 0, rect(70.0, 84.0, 80.0, 96.0)),
        ];
        let document = build_semantic_document("fixture", &runs, Vec::new());

        assert_eq!(document.nodes.len(), 1);
        assert_eq!(document.nodes[0].kind, SemanticNodeKind::TableCandidate);
        assert_eq!(document.nodes[0].confidence, 0.55);
        assert_eq!(
            document.nodes[0].normalized_text.as_deref(),
            Some("A1 A2 B1 B2")
        );
    }

    #[test]
    fn keeps_single_run_short_text_as_paragraph_not_table() {
        let document = build_semantic_document(
            "fixture",
            &[text_run(
                "run",
                "A1 A2 B1 B2",
                0,
                rect(10.0, 100.0, 120.0, 112.0),
            )],
            Vec::new(),
        );

        assert_eq!(document.nodes[0].kind, SemanticNodeKind::Paragraph);
    }

    fn text_run(id: &str, text: &str, page_index: usize, bbox: Rect) -> TextRun {
        TextRun {
            id: id.into(),
            text: text.into(),
            normalized_text: text.into(),
            glyphs: Vec::new(),
            bbox,
            source: Provenance {
                page_index: Some(page_index),
                ..Provenance::unknown()
            },
        }
    }

    fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
        Rect { x0, y0, x1, y1 }
    }
}
