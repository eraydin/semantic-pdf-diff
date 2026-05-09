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
