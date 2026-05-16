# spdfdiff_cli

CLI entry point for `spdfdiff diff`, `inspect`, `extract`, and `corpus`.

Current command behavior:

- `diff`: runs the vertical-slice semantic diff pipeline and emits JSON/Markdown/HTML.
  `--fail-on-changes` exits with code `1` when a completed diff contains changes.
- `inspect`: parses a PDF with `pdf_core` and reports deterministic parser/object diagnostics summary.
- `extract`: runs parse/content/text/semantic extraction across parsed page
  content and reports extracted paragraph text plus diagnostics summary.
- `corpus`: scans a folder for `.pdf` files, runs parse/extract for each file,
  and writes stable aggregate totals (`total`, `parsed`, `partial`, `failed`),
  per-file status, extracted node counts, and diagnostic-code frequency.

The CLI compares image XObject payloads and selected annotation, attachment,
outline, and metadata objects by deterministic hash and emits object-level
changes in diff reports. It still emits stable unsupported-feature diagnostics
for native vector graphic comparison, incomplete annotation/link semantics, and
image-only PDFs that require OCR.
