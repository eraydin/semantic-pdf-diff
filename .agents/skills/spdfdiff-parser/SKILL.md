---
name: spdfdiff-parser
description: Implement or review semantic-pdf-diff low-level parser work in crates/pdf_core. Use for primitive PDF parsing, indirect objects, xref tables or streams, trailers, stream decoding, parser diagnostics, ObjectStore design, ParseConfig, ResourceLimits, malformed PDF handling, or any task touching pdf_core parser correctness and safety.
---

# SPDFDiff Parser

## Workflow

1. Read `AGENTS.md` first, then read `references/parser-plan.md` for parser-specific scope.
2. Keep public shared IDs, geometry, diagnostics, provenance, errors, and limits in `crates/spdfdiff_types`.
3. Keep `pdf_core` independent of PDF-specific parser/rendering libraries.
4. Treat PDF bytes as hostile input. Apply `ParseConfig` and `ResourceLimits` before allocations, stream decoding, recursion, or object expansion.
5. Prefer typed errors only for unrecoverable failures. Use structured diagnostics for unsupported or degraded PDF features.
6. Add focused parser tests for every behavior change, including malformed inputs and unsupported-feature diagnostics.
7. After parser capability changes, check whether `AGENTS.md`, this skill, README, and plan files need updates. Update them in the same slice when behavior, diagnostics, limits, or compatibility labels changed.

## Parser Rules

- Preserve byte ranges and object provenance as soon as parser data can cross crate boundaries.
- Never panic on invalid input. Return `PdfDiffError` or a `Diagnostic`.
- Controlled classic xref tables, `/Type /XRef` streams, `/Type /ObjStm`
  extraction, no-filter streams, `FlateDecode`, `ASCIIHexDecode`, and
  `RunLengthDecode` streams, catalog `/Pages` traversal with ordered `/Kids`,
  inherited page resources, MediaBox/CropBox dimensions, rotation, and simple
  `/StructTreeRoot` structure-tree plus `/ParentTree` entry parsing are
  implemented parser capabilities. Extend these paths rather than reintroducing
  broad unsupported diagnostics for them.
- Incremental-update markers and xref recovery should be diagnostic-backed:
  select the latest `startxref` marker, emit prior-revision diagnostics when
  `/Prev` is present, expose deterministic incremental-update offset metadata,
  and emit `XREF_RECOVERY_USED` only when an actual xref surface failed and
  indirect-object scanning recovered the file.
- Do not call unsupported modern PDF variants malformed when partial recovery is possible. Add exact diagnostics or stable `PdfDiffError` text for unsupported variants and malformed compatibility-gate cases.
- Resource-limit failures should include stable `RESOURCE_LIMIT_*` code text.
- Keep output deterministic: object ordering, diagnostics, and summaries must not depend on hash-map iteration.
- Keep parser implementation narrow. Do not add page layout, text extraction, semantic diff, report, GUI, or rendering work in parser slices.

## Verification

Run:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

For parser slices, include tests that prove invalid PDFs fail softly and limits are visible through public APIs.
