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

## Parser Rules

- Preserve byte ranges and object provenance as soon as parser data can cross crate boundaries.
- Never panic on invalid input. Return `PdfDiffError` or a `Diagnostic`.
- Do not call unsupported modern PDF structures malformed. Emit exact diagnostics such as `UNSUPPORTED_XREF_STREAM` or `UNSUPPORTED_OBJECT_STREAM` until implemented.
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
