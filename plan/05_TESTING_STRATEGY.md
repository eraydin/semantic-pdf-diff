# Testing Strategy — Semantic PDF Diff Engine

## 1. Testing philosophy

PDF parsing and semantic extraction are failure-prone. Testing must cover both correctness and graceful degradation.

The engine should be tested at every layer:

1. byte parser;
2. object graph;
3. stream decoding;
4. content tokenizer;
5. text extraction;
6. layout segmentation;
7. semantic tree;
8. diff engine;
9. reports and CLI.

Avoid only testing end-to-end behavior. End-to-end tests are necessary, but they are too coarse to debug PDF internals.

## 2. Test fixture categories

```text
tests/fixtures/
  synthetic/
    minimal_one_page.pdf
    two_pages.pdf
    inserted_paragraph_old.pdf
    inserted_paragraph_new.pdf
    modified_paragraph_old.pdf
    modified_paragraph_new.pdf
    moved_paragraph_old.pdf
    moved_paragraph_new.pdf
    layout_shift_old.pdf
    layout_shift_new.pdf
  malformed/
    truncated_object.pdf
    broken_xref.pdf
    missing_stream_length.pdf
    unsupported_filter.pdf
    recursive_reference.pdf
    decompression_bomb.pdf
  compatibility/
    xref_stream.pdf
    object_stream.pdf
    hybrid_reference.pdf
  tagged/
    simple_tagged_paragraphs.pdf
    malformed_struct_tree.pdf
  real_world/
    README.md
```

Synthetic PDFs should be deterministic and small. Real-world PDFs should be stored only when licensing allows it; otherwise keep local-only corpus instructions.

## 3. Unit tests

### `pdf_core`

Test cases:

- primitive parsing;
- string escaping;
- hex strings;
- nested dictionaries;
- object references;
- stream length resolution;
- Flate decoding;
- xref lookup;
- trailer root lookup.

Required invariant:

```text
Invalid input must not panic.
```

### `pdf_content`

Test cases:

- content lexical tokens;
- operator parsing;
- operand stack behavior;
- text-state transitions;
- `TJ` array handling;
- graphics-state stack.

Required invariant:

```text
Unknown operators must be preserved as diagnostics, not discarded silently.
```

### `pdf_text`

Test cases:

- ToUnicode `bfchar`;
- ToUnicode `bfrange`;
- single-byte and multi-byte codes;
- missing ToUnicode;
- glyph position monotonicity;
- text run grouping.

Required invariant:

```text
If Unicode mapping is not known, the engine must preserve raw bytes and emit diagnostics.
```

### `pdf_semantic`

Test cases:

- line clustering;
- paragraph clustering;
- heading candidate detection;
- list candidate detection;
- table candidate detection;
- reading order;
- stable anchors.

Required invariant:

```text
Semantic IDs and anchors must be deterministic.
```

### `diff_core`

Test cases:

- identical documents;
- inserted block;
- deleted block;
- modified block;
- moved block;
- layout-only change;
- style-only change;
- page count change;
- low-confidence unmatched cases.

Required invariant:

```text
Diff output ordering must be deterministic.
```

## 4. Golden snapshot tests

Use snapshot testing for:

- parsed object summaries;
- extracted text runs;
- semantic document JSON;
- diff document JSON;
- Markdown report;
- basic HTML report structural fragments when HTML reporting is enabled;
- SVG overlay fragments starting in the layout-aware v0.3 phase.

Recommended tool:

- `insta` for Rust snapshot tests.

Snapshot rule:

- Snapshots should not include timing, absolute paths, random IDs, or nondeterministic map order.

## 5. Property tests

Recommended tool:

- `proptest`.

Useful properties:

### Parser properties

- whitespace insertion around tokens should not change parsed primitives;
- dictionary key order should not change semantic meaning;
- arrays preserve item order;
- valid generated primitives should round-trip through debug serialization if a writer exists later.

### Diff properties

- diffing a document with itself returns zero semantic changes;
- swapping old/new should invert insert/delete changes;
- identical normalized text with small layout movement should not become text modification;
- raising layout tolerance should not increase layout-change count.

## 6. Fuzzing

Recommended tool:

- `cargo-fuzz`.

Targets:

```text
fuzz_targets/parse_pdf.rs
fuzz_targets/parse_object.rs
fuzz_targets/parse_content_stream.rs
```

Fuzzing acceptance:

- no panics;
- no unbounded memory allocation;
- no infinite loops;
- no stack overflow on deeply nested objects;
- decompressed output never exceeds configured limits.

