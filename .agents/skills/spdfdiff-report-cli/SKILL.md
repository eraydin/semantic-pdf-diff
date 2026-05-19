---
name: spdfdiff-report-cli
description: Implement or review semantic-pdf-diff report generation and CLI work in crates/diff_report and crates/spdfdiff_cli. Use for stable JSON reports, AI review JSON, Markdown summaries, HTML reports, inline SVG evidence overlays, CLI commands, command arguments, exit codes, output files, corpus command integration, and report snapshot tests.
---

# SPDFDiff Report CLI

## Workflow

1. Read `AGENTS.md`, then read `references/report-cli-plan.md`.
2. Keep `DiffDocument` serialization stable and machine-readable first.
3. Treat JSON, AI review JSON, Markdown, HTML, and inline SVG evidence overlays as deterministic report outputs.
4. Keep the public CLI shape stable: `spdfdiff diff`, `inspect`, `extract`, and `corpus`.
5. Add snapshot-style tests for report output whenever fields or ordering change.

## Report Rules

- Do not include timestamps, absolute paths, random IDs, nondeterministic map order, or machine-specific timings by default.
- Keep JSON canonical enough for agents and CI to consume.
- AI review JSON must stay deterministic, evidence-preserving, and neutral:
  include question hints, candidate tags, confidence buckets, explanation
  templates, semantic node identities, and prompt-ready evidence bundles without
  embedding an LLM or making legal/business conclusions.
- Markdown should summarize counts, changes, page references, and diagnostics.
- HTML must not depend on external network resources.
- Basic HTML diff reports should render old/new evidence side by side and show
  available page/bbox evidence.
- Inline SVG evidence overlays must be deterministic and must state that
  bounding boxes are in PDF user space.
- CLI extraction currently walks parsed page content across all pages. Diff
  reports compare image XObject payloads by deterministic stream hash and
  compare native vector path and graphic-style operations by deterministic
  parsed-operation signatures. Full annotation/link semantics still need stable
  unsupported-feature diagnostics until semantic interpreters exist. Image-only
  PDFs can use the external OCR adapter when `SPDFDIFF_OCR_COMMAND` or
  `tesseract` is available; OCR text must preserve image-object provenance and
  deterministic diagnostics.
- Inspect and extract JSON reports include simple tagged-structure and
  parent-tree summaries when `pdf_core` parses `/StructTreeRoot`; mapped MCID
  text can now produce tagged semantic nodes, while broader tagged-PDF coverage
  remains compatibility-gate work.

## CLI Rules

- Public commands:
  - `spdfdiff diff old.pdf new.pdf --format json|ai-json|md|html --output out`
  - `spdfdiff diff old.pdf new.pdf --fail-on-changes`
  - `spdfdiff inspect file.pdf --format json`
  - `spdfdiff extract file.pdf --format json`
  - `spdfdiff corpus tests/fixtures/real_world --output corpus_report.json`
  - `spdfdiff benchmark --pages 50 --output benchmark.json`
  - `spdfdiff review review.ai.json --endpoint http://127.0.0.1:8080/v1 --model local-model --output review.llm.json`
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
