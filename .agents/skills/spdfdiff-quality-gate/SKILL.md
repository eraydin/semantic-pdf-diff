---
name: spdfdiff-quality-gate
description: Verify semantic-pdf-diff implementation quality, test strategy, fixtures, fuzzing, corpus runner, compatibility claims, release gates, and deterministic snapshots. Use before merging broad changes, when adding fixtures or corpus metrics, when validating public-alpha readiness, or when checking that unsupported PDF features have stable diagnostics.
---

# SPDFDiff Quality Gate

## Workflow

1. Read `AGENTS.md`, then read `references/quality-plan.md`.
2. Identify the changed subsystem and its expected test layer.
3. Run the standard workspace gate.
4. Add or request focused tests for any changed behavior without direct coverage.
5. Check compatibility claims against diagnostics and corpus evidence.

## Required Gate

Run:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

When relevant, also run or add:

```bash
cargo test --workspace --features fuzzing
cargo check --manifest-path fuzz/Cargo.toml --bins
cargo bench --workspace
spdfdiff corpus tests/fixtures/real_world --output corpus_report.json
spdfdiff corpus samples --manifest samples/compatibility_corpus_manifest.json --output corpus_report.json --fail-on-gate
spdfdiff benchmark --pages 50 --output benchmark.json
spdfdiff check --config .spdfdiff.toml
```

Only run extended commands when the needed features/tools exist.
In repo CI, fuzz-feature tests, manifest corpus checks, direct sample-PDF CLI
smoke checks, benchmark smoke, and sample check run as separate non-release
quality-gate jobs so their status is visible independently from the minimum Rust
gate.

## Quality Rules

- Invalid input must not panic.
- Unsupported features must emit stable diagnostic codes.
- Snapshot output must not include timestamps, absolute paths, random IDs, or nondeterministic map ordering.
- Public-alpha claims require xref streams, object streams, resource limits,
  curated corpus metrics, documented unsupported cases, and a manifest
  compatibility label whose release blockers are empty.
- External PDF tools may be used only as optional dev/reference comparisons, not core dependencies.

## Review Checklist

- Confirm changed files match the intended crate boundary.
- Confirm new behavior has unit tests or snapshots at the lowest useful layer.
- Confirm resource-limit and diagnostic behavior for hostile or malformed inputs.
- Confirm report-facing output remains deterministic.
- Confirm corpus manifest gates cover partial-file and diagnostic-code
  regressions when compatibility claims depend on corpus evidence.
- Confirm `AGENTS.md` and plan files remain aligned if public workflow rules changed.
- For fuzzing slices, confirm standalone targets compile through
  `cargo check --manifest-path fuzz/Cargo.toml --bins`; run `cargo fuzz run`
  locally when `cargo-fuzz` is installed and time budget allows.
