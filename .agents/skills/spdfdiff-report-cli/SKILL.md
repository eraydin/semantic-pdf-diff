---
name: spdfdiff-report-cli
description: Implement or review semantic-pdf-diff report generation and CLI work in crates/diff_report and crates/spdfdiff_cli. Use for stable JSON reports, AI review JSON, Markdown summaries, HTML reports, inline SVG evidence overlays, CLI commands, command arguments, exit codes, output files, corpus command integration, and report snapshot tests.
---

# SPDFDiff Report CLI

## Workflow

1. Read `AGENTS.md`, then read `references/report-cli-plan.md`.
2. Keep `DiffDocument` serialization stable and machine-readable first.
3. Treat JSON, AI review JSON, Markdown, HTML, and inline SVG evidence overlays as deterministic report outputs.
4. Keep the public CLI shape stable: `spdfdiff diff`, `inspect`, `extract`,
   `corpus`, `benchmark`, `review`, and `check`.
5. Add snapshot-style tests for report output whenever fields or ordering change.

## Report Rules

- Do not include timestamps, absolute paths, random IDs, nondeterministic map order, or machine-specific timings by default.
- Keep JSON canonical enough for agents and CI to consume.
- AI review JSON must stay deterministic, evidence-preserving, and neutral:
  include question hints, candidate tags, confidence buckets, explanation
  templates, semantic node identities, semantic roles when available, and
  prompt-ready evidence bundles without embedding an LLM or making
  legal/business conclusions.
- Markdown should summarize counts, changes, page references, and diagnostics.
- HTML must not depend on external network resources.
- Basic HTML diff reports should render old/new evidence side by side and show
  available page/bbox evidence.
- Inline SVG evidence overlays must be deterministic and must state that
  bounding boxes are in PDF user space.
- CLI extraction currently walks parsed page content across all pages. Diff
  reports compare image XObject payloads by deterministic stream hash and
  compare native vector path and graphic-style operations by deterministic
  parsed-operation signatures. Text font resource and font-size changes for
  unchanged text should be emitted as deterministic `StyleChanged` entries.
  Annotation/link reports compare deterministic semantic fields such as subtype,
  rectangle, URI or destination, contents, color, border, and quad points without
  executing actions. Report-facing AcroForm/widget fields, outlines/bookmarks,
  name trees, document-info/XMP metadata, and embedded file/FileSpec surfaces
  should use deterministic typed signatures instead of raw object-surface hashes.
  Image-only PDFs can use the external OCR adapter when
  `SPDFDIFF_OCR_COMMAND` or `tesseract` is available; OCR text must preserve
  image-object provenance and deterministic diagnostics. Renderer-grade visual
  diffing stays in the CLI/report layer through `spdfdiff visual-diff`: invoke
  an external renderer command, require deterministic RGB PPM page outputs,
  compare pixels with a stable threshold, and write deterministic JSON plus
  optional PPM heatmaps without adding renderer dependencies to core crates.
- Inspect and extract JSON reports include simple tagged-structure,
  `/RoleMap`, and parent-tree summaries when `pdf_core` parses
  `/StructTreeRoot`; mapped MCID text can now produce tagged semantic nodes,
  while broader tagged-PDF coverage remains compatibility-gate work.
- AI review JSON tags repeated page-region evidence when changed nodes carry
  header, footer, or page-template candidate roles.

## CLI Rules

- Public commands:
  - `spdfdiff diff old.pdf new.pdf --format json|ai-json|md|html --output out`
  - `spdfdiff diff old.pdf new.pdf --fail-on-changes`
  - `spdfdiff inspect file.pdf --format json`
  - `spdfdiff extract file.pdf --format json`
  - extract JSON table entries include repeated header-row and continuation
    group evidence when page-split table candidates share a stable header
    signature.
  - `spdfdiff corpus tests/fixtures/real_world --output corpus_report.json`
  - corpus manifests may include partial-file and diagnostic-code ceilings for
    compatibility regression gates; partial files are extraction successes with
    warning/error diagnostics, while informational diagnostics remain counted
  - `spdfdiff benchmark --pages 50 --output benchmark.json`
  - `spdfdiff review review.ai.json --endpoint http://127.0.0.1:8080/v1 --model local-model --output review.llm.json`
  - `spdfdiff check --config .spdfdiff.toml`
  - `spdfdiff visual-diff old.pdf new.pdf --renderer-command <cmd> --output visual.json`
- Exit codes must match `plan/01_ARCHITECTURE.md`.
- Missing input files and unsupported encrypted/protected PDFs need useful user-facing errors.

## Verification

Run:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

For CLI/report slices, include command tests or snapshot tests for JSON, Markdown, missing files, output writing, and deterministic ordering.
