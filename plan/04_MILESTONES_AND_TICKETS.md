# Milestones and Implementation Tickets

## Implementation Status Snapshot

Legend:

- `Implemented`: ticket acceptance is covered by code and tests, sometimes at
  the compatibility-gate scope noted in the ticket.
- `Partial`: useful code exists, but one or more ticket acceptance points remain
  incomplete or only covered by controlled heuristics.
- `Not implemented`: no meaningful implementation found in the current codebase.

### Milestone Rollup

| Milestone | Status | Notes |
| --- | --- | --- |
| Milestone 0 — Repository skeleton | Implemented | Workspace, shared types, lints, and CI foundation are in place. |
| Milestone 1 — Minimal PDF parser | Implemented | Primitive, indirect object, classic xref/trailer, and stream decoding coverage exists. |
| Milestone 1.5 — Safety and modern-PDF compatibility gate | Partial | Resource limits, xref streams, and object streams are implemented; corpus manifest and release threshold remain. |
| Milestone 2 — Fixture generator and corpus runner | Partial | Corpus runner exists; reusable fixture writer and full fixture snapshot matrix remain incomplete. |
| Milestone 3 — Page tree and content stream parsing | Implemented | Page tree traversal, inherited page attributes, content stream resolution, tokenization, and text operator interpretation are covered. |
| Milestone 4 — Text extraction | Partial | Text extraction, ToUnicode, and glyph positioning work; full public font resource model remains limited. |
| Milestone 5 — Layout and semantic extraction | Partial | Blocks, headings, lists, anchors, and simple aligned text-grid table row/cell candidates exist; rectangle/path table-border hints and robust arbitrary table reconstruction remain. |
| Milestone 6 — Core diff engine | Implemented | Matching, hunks, layout diffs, summary, severity, and deterministic ordering are covered. |
| Milestone 7 — Reports and CLI | Implemented | JSON, AI JSON, Markdown, HTML, CLI commands, outputs, and exit behavior are covered. |
| Milestone 8 — Hardening | Partial | Incremental markers, fonts, tagged PDFs, benchmark, and malformed-input checks exist; prior-revision exposure and standalone fuzz targets remain. |

### Ticket Status

