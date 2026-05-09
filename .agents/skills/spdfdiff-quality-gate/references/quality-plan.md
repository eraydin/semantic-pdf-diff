# Quality Gate Plan Reference

Use this reference with `spdfdiff-quality-gate`.

## Primary Files

- `AGENTS.md`
- `plan/05_TESTING_STRATEGY.md`
- `plan/09_REVIEW_AND_RISK_REGISTER.md`
- `plan/04_MILESTONES_AND_TICKETS.md`
- `plan/07_ROADMAP.md`

## Test Layers

1. Byte parser
2. Object graph
3. Stream decoding
4. Content tokenizer
5. Text extraction
6. Layout segmentation
7. Semantic tree
8. Diff engine
9. Reports and CLI

## Fixture Categories

- `tests/fixtures/synthetic`
- `tests/fixtures/malformed`
- `tests/fixtures/compatibility`
- `tests/fixtures/tagged`
- `tests/fixtures/real_world`
- `tests/golden`

## Public Alpha Gate

Public alpha requires:

- xref stream support;
- object stream support;
- resource limits enforced;
- curated real-world corpus report;
- stable diagnostics for unsupported features;
- docs that clearly state supported and unsupported behavior.

## Known High Risks

- Modern PDFs often require xref streams and object streams.
- Text extraction without `/ToUnicode` is unreliable.
- Geometry is approximate without a full renderer.
- Semantic layout heuristics can create false confidence.
- PDF inputs can be hostile.
