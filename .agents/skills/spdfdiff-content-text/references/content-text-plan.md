# Content And Text Plan Reference

Use this reference with `spdfdiff-content-text`.

## Primary Files

- `AGENTS.md`
- `plan/01_ARCHITECTURE.md`
- `plan/04_MILESTONES_AND_TICKETS.md`
- `plan/05_TESTING_STRATEGY.md`

## Scope

- Crates: `crates/pdf_content`, `crates/pdf_text`
- Inputs: decoded page content stream bytes, page resources, object provenance
- Outputs: content operations, glyph tokens, text runs, diagnostics

## Milestone Targets

- M3-T3: content tokenizer
- M3-T4: text operator interpreter
- M4-T1: font resource model
- M4-T2: `/ToUnicode` parser MVP
- M4-T3: glyph positioning MVP
- M4-T4: text run grouping

## Test Expectations

- Parse `BT /F1 12 Tf 72 720 Td (Hello) Tj ET`.
- Handle `TJ` arrays with strings and numeric spacing adjustments.
- Track simple text matrix changes.
- Use simple-font `/FirstChar` and `/Widths` metrics when present.
- Extract `Hello World` from synthetic PDFs with `/ToUnicode`.
- Emit diagnostics for missing font resources, missing `/ToUnicode`, unsupported CMap syntax, and unknown operators.