| Ticket | Status | Evidence / remaining gap |
| --- | --- | --- |
| M0-T1 — Create workspace | Implemented | Rust workspace, crates, lints, CI, and shared types exist. |
| M0-T2 — Shared error, diagnostic, resource-limit conventions | Implemented | `PdfDiffError`, stable diagnostics, `ParseConfig`, and `ResourceLimits` are shared through `spdfdiff_types`. |
| M1-T1 — Primitive tokenizer/parser | Implemented | Primitive parser tests cover scalars, arrays, dictionaries, strings, names, comments, and malformed input. |
| M1-T2 — Indirect objects | Implemented | Indirect object, stream object, byte-range, and unterminated-object tests pass. |
| M1-T3 — Xref table and trailer | Implemented | Classic xref/trailer and `ObjectStore` tests pass; xref-stream handling moved beyond this ticket into M1.5-T2. |
| M1-T4 — Stream decoding | Implemented | No-filter, `FlateDecode`, `ASCIIHexDecode`, `RunLengthDecode`, failed decode, and unsupported-filter diagnostics exist. |
| M1.5-T1 — Resource limits | Implemented | File, object, reference-depth, stream, decoded-stream, content-op, and page-count limit checks have stable `RESOURCE_LIMIT_*` errors/tests. |
| M1.5-T2 — Xref stream parser | Implemented | Controlled `/Type /XRef` streams, `/W`, `/Index`, compressed entries, and malformed variants are tested. |
| M1.5-T3 — Object stream extraction | Implemented | Controlled `/ObjStm` extraction resolves embedded objects with provenance; malformed object streams fail softly. |
| M1.5-T4 — Compatibility corpus gate | Partial | `spdfdiff corpus` emits deterministic per-file and diagnostic counts, but there is no committed corpus manifest or release-blocker threshold yet. |
| M2-T1 — Minimal PDF writer for tests | Partial | Deterministic test helpers generate minimal PDFs, but there is no reusable test-only writer module with viewer/opening coverage. |
| M2-T2 — Diff pair fixtures | Partial | Synthetic and real-sample diff pairs are covered by integration tests, but expected snapshots are not present for the full generated fixture matrix. |
| M2-T3 — Corpus runner | Implemented | `spdfdiff corpus <folder> --output <json>` reports parsed/partial/failed files and diagnostics without stopping at first failure. |
| M3-T1 — Page tree resolver | Implemented | Catalog `/Pages` traversal, ordered `/Kids`, inherited resources, MediaBox/CropBox dimensions, rotation, and page-count limits are covered. |
| M3-T2 — Content stream resolver | Implemented | Single stream, content-stream arrays, multi-stream pages, and all parsed pages are covered with provenance and diagnostics. |
| M3-T3 — Content tokenizer | Implemented | Content tokenizer handles numbers, names, strings, arrays, dictionaries, operators, `TJ`, and unknown operators. |
| M3-T4 — Text operator interpreter | Implemented | MVP text operators and graphics-state save/restore are interpreted with matrix/position tests. |
| M4-T1 — Font resource model | Partial | Font references and `/ToUnicode` maps are discovered for extraction, but a full public font resource model with subtype/encoding capture is still limited. |
| M4-T2 — ToUnicode parser MVP | Implemented | `bfchar`, `bfrange`, multi-byte hex mapping, and unsupported CMap diagnostics are tested. |
| M4-T3 — Glyph positioning MVP | Implemented | Text matrix, font size, spacing, `TJ` adjustment, width heuristics, and non-empty bboxes are covered. |
| M4-T4 — Text run grouping | Implemented | Text runs preserve raw text, normalized text, glyph raw bytes, bboxes, source provenance, and stable output order. |
| M5-T1 — Line and block clustering | Implemented | Baseline clustering, x ordering, paragraph grouping, bboxes, and two-column reading order are tested. |
| M5-T2 — Heading candidates | Implemented | Controlled heading heuristic and confidence tests exist. |
| M5-T3 — Lists and table candidates | Partial | Bullet/numbered list and simple aligned text-grid table row/cell candidates exist; rectangle/path table-border hints remain future work. |
| M5-T4 — Semantic anchors | Implemented | Strong/weak text anchors, geometry buckets, and heading context are tested for stability. |
| M6-T1 — Exact and fuzzy matching | Implemented | Exact anchors, fuzzy edited paragraph matching, move relabeling, low-confidence unmatched cases, and bounded fallback matching are tested. |
| M6-T2 — Text hunks | Implemented | Token-level hunks, numeric replacements, small character fallback, and report output are implemented. |
| M6-T3 — Layout diff | Implemented | Structured layout evidence, bbox deltas, reading-order changes, tolerance config, and CLI tolerance tests are implemented. |
| M6-T4 — Summary and severity | Implemented | Change counts, deterministic IDs/order, default severity, confidence, and classifier override tests exist. |
| M7-T1 — JSON report | Implemented | Stable JSON schema, deterministic changes, evidence, diagnostics, text hunks, and layout evidence are emitted. |
| M7-T2 — Markdown report | Implemented | Markdown includes summary, changes, page/evidence lines, text hunks, layout diffs, and diagnostics. |
| M7-T3 — Basic HTML report | Implemented | Self-contained HTML side-by-side report includes page/bbox evidence and inline SVG overlays. |
| M7-T4 — CLI integration | Implemented | `diff`, `extract`, `inspect`, `corpus`, `benchmark`, formats, outputs, OCR path, and exit-code behavior are integration-tested. |
| M8-T1 — Incremental updates and recovery parsing | Partial | Latest `startxref`, `/Prev` diagnostics, and xref recovery exist; prior revision data is not separately exposed beyond diagnostics. |
| M8-T2 — Better font handling | Implemented | CID/Type0 missing-`ToUnicode` diagnostics and deterministic glyph-width heuristics are implemented. |
| M8-T3 — Tagged PDF structure | Implemented | Simple structure trees, parent-tree summaries, MCID preservation, and tagged semantic node ordering are implemented for controlled cases. |
| M8-T4 — Fuzzing and malformed PDFs | Partial | Feature-gated malformed-input tests exist for parser/content tokenizer, but standalone fuzz targets/corpus are not yet present. |
| M8-T5 — Benchmark target | Implemented | CLI benchmark reports parse/extract/semantic/diff/report timings, threshold result, diagnostics, summary, and memory sample when available. |

