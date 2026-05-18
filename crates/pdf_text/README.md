# pdf_text

Positioned PDF text extraction primitives for semantic PDF comparison.

`pdf_text` consumes interpreted content stream operations and emits positioned
text runs with glyph evidence, normalized text, bounding boxes, provenance, and
marked-content references. It is the text-extraction stage used by
`semantic-pdf-diff` before layout clustering and semantic diffing.

## What This Crate Provides

- Extraction of text from `Tj` and `TJ` operations.
- Basic text matrix, text line movement, leading, character spacing, word
  spacing, and horizontal scaling support.
- Glyph token evidence, raw text bytes, normalized text, and approximate
  bounding boxes.
- Marked-content tag and `/MCID` preservation for tagged-PDF mapping.
- Whitespace normalization for deterministic downstream matching.
- `MISSING_TOUNICODE` diagnostics when extraction falls back to literal or hex
  string bytes instead of a Unicode map.

## Pipeline Context

`pdf_text` sits between content interpretation and semantic layout:

```text
pdf_content ContentProgram -> pdf_text TextRun values -> pdf_semantic nodes
```

The current CLI applies a narrow `/ToUnicode` CMap mapping before calling this
crate when page resources expose a decoded CMap stream. A fuller public font
resource model remains planned work.

## Current Compatibility Boundary

This crate is designed for digitally generated PDFs with extractable text. It
does not perform OCR, full font shaping, real glyph outline measurement, or
renderer-grade text positioning. Image-only PDFs should use the CLI's external
OCR adapter path, not this crate as a substitute for OCR.

Bounding boxes are approximate extraction evidence for diff reports, not a
pixel-accurate rendering contract.
