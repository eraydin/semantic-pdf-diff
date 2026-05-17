# spdfdiff_cli

CLI entry point for `spdfdiff diff`, `inspect`, `extract`, and `corpus`.

Current command behavior:

- `diff`: runs the vertical-slice semantic diff pipeline and emits JSON/Markdown/HTML.
  `--fail-on-changes` exits with code `1` when a completed diff contains changes.
- `inspect`: parses a PDF with `pdf_core` and reports deterministic
  parser/object diagnostics plus simple tagged-structure and parent-tree
  summaries when present.
- `extract`: runs parse/content/text/semantic extraction across parsed page
  content and reports extracted paragraph text, diagnostics summary, and simple
  tagged-structure summary when present.
- `corpus`: scans a folder for `.pdf` files, runs parse/extract for each file,
  and writes stable aggregate totals (`total`, `parsed`, `partial`, `failed`),
  per-file status, extracted node counts, and diagnostic-code frequency.

The CLI compares image XObject payloads and selected annotation, attachment,
outline, and metadata objects by deterministic hash and emits object-level
changes in diff reports. It still emits stable unsupported-feature diagnostics
for native vector graphic comparison and incomplete annotation/link semantics.
For image-only PDFs, the CLI can OCR supported high-contrast image XObjects with
an external engine. Set `SPDFDIFF_OCR_COMMAND` to a command that accepts a PPM
path and writes recognized text to stdout, or install `tesseract` for the
default `tesseract <image> stdout --psm 6` adapter.
