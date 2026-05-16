# Architecture — Semantic PDF Diff Engine

## 1. High-level pipeline

```text
Input A.pdf + B.pdf
        |
        v
PDF Parser
        |
        v
Object Graph + Page Tree
        |
        v
Content Stream Decoder
        |
        v
Graphics/Text Operator Interpreter
        |
        v
Glyph Stream + Drawing Stream
        |
        v
Layout Segmentation
        |
        v
Semantic Tree Builder
        |
        v
Semantic Anchor Generator
        |
        v
Layered Diff Engine
        |
        v
JSON / Markdown / HTML report
        |
        v
SVG overlays in layout-aware report phase
```

The engine must never throw away lower-level evidence. Each higher-level node should preserve enough provenance to explain how it was derived.

## 1.1 MVP compatibility boundary

There are two implementation targets:

1. **Vertical slice target:** classic xref table, direct/indirect objects, Flate streams, page tree, simple text operators, `/ToUnicode`, paragraph diff.
2. **Public alpha target:** xref streams, object streams, resource limits, better font fallbacks, and corpus metrics.

This distinction matters because many modern PDFs use compressed xref/object streams. If those are deferred, the CLI must still fail softly with precise diagnostics rather than claiming unsupported files are malformed.

## 2. Crate architecture

### `spdfdiff_types`

Responsibility: own cross-crate types that would otherwise create dependency cycles.

Public API ownership:

- stable IDs for documents, pages, objects, text runs, semantic nodes, and changes;
- geometry primitives such as `Point`, `Rect`, `Matrix`, and page boxes;
- provenance structs, including file role, page index, object references, content-operation index, and byte range;
- shared diagnostics, diagnostic severity, and diagnostic code conventions;
- report-facing IR structs that must serialize deterministically.

Rules:

- lower-level crates may re-export selected shared types for ergonomics, but they should not define competing versions;
- `spdfdiff_types` must not depend on parser, text extraction, semantic, diff, or report crates;
- report-facing structs should use deterministic ordering and stable serialization defaults.

### `pdf_core`

Responsibility: parse bytes into a resilient PDF object graph.

Public API structs use `spdfdiff_types` for shared IDs, diagnostics, geometry, provenance, and limits where applicable.

Public API draft:

```rust
pub struct PdfDocument {
    pub version: PdfVersion,
    pub trailer: Trailer,
    pub objects: ObjectStore,
    pub diagnostics: Vec<PdfDiagnostic>,
}

pub struct ObjectId {
    pub number: u32,
    pub generation: u16,
}

pub enum PdfObject {
    Null,
    Bool(bool),
    Integer(i64),
    Real(f64),
    Name(PdfName),
    String(PdfString),
    Array(Vec<PdfObject>),
    Dictionary(PdfDict),
    Stream(PdfStream),
    Reference(ObjectId),
}
```

MVP responsibilities:

- header detection;
- classic xref table parsing;
- trailer parsing;
- indirect object parsing;
- dictionary and stream parsing;
- stream length resolution;
- Flate stream decoding;
- object reference resolution with cycle protection;
- diagnostics for unsupported xref streams, object streams, encryption, malformed objects during the vertical slice;
- hard resource limits for object count, stream size, nesting depth, and decompressed bytes.

Compatibility-gate responsibilities before public alpha:

- xref stream parsing;
- object stream extraction;
- hybrid-reference file handling where practical;
- explicit diagnostics for incremental-update sections that are parsed partially.

Later responsibilities:

- incremental update support beyond the latest revision;
- recovery parser for damaged xref tables and damaged PDFs;
- encryption handler.

### `pdf_content`

Responsibility: parse and interpret page content streams.

Public API draft:

```rust
pub struct ContentProgram {
    pub operations: Vec<ContentOp>,
    pub diagnostics: Vec<ContentDiagnostic>,
}

pub enum ContentOp {
    BeginText,
    EndText,
    SetFont { name: PdfName, size: f32 },
    ShowText { bytes: Vec<u8> },
    ShowTextAdjusted { items: Vec<TextShowItem> },
    MoveTextPosition { tx: f32, ty: f32 },
    SetTextMatrix { matrix: Matrix },
    SaveGraphicsState,
    RestoreGraphicsState,
    ConcatMatrix(Matrix),
    DrawImage { name: PdfName },
    Path(PathOp),
    Unknown { operator: String, operands: Vec<ContentValue> },
}
```

