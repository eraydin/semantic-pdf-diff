# semantic-pdf-diff

`semantic-pdf-diff` is a Rust CLI for comparing text content in simple,
digitally generated PDFs and producing stable semantic diff reports.

The current CLI entry point is `spdfdiff`.

## Current Capabilities

The `spdfdiff diff` command currently compares extracted text from simple,
digitally generated PDFs and writes stable diff JSON, AI review JSON, Markdown,
or basic self-contained HTML reports.

The CLI extraction path resolves page content streams across all parsed pages
and applies simple font resource dictionaries with `/ToUnicode` CMap streams
before building semantic text blocks. This covers the current sample PDFs that
use Type0 fonts and hex `Tj`/`TJ` text. Broader font resource modeling, image
payloads are compared by deterministic stream hash for object-level image
changes. Native vector graphic comparison, annotation/link comparison, OCR,
style classification, and table-cell semantics remain incremental compatibility
work rather than public-alpha claims. Unsupported vector, annotation, and
missing text-layer surfaces are emitted as stable diagnostics instead of being
silently treated as fully supported semantic diffs.
The diff engine also emits structured word-level text hunks for modified
paragraphs and compares selected report-facing document surfaces, including
image payloads, link/annotation dictionaries, embedded-file/FileSpec objects,
outline-like objects, and metadata/XMP objects by deterministic object hashes.
These object-level comparisons preserve evidence but are not yet full semantic
annotation, attachment, outline, or metadata interpreters.
Common non-text drawing, color, clipping, marked-content, and XObject operators
are recognized so visual PDF content does not create `CONTENT_OPERATOR_UNKNOWN`
noise during text extraction.
Large exact-anchor and fuzzy block comparisons are resource-bounded; when a
comparison would exceed configured matrix limits, the diff engine emits a stable
diagnostic and uses deterministic fallback matching instead of allocating an
unbounded matrix.
Incremental-update markers, xref recovery, CID/Type0 fonts without `/ToUnicode`,
simple tagged-PDF structure trees, parent-tree entries, and marked-content IDs
are surfaced as stable diagnostics and extract/inspect summaries so hardening
gaps stay visible in corpus output. When structure elements map cleanly to
marked-content text runs, semantic extraction builds high-confidence tagged
nodes in tagged reading order before falling back to layout heuristics for
unmapped text.
For agent workflows, `diff --format ai-json` emits a compact deterministic
review artifact with summary counts, question hints, neutral candidate tags,
confidence buckets, explanation templates, semantic node identities, and
prompt-ready evidence bundles. It does not call an LLM and does not make legal
or business conclusions.

The `pdf_core` library crate also exposes parser APIs for:

- PDF headers, primitive objects, indirect objects, and stream objects;
- no-filter, `FlateDecode`, `ASCIIHexDecode`, and `RunLengthDecode` stream
  bytes;
- classic xref tables and trailers;
- controlled `/Type /XRef` streams with `/W` and `/Index`;
- controlled `/Type /ObjStm` object streams through `ObjectStore`;
- simple `/StructTreeRoot` structure trees with structure types, parent-tree
  entries, and MCID references;
- resource limits for file size, object count, reference depth, stream bytes,
  decoded stream bytes, content operators, and page count.

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

Write AI review JSON to a file:

```powershell
.\target\debug\spdfdiff.exe diff .\old.pdf .\new.pdf --format ai-json --output .\ai-review.json
```

Run without building the binary first:

```powershell
cargo run -p spdfdiff_cli -- diff .\old.pdf .\new.pdf --format md
```

Return exit code `1` when changes are found:

```powershell
.\target\debug\spdfdiff.exe diff .\old.pdf .\new.pdf --fail-on-changes
```

## Benchmark

Run the synthetic 50-page benchmark gate:

```powershell
.\target\debug\spdfdiff.exe benchmark --pages 50 --output .\benchmark.json
```

The benchmark report includes deterministic phase timing fields for parse,
extract, semantic, diff, and report work, plus the target threshold result.

## Versioning And Releases

The repository uses `VERSION` as the next stable release version. All workspace
crates share that version in source.

Release automation is manually triggered and branch-based:

- running the `Release` workflow against `dev` publishes preview crates as
  `VERSION-preview.<github-run-number>` and create a GitHub prerelease;
- running the `Release` workflow against `main` publishes the stable `VERSION`
  crates and creates the GitHub release marked as latest.

The release workflow rewrites all workspace crate versions and internal
dependency requirements before publishing. Preview releases use exact internal
dependency requirements so every preview crate resolves against the matching
preview set. Published crates are skipped if that exact version already exists,
and crates.io new-crate rate limits are retried automatically.

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

## AI Review JSON

Use `--format ai-json` when another agent or review workflow needs a compact
view over the stable diff report:

```powershell
.\target\debug\spdfdiff.exe diff .\old.pdf .\new.pdf --format ai-json
```

The AI review report includes question hints such as whether payment terms were
modified, neutral tags such as `PaymentTermsCandidate` or `NumericValueChanged`,
old/new semantic node IDs, section hints when detected from the changed text,
text hunks, page/bbox evidence, provenance, and diagnostic counts.

## Parser Library Example

Use `pdf_core` directly when you need parser-level access to objects and xref
data:

```rust
use pdf_core::parse_object_store;
use spdfdiff_types::{ObjectId, ParseConfig};

let bytes = std::fs::read("sample.pdf")?;
let store = parse_object_store(&bytes, ParseConfig::default())?;

if let Some(object) = store.get(ObjectId { number: 1, generation: 0 }) {
    println!("{:?}", object.value);
}
```

For embedded object-stream members, `object.embedded_source` identifies the
containing object stream and embedded object index.

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
