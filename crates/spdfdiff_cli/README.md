# spdfdiff_cli

Command-line semantic PDF diff and PDF comparison tool.

`spdfdiff_cli` provides the `spdfdiff` executable. It compares digitally
generated PDFs through the workspace pipeline and writes deterministic JSON,
AI-review JSON, Markdown, and self-contained HTML reports. It is intended for
automation, regression checks, release gates, and evidence-preserving review
workflows where a text-only or screenshot-only PDF diff is not enough.

## Commands

- `diff <old.pdf> <new.pdf>` runs the semantic PDF comparison pipeline and emits
  JSON, AI-review JSON, Markdown, or HTML. `--fail-on-changes` exits with code
  `1` when a completed diff contains changes.
- `inspect <file.pdf>` parses a PDF with `pdf_core` and reports deterministic
  parser/object diagnostics plus simple tagged-structure and parent-tree
  summaries and incremental-update offsets when present.
- `extract <file.pdf>` runs parse/content/text/semantic extraction across parsed
  page content and reports paragraph text, aligned text-grid table row/cell,
  row-span, column-span, merged-cell, rectangle table-border hints, diagnostics,
  and tagged-structure summaries.
- `corpus <folder>` scans `.pdf` files, runs parse/extract for each file, and
  writes stable aggregate totals, per-file status, extracted node counts, and
  diagnostic-code frequencies. With `--manifest <json>`, it also checks required
  files, runs declared diff pairs, emits diff diagnostic counts, and reports a
  deterministic release gate. Manifests can pin maximum partial-file counts,
  file diagnostic counts, and diff diagnostic counts as compatibility
  regression baselines. With `--fail-on-gate`, a failed gate exits with code
  `1`.
- `benchmark --pages <n>` runs the synthetic benchmark path and reports
  deterministic phase timing fields for parse, extract, semantic, diff, report,
  and total work.
- `review <review.ai.json>` sends deterministic AI-review JSON to an optional
  OpenAI-compatible HTTP endpoint such as local llama.cpp `llama-server` and
  writes a request/response envelope. This is outside the deterministic diff
  path.

## Example

```powershell
spdfdiff diff old.pdf new.pdf --format html --output diff.html
spdfdiff diff old.pdf new.pdf --format ai-json --output review.json
spdfdiff review review.json --endpoint http://127.0.0.1:8080/v1 --model local-model --output llm-review.json
spdfdiff extract old.pdf --format json --output extract.json
spdfdiff corpus samples --manifest samples\compatibility_corpus_manifest.json --output corpus.json --fail-on-gate
```

## Local LLM Review

The `review` command targets local OpenAI-compatible HTTP servers. For
llama.cpp:

```powershell
llama-server -m C:\models\model.gguf --host 127.0.0.1 --port 8080 -c 8192
spdfdiff review review.ai.json --endpoint http://127.0.0.1:8080/v1 --model local-model --output review.llm.json
```

The command supports optional `--api-key`, `--timeout-seconds`, and
`--max-review-items`. It supports plain `http://` endpoints so local-first
review works without adding TLS or hosted provider dependencies.

## What It Compares Today

- Extracted paragraph text and deterministic text hunks.
- Controlled multi-column reading order plus repeated header, footer, and
  page-template candidate counts in extract JSON.
- Moved blocks and layout-only changes when text anchors and bounding boxes
  support them.
- Simple aligned text-grid table candidates with row/cell, sparse blank-cell,
  row-span, column-span, merged-cell, and rectangle border-hint evidence.
- Image XObject payload changes by deterministic stream hash.
- Native vector path operations and graphic-style operations by deterministic
  parsed content-operation signature.
- Text font resource and font-size changes for unchanged text as
  deterministic `StyleChanged` entries.
- Link/annotation semantic fields, including subtype, rectangle, URI or
  destination, contents, color, border, and quad-point evidence.
- Selected report-facing document surfaces such as embedded-file/FileSpec
  objects, outline-like objects, and metadata/XMP objects by deterministic
  object hash.
- Simple tagged-PDF structure markers and MCID-backed text mapping.

## OCR Path

For image-only PDFs, the CLI can OCR supported high-contrast image XObjects with
an external engine. Set `SPDFDIFF_OCR_COMMAND` to a command that accepts a PPM
path and writes recognized text to stdout, or install `tesseract` for the
default adapter:

```text
tesseract <image> stdout --psm 6
```

OCR is an adapter path, not a replacement for parser/content diagnostics.

## Current Compatibility Boundary

Native vector/style comparison is a parsed-operation signature comparison, not a
pixel renderer. Text style classification currently covers content-stream font
resource and font-size changes for unchanged text. Link/annotation comparison is
field-level semantic comparison, not JavaScript/action execution. Renderer-grade visual diffing and
renderer-grade table reconstruction from arbitrary drawing geometry remain
incremental compatibility work. Unsupported surfaces are reported through stable
diagnostics instead of being silently treated as supported semantic diffs.