MVP operators:

- text: `BT`, `ET`, `Tf`, `Tj`, `TJ`, `'`, `"`, `Td`, `TD`, `Tm`, `T*`, `Tc`, `Tw`, `Tz`, `TL`, `Tr`, `Ts`;
- graphics state: `q`, `Q`, `cm`;
- XObject image/form detection: `Do`;
- basic path operators for figure/table hints: `m`, `l`, `re`, `S`, `s`, `f`, `F`, `B`, `b`, `n`.

### `pdf_text`

Responsibility: convert content operators into positioned glyphs and text runs.

Public API draft:

```rust
pub struct GlyphToken {
    pub id: GlyphId,
    pub unicode: Option<String>,
    pub raw_bytes: Vec<u8>,
    pub page_index: usize,
    pub bbox: Rect,
    pub baseline: LineSegment,
    pub font_ref: Option<ObjectId>,
    pub source: Provenance,
}

pub struct TextRun {
    pub text: String,
    pub glyphs: Vec<GlyphId>,
    pub bbox: Rect,
    pub style: TextStyle,
    pub source: Provenance,
}
```

MVP responsibilities:

- resolve page resource fonts;
- parse simple `/ToUnicode` CMaps;
- implement selected fallback encodings such as WinAnsi for simple fonts when safe;
- decode text shown by `Tj` and `TJ`;
- calculate approximate glyph positions using text matrix, line matrix, CTM, font size, horizontal scaling, character spacing, word spacing, leading, and text rise;
- group glyphs into words and lines;
- normalize Unicode and whitespace;
- preserve raw bytes when Unicode mapping is missing.

### `pdf_semantic`

Responsibility: build the normalized document model used for diffing.

Public API draft:

```rust
pub struct SemanticDocument {
    pub doc_id: DocumentId,
    pub metadata: DocumentMetadata,
    pub pages: Vec<SemanticPage>,
    pub nodes: Vec<SemanticNode>,
    pub diagnostics: Vec<SemanticDiagnostic>,
}

pub enum SemanticNodeKind {
    Page,
    HeadingCandidate { level: Option<u8> },
    Paragraph,
    List,
    ListItem,
    TableCandidate,
    TableRowCandidate,
    TableCellCandidate,
    FigureCandidate,
    Annotation,
    FormField,
    UnknownBlock,
}
```

MVP responsibilities:

- page model;
- line detection;
- block clustering;
- reading order inference;
- heading candidate detection using font size, weight approximation, vertical spacing, numbering patterns;
- paragraph detection;
- simple table candidate detection based on aligned text columns and ruled rectangles;
- figure/image placeholder nodes;
- semantic anchors.

### `diff_core`

Responsibility: compare two semantic documents.

Public API draft:

```rust
pub struct DiffConfig {
    pub ignore_whitespace: bool,
    pub ignore_case: bool,
    pub detect_moves: bool,
    pub layout_tolerance_pt: f32,
    pub min_match_score: f32,
    pub max_match_matrix_cells: usize,
    pub max_greedy_match_candidates: usize,
}

pub struct DiffDocument {
    pub old_fingerprint: String,
    pub new_fingerprint: String,
    pub changes: Vec<SemanticChange>,
    pub summary: DiffSummary,
    pub diagnostics: Vec<DiffDiagnostic>,
}

pub enum ChangeKind {
    Inserted,
    Deleted,
    Modified,
    Moved,
    LayoutChanged,
    StyleChanged,
    MetadataChanged,
    AnnotationChanged,
    Unknown,
}
```

MVP responsibilities:

- stable node fingerprinting;
- text block matching;
- page-aware matching;
- edit distance for paragraphs;
- block move detection;
- bounded exact and fuzzy match fallbacks with diagnostics when a comparison
  would exceed configured matrix limits;
- layout-only change detection;
- confidence scoring;
- severity classification.

### `diff_report`

Responsibility: report generation.

Vertical-slice outputs:

- JSON: canonical machine-readable format;
- Markdown: human-readable summary.

Later report outputs:

- basic HTML report when the report crate is ready;
- SVG overlay report in the layout-aware v0.3 phase, using page coordinate boxes for changed nodes.

### `spdfdiff_cli`

Responsibility: stable command-line interface.

Command draft:

