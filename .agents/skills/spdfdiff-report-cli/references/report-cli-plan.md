# Report And CLI Plan Reference

Use this reference with `spdfdiff-report-cli`.

## Primary Files

- `AGENTS.md`
- `plan/01_ARCHITECTURE.md`
- `plan/02_DATA_MODEL_AND_DIFF_IR.md`
- `plan/04_MILESTONES_AND_TICKETS.md`
- `plan/05_TESTING_STRATEGY.md`

## Scope

- Crates: `crates/diff_report`, `crates/spdfdiff_cli`
- Vertical-slice outputs: stable JSON and simple Markdown
- Later outputs: basic HTML, then SVG overlays in v0.3

## CLI Contract

- `spdfdiff diff old.pdf new.pdf`
- `spdfdiff inspect file.pdf --format json`
- `spdfdiff extract file.pdf --format json`
- `spdfdiff corpus tests/fixtures/real_world --output corpus_report.json`

## Test Expectations

- JSON schema version included.
- All changes include evidence.
- No nondeterministic fields by default.
- Markdown includes summary table, change list, and diagnostics.
- Basic HTML has no external network resources.
- CLI exit codes are documented and tested.