## Milestone 0 — Repository skeleton

### Goal

Create a compilable Rust workspace with crate boundaries, shared types, CI, formatting, and test conventions.

### Tickets

#### M0-T1 — Create workspace

Tasks:

- create root `Cargo.toml`;
- create crates: `spdfdiff_types`, `pdf_core`, `pdf_content`, `pdf_text`, `pdf_semantic`, `diff_core`, `diff_report`, `spdfdiff_cli`;
- set workspace lints;
- add `rustfmt.toml` and basic CI script.

Acceptance:

- `cargo test --workspace` passes;
- `cargo clippy --workspace --all-targets` passes;
- each crate has a minimal public API and README;
- shared public geometry, provenance, diagnostics, IDs, and report-facing IR types are owned by `spdfdiff_types`.

#### M0-T2 — Add shared error, diagnostic, and resource-limit conventions

Tasks:

- define `PdfDiffError`;
- define diagnostic severity and code style;
- add crate-local diagnostics with conversion into report diagnostics;
- define `ResourceLimits` with safe defaults and make parser-facing APIs accept limits or a parse config from the start.

Acceptance:

- no crate uses raw `String` errors in public APIs;
- no parser code panics for invalid input;
- limits are visible in public API even if some hostile-fixture enforcement remains in Milestone 1.5.

## Milestone 1 — Minimal PDF parser

### Goal

Parse enough PDF structure to load a minimal one-page PDF.

### Tickets

#### M1-T1 — Primitive tokenizer/parser

Implement:

- comments;
- whitespace;
- booleans;
- null;
- integers/reals;
- names;
- literal strings;
- hex strings;
- arrays;
- dictionaries;
- references.

Acceptance:

- unit tests for each primitive;
- malformed primitive tests return errors or diagnostics, not panics.

#### M1-T2 — Indirect objects

Implement:

- `n g obj ... endobj` parser;
- stream object detection;
- byte-range preservation.

Acceptance:

- parses object with dictionary;
- parses object with stream;
- rejects unterminated object gracefully.

#### M1-T3 — Xref table and trailer

Implement:

- locate `startxref`;
- parse classic xref table;
- parse trailer dictionary;
- build `ObjectStore`.

Acceptance:

- parses fixture generated by `pdf_fixture_gen`;
- object lookup by `ObjectId` works;
- unsupported xref stream emits diagnostic.

#### M1-T4 — Stream decoding

Implement:

- stream length lookup;
- `/Filter /FlateDecode`;
- no-filter stream passthrough;
- unsupported filter diagnostic.

Acceptance:

- decodes Flate fixture;
- preserves raw bytes when decode fails;
- diagnostics include object ID.


## Milestone 1.5 — Safety and modern-PDF compatibility gate

### Goal

Prevent the MVP from overpromising. The first parser can support classic xref PDFs, but the public alpha must either support common modern PDF structures or clearly fail with diagnostics.

### Tickets

#### M1.5-T1 — Resource limits

Implement:

- maximum file size;
- maximum object count;
- maximum reference depth;
- maximum stream length;
- maximum decoded stream output;
- maximum content operators per page.

Acceptance:

- decompression-bomb fixture is rejected safely;
- recursive-object fixture is rejected safely;
- diagnostics include limit code and location when known.

#### M1.5-T2 — Xref stream parser

Implement:

- `/Type /XRef` stream detection;
- `/W` field parsing;
- `/Index` handling;
- Flate-decoded xref stream entries;
- free/in-use/compressed object entry handling.

