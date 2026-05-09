# Semantic PDF Diff Agent Instructions

## Scope

Build the project as a Rust library and CLI for semantic PDF diffing. Start with the
engine and stable report IR; do not build a GUI, editor, renderer, or OCR path in the
early vertical slice.

## Required Workflow

- Keep public shared types in `crates/spdfdiff_types`.
- Do not add PDF-specific parser libraries to core crates.
- Prefer diagnostics and partial results over panics for invalid or unsupported PDFs.
- Preserve provenance whenever data crosses crate boundaries.
- Keep output deterministic: no timestamps, random IDs, absolute paths, or unordered
  report-facing maps in snapshots.
- Add tests with behavior changes.

## Verification

Run these before considering a code slice complete:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

If Rust tooling is unavailable locally, state that clearly and run the non-Cargo checks
that are available.
