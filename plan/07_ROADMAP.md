# Roadmap — Semantic PDF Diff Engine

## Phase 1 — Vertical-slice semantic text diff

### Objective

Produce reliable semantic text/block diffs for controlled digitally generated PDFs using the supported subset.

### Features

- classic xref parsing;
- page tree resolution;
- Flate stream decoding;
- content stream text operator parsing;
- simple font and ToUnicode handling;
- glyph/text-run extraction;
- paragraph/heading candidate detection;
- block-level insert/delete/modify/move/layout diff;
- stable JSON report;
- simple Markdown summary;
- CLI.

### Expected result

Useful for proving the architecture on generated contracts, invoices, reports, and documentation PDFs with extractable text. Not yet a broad real-world compatibility claim.

## Phase 1.5 — Public-alpha compatibility gate

### Objective

Handle enough modern PDFs to make the public alpha honest.

### Features

- xref streams;
- object streams;
- selected incremental-update support or exact diagnostics;
- more filters;
- better CID fonts;
- better CMap parser;
- annotations;
- file attachments and embedded file specifications;
- outlines/bookmarks;
- metadata and XMP diff;
- redaction overlays, hidden text, and layer/visibility semantics;
- corpus runner improvements.

### Expected result

Can process a local corpus with partial failures clearly diagnosed and xref/object-stream support measured explicitly.

## Phase 2 — Tagged PDF semantic mode

### Objective

Use actual PDF semantic structure when available.

### Features

- `/StructTreeRoot` parser;
- structure element tree;
- parent tree resolution;
- MCID mapping;
- tagged reading order;
- heading/list/table/figure mapping from structure types;
- confidence preference for tagged semantic nodes.

### Expected result

High-precision semantic diffs for accessible/tagged PDFs, with fallback to layout heuristics for untagged PDFs.

Current compatibility-gate progress: simple `/StructTreeRoot` trees are parsed
into deterministic structure elements with structure type names and MCID
references, controlled `/ParentTree` entries are summarized, CLI
inspect/extract JSON reports expose tagged summaries, and explicit MCID-to-text
matches can produce high-confidence tagged semantic nodes in structure order.
Full parent-tree use and broader tagged-PDF coverage remain future Phase 2
work.

## Phase 3 — Advanced layout and table diff

### Objective

Make layout and table comparison genuinely useful.

### Features

- robust multi-column reading order;
- header/footer detection;
- table row/cell reconstruction;
- table cell-level diff;
- figure/image movement detection;
- redaction-layer classification;
- hidden-text and overlapping-layer categorization;
- repeated content detection;
- page template detection.

### Expected result

Useful for reports, statements, financial documents, and formal multi-column PDFs.

## Phase 4 — Visual fallback mode

### Objective

Add optional page-render/pixel comparison for unsupported or ambiguous cases.

### Features

- optional renderer integration behind feature flag;
- page raster diff;
- visual heatmap;
- align visual differences with semantic nodes;
- scanned/OCR detection.

### Expected result

The engine can say: “semantic text changed here” versus “visual-only change here” versus “unsupported scanned content.”

## Phase 5 — AI-native review layer

### Objective

Make output maximally useful to AI agents without embedding an LLM in the core engine.

### Features

- compact AI summary JSON;
- severity plugins;
- domain-specific classifiers;
- clause/section identity tracking;
- change explanation templates;
- prompt-ready evidence bundles;
- optional external LLM adapter crate.

### Expected result

AI agents can ask and answer:

- “Which contractual obligations changed?”
- “Were payment terms modified?”
- “Did layout change without text changing?”
- “Which changes are low-confidence because extraction was incomplete?”

## Phase 6 — Library ecosystem and integrations

### Objective

Make the engine embeddable.

### Features

- stable Rust library API;
- C ABI wrapper;
- WASM build;
- Node/Python bindings;
- GitHub Action for PDF regression tests;
- desktop-app integration layer.

### Expected result

The engine can power CI checks, web apps, desktop PDF tools, and AI document pipelines.

## Prioritization matrix

| Feature | User value | Difficulty | Priority |
|---|---:|---:|---:|
| JSON semantic diff | High | Medium | P0 |
| Text extraction with positions | High | High | P0 |
| Paragraph/block diff | High | Medium | P0 |
| HTML report | High | Medium | P1 |
| Move detection | High | Medium | P1 |
| Xref streams | High | High | P0.5 |
| Object streams | High | High | P0.5 |
| Tagged PDF | High | High | P1/P2 |
| Table cell diff | Medium/High | High | P2 |
| Visual fallback | Medium | High | P3 |
| OCR | Medium | Very High | Later |
| PDF editing | High | Very High | Separate product line |

## Recommended MVP release name

`v0.1.0 — Semantic Text Diff Vertical Slice`

Release requirements:

- CLI usable;
- JSON stable enough for internal use;
- simple Markdown summary;
- supports synthetic corpus and selected controlled PDFs;
- diagnostics documented;
- known limitations explicit.

## Recommended v0.2.0 release name

`v0.2.0 — Public Alpha Compatibility Gate`

Release requirements:

- xref streams;
- object streams;
- resource limits;
- corpus metrics;
- compatibility diagnostics.

## Recommended v0.3.0 release name

`v0.3.0 — Layout-Aware Diff`

Release requirements:

- better layout-only change detection;
- heading/list/table candidates;
- HTML/SVG overlay report;
- corpus runner metrics.

## Recommended v0.4.0 release name

`v0.4.0 — Tagged PDF Semantics`

Release requirements:

- structure tree parser;
- MCID mapping;
- tagged reading order;
- tagged semantic nodes preferred over heuristics.
