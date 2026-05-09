# Data Model and Diff IR

## 1. Why a dedicated IR is necessary

A PDF is not a semantic document format by default. It is primarily a page description format. Text may be split across many drawing operations, drawn out of reading order, encoded through custom fonts, or positioned manually.

The engine therefore needs a normalized intermediate representation that separates:

- raw PDF object structure;
- page drawing instructions;
- text/glyph evidence;
- inferred layout;
- semantic interpretation;
- final diff result.

Do not make the diff engine depend directly on raw PDF objects. Raw object diffs are useful, but semantic comparison must operate on normalized structures.

## 2. Layered IR

```text
PdfDocument
  ObjectStore
  PageTree
    PageObject
      ContentProgram
        ContentOp[]
          GlyphToken[]
            TextRun[]
              LayoutLine[]
                LayoutBlock[]
                  SemanticNode[]
                    SemanticChange[]
```

Each layer should have its own tests and snapshots. IDs and ordering must be deterministic across machines and runs.

Shared report-facing types belong in `spdfdiff_types`. This includes IDs, geometry, provenance, diagnostic severity/codes, and serialized IR structs used across crate boundaries. Individual crates may define private parser/interpreter helpers, but they must not create incompatible public versions of these shared models.

## 3. Deterministic identity policy

Do not use Rust's `DefaultHasher`, random UUIDs, pointer addresses, or map iteration order for stable IDs. Use one of these instead:

- explicit structural paths such as `page:3/block:12`;
- canonicalized content hashes such as `sha256(kind + normalized_text + local_context)`;
- deterministic counters assigned after sorting by page, reading order, and bbox.

Object numbers are useful provenance, but they must not be the only semantic identity because two equivalent PDFs may renumber every object.

## 4. Provenance

Every extracted item should preserve provenance.

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    pub file_role: FileRole,
    pub object_id: Option<ObjectId>,
    pub page_index: Option<usize>,
    pub stream_object_id: Option<ObjectId>,
    pub content_op_index: Option<usize>,
    pub byte_range: Option<ByteRange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileRole {
    Old,
    New,
}
```

Why it matters:

- AI agents can cite exact evidence.
- Debugging bad extraction becomes possible.
- Reports can link semantic changes back to PDF internals.
- Future editor features can map a change to editable objects.

## 5. Geometry

```rust
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Matrix {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}
```

Rules:

- Store coordinates in PDF points.
- Normalize page rotation early.
- Preserve original MediaBox/CropBox.
- Avoid integer rounding in core logic.
- Only convert coordinates for display in `diff_report`.

## 6. Text evidence model

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextRun {
    pub id: TextRunId,
    pub text: String,
    pub normalized_text: String,
    pub bbox: Rect,
    pub baseline: Option<LineSegment>,
    pub font: Option<FontRef>,
    pub font_size: Option<f32>,
    pub glyphs: Vec<GlyphToken>,
    pub source: Provenance,
}
```

Normalization policy:

- Unicode NFC normalization.
- Collapse repeated whitespace by default for semantic matching.
- Preserve original text separately.
- Normalize hyphenation only behind a config option.
- Normalize smart quotes only behind a config option.

