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

#[derive(Debug, Clone, Copy)]
struct TextState {
    x: f32,
    y: f32,
    font_size: f32,
    leading: f32,
    character_spacing: f32,
    word_spacing: f32,
    horizontal_scale: f32,
}

impl Default for TextState {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            font_size: 12.0,
            leading: 0.0,
            character_spacing: 0.0,
            word_spacing: 0.0,
            horizontal_scale: 100.0,
        }
    }
}

#[must_use]
pub fn extract_text_runs(program: &ContentProgram, page_index: usize) -> TextExtraction {
    let mut state = TextState::default();
    let mut graphics_stack = Vec::new();
    let mut runs = Vec::new();
    let mut diagnostics = program.diagnostics.clone();
    let mut emitted_missing_tounicode = false;

    for operation in &program.operations {
        match operation {
            ContentOp::SetFont { size, .. } => {
                state.font_size = *size;
            }
            ContentOp::MoveTextPosition {
                tx,
                ty,
                set_leading,
                ..
            } => {
                state.x += tx;
                state.y += ty;
                if let Some(leading) = set_leading {
                    state.leading = *leading;
                }
            }
            ContentOp::MoveToNextLine { .. } => {
                state.y -= state.leading;
            }
            ContentOp::SetTextLeading { leading, .. } => {
                state.leading = *leading;
            }
            ContentOp::SetCharacterSpacing { spacing, .. } => {
                state.character_spacing = *spacing;
            }
            ContentOp::SetWordSpacing { spacing, .. } => {
                state.word_spacing = *spacing;
            }
            ContentOp::SetHorizontalScaling { scale, .. } => {
                state.horizontal_scale = *scale;
            }
            ContentOp::SetTextMatrix { e, f, .. } => {
                state.x = *e;
                state.y = *f;
            }
            ContentOp::ShowText {
                text,
                raw_bytes,
                source,
            }
            | ContentOp::ShowAdjustedText {
                text,
                raw_bytes,
                source,
                ..
            } => {
                let run_page_index = source.page_index.unwrap_or(page_index);
                if !emitted_missing_tounicode {
                    diagnostics.push(
                        Diagnostic::warning(
                            "MISSING_TOUNICODE",
                            "using literal-string fallback text because no ToUnicode map is available",
                        )
                        .with_page(run_page_index),
                    );
                    emitted_missing_tounicode = true;
                }
                emit_run(
                    &mut runs,
                    run_page_index,
                    text,
                    raw_bytes,
                    source.clone(),
                    &mut state,
                );
            }
            ContentOp::SaveGraphicsState { .. } => {
                graphics_stack.push(state);
            }
            ContentOp::RestoreGraphicsState { .. } => {
                if let Some(saved) = graphics_stack.pop() {
                    state = saved;
                }
            }
            ContentOp::BeginText { .. }
            | ContentOp::EndText { .. }
            | ContentOp::ConcatMatrix { .. }
            | ContentOp::RecognizedNonText { .. }
            | ContentOp::Unknown { .. } => {}
        }
    }

    TextExtraction { runs, diagnostics }
}

fn emit_run(
    runs: &mut Vec<TextRun>,
    page_index: usize,
    text: &str,
    raw_bytes: &[u8],
    source: Provenance,
    state: &mut TextState,
) {
    let width = estimate_text_width(text, *state);
    let bbox = Rect {
        x0: state.x,
        y0: state.y,
        x1: state.x + width,
        y1: state.y + state.font_size,
    };
    let glyph = GlyphToken {
        id: format!("p{page_index}.g{:04}", runs.len()),
        unicode: Some(text.to_owned()),
        raw_bytes: raw_bytes.to_vec(),
        page_index,
        bbox,
        baseline: LineSegment {
            start: Point {
                x: state.x,
                y: state.y,
            },
            end: Point {
                x: state.x + width,
                y: state.y,
            },
        },
        source: source.clone(),
    };
    runs.push(TextRun {
        id: format!("p{page_index}.r{:04}", runs.len()),
        text: text.to_owned(),
        normalized_text: normalize_text(text),
        glyphs: vec![glyph],
        bbox,
        source,
    });
    state.x += width;
}

#[must_use]
pub fn normalize_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn estimate_text_width(text: &str, state: TextState) -> f32 {
    let base_width = text
        .chars()
        .map(|character| approximate_glyph_width(character) * state.font_size)
        .sum::<f32>();
    let spacing = text.chars().fold(0.0, |total, character| {
        let word_spacing = if character == ' ' {
            state.word_spacing
        } else {
            0.0
        };
        total + state.character_spacing + word_spacing
    });
    (base_width + spacing) * (state.horizontal_scale / 100.0)
}

fn approximate_glyph_width(character: char) -> f32 {
    match character {
        ' ' => 0.25,
        'i' | 'l' | 'I' | '!' | '.' | ',' | ':' | ';' | '|' => 0.28,
        'm' | 'w' | 'M' | 'W' => 0.78,
        character if character.is_ascii_digit() => 0.5,
        character if character.is_ascii_punctuation() => 0.35,
        character if character.len_utf8() > 1 => 0.6,
        _ => 0.5,
    }
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

    #[test]
    fn extracts_adjusted_text_array_as_one_run() {
        let program = parse_content_stream(b"BT /F1 10 Tf 10 20 Td [(Hel) -120 (lo)] TJ ET");
        let extraction = extract_text_runs(&program, 0);

        assert_eq!(extraction.runs.len(), 1);
        assert_eq!(extraction.runs[0].text, "Hello");
        assert_eq!(extraction.runs[0].glyphs[0].raw_bytes, b"Hello");
        assert!(extraction.runs[0].bbox.width() > 0.0);
    }

    #[test]
    fn line_movement_changes_y_coordinate() {
        let program =
            parse_content_stream(b"BT /F1 12 Tf 72 720 Td 14 TL (First) Tj T* (Second) Tj ET");
        let extraction = extract_text_runs(&program, 0);

        assert_eq!(extraction.runs.len(), 2);
        assert_eq!(extraction.runs[0].bbox.y0, 720.0);
        assert_eq!(extraction.runs[1].bbox.y0, 706.0);
    }

    #[test]
    fn spacing_state_affects_text_advance() {
        let plain = extract_text_runs(
            &parse_content_stream(b"BT /F1 10 Tf 0 0 Td (A A) Tj (B) Tj ET"),
            0,
        );
        let spaced = extract_text_runs(
            &parse_content_stream(b"BT /F1 10 Tf 0 0 Td 2 Tc 4 Tw (A A) Tj (B) Tj ET"),
            0,
        );

        assert!(spaced.runs[1].bbox.x0 > plain.runs[1].bbox.x0);
    }

    #[test]
    fn glyph_width_estimate_uses_character_shape_heuristics() {
        let state = TextState {
            font_size: 10.0,
            ..TextState::default()
        };

        assert!(estimate_text_width("WWW", state) > estimate_text_width("iii", state));
        assert!(estimate_text_width("A A", state) < estimate_text_width("AAA", state));
    }
}
