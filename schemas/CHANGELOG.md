# Schema Changelog

## 0.1.9

- Added optional `semantic_role` evidence to diff reports so consumers can
  distinguish paragraph changes from candidate headers, footers, page templates,
  tables, and other semantic-node classes.
- Added AI review support for optional old/new semantic roles and the
  `RepeatedPageRegion` tag.

## 0.1.8

- Added JSON Schema files for the stable `DiffDocument` report, AI review JSON,
  and the CI `check` summary report.
- Added `check_report.schema.json` with deterministic pair status, artifact
  paths, suppression counts, diagnostics, and failure messages for CI consumers.

## 0.1.0

- Initial stable diff and AI review report schema versions.
