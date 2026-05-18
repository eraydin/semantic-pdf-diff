# pdf_content

PDF content stream tokenizer and operator model for semantic PDF diffing.

`pdf_content` turns decoded PDF page content streams into deterministic drawing
and text operations. It is a middle layer for PDF comparison tools that need to
preserve text/layout evidence without building a full PDF renderer.

## What This Crate Provides

- Tokenization of content stream operands and operators.
- Text object/operator recognition for `BT`, `ET`, `Tf`, `Tj`, `TJ`, `Td`,
  `TD`, `Tm`, `T*`, `Tc`, `Tw`, `Tz`, and `TL`.
- Graphics-state operators used by text extraction, including `q`, `Q`, and
  `cm`.
- Recognition of common non-text drawing, color, clipping, marked-content, and
  XObject operators so ordinary visual content does not become
  `CONTENT_OPERATOR_UNKNOWN` noise.
- Marked-content preservation for `BMC`, `BDC`, and `EMC`, including controlled
  tag and `/MCID` evidence for downstream tagged-PDF mapping.
- Stable diagnostics for truly unknown or unsupported content operators.

## Pipeline Context

`pdf_content` consumes decoded streams from `pdf_core` and feeds interpreted
operators to `pdf_text`:

```text
pdf_core page streams -> pdf_content ContentProgram -> pdf_text TextRun values
```

The crate keeps operator provenance so extracted text runs can point back to
page, stream, content-op index, and byte-range evidence.

## Current Compatibility Boundary

This crate recognizes many drawing operators for diagnostic hygiene, but it is
not a renderer and does not attempt native vector graphic comparison. Vector
graphics, complex clipping, transparency, patterns, shadings, and image drawing
are surfaced to later comparison stages as unsupported or object-level surfaces
where appropriate.

Use `parse_content_stream_with_limits` when operator count limits matter.
