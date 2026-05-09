# Semantic PDF Diff Agent Instructions

## Mission

Build `semantic-pdf-diff` as a Rust library and CLI that produces stable,
evidence-preserving semantic diffs for digitally generated PDFs.

The first usable target is the vertical slice:

```text
minimal_old.pdf + minimal_new.pdf
  -> parse object graph
  -> resolve page
  -> parse content stream
  -> extract positioned text
  -> build paragraph blocks
  -> diff text blocks
  -> emit stable JSON and simple Markdown
```

## Non-Negotiable Constraints

- Do not use third-party PDF parser/rendering libraries in core crates.
- Do not start with a GUI, PDF editor, full renderer, OCR path, or visual diff mode.
- Do not hide unsupported PDF features; emit stable diagnostics.
- Do not discard provenance when data crosses crate boundaries.
- Do not introduce nondeterministic report output.
- Do not claim broad real-world PDF compatibility until xref streams, object streams,
  resource limits, and corpus metrics exist.
- Do not parse untrusted PDF streams without applying `ResourceLimits`.
- Do not couple semantic diff logic directly to raw PDF object internals.

## Rust Standards

- Use Rust 2024 for new code.
- Maintain MSRV `1.85` unless the plan is deliberately updated.
- Keep workspace lints active and fix warnings instead of suppressing them.
- Keep `unsafe` out of the workspace unless a future plan explicitly justifies it.
- Add tests with behavior changes; prefer a test-first workflow for parser, extraction,
  semantic, diff, and report behavior.

## Crate Boundaries

- `crates/spdfdiff_types` owns shared public IDs, geometry, provenance,
  diagnostics, resource limits, errors, and report-facing IR.
- Downstream crates may re-export shared types for ergonomics, but must not define
  incompatible public versions of those models.
- `pdf_core` owns low-level parsing, object graph, streams, xref handling, and parser
  diagnostics.
- `pdf_content` owns content stream tokenization and operator interpretation.
- `pdf_text` owns font decoding, `/ToUnicode`, glyphs, and text runs.
- `pdf_semantic` owns layout blocks, semantic nodes, reading order, and anchors.
- `diff_core` owns matching, text hunks, move detection, confidence, and neutral
  severity defaults.
- `diff_report` owns stable JSON, Markdown, and later HTML/SVG report generation.
- `spdfdiff_cli` owns the public CLI shape: `diff`, `inspect`, `extract`, and
  `corpus`.

## Repo-Local Skills

- Use repo-local skills from `.agents/skills` when a task matches their scope.
- Use `spdfdiff-orchestrator` before coordinating parallel agents, changing shared
  API boundaries, or merging cross-crate work.
- Keep skills aligned with `AGENTS.md` and the plan files when workflow rules, crate
  boundaries, diagnostics, or verification requirements change.
- Prefer updating the relevant skill in the same change that updates the canonical
  plan or instructions it depends on.

## Diagnostics And Compatibility

- Prefer explicit diagnostics and partial results over panics.
- Every unsupported feature needs a stable code, such as `UNSUPPORTED_XREF_STREAM`,
  `UNSUPPORTED_ENCRYPTION`, `MISSING_TOUNICODE`, or `CONTENT_OPERATOR_UNKNOWN`.
- Use compatibility labels honestly:
  - `vertical-slice`: controlled fixtures only.
  - `compatibility-gate`: modern PDF constructs such as xref/object streams.
  - `public-alpha`: corpus-backed behavior with documented limitations.
- Public-alpha claims require tests, diagnostics, and corpus evidence.

## Determinism

- Do not use random UUIDs, pointer addresses, timestamps, absolute paths, or
  unordered map iteration in report-facing output.
- Keep IDs deterministic through structural paths, canonical hashes, or sorted
  counters.
- Snapshot output must not include machine-specific paths, timings, or unstable
  ordering.
- The default engine classifier must not emit legal/business `Critical` severity;
  reserve that for caller-provided domain classifiers.

## Verification

Run these before considering a code slice complete:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

If Rust tooling is unavailable locally, state that clearly and run the non-Cargo
checks that are available. Do not claim Cargo verification passed unless it actually
ran successfully.
