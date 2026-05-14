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
    pub source: Vec<Provenance>,
    pub confidence: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticNodeKind {
    Page,
    HeadingCandidate,
    Paragraph,
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
        source: line.source.clone(),
        confidence: 0.7,
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
