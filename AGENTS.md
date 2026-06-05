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
- Do not start with a GUI, PDF editor, or full renderer. Visual diffing must
  stay an optional external-renderer adapter path, not a core-renderer
  substitute for parser/content diagnostics.
- Keep OCR as an external adapter path; do not embed a large OCR model or make
  OCR a substitute for parser/content diagnostics.
- Do not hide unsupported PDF features; emit stable diagnostics.
- Do not discard provenance when data crosses crate boundaries.
- Do not introduce nondeterministic report output.
- Do not claim broad real-world PDF compatibility until xref streams, object streams,
  resource limits, and corpus metrics exist.
- Do not parse untrusted PDF streams without applying `ResourceLimits`.
- Do not couple semantic diff logic directly to raw PDF object internals.
- Do not leave `AGENTS.md`, repo-local skills, or user-facing docs stale after a
  change that alters implemented capabilities, workflow rules, diagnostics, crate
  boundaries, or compatibility labels.

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
- `diff_report` owns stable JSON, AI review JSON, Markdown, HTML report
  generation, and deterministic inline SVG evidence overlays.
- `spdfdiff_cli` owns the public CLI shape: `diff`, `inspect`, `extract`,
  `corpus`, `benchmark`, `review`, and `check`.

## Repo-Local Skills

- Use repo-local skills from `.agents/skills` when a task matches their scope.
- Use `spdfdiff-orchestrator` before coordinating parallel agents, changing shared
  API boundaries, or merging cross-crate work.
- Keep skills aligned with `AGENTS.md` and the plan files when workflow rules, crate
  boundaries, diagnostics, or verification requirements change.
- Prefer updating the relevant skill in the same change that updates the canonical
  plan or instructions it depends on.
- After each implementation slice, explicitly check whether `AGENTS.md`, the
  matching repo-local skill, and README/plan files need updates. If no docs need
  updates, mention that in the final response; if they do, update them in the same
  slice before commit/push.

## Current Parser Capability Boundary

- `pdf_core` currently supports parser primitives, indirect objects, classic xref
  tables/trailers, no-filter, `FlateDecode`, `ASCIIHexDecode`, and
  `RunLengthDecode` streams, controlled `/Type /XRef` streams, controlled
  `/Type /ObjStm` extraction through `ObjectStore`, embedded object provenance,
  catalog `/Pages` traversal with ordered `/Kids`, inherited page resources,
  MediaBox/CropBox dimensions, and rotation, ordered stream filter chains for
  supported no-filter, `FlateDecode`, `ASCIIHexDecode`, and `RunLengthDecode`
  filters with paired `/DecodeParms` metadata,
  simple `/StructTreeRoot` structure-tree parsing with structure types,
  `/RoleMap` entries, parent-tree entries, and MCID references, encrypted-PDF rejection, and
  resource-limit enforcement for parser-owned limits, plus deterministic
  incremental-update metadata for repeated `startxref` and trailer `/Prev`
  offsets.
- This is still a `compatibility-gate` parser foundation, not a public-alpha
  compatibility claim. Public-alpha still requires corpus metrics, documented
  unsupported cases, and broader extraction/report evidence.
- Continue extending parser support in `pdf_core`; do not bypass it with raw string
  parsing in downstream crates.

## Diagnostics And Compatibility

- Prefer explicit diagnostics and partial results over panics.
- Every unsupported or degraded report-facing feature needs a stable code, such as
  `UNSUPPORTED_ENCRYPTION`, `UNSUPPORTED_STREAM_FILTER`, `STREAM_DECODE_FAILED`,
  `MISSING_TOUNICODE`, or `CONTENT_OPERATOR_UNKNOWN`.
- Parser resource-limit errors must include stable `RESOURCE_LIMIT_*` code text.
- Do not use `UNSUPPORTED_XREF_STREAM` or `UNSUPPORTED_OBJECT_STREAM` as blanket
  diagnostics for controlled xref/object stream cases that `pdf_core` now handles.
- Use compatibility labels honestly:
  - `vertical-slice`: controlled fixtures only.
  - `compatibility-gate`: modern PDF constructs such as xref/object streams.
  - `public-alpha`: corpus-backed behavior with documented limitations.
- Public-alpha claims require tests, diagnostics, and corpus evidence.
- Corpus gate manifests may pin maximum partial-file and diagnostic-code counts;
  treat those thresholds as compatibility regression baselines, not broad
  compatibility claims.
- Corpus gate manifests may declare a compatibility label. A `public-alpha`
  label is release-blocking unless the manifest includes a curated release gate
  with at least 30 parsed files, explicit partial-file thresholds, diff pairs,
  and zero failed files/pairs.

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

For fuzzing-target changes, also run:

```bash
cargo check --manifest-path fuzz/Cargo.toml --bins
```

Run `cargo fuzz run <target>` when `cargo-fuzz` is installed and the time budget
allows.


<!-- headroom:rtk-instructions -->
# RTK (Rust Token Killer) - Token-Optimized Commands

When running shell commands, **always prefix with `rtk`**. This reduces context
usage by 60-90% with zero behavior change. If rtk has no filter for a command,
it passes through unchanged — so it is always safe to use.

## Key Commands
```bash
# Git (59-80% savings)
rtk git status          rtk git diff            rtk git log

# Files & Search (60-75% savings)
rtk ls <path>           rtk read <file>         rtk grep <pattern>
rtk find <pattern>      rtk diff <file>

# Test (90-99% savings) — shows failures only
rtk pytest tests/       rtk cargo test          rtk test <cmd>

# Build & Lint (80-90% savings) — shows errors only
rtk tsc                 rtk lint                rtk cargo build
rtk prettier --check    rtk mypy                rtk ruff check

# Analysis (70-90% savings)
rtk err <cmd>           rtk log <file>          rtk json <file>
rtk summary <cmd>       rtk deps                rtk env

# GitHub (26-87% savings)
rtk gh pr view <n>      rtk gh run list         rtk gh issue list

# Infrastructure (85% savings)
rtk docker ps           rtk kubectl get         rtk docker logs <c>

# Package managers (70-90% savings)
rtk pip list            rtk pnpm install        rtk npm run <script>
```

## Rules
- In command chains, prefix each segment: `rtk git add . && rtk git commit -m "msg"`
- For debugging, use raw command without rtk prefix
- `rtk proxy <cmd>` runs command without filtering but tracks usage
<!-- /headroom:rtk-instructions -->
