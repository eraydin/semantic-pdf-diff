# Parallel Workstreams for AI Agents

## Coordination rules

1. Use `spdfdiff-orchestrator` when assigning or integrating multiple parallel workstreams.
2. Every agent works in one crate or one clearly named folder.
3. Every agent must add tests for new behavior.
4. Shared public IR, geometry, provenance, IDs, and diagnostics belong in `crates/spdfdiff_types`.
5. Public structs must derive `Debug`, `Clone` where reasonable, and `Serialize`/`Deserialize` only for report/IR-facing types.
6. Do not introduce third-party PDF libraries into core crates.
7. Do not change public IR fields without updating golden tests and `02_DATA_MODEL_AND_DIFF_IR.md`.
8. Use small PRs. Each PR should compile independently.
9. Prefer explicit diagnostics over panics.
10. Add TODO comments only with issue-style tags: `TODO(spdfdiff-123): ...`.
11. Any public claim about PDF compatibility must be backed by a corpus metric or a diagnostic test.

## Orchestrator role

The orchestrator is a thin technical integration role, not a feature owner.

Responsibilities:

- assign crate/folder ownership before parallel work begins;
- protect `crates/spdfdiff_types`, `AGENTS.md`, plan files, diagnostics, serialized IR, and CLI shape from uncoordinated edits;
- sequence shared-boundary changes before downstream work;
- verify that each specialist uses the matching repo-local skill;
- run or require the full workspace gate after integration;
- label scope honestly as `vertical-slice`, `compatibility-gate`, or `public-alpha`;
- pause for clarification when two agents need the same files or a change broadens product claims.

## Workstream A — Low-level PDF parser

### Owner role

Parser agent.

### Crate

`crates/pdf_core`

### Responsibilities

- byte scanner;
- PDF primitive parser;
- indirect object parser;
- xref table parser;
- xref stream/object stream support for the compatibility gate;
- trailer parser;
- stream parser;
- Flate decoder integration;
- object resolver;
- diagnostics.

### Deliverables

- `PdfDocument::parse(bytes: &[u8]) -> Result<PdfDocument, PdfDiffError>`
- `PdfDocument::parse_with_config(bytes: &[u8], config: ParseConfig) -> Result<PdfDocument, PdfDiffError>`
- fixture tests for minimal PDFs;
- malformed input tests;
- object lookup tests.

### Acceptance criteria

- Parses internally generated one-page PDF.
- Resolves page catalog and page tree references.
- Decodes Flate stream fixture.
- Does not panic on truncated files.
- Emits diagnostics for encrypted PDFs.
- Before public alpha, parses xref streams/object streams or records a blocking compatibility gap.

### Can run in parallel with

Workstream B after primitive object parser exists. Workstream G can start immediately using synthetic fixtures.

## Workstream B — Content stream tokenizer/interpreter

### Owner role

Content-program agent.

### Crate

`crates/pdf_content`

### Responsibilities

- content stream lexical tokenizer;
- operand stack;
- operator mapping;
- text-state tracking;
- graphics-state stack;
- initial path/image hints.

### Deliverables

- `parse_content_stream(bytes: &[u8]) -> ContentProgram`
- `interpret_page_content(page, resources) -> PageProgram`
- support for MVP text operators.

### Acceptance criteria

- Parses `BT /F1 12 Tf 72 720 Td (Hello) Tj ET`.
- Handles `TJ` arrays with strings and numeric spacing adjustments.
- Preserves unknown operators as diagnostics.
- Tracks text matrix updates for simple text placement.

### Can run in parallel with

Workstream A using standalone content stream fixtures.

## Workstream C — Font decoding and text extraction

### Owner role

Text extraction agent.

### Crate

`crates/pdf_text`

### Responsibilities

- font resource resolution;
- `/ToUnicode` parser for common CMap patterns;
- byte-to-Unicode mapping;
- glyph positioning;
- text run grouping;
- whitespace normalization.

### Deliverables

- `extract_glyphs(page_program, resources) -> Vec<GlyphToken>`
- `group_text_runs(glyphs) -> Vec<TextRun>`
- diagnostics for missing font metrics or ToUnicode.

### Acceptance criteria

- Extracts `Hello World` from synthetic PDF with ToUnicode.
- Handles multiple `Tj` operations on one line.
- Handles `TJ` array spacing.
- Produces non-empty bounding boxes.
- Does not silently invent Unicode when mapping is unavailable; preserve raw bytes and emit diagnostic.

### Can run in parallel with

Workstream D once TextRun fixtures exist.

## Workstream D — Layout segmentation and semantic tree

### Owner role

Semantic extraction agent.

### Crate

`crates/pdf_semantic`

### Responsibilities

- line clustering;
- block clustering;
- reading order;
- heading candidate detection;
- paragraph detection;
- simple list and table candidate detection;
- semantic anchor generation;
- optional tagged-PDF structure integration later.

### Deliverables

- `build_semantic_document(extracted_pages) -> SemanticDocument`
- deterministic anchors;
- semantic snapshot tests.

### Acceptance criteria

- Groups synthetic multi-line paragraph into one block.
- Detects heading candidate based on larger font and spacing.
- Detects two-column layout reading order in controlled fixture.
- Produces stable semantic node IDs.

### Can run in parallel with

Workstream E using mocked semantic documents.

## Workstream E — Diff algorithm

### Owner role

Diff agent.

### Crate

`crates/diff_core`

### Responsibilities

- node fingerprinting;
- exact matching;
- fuzzy text matching;
- edit distance hunks;
- moved-block detection;
- layout-only change detection;
- severity and confidence scoring.

### Deliverables

