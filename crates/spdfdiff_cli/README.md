# spdfdiff_cli

CLI entry point for `spdfdiff diff`, `inspect`, `extract`, and `corpus`.

Current command behavior:

- `diff`: runs the vertical-slice semantic diff pipeline and emits JSON/Markdown/HTML.
- `inspect`: parses a PDF with `pdf_core` and reports deterministic parser/object diagnostics summary.
- `extract`: runs parse/content/text/semantic extraction across parsed page
  content and reports extracted paragraph text plus diagnostics summary.
- `corpus`: scans a folder for `.pdf` files, runs parse/extract for each file,
  and writes stable aggregate totals (`total`, `parsed`, `partial`, `failed`),
  per-file status, extracted node counts, and diagnostic-code frequency.

The CLI compares image XObject payloads by deterministic stream hash and emits
object-level image changes in diff reports. It still emits stable
unsupported-feature diagnostics for native vector graphic comparison,
annotation/link comparison, and image-only PDFs that require OCR.