Acceptance:

- parses controlled xref-stream fixture;
- classic xref fixtures remain unchanged;
- unsupported variants produce exact diagnostics.

#### M1.5-T3 — Object stream extraction

Implement:

- `/Type /ObjStm` detection;
- `/N` and `/First` parsing;
- embedded object offset table;
- lazy extraction into `ObjectStore`.

Acceptance:

- resolves indirect objects stored inside object streams;
- object provenance distinguishes top-level object stream and embedded object index;
- malformed object stream fails softly.

#### M1.5-T4 — Compatibility corpus gate

Implement:

- a small local-only corpus manifest;
- per-file parse/extract/diff status;
- diagnostic frequency report;
- release-blocker threshold.

Acceptance:

- corpus runner produces deterministic JSON;
- public-alpha readiness can be evaluated from metrics;
- unsupported features are counted explicitly.

## Milestone 2 — Fixture generator and corpus runner

### Goal

Create controlled PDFs for deterministic testing.

### Tickets

#### M2-T1 — Minimal PDF writer for tests

Implement a test-only PDF generator that can write:

- catalog;
- pages tree;
- one or more page objects;
- one font resource;
- content stream;
- xref table;
- trailer.

Acceptance:

- generated file opens in common PDF viewers;
- generated file is parsed by `pdf_core`;
- generator is deterministic.

#### M2-T2 — Diff pair fixtures

Generate pairs:

- identical files;
- inserted paragraph;
- deleted paragraph;
- modified paragraph;
- moved paragraph;
- layout-only movement;
- changed page count.

Acceptance:

- fixtures committed or generated in tests;
- expected diff snapshots exist.

#### M2-T3 — Corpus runner

Implement:

```bash
spdfdiff corpus tests/fixtures/real_world --output corpus_report.json
```

Acceptance:

- reports parse success/failure count;
- captures diagnostics;
- does not stop at first failure.

## Milestone 3 — Page tree and content stream parsing

### Goal

Resolve pages and tokenize text drawing operations.

### Tickets

#### M3-T1 — Page tree resolver

Implement:

- catalog lookup;
- `/Pages` traversal;
- inherited page resources;
- MediaBox/CropBox extraction;
- rotation extraction.

Acceptance:

- gets correct page count;
- returns page dimensions;
- handles inherited resources.

#### M3-T2 — Content stream resolver

Implement:

- resolve page `/Contents` as stream or array of streams;
- decode streams;
- concatenate content streams with provenance boundaries.

Acceptance:

- one-stream and multi-stream fixtures pass;
- diagnostics identify failing content streams.

#### M3-T3 — Content tokenizer

Implement tokenizer for:

- numbers;
- names;
- strings;
- arrays;
- dictionaries where needed;
- operators.

Acceptance:

- parses simple text content;
- parses `TJ` arrays;
- preserves unknown operators.

#### M3-T4 — Text operator interpreter

Implement MVP operators:

- `BT`, `ET`, `Tf`, `Tj`, `TJ`, `Td`, `TD`, `Tm`, `T*`, `Tc`, `Tw`, `Tz`, `TL`, `q`, `Q`, `cm`.

Acceptance:

- emits text show operations with current text state;
- maintains text matrix for simple examples;
- handles graphics-state save/restore.

## Milestone 4 — Text extraction

### Goal

Extract Unicode text with approximate positions.

### Tickets

#### M4-T1 — Font resource model

Implement:

- resource font lookup;
- base font name extraction;
- subtype extraction;
- encoding dictionary/name capture;
- ToUnicode stream lookup.

Acceptance:

- identifies fonts used by page content;
- missing resources emit diagnostics.

#### M4-T2 — ToUnicode parser MVP

Implement support for common CMap constructs:

- `beginbfchar` / `endbfchar`;
- `beginbfrange` / `endbfrange`;
- hex code mapping.

Acceptance:

- maps fixture bytes to Unicode;
- handles multi-byte hex codes;
- unsupported CMap syntax emits diagnostic.

