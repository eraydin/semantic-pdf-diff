use pdf_content::{ContentOp, ContentProgram};
use spdfdiff_types::{Diagnostic, LineSegment, Point, Provenance, Rect};

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

#[derive(Debug, Clone, PartialEq)]
pub struct TextExtraction {
    pub runs: Vec<TextRun>,
    pub diagnostics: Vec<Diagnostic>,
}

#[must_use]
pub fn extract_text_runs(program: &ContentProgram, page_index: usize) -> TextExtraction {
    let mut x = 0.0;
    let mut y = 0.0;
    let mut font_size = 12.0;
    let mut runs = Vec::new();
    let mut diagnostics = program.diagnostics.clone();
    let mut emitted_missing_tounicode = false;

    for operation in &program.operations {
        match operation {
            ContentOp::SetFont { size, .. } => {
                font_size = *size;
            }
            ContentOp::MoveTextPosition { tx, ty, .. } => {
                x += tx;
                y += ty;
            }
            ContentOp::SetTextMatrix { e, f, .. } => {
                x = *e;
                y = *f;
            }
            ContentOp::ShowText {
                text,
                raw_bytes,
                source,
            } => {
                if !emitted_missing_tounicode {
                    diagnostics.push(
                        Diagnostic::warning(
                            "MISSING_TOUNICODE",
                            "using literal-string fallback text because no ToUnicode map is available",
                        )
                        .with_page(page_index),
                    );
                    emitted_missing_tounicode = true;
                }
                let width = estimate_text_width(text, font_size);
                let bbox = Rect {
                    x0: x,
                    y0: y,
                    x1: x + width,
                    y1: y + font_size,
                };
                let glyph = GlyphToken {
                    id: format!("p{page_index}.g{:04}", runs.len()),
                    unicode: Some(text.clone()),
                    raw_bytes: raw_bytes.clone(),
                    page_index,
                    bbox,
                    baseline: LineSegment {
                        start: Point { x, y },
                        end: Point { x: x + width, y },
                    },
                    source: source.clone(),
                };
                runs.push(TextRun {
                    id: format!("p{page_index}.r{:04}", runs.len()),
                    text: text.clone(),
                    normalized_text: normalize_text(text),
                    glyphs: vec![glyph],
                    bbox,
                    source: source.clone(),
                });
                x += width;
            }
            ContentOp::BeginText { .. } | ContentOp::EndText { .. } | ContentOp::Unknown { .. } => {
            }
        }
    }

    TextExtraction { runs, diagnostics }
}

#[must_use]
pub fn normalize_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn estimate_text_width(text: &str, font_size: f32) -> f32 {
    text.chars().count() as f32 * font_size * 0.5
}

trait DiagnosticExt {
    fn with_page(self, page_index: usize) -> Self;
}

impl DiagnosticExt for Diagnostic {
    fn with_page(mut self, page_index: usize) -> Self {
        self.page_index = Some(page_index);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_content::parse_content_stream;
    use spdfdiff_types::DiagnosticSeverity;

    #[test]
    fn extracts_positioned_text_run() {
        let program = parse_content_stream(b"BT /F1 12 Tf 72 720 Td (Hello) Tj ET");
        let extraction = extract_text_runs(&program, 0);

        assert_eq!(extraction.runs.len(), 1);
        assert_eq!(extraction.runs[0].text, "Hello");
        assert_eq!(extraction.runs[0].normalized_text, "Hello");
        assert_eq!(extraction.runs[0].bbox.x0, 72.0);
        assert_eq!(extraction.runs[0].bbox.y0, 720.0);
        assert!(
            extraction
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "MISSING_TOUNICODE"
                    && diagnostic.severity == DiagnosticSeverity::Warning)
        );
    }
}
