# spdfdiff_cli

CLI entry point for `spdfdiff diff`, `inspect`, `extract`, and `corpus`.

Current command behavior:

- `diff`: runs the vertical-slice semantic diff pipeline and emits JSON/Markdown/HTML.
- `inspect`: parses a PDF with `pdf_core` and reports deterministic parser/object diagnostics summary.
- `extract`: runs parse/content/text/semantic extraction for first-page content and reports extracted paragraph text plus diagnostics summary.
- `corpus`: scans a folder for `.pdf` files, parses each deterministically, and writes stable aggregate totals (`total`, `parsed`, `partial`, `failed`).
