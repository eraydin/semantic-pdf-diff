# Semantic And Diff Plan Reference

Use this reference with `spdfdiff-semantic-diff`.

## Primary Files

- `AGENTS.md`
- `plan/01_ARCHITECTURE.md`
- `plan/02_DATA_MODEL_AND_DIFF_IR.md`
- `plan/04_MILESTONES_AND_TICKETS.md`
- `plan/05_TESTING_STRATEGY.md`

## Scope

- Crates: `crates/pdf_semantic`, `crates/diff_core`
- Inputs: text runs, glyphs, bounding boxes, style hints, provenance
- Outputs: semantic documents, semantic anchors, diff documents, confidence, diagnostics

## Milestone Targets

- M5-T1: line and block clustering
- M5-T2: heading candidates
- M5-T3: list and table candidates
- M5-T4: semantic anchors
- M6-T1: exact and fuzzy matching
- M6-T2: text hunks
- M6-T3: layout diff
- M6-T4: summary and severity

## Test Expectations

- Multi-line paragraph groups into one block.
- Separate paragraphs remain separate.
- Two-column fixture has stable reading order.
- Heading candidate detected from controlled fixture.
- Edited paragraphs match as modifications.
- Moved paragraphs are not reduced to delete plus insert.
- JSON snapshots are deterministic.