#### M4-T3 — Glyph positioning MVP

Implement:

- text matrix application;
- font size scaling;
- approximate advance widths;
- TJ spacing adjustments;
- glyph bounding boxes.

Acceptance:

- glyph positions increase left-to-right in simple fixture;
- line movement changes y coordinate;
- bbox is non-empty.

#### M4-T4 — Text run grouping

Implement:

- glyph-to-word grouping;
- word-to-line grouping;
- line-to-run grouping;
- whitespace normalization.

Acceptance:

- extracts expected text from one-line and multi-line fixtures;
- stable output order;
- original and normalized text are both preserved.

## Milestone 5 — Layout and semantic extraction

### Goal

Convert text runs into semantically meaningful blocks.

### Tickets

#### M5-T1 — Line and block clustering

Implement:

- y-axis baseline clustering;
- x-axis ordering;
- vertical gap-based paragraph grouping;
- block bounding boxes.

Acceptance:

- multi-line paragraph groups into one block;
- separate paragraphs remain separate;
- two-column fixture has stable reading order.

#### M5-T2 — Heading candidates

Implement heading heuristic based on:

- font size relative to page median;
- vertical whitespace;
- numbering pattern;
- short text length;
- bold-like font name hints.

Acceptance:

- synthetic heading detected;
- body paragraph not detected as heading;
- confidence score present.

#### M5-T3 — Lists and table candidates

Implement simple detection:

- bullet/numbered list patterns;
- repeated aligned x positions;
- rectangle/path hints for table borders;
- rows/cells as candidate nodes.

Acceptance:

- basic numbered list fixture detected;
- simple 2x2 text table fixture detected;
- uncertain cases remain `UnknownBlock` rather than false confidence.

#### M5-T4 — Semantic anchors

Implement:

- normalized text hash;
- weak text signature/shingles;
- geometry bucket;
- heading context anchor.

Acceptance:

- anchors stable across runs;
- minor layout shift keeps text anchor stable;
- text edit changes strong hash but weak signature remains comparable.

## Milestone 6 — Core diff engine

### Goal

Compare two semantic documents and produce stable change output.

### Tickets

#### M6-T1 — Exact and fuzzy matching

Implement:

- exact anchor matching;
- page-local fuzzy text matching;
- cross-page moved-node candidate matching.

Acceptance:

- unchanged blocks match exactly;
- edited paragraphs match as modifications;
- moved paragraph detected as moved.

#### M6-T2 — Text hunks

Implement:

- token-level diff;
- word-level hunks;
- character-level fallback for small replacements.

Acceptance:

- `30 days` -> `15 days` reported as replacement;
- insertion and deletion hunks are stable;
- whitespace-only changes obey config.

Implemented behavior:

- modified semantic changes include deterministic token-level `text_hunks` with
  normalized text ranges in the JSON report;
- small non-numeric word replacements include character-level fallback hunks,
  while numeric replacements such as `30` -> `15` remain token replacements;
- Markdown and HTML reports surface text hunk evidence for modified paragraphs.

#### M6-T3 — Layout diff

Implement:

- bbox delta;
- page change;
- reading-order change;
- layout tolerance config.

Acceptance:

- layout-only fixture emits `LayoutChanged`;
- tiny changes below tolerance ignored;
- moved text with changed location reports both move and layout evidence.

Current implementation:

- `spdfdiff_types::LayoutDiff` preserves old/new bounding boxes, bbox deltas,
  page-change evidence, and reading-order-change evidence in JSON and AI-review
  JSON;
- `diff_core` attaches layout evidence to layout-only changes, fuzzy matched
  modifications with bbox/page movement, and moved content relabeled from
  insert/delete pairs;
- `spdfdiff diff --layout-tolerance-pt` exposes the bbox/page movement tolerance
  for CLI runs;
- Markdown and HTML reports summarize layout deltas next to text and source
  evidence.

#### M6-T4 — Summary and severity

Implement:

- change counts;
- severity classifier;
- confidence model;
- deterministic change ordering.

