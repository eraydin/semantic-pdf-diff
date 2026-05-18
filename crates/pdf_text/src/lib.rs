use pdf_content::{ContentOp, ContentProgram};
use spdfdiff_types::{Diagnostic, LineSegment, ObjectId, Point, Provenance, Rect};
use std::collections::{BTreeMap, BTreeSet};

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
    pub marked_content: Option<MarkedContentRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkedContentRef {
    pub tag: String,
    pub mcid: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextExtraction {
    pub runs: Vec<TextRun>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontResourceSet {
    pub fonts: BTreeMap<String, FontResource>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontResource {
    pub resource_name: String,
    pub object_id: ObjectId,
    pub subtype: Option<String>,
    pub base_font: Option<String>,
    pub encoding: Option<String>,
    pub to_unicode: Option<ObjectId>,
    pub descendant_font_object_ids: Vec<ObjectId>,
    pub descendant_subtypes: Vec<String>,
}

impl FontResource {
    #[must_use]
    pub fn is_cid_or_type0(&self) -> bool {
        self.subtype.as_deref() == Some("Type0")
            || self
                .descendant_subtypes
                .iter()
                .any(|subtype| subtype.starts_with("CIDFont"))
    }
}

#[must_use]
pub fn font_resources_from_document(document: &pdf_core::PdfDocument) -> FontResourceSet {
    let objects_by_id = document
        .objects
        .iter()
        .map(|object| (object.id, object))
        .collect::<BTreeMap<_, _>>();
    let mut fonts = BTreeMap::new();
    let mut diagnostics = Vec::new();

    for object in &document.objects {
        if is_font_object_body(&object.body) {
            continue;
        }

        let scoped_font_references = font_resource_references(&object.body);
        for (resource_name, object_id) in named_references(&object.body) {
            let Some(font_object) = objects_by_id.get(&object_id) else {
                continue;
            };
            if !is_font_object_body(&font_object.body) {
                continue;
            }
            fonts.insert(
                resource_name.clone(),
                font_resource_from_object(
                    resource_name,
                    object_id,
                    font_object.body.as_str(),
                    &objects_by_id,
                ),
            );
        }

        let mut diagnosed_scoped_references = BTreeSet::new();
        for (resource_name, object_id) in scoped_font_references {
            if !diagnosed_scoped_references.insert((resource_name.clone(), object_id)) {
                continue;
            }
            match objects_by_id.get(&object_id) {
                Some(font_object) if is_font_object_body(&font_object.body) => {}
                Some(_) => diagnostics.push(Diagnostic::warning(
                    "FONT_RESOURCE_NOT_FONT",
                    format!(
                        "font resource /{resource_name} points to non-font object {}",
                        object_id.number
                    ),
                )),
                None => diagnostics.push(Diagnostic::warning(
                    "MISSING_FONT_RESOURCE",
                    format!(
                        "font resource /{resource_name} points to missing object {}",
                        object_id.number
                    ),
                )),
            }
        }
    }

    FontResourceSet { fonts, diagnostics }
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
    let mut marked_content_stack: Vec<MarkedContentRef> = Vec::new();

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
                    marked_content_stack.last().cloned(),
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
            ContentOp::BeginMarkedContent { tag, mcid, .. } => {
                marked_content_stack.push(MarkedContentRef {
                    tag: tag.clone(),
                    mcid: *mcid,
                });
            }
            ContentOp::EndMarkedContent { .. } => {
                marked_content_stack.pop();
            }
            ContentOp::BeginText { .. }
            | ContentOp::EndText { .. }
            | ContentOp::ConcatMatrix { .. }
            | ContentOp::AppendRectangle { .. }
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
    marked_content: Option<MarkedContentRef>,
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
        marked_content,
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

fn font_resource_from_object(
    resource_name: String,
    object_id: ObjectId,
    body: &str,
    objects_by_id: &BTreeMap<ObjectId, &pdf_core::PdfObject>,
) -> FontResource {
    let descendant_font_object_ids = named_references(body)
        .into_iter()
        .filter_map(|(name, object_id)| (name == "DescendantFonts").then_some(object_id))
        .collect::<Vec<_>>();
    let descendant_subtypes = descendant_font_object_ids
        .iter()
        .filter_map(|object_id| objects_by_id.get(object_id))
        .filter_map(|object| name_after_key(&object.body, "Subtype"))
        .collect::<Vec<_>>();

    FontResource {
        resource_name,
        object_id,
        subtype: name_after_key(body, "Subtype"),
        base_font: name_after_key(body, "BaseFont"),
        encoding: value_after_pdf_name(body, "Encoding"),
        to_unicode: reference_after_key(body, "ToUnicode"),
        descendant_font_object_ids,
        descendant_subtypes,
    }
}

fn font_resource_references(body: &str) -> Vec<(String, ObjectId)> {
    body.match_indices("/Font")
        .flat_map(|(font_index, _)| {
            let after_font = &body[font_index + "/Font".len()..];
            let Some(dictionary_start) = after_font.find("<<") else {
                return Vec::new();
            };
            let after_dictionary_start = &after_font[dictionary_start + "<<".len()..];
            let Some(dictionary_end) = after_dictionary_start.find(">>") else {
                return Vec::new();
            };
            named_references(&after_dictionary_start[..dictionary_end])
        })
        .collect()
}

fn is_font_object_body(body: &str) -> bool {
    body.contains("/Type /Font")
        || body.contains("/Subtype /Type0")
        || body.contains("/Subtype /Type1")
        || body.contains("/Subtype /TrueType")
        || body.contains("/Subtype /CIDFontType")
}

fn named_references(body: &str) -> Vec<(String, ObjectId)> {
    let tokens = body_tokens(body);
    let mut references = Vec::new();
    for index in 0..tokens.len().saturating_sub(3) {
        let Some(name) = tokens[index].strip_prefix('/') else {
            continue;
        };
        let Ok(number) = tokens[index + 1].parse::<u32>() else {
            continue;
        };
        let Ok(generation) = tokens[index + 2].parse::<u16>() else {
            continue;
        };
        if tokens[index + 3] == "R" {
            references.push((name.to_owned(), ObjectId { number, generation }));
        }
    }
    references
}

fn reference_after_key(body: &str, key: &str) -> Option<ObjectId> {
    let start = body.find(&format!("/{key}"))? + key.len() + 1;
    parse_reference_at(&body[start..])
}

fn parse_reference_at(body: &str) -> Option<ObjectId> {
    let tokens = body_tokens(body);
    let number = tokens.first()?.parse().ok()?;
    let generation = tokens.get(1)?.parse().ok()?;
    if tokens.get(2)? != "R" {
        return None;
    }
    Some(ObjectId { number, generation })
}

fn name_after_key(body: &str, key: &str) -> Option<String> {
    value_after_pdf_name(body, key).and_then(|value| value.strip_prefix('/').map(ToOwned::to_owned))
}

fn value_after_pdf_name(body: &str, key: &str) -> Option<String> {
    let start = body.find(&format!("/{key}"))? + key.len() + 1;
    let remaining = body[start..].trim_start();
    if let Some(value) = remaining.strip_prefix('(') {
        return value
            .split_once(')')
            .map(|(value, _)| value.chars().take(120).collect());
    }
    if let Some(value) = remaining.strip_prefix('/') {
        return value
            .split_whitespace()
            .next()
            .map(|value| format!("/{value}"));
    }
    Some(
        remaining
            .split_whitespace()
            .take(3)
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(120)
            .collect::<String>(),
    )
}

fn body_tokens(body: &str) -> Vec<String> {
    body.replace("<<", " ")
        .replace(">>", " ")
        .replace('/', " /")
        .replace(['[', ']'], " ")
        .split_whitespace()
        .map(ToOwned::to_owned)
        .collect()
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

    #[test]
    fn preserves_marked_content_mcid_on_text_runs() {
        let program = parse_content_stream(b"/P << /MCID 3 >> BDC BT (Tagged) Tj ET EMC");
        let extraction = extract_text_runs(&program, 0);

        assert_eq!(extraction.runs.len(), 1);
        assert_eq!(
            extraction.runs[0].marked_content,
            Some(MarkedContentRef {
                tag: "P".to_owned(),
                mcid: Some(3),
            })
        );
    }

    #[test]
    fn builds_font_resource_model_from_page_resources() {
        let document = pdf_core::PdfDocument::parse(font_resource_pdf().as_slice())
            .expect("font resource fixture should parse");
        let resources = font_resources_from_document(&document);
        let font = resources.fonts.get("F1").expect("F1 should resolve");

        assert!(resources.diagnostics.is_empty());
        assert_eq!(font.resource_name, "F1");
        assert_eq!(
            font.object_id,
            ObjectId {
                number: 5,
                generation: 0
            }
        );
        assert_eq!(font.subtype.as_deref(), Some("Type0"));
        assert_eq!(font.base_font.as_deref(), Some("CIDFont"));
        assert_eq!(font.encoding.as_deref(), Some("/Identity-H"));
        assert_eq!(
            font.to_unicode,
            Some(ObjectId {
                number: 7,
                generation: 0
            })
        );
        assert_eq!(
            font.descendant_font_object_ids,
            vec![ObjectId {
                number: 6,
                generation: 0
            }]
        );
        assert_eq!(font.descendant_subtypes, vec!["CIDFontType2".to_owned()]);
        assert!(font.is_cid_or_type0());
    }

    #[test]
    fn builds_font_resource_model_from_indirect_resource_dictionary() {
        let document = pdf_core::PdfDocument::parse(indirect_font_resource_pdf().as_slice())
            .expect("indirect font resource fixture should parse");
        let resources = font_resources_from_document(&document);

        assert!(resources.diagnostics.is_empty());
        assert_eq!(
            resources
                .fonts
                .get("FIndirect")
                .expect("FIndirect should resolve")
                .to_unicode,
            Some(ObjectId {
                number: 7,
                generation: 0
            })
        );
    }

    #[test]
    fn font_resource_model_reports_missing_font_objects() {
        let document = pdf_core::PdfDocument::parse(missing_font_resource_pdf().as_slice())
            .expect("missing-font fixture should parse");
        let resources = font_resources_from_document(&document);

        assert!(resources.fonts.is_empty());
        assert!(
            resources
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "MISSING_FONT_RESOURCE")
        );
    }

    fn font_resource_pdf() -> Vec<u8> {
        "%PDF-1.7
1 0 obj
<< /Type /Catalog /Pages 2 0 R >>
endobj
2 0 obj
<< /Type /Pages /Kids [3 0 R] /Count 1 >>
endobj
3 0 obj
<< /Type /Page /Parent 2 0 R /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>
endobj
4 0 obj
<< /Length 38 >>
stream
BT /F1 12 Tf 72 720 Td (Hello) Tj ET
endstream
endobj
5 0 obj
<< /Type /Font /Subtype /Type0 /BaseFont /CIDFont /Encoding /Identity-H /DescendantFonts [6 0 R] /ToUnicode 7 0 R >>
endobj
6 0 obj
<< /Type /Font /Subtype /CIDFontType2 /BaseFont /CIDFont >>
endobj
7 0 obj
<< /Length 0 >>
stream

endstream
endobj
"
        .as_bytes()
        .to_vec()
    }

    fn indirect_font_resource_pdf() -> Vec<u8> {
        "%PDF-1.7
1 0 obj
<< /Type /Catalog /Pages 2 0 R >>
endobj
2 0 obj
<< /Type /Pages /Kids [3 0 R] /Count 1 >>
endobj
3 0 obj
<< /Type /Page /Parent 2 0 R /Resources 8 0 R /Contents 4 0 R >>
endobj
4 0 obj
<< /Length 38 >>
stream
BT /FIndirect 12 Tf 72 720 Td (Hello) Tj ET
endstream
endobj
5 0 obj
<< /Type /Font /Subtype /Type0 /BaseFont /CIDFont /Encoding /Identity-H /DescendantFonts [6 0 R] /ToUnicode 7 0 R >>
endobj
6 0 obj
<< /Type /Font /Subtype /CIDFontType2 /BaseFont /CIDFont >>
endobj
7 0 obj
<< /Length 0 >>
stream

endstream
endobj
8 0 obj
<< /Font << /FIndirect 5 0 R >> >>
endobj
"
        .as_bytes()
        .to_vec()
    }

    fn missing_font_resource_pdf() -> Vec<u8> {
        "%PDF-1.7
1 0 obj
<< /Type /Catalog /Pages 2 0 R >>
endobj
2 0 obj
<< /Type /Pages /Kids [3 0 R] /Count 1 >>
endobj
3 0 obj
<< /Type /Page /Parent 2 0 R /Resources << /Font << /Missing 99 0 R >> >> /Contents 4 0 R >>
endobj
4 0 obj
<< /Length 38 >>
stream
BT /Missing 12 Tf 72 720 Td (Hello) Tj ET
endstream
endobj
"
        .as_bytes()
        .to_vec()
    }
}