## 7. Layout model

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutBlock {
    pub id: LayoutBlockId,
    pub page_index: usize,
    pub bbox: Rect,
    pub lines: Vec<LayoutLine>,
    pub kind_hint: BlockKindHint,
    pub reading_order: usize,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BlockKindHint {
    BodyText,
    HeadingCandidate,
    ListCandidate,
    TableCandidate,
    FigureCandidate,
    HeaderFooterCandidate,
    Unknown,
}
```

## 8. Semantic document model

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticDocument {
    pub fingerprint: String,
    pub metadata: DocumentMetadata,
    pub pages: Vec<SemanticPage>,
    pub nodes: Vec<SemanticNode>,
    pub roots: Vec<SemanticNodeId>,
    pub diagnostics: Vec<SemanticDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticNode {
    pub id: SemanticNodeId,
    pub kind: SemanticNodeKind,
    pub parent: Option<SemanticNodeId>,
    pub children: Vec<SemanticNodeId>,
    pub page_span: PageSpan,
    pub bbox: Option<Rect>,
    pub text: Option<String>,
    pub normalized_text: Option<String>,
    pub style: Option<StyleSummary>,
    pub anchor: SemanticAnchor,
    pub source: Vec<Provenance>,
    pub confidence: f32,
}
```

## 9. Diff output model

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffDocument {
    pub schema_version: String,
    pub old_fingerprint: String,
    pub new_fingerprint: String,
    pub summary: DiffSummary,
    pub changes: Vec<SemanticChange>,
    pub diagnostics: Vec<DiffDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionQuality {
    pub text_confidence: f32,
    pub layout_confidence: f32,
    pub semantic_confidence: f32,
    pub diagnostic_codes: Vec<String>,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticChange {
    pub id: ChangeId,
    pub kind: ChangeKind,
    pub severity: ChangeSeverity,
    pub old_node: Option<SemanticNodeEvidence>,
    pub new_node: Option<SemanticNodeEvidence>,
    pub text_diff: Option<TextDiff>,
    pub layout_diff: Option<LayoutDiff>,
    pub confidence: f32,
    pub reason: ChangeReason,
}
```

## 10. Change kinds

```rust
pub enum ChangeKind {
    Inserted,
    Deleted,
    Modified,
    Moved,
    LayoutChanged,
    StyleChanged,
    MetadataChanged,
    AnnotationChanged,
    FormFieldChanged,
    ObjectChanged,
    Unknown,
}
```

## 11. Severity model

```rust
pub enum ChangeSeverity {
    Critical,  // reserved for caller-provided/domain classifiers
    Major,     // paragraph modified, table row changed
    Minor,     // style/layout-only change
    Info,      // metadata or object-level-only change
}
```

The core engine should not hard-code legal/business semantics. It may emit neutral candidate signals such as `ValueChanged`, `DateChanged`, or `NumberChanged`, but business severity must come from a caller-provided classifier. It should provide hooks:

```rust
pub trait SeverityClassifier {
    fn classify(&self, change: &SemanticChange) -> ChangeSeverity;
}
```

Default classifier:

- never emits `Critical`;
- text insertion/deletion/modification: `Major` unless the caller chooses a different policy;
- moved with no text change: `Minor`;
- layout-only: `Minor`;
- metadata-only: `Info`;
- failed extraction in changed region: `Major` with low confidence.

## 12. Text diff representation

```rust
pub struct TextDiff {
    pub old_text: String,
    pub new_text: String,
    pub hunks: Vec<TextDiffHunk>,
}

pub struct TextDiffHunk {
    pub kind: TextHunkKind,
    pub old_range: Option<TextRange>,
    pub new_range: Option<TextRange>,
    pub old_text: Option<String>,
    pub new_text: Option<String>,
}

pub enum TextHunkKind {
    Equal,
    Insert,
    Delete,
    Replace,
}
```

## 13. Layout diff representation

```rust
pub struct LayoutDiff {
    pub old_bbox: Option<Rect>,
    pub new_bbox: Option<Rect>,
    pub delta_x: Option<f32>,
    pub delta_y: Option<f32>,
    pub delta_width: Option<f32>,
    pub delta_height: Option<f32>,
    pub page_changed: bool,
    pub reading_order_changed: bool,
}
```

## 14. Stable JSON policy

Output JSON must be stable:

- sort keys where practical, preferably with `BTreeMap` for report-facing maps;
- deterministic IDs;
- deterministic ordering of changes;
- no timestamps unless explicitly requested;
- no random UUIDs;
- all floating-point values rounded at report boundary, not in internal computation.

Recommended rounding for reports:

- coordinates: 2 decimals;
- confidence: 3 decimals;
- timings: optional and disabled by default.

## 15. Example JSON shape

```json
{
  "schema_version": "0.1.0",
  "old_fingerprint": "sha256:...",
  "new_fingerprint": "sha256:...",
  "summary": {
    "inserted": 1,
    "deleted": 0,
    "modified": 2,
    "moved": 1,
    "layout_changed": 3
  },
  "changes": [
    {
      "id": "chg_0001",
      "kind": "Modified",
      "severity": "Major",
      "confidence": 0.94,
      "reason": "same_heading_context_and_high_text_similarity",
      "old_node": {
        "node_id": "old_p3_b12",
        "page": 3,
        "bbox": { "x0": 72.0, "y0": 420.2, "x1": 510.0, "y1": 455.3 },
        "text": "Payment is due within 30 days."
      },
      "new_node": {
        "node_id": "new_p3_b12",
        "page": 3,
        "bbox": { "x0": 72.0, "y0": 420.2, "x1": 510.0, "y1": 455.3 },
        "text": "Payment is due within 15 days."
      }
    }
  ],
  "diagnostics": []
}
```
