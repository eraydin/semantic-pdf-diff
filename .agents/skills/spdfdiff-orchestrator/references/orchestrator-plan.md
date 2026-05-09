# Orchestrator Plan Reference

Use this reference with `spdfdiff-orchestrator`.

## Primary Files

- `AGENTS.md`
- `plan/03_PARALLEL_WORKSTREAMS.md`
- `plan/04_MILESTONES_AND_TICKETS.md`
- `plan/06_AGENT_EXECUTION_GUIDE.md`
- `plan/09_REVIEW_AND_RISK_REGISTER.md`

## Specialist Skills

- `spdfdiff-parser`: `crates/pdf_core`
- `spdfdiff-content-text`: `crates/pdf_content`, `crates/pdf_text`
- `spdfdiff-semantic-diff`: `crates/pdf_semantic`, `crates/diff_core`
- `spdfdiff-report-cli`: `crates/diff_report`, `crates/spdfdiff_cli`
- `spdfdiff-quality-gate`: fixtures, corpus, fuzzing, compatibility, release gates

## Shared-Boundary Ownership

Coordinate changes to:

- `crates/spdfdiff_types`
- `AGENTS.md`
- `.agents/skills`
- report-facing IR and serialized field names
- diagnostic codes and severity rules
- public CLI command shape
- compatibility claims and release gates

## Recommended Early Parallel Slices

1. Shared types owner: stabilize IDs, diagnostics, errors, `ResourceLimits`, and minimal IR.
2. Parser owner: implement primitives and xref table work in `pdf_core`.
3. Fixture owner: implement deterministic synthetic fixtures.
4. Content owner: implement tokenizer and text operators against standalone content fixtures.
5. Diff owner: implement `diff_core` against handcrafted semantic documents.
6. Report/CLI owner: implement stable JSON/Markdown shape and command plumbing.

## Merge Order

1. Shared types and tests.
2. Synthetic fixtures and parser primitives.
3. Content tokenizer and text extraction.
4. Semantic extraction and diff core.
5. Reports and CLI.
6. Quality gates, corpus metrics, and compatibility claims.

## Review Checklist

- Does each agent have one owner path?
- Does the change match the selected repo-local skill?
- Are shared API changes minimized and documented?
- Are unsupported features diagnostic-backed?
- Are tests present at the lowest useful layer?
- Did the full gate pass after integration?
