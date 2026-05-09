use pdf_text::TextRun;
use spdfdiff_types::{Diagnostic, Provenance, Rect};

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

#[must_use]
pub fn build_semantic_document(
    fingerprint: impl Into<String>,
    runs: &[TextRun],
    diagnostics: Vec<Diagnostic>,
) -> SemanticDocument {
    let mut nodes = Vec::new();
    for (index, run) in runs.iter().enumerate() {
        if run.normalized_text.is_empty() {
            continue;
        }
        nodes.push(SemanticNode {
            id: format!("n{index:04}"),
            kind: SemanticNodeKind::Paragraph,
            page_index: run.source.page_index.unwrap_or(0),
            bbox: Some(run.bbox),
            normalized_text: Some(run.normalized_text.clone()),
            source: vec![run.source.clone()],
            confidence: 0.75,
        });
    }

    SemanticDocument {
        fingerprint: fingerprint.into(),
        nodes,
        diagnostics,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spdfdiff_types::{LineSegment, Point};

    #[test]
    fn turns_text_runs_into_paragraph_nodes() {
        let run = TextRun {
            id: "run1".into(),
            text: "Hello".into(),
            normalized_text: "Hello".into(),
            glyphs: Vec::new(),
            bbox: Rect {
                x0: 10.0,
                y0: 20.0,
                x1: 40.0,
                y1: 32.0,
            },
            source: Provenance {
                page_index: Some(0),
                ..Provenance::unknown()
            },
        };
        let document = build_semantic_document("fixture", &[run], Vec::new());

        assert_eq!(document.nodes.len(), 1);
        assert_eq!(document.nodes[0].kind, SemanticNodeKind::Paragraph);
        assert_eq!(document.nodes[0].normalized_text.as_deref(), Some("Hello"));
        let _ = LineSegment {
            start: Point { x: 0.0, y: 0.0 },
            end: Point { x: 1.0, y: 1.0 },
        };
    }
}
