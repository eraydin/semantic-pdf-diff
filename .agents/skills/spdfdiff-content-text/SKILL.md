---
name: spdfdiff-content-text
description: Implement or review semantic-pdf-diff content stream and text extraction work in crates/pdf_content and crates/pdf_text. Use for content tokenization, PDF text operators, graphics/text state, matrices, font resources, ToUnicode CMaps, glyph positioning, text runs, whitespace normalization, missing font diagnostics, or positioned-text extraction tests.
---

# SPDFDiff Content Text

## Workflow

1. Read `AGENTS.md`, then read `references/content-text-plan.md`.
2. Keep the boundary clear: `pdf_content` emits interpreted drawing/text operations; `pdf_text` converts those operations and resources into positioned glyphs and text runs.
3. Preserve content operation indices, raw shown bytes, transformation matrices, font references, page indices, and provenance.
4. Use diagnostics instead of panics for unknown operators, missing resources, malformed CMaps, unsupported fonts, and incomplete extraction.
5. Add focused tests at the lowest useful layer before integration tests.

## Content Stream Rules

- Preserve unknown operators as diagnostics, not silent drops.
- Malformed standalone delimiters must always advance the tokenizer and emit
  diagnostics; never let malformed content build unbounded token lists.
- Maintain text state and graphics state explicitly.
- Recognize MVP text operators: `BT`, `ET`, `Tf`, `Tj`, `TJ`, `Td`, `TD`, `Tm`, `T*`, `Tc`, `Tw`, `Tz`, `TL`, `q`, `Q`, `cm`.
- Keep common non-text drawing, color, clipping, marked-content, and XObject operators out of `CONTENT_OPERATOR_UNKNOWN`; preserve them as recognized operations with canonical operands so downstream vector/style surface comparison can use deterministic signatures.
- Preserve controlled marked-content tag and `/MCID` evidence from
  `BMC`/`BDC`/`EMC` so `pdf_text` and `pdf_semantic` can map text runs to
  tagged-PDF structure elements.
- Do not implement semantic block grouping in this skill; emit data for `pdf_semantic`.

## Text Extraction Rules

- Prefer `/ToUnicode`; preserve raw bytes when Unicode mapping is missing.
- CID/Type0 fonts without `/ToUnicode` must produce stable diagnostics; width
  estimates should remain deterministic and use simple character-shape
  heuristics instead of a single fixed glyph width.
- For simple fonts, prefer parsed `/FirstChar` and `/Widths` metrics when
  present; fall back to deterministic character-shape heuristics when metrics
  are missing or cannot safely apply.
- For simple Base14 Latin fonts, allow fallback decoding only for safe
  Helvetica, Times, and Courier-family Type1 fonts with absent,
  `/StandardEncoding`, or `/WinAnsiEncoding` encodings and printable
  ASCII/basic-whitespace bytes.
- `pdf_text` exposes the public font resource model plus `/ToUnicode` CMap
  parsing and application helpers for `bfchar` and controlled `bfrange`
  mappings. Keep expanding resource-aware font decoding in `pdf_text` instead
  of duplicating ad hoc mapping logic elsewhere.
- Implement selected fallback encodings only when safe and diagnostic-backed.
- Keep glyph positions approximate but deterministic.
- Preserve original text and normalized text separately.
- Preserve current marked-content tag and `/MCID` on text runs when present.
- Do not invent text or silently hide low-confidence extraction.

## Verification

Run the full workspace gate:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

For text slices, add tests for `Tj`, `TJ`, `/ToUnicode`, missing mappings, glyph position monotonicity, and text-run grouping.