Current implementation:

- standalone `cargo-fuzz` targets live under `fuzz/fuzz_targets` for
  whole-PDF parsing, primitive/object parsing, and content stream tokenization;
- seed corpora live under `fuzz/corpus`;
- `/ToUnicode` fuzzing remains tied to the existing feature-gated malformed
  tests until the CMap parser is moved behind a public library API.

## 7. Compatibility-gate tests

The public alpha should include explicit tests for modern-PDF constructs:

- xref streams;
- object streams;
- hybrid-reference files where feasible;
- incrementally updated PDFs;
- missing or malformed `/ToUnicode`;
- simple WinAnsi fallback fonts.

These tests prevent the documentation from claiming broader support than the engine actually has.

## 8. Differential testing

The core implementation should not depend on third-party PDF libraries, but external tools can be used in optional development-only tests to compare extraction behavior.

Use cases:

- compare page count;
- compare rough text extraction;
- compare whether a file is parseable;
- detect major gaps in real-world PDFs.

Keep these tests optional and excluded from default CI unless the dependency is lightweight and license-compatible.

## 9. Corpus runner

Command:

```bash
spdfdiff corpus tests/fixtures/real_world --output corpus_report.json
```

Report fields:

```json
{
  "total": 100,
  "parsed": 87,
  "partial": 9,
  "failed": 4,
  "diagnostic_counts": {
    "MISSING_TOUNICODE": 22,
    "UNSUPPORTED_XREF_STREAM": 5
  }
}
```

Use the corpus runner to track progress over time.
Manifest gates should pin compatibility regression baselines, including maximum
partial files and selected diagnostic-code counts, not just parse failures.
Manifest compatibility labels should remain `vertical-slice` or
`compatibility-gate` until a `public-alpha` manifest has at least 30 parsed
curated files, explicit partial-file thresholds, diff pairs, and zero failed
files/pairs.

## 10. Benchmarking

Recommended tool:

- `criterion` for microbenchmarks.

Benchmark phases:

- parse only;
- page tree resolution;
- content tokenization;
- text extraction;
- semantic build;
- diff only over prebuilt IR;
- full pipeline.

Benchmark fixtures:

- 1-page simple PDF;
- 10-page text PDF;
- 50-page text PDF;
- 100-page text PDF;
- paragraph-heavy contract-like PDF;
- table-heavy report-like PDF.

Initial performance targets:

| Scenario | Target |
|---|---:|
| 1-page vs 1-page | < 300 ms |
| 10-page vs 10-page | < 1.5 s |
| 50-page vs 50-page | < 5 s |
| diff over prebuilt IR | < 500 ms for 50 pages |

These are early targets, not hard guarantees.

## 11. CI checks

Minimum CI:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Extended CI:

```bash
cargo test --workspace --features fuzzing
cargo check --manifest-path fuzz/Cargo.toml --bins
cargo run -p spdfdiff_cli -- corpus samples --manifest samples/compatibility_corpus_manifest.json --output target/ci/corpus.json --fail-on-gate
target/debug/spdfdiff inspect samples/document_v1.pdf --format json --output target/ci/sample-cli/inspect-document_v1.json
target/debug/spdfdiff extract samples/document_v1.pdf --format json --output target/ci/sample-cli/extract-document_v1.json
target/debug/spdfdiff diff samples/document_v1.pdf samples/document_v2.pdf --format json --output target/ci/sample-cli/diff-document.json
cargo run -p spdfdiff_cli -- benchmark --pages 50 --output target/ci/benchmark.json
spdfdiff check --config .spdfdiff.toml
```

The repository CI runs the extended commands as separate non-release quality-gate
jobs so fuzz-feature coverage, manifest corpus thresholds, direct sample-PDF CLI
smoke output, and benchmark smoke results are visible independently from the
minimum Rust fmt/clippy/test job.

## 12. Bug report template

Every parser/extraction bug should include:

- input PDF or minimized fixture;
- command used;
- expected behavior;
- actual behavior;
- diagnostics output;
- whether the file is tagged;
- whether text extraction works in external reference tools;
- minimized content stream if possible.

## 13. Definition of done for quality

The MVP is not done until:

- synthetic corpus passes;
- golden snapshots are stable;
- fuzz targets exist for parser and content stream;
- no known panic on malformed fixtures;
- corpus runner produces a useful summary;
- compatibility gate reports xref/object-stream status explicitly;
- every unsupported feature emits a diagnostic code.