```bash
spdfdiff diff old.pdf new.pdf \
  --format json \
  --output diff.json \
  --ignore-whitespace \
  --detect-moves \
  --layout-tolerance 2.0
```

Subcommands:

```bash
spdfdiff diff old.pdf new.pdf
spdfdiff inspect file.pdf --format json
spdfdiff extract file.pdf --format json
spdfdiff corpus tests/fixtures/real_world --output corpus_report.json
spdfdiff benchmark --pages 50 --output benchmark.json
```

## 3. Semantic anchor strategy

Each block gets multiple anchors:

```rust
pub struct SemanticAnchor {
    pub strong_hash: String,       // normalized text + kind + local structure
    pub weak_hash: String,         // shingles/simhash for fuzzy matching
    pub geometry_hash: String,     // page-relative geometry bucket
    pub reading_order_key: String, // page + block order
    pub source_refs: Vec<Provenance>,
}
```

Matching order:

1. exact strong hash match;
2. same page + high weak text similarity;
3. neighboring heading/section context;
4. cross-page fuzzy match for moved content;
5. layout proximity fallback;
6. unmatched = inserted/deleted.

## 4. Coordinate model

Use PDF points internally. Normalize all rectangles into page coordinate space with a consistent origin. Preserve original page boxes separately, then expose normalized coordinates for semantic matching and report overlays. Normalize page rotation before layout segmentation, but keep original rotation in provenance.

```rust
pub struct Rect {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}
```

Every report can later convert this into SVG/HTML coordinates. Reports must document whether coordinates are in PDF user space or normalized page space.

## 5. Diagnostics model

Diagnostics are first-class output, not logs only.

```rust
pub enum DiagnosticSeverity {
    Info,
    Warning,
    Error,
}

pub struct PdfDiagnostic {
    pub severity: DiagnosticSeverity,
    pub code: String,
    pub message: String,
    pub object: Option<ObjectId>,
    pub page_index: Option<usize>,
}
```

Examples:

- `UNSUPPORTED_XREF_STREAM`
- `UNSUPPORTED_ENCRYPTION`
- `MISSING_TOUNICODE`
- `FONT_WIDTH_UNAVAILABLE`
- `CONTENT_OPERATOR_UNKNOWN`
- `STRUCT_TREE_MALFORMED`
- `PARTIAL_TEXT_EXTRACTION`

## 6. Threading and performance

Initial parallelism:

- parse each PDF independently;
- process pages in parallel after object graph construction;
- build semantic pages in parallel;
- diff matching can be page-grouped, then cross-page move detection can run globally.

Use `rayon` behind a feature flag:

```toml
[features]
default = ["parallel"]
parallel = ["dep:rayon"]
```

## 7. Feature flags

```toml
[features]
default = ["flate", "json", "html", "parallel"]
flate = ["dep:flate2"]
json = ["dep:serde", "dep:serde_json"]
html = []
parallel = ["dep:rayon"]
fuzzing = []
unstable_visual = []
```

## 8. Error policy

- Library functions return `Result<T, PdfDiffError>` only for unrecoverable problems.
- Unsupported PDF features produce diagnostics and partial results when possible.
- CLI exits with:
  - `0`: diff succeeded;
  - `1`: diff succeeded and changes found, if `--fail-on-changes` is enabled;
  - `2`: input or parse failure;
  - `3`: unsupported encrypted or protected PDF;
  - `4`: internal invariant failure.


## 9. Safety and resource limits

PDFs are untrusted binary inputs. The workspace must define configurable limits from the start. Milestone 0/1 should introduce the public `ResourceLimits` type and safe defaults; Milestone 1.5 expands enforcement with hostile fixtures, decompression-bomb tests, recursion caps, and compatibility-gate reporting.

```rust
pub struct ResourceLimits {
    pub max_file_bytes: usize,
    pub max_objects: usize,
    pub max_indirect_depth: usize,
    pub max_stream_bytes: usize,
    pub max_decoded_stream_bytes: usize,
    pub max_content_ops_per_page: usize,
    pub max_pages: usize,
}
```

Rules:

- Never allocate based only on a PDF-declared length without checking limits.
- Detect decompression bombs by bounding decoded output.
- Detect reference cycles and excessive nesting.
- Treat malformed files as diagnostics plus recoverable errors where possible.
- Keep fuzz targets for parser, stream decoding, content tokenization, and CMap parsing.
