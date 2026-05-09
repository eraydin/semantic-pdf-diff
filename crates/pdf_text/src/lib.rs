use spdfdiff_types::{LineSegment, Provenance, Rect};

#[derive(Debug, Clone, PartialEq)]
pub struct GlyphToken {
    pub id: String,
    pub unicode: Option<String>,
    pub raw_bytes: Vec<u8>,
    pub page_index: usize,
    pub bbox: Rect,
    pub baseline: LineSegment,
    pub source: Provenance,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextRun {
    pub id: String,
    pub text: String,
    pub normalized_text: String,
    pub glyphs: Vec<GlyphToken>,
    pub bbox: Rect,
    pub source: Provenance,
}
