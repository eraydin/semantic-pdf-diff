# semantic-pdf-diff

`semantic-pdf-diff` is a Rust CLI for comparing text content in simple,
digitally generated PDFs and producing stable semantic diff reports.

The current CLI entry point is `spdfdiff`.

## Build

From the repository root:

```powershell
cargo build --workspace
```

The debug binary is written to:

```powershell
.\target\debug\spdfdiff.exe
```

You can also run it directly through Cargo:

```powershell
cargo run -p spdfdiff_cli -- diff .\old.pdf .\new.pdf
```

## Compare Two PDFs

Generate the default JSON report:

```powershell
.\target\debug\spdfdiff.exe diff .\old.pdf .\new.pdf
```

Write JSON to a file:

```powershell
.\target\debug\spdfdiff.exe diff .\old.pdf .\new.pdf --format json --output .\diff.json
```

Write Markdown to a file:

```powershell
.\target\debug\spdfdiff.exe diff .\old.pdf .\new.pdf --format md --output .\diff.md
```

Run without building the binary first:

```powershell
cargo run -p spdfdiff_cli -- diff .\old.pdf .\new.pdf --format md
```

## JSON Example

For a PDF where one paragraph changes from `Hello` to `Hello world`, the JSON
report includes this kind of summary and change entry:

```json
{
  "schema_version": "0.1.0",
  "old_fingerprint": ".\\old.pdf",
  "new_fingerprint": ".\\new.pdf",
  "summary": {
    "inserted": 0,
    "deleted": 0,
    "modified": 1,
    "moved": 0,
    "layout_changed": 0
  },
  "changes": [
    {
      "id": "change-0000",
      "kind": "Modified",
      "severity": "Major",
      "confidence": 0.9,
      "reason": "paragraph text differs at the same reading-order position"
    }
  ],
  "diagnostics": [
    {
      "severity": "Warning",
      "code": "MISSING_TOUNICODE",
      "message": "using literal-string fallback text because no ToUnicode map is available",
      "object": null,
      "page_index": 0
    }
  ]
}
```

Each change includes old/new evidence when extracted text is available, including
page number, bounding box, text, and provenance fields.

## Markdown Example

```powershell
.\target\debug\spdfdiff.exe diff .\old.pdf .\new.pdf --format md
```

Example output:

```markdown
# Semantic PDF Diff

| Metric | Count |
| --- | ---: |
| Inserted | 0 |
| Deleted | 0 |
| Modified | 1 |
| Moved | 0 |
| Layout changed | 0 |

## Changes

- `change-0000` Modified Major: paragraph text differs at the same reading-order position

## Diagnostics

- `Warning` `MISSING_TOUNICODE` using literal-string fallback text because no ToUnicode map is available
```

## Exit Behavior

- `0`: command completed successfully.
- `2`: input, parse, write, or internal processing error.

Diagnostics are included in successful reports when extraction can continue with
partial information.
