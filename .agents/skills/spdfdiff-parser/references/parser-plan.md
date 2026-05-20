# Parser Plan Reference

Use this reference with `spdfdiff-parser`.

## Primary Files

- `AGENTS.md`
- `plan/01_ARCHITECTURE.md`
- `plan/02_DATA_MODEL_AND_DIFF_IR.md`
- `plan/04_MILESTONES_AND_TICKETS.md`
- `plan/05_TESTING_STRATEGY.md`

## Scope

- Crate: `crates/pdf_core`
- Shared dependencies: `crates/spdfdiff_types`
- First milestone: M0/M1 parser foundation
- Compatibility gate: M1.5 resource limits, xref streams, object streams, corpus metrics

## Required Behaviors

- Parse a PDF header and grow toward primitives, indirect objects, xref tables, trailers, object store, and streams.
- Support parser APIs that accept `ParseConfig` or limits from the start.
- Enforce hard resource limits for file size, object count, recursion depth, stream size, decoded stream bytes, content operators, and pages.
- Return partial output plus diagnostics when practical.
- Preserve ordered stream filter chains and paired `/DecodeParms` metadata;
  decode supported no-filter, `FlateDecode`, `ASCIIHexDecode`, and
  `RunLengthDecode` chains within resource limits.
- Keep xref streams and object streams as compatibility-gate work if not part of the current slice.

## Test Expectations

- Unit tests for primitives and malformed inputs.
- Header and minimal fixture tests before page-tree work.
- Limit tests for file size, recursion, stream length, and decoded output as features land.
- Diagnostic tests for unsupported xref streams, object streams, encryption, unsupported filters, filter-chain decoding, decode-parameter preservation, and malformed objects.
