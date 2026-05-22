# semantic-pdf-diff

`semantic-pdf-diff` is a Rust CLI and library for producing stable,
evidence-preserving semantic diffs for digitally generated PDFs.

The CLI binary is `spdfdiff`.

Full documentation lives in [`docs/`](docs/) and is published at
[eraydin.github.io/semantic-pdf-diff](https://eraydin.github.io/semantic-pdf-diff/).

## Status

Project status: `compatibility-gate`.

The project is useful for controlled digitally generated PDFs and committed
sample scenarios. It is not yet a broad public-alpha compatibility claim for
arbitrary real-world PDFs. Unsupported or degraded PDF features should appear as
stable diagnostics rather than silent success.

## What It Supports Today

- Semantic text diffing from extracted positioned text blocks.
- Stable JSON, AI review JSON, Markdown, and self-contained HTML reports.
- Report evidence carries semantic roles for extracted blocks, including
  candidate headers, footers, page templates, tables, lists, and headings.
- Parser-backed diagnostics and partial results for unsupported or degraded PDF
  surfaces.
- CI checks with configured PDF pairs, thresholds, baseline suppression, and
  deterministic artifacts.
- Selected document-surface comparisons for images, vector/style signatures,
  annotations, links, form fields, outlines, name trees, metadata/XMP, and
  embedded-file surfaces.
- Simple tagged-PDF summaries with `/RoleMap`, parent-tree, and MCID-backed
  text mapping for controlled cases.
- Optional external OCR through `SPDFDIFF_OCR_COMMAND` or `tesseract` for
  supported image-only samples.

Renderer-grade visual diffing, arbitrary table reconstruction, broad tagged-PDF
coverage, and corpus-backed public-alpha compatibility remain incremental work.

## Quickstart

Build the workspace:

```powershell
cargo build --workspace
```

Compare two PDFs:

```powershell
.\target\debug\spdfdiff.exe diff .\old.pdf .\new.pdf --format json --output .\diff.json
```

Run through Cargo without using the built binary directly:

```powershell
cargo run -p spdfdiff_cli -- diff .\old.pdf .\new.pdf --format md
```

## CLI Essentials

```powershell
# Compare two PDFs.
.\target\debug\spdfdiff.exe diff .\old.pdf .\new.pdf --format json

# Run configured CI checks.
.\target\debug\spdfdiff.exe check --config .\.spdfdiff.toml

# Evaluate the committed sample corpus gate.
.\target\debug\spdfdiff.exe corpus .\samples --manifest .\samples\compatibility_corpus_manifest.json --fail-on-gate

# Run the synthetic benchmark smoke gate.
.\target\debug\spdfdiff.exe benchmark --pages 50 --output .\benchmark.json
```

Other CLI commands include `inspect`, `extract`, and `review`. See the
[documentation site](https://eraydin.github.io/semantic-pdf-diff/#cli) for the
full command reference.

## CI

Use `spdfdiff check` with a repository config:

```powershell
.\target\debug\spdfdiff.exe check --config .\.spdfdiff.toml
```

In GitHub Actions:

```yaml
- uses: eraydin/semantic-pdf-diff@main
  with:
    config: .spdfdiff.toml
```

The composite action uses an existing `spdfdiff` on `PATH` when available;
otherwise it installs the CLI from the checked-out action source.

## Reports And Schemas

Report formats:

- `json`: stable machine-readable diff report.
- `ai-json`: compact deterministic review artifact for agent workflows.
- `md`: Markdown summary for code review.
- `html`: self-contained evidence report.

Machine-readable schemas live in [`schemas/`](schemas/), with schema history in
[`schemas/CHANGELOG.md`](schemas/CHANGELOG.md).

## Crate Map

- `spdfdiff_types`: shared IDs, geometry, provenance, diagnostics, limits, and
  report-facing IR.
- `pdf_core`: low-level parser, object graph, streams, xref handling, and parser
  diagnostics.
- `pdf_content`, `pdf_text`, `pdf_semantic`: content interpretation, positioned
  text extraction, and semantic blocks.
- `diff_core`, `diff_report`, `spdfdiff_cli`: matching, report rendering, and the
  public CLI.

Core crates do not use third-party PDF parser or renderer libraries.

## Development Gates

Before considering a code change complete, run:

```powershell
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

For fuzzing-target changes, also run:

```powershell
cargo check --manifest-path fuzz/Cargo.toml --bins
```