- `diff_documents(old, new, config) -> DiffDocument`
- unit tests over synthetic `SemanticDocument` fixtures;
- golden JSON output.

### Acceptance criteria

- Detects inserted paragraph.
- Detects deleted paragraph.
- Detects modified paragraph with text hunks.
- Detects moved paragraph separately from delete+insert.
- Detects layout-only movement when text is unchanged.
- Output ordering is deterministic.

### Can run in parallel with

Workstream D using handcrafted IR fixtures.

## Workstream F — Report generation and CLI

### Owner role

CLI/report agent.

### Crates

- `crates/diff_report`
- `crates/spdfdiff_cli`

### Responsibilities

- JSON serialization;
- Markdown report;
- basic HTML report;
- SVG overlay rendering in the layout-aware v0.3 phase;
- CLI argument parsing;
- exit code behavior;
- basic logging.

### Deliverables

- `spdfdiff diff old.pdf new.pdf`
- `spdfdiff inspect file.pdf`
- `spdfdiff extract file.pdf`
- report snapshot tests.

### Acceptance criteria

- CLI can run against two synthetic PDFs.
- JSON output validates against snapshot.
- Markdown summary includes counts and change list.
- HTML report includes changed block evidence and page references.
- CLI returns useful errors for missing files and encrypted files.

### Can run in parallel with

Workstream E once `DiffDocument` type is stabilized.

## Workstream G — Test corpus, fixtures, fuzzing, benchmarks

### Owner role

Quality agent.

### Folders/crates

- `tools/pdf_fixture_gen`
- `tests/fixtures`
- `tests/golden`
- `benches`

### Responsibilities

- minimal PDF generator;
- corpus runner;
- golden snapshots;
- property tests;
- fuzz targets;
- performance benchmarks.

### Deliverables

- generate one-page text PDF;
- generate multi-page text PDF;
- generate changed paragraph pairs;
- generate moved paragraph pairs;
- generate layout-only change pairs;
- generate malformed/truncated PDFs;
- benchmark runner.

### Acceptance criteria

- `cargo test` creates or validates synthetic fixtures.
- Parser fuzz target exists.
- Content tokenizer fuzz target exists.
- Golden diff outputs are stable.
- Benchmarks report parsing, extraction, semantic build, and diff phases separately.

### Can run in parallel with

All workstreams.

## Workstream H — Tagged PDF and structure tree support

### Owner role

Tagged-PDF agent.

### Crates

- `crates/pdf_core`
- `crates/pdf_semantic`

### Responsibilities

- detect `/StructTreeRoot`;
- parse structure elements;
- resolve parent tree;
- map marked content IDs to content spans;
- use tags as high-confidence semantic nodes.

### Deliverables

- `TaggedStructure` model;
- integration into `SemanticDocument`;
- fixtures with tagged PDF examples.

### Acceptance criteria

- Detects when a PDF has no structure tree.
- Parses a simple structure tree with headings and paragraphs.
- Uses tagged order when available.
- Falls back to layout heuristics when tags are missing or malformed.

### Can run in parallel with

Workstreams D and G after page/content provenance exists.

## Workstream I — Object-level diff

### Owner role

Object-diff agent.

### Crates

- `crates/pdf_core`
- `crates/diff_core`

### Responsibilities

- object graph fingerprinting;
- metadata diff;
- page count and page box changes;
- resource changes;
- image object changes;
- annotation dictionary changes.

### Deliverables

- object diff summary section in `DiffDocument`;
- object-level diagnostics.

### Acceptance criteria

- Detects metadata changes.
- Detects page count changes.
- Detects changed image object stream hash.
- Does not confuse object renumbering with semantic change when content is stable.

## Workstream J — Safety, compatibility, and release gates

### Owner role

Release-quality agent.

### Folders/crates

- `crates/pdf_core`
- `crates/spdfdiff_cli`
- `tests/fixtures/compatibility`
- `tools/corpus_runner` as the implementation behind `spdfdiff corpus`

### Responsibilities

- resource limits;
- decompression-bomb protection;
- reference-cycle detection;
- compatibility metrics;
- blocking/non-blocking diagnostic policy;
- public-MVP readiness checklist.

### Deliverables

- resource-limit enforcement and release-gate checks;
- corpus summary report;
- compatibility gate tests;
- release checklist.

### Acceptance criteria

- malformed or hostile fixtures do not panic;
- decoded stream output is bounded;
- recursive references are capped;
- CLI reports unsupported modern-PDF features precisely;
- public alpha is blocked if xref/object-stream support is absent and real-world corpus target is still claimed.

## Workstream dependency map

```text
Minimal `spdfdiff_types` should land before public crate APIs stabilize.
G can start immediately.
A can start after shared IDs, diagnostics, and baseline limits exist.
B can start after content tokenizer fixtures are defined; it does not need full A.
C depends on B for real integration but can start with mocked ContentProgram.
D depends on C for real integration but can start with mocked TextRun fixtures.
E depends on D types but can start with handcrafted SemanticDocument fixtures.
F depends on E public DiffDocument.
H depends on A/B/C provenance and D semantic model.
I depends on A and E.
J can start immediately and should review A/G/F before public alpha.
```

## Recommended first sprint allocation

- Agent 1: `spdfdiff_types`, shared diagnostics, and baseline resource limits.
- Agent 2: `pdf_core` primitive parser and xref table.
- Agent 3: `tools/pdf_fixture_gen` and synthetic fixtures.
- Agent 4: `pdf_content` tokenizer and text operators.
- Agent 5: `diff_core` using mocked semantic documents.
- Agent 6: CLI skeleton, JSON/Markdown report shape, and compatibility checklist.

This allows fast integration without waiting for full text extraction.