Acceptance:

- summary counts match change list;
- default severity is predictable;
- JSON snapshots stable.

## Milestone 7 — Reports and CLI

### Goal

Make the engine usable from terminal and consumable by AI agents.

### Tickets

#### M7-T1 — JSON report

Acceptance:

- JSON schema version included;
- all changes include evidence;
- no nondeterministic fields by default.

#### M7-T2 — Markdown report

Acceptance:

- includes summary table;
- includes change list with page references;
- includes diagnostics section.

#### M7-T3 — Basic HTML report

Acceptance:

- renders side-by-side old/new change list;
- includes page numbers and bounding boxes;
- no external network resources required;
- SVG overlays are deferred to the layout-aware v0.3 phase unless implemented behind an explicit unstable feature.

Implemented behavior:

- HTML diff reports are self-contained and render old/new evidence side by side;
- page numbers and available bounding boxes are shown in PDF user space.

#### M7-T4 — CLI integration

Acceptance:

- `spdfdiff diff old.pdf new.pdf --format json` works;
- `spdfdiff extract file.pdf --format json` works;
- `spdfdiff inspect file.pdf` prints page/object diagnostics;
- `spdfdiff corpus tests/fixtures/real_world --output corpus_report.json` works once the corpus runner exists;
- exit codes documented and tested.

Implemented behavior:

- `--fail-on-changes` returns exit code `1` when a diff completes and changes
  are present;
- encrypted/protected PDFs are rejected with exit code `3` and stable
  `UNSUPPORTED_ENCRYPTION` error text.

## Milestone 8 — Hardening

### Goal

Improve real-world coverage beyond the public-alpha compatibility gate.

### Tickets

#### M8-T1 — Incremental updates and recovery parsing

Acceptance:

- detects and selects the latest revision in incrementally updated PDFs;
- reports prior revisions where practical;
- recovers from selected damaged xref tables without regressing strict parsing.

Implemented compatibility-gate behavior:

- latest `startxref` is selected by the parser;
- repeated `startxref` and trailer `/Prev` markers emit stable diagnostics;
- xref/object-store failures recover through indirect-object scanning with
  `XREF_RECOVERY_USED` when an xref surface was actually present.

#### M8-T2 — Better font handling

Acceptance:

- supports more CID font cases;
- better glyph widths;
- better missing-ToUnicode diagnostics.

Implemented compatibility-gate behavior:

- CID/Type0 fonts without `/ToUnicode` emit `MISSING_TOUNICODE_CID_FONT`;
- glyph width estimation uses deterministic character-shape heuristics.

#### M8-T3 — Tagged PDF structure

Acceptance:

- parses simple structure tree;
- maps MCIDs to semantic nodes;
- uses tagged reading order when confidence is high.

Implemented compatibility-gate behavior:

- simple `/StructTreeRoot` trees are parsed into deterministic structure
  elements with structure type names and MCID references;
- controlled `/ParentTree` number-tree entries are parsed and summarized;
- inspect/extract JSON reports include tagged-structure summaries when present;
- content-stream `/MCID` markers are preserved on text runs;
- tagged reading order is used for mapped semantic nodes when explicit MCID
  mapping is available with confidence.

#### M8-T4 — Fuzzing and malformed PDFs

Acceptance:

- fuzz target for parser;
- fuzz target for content tokenizer;
- no known panic on malformed fixture set.

Implemented compatibility-gate behavior:

- `cargo test --workspace --features fuzzing` enables parser and content
  tokenizer malformed-input fuzz-target tests.

#### M8-T5 — Benchmark target

Acceptance:

- reports timing for parse/extract/semantic/diff/report phases;
- tracks memory usage where possible;
- 50-page synthetic benchmark under target threshold.

Implemented compatibility-gate behavior:

- `spdfdiff benchmark --pages 50 --output benchmark.json` reports phase timings,
  threshold result, diagnostics, summary, and memory sample when a safe
  platform probe is available;
- `diff_core` includes a Criterion benchmark for 50-page semantic documents.
