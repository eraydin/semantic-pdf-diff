# pdf_semantic

Semantic layout extraction for PDF diff and document comparison.

`pdf_semantic` turns positioned text runs into deterministic semantic nodes:
paragraphs, heading candidates, list candidates, table candidates, figure
candidates, anchors, and tagged-structure-backed nodes. It is the bridge between
raw PDF text extraction and semantic diff matching.

## What This Crate Provides

- Line and paragraph clustering using page index, baseline proximity, x order,
  and vertical gap thresholds.
- Candidate detection for controlled headings, bullet/numbered lists, and simple
  aligned text-grid tables.
- Best-effort table row/cell evidence for aligned text grids, including sparse
  rows with deterministic blank cells, conservative row spans, column spans,
  merged-cell placeholders, cell text, bounding boxes, provenance, and column
  positions.
- Table border hint evidence from page-scoped rectangle paths when it overlaps a
  detected table candidate.
- Deterministic semantic anchors with strong normalized-text hashes, weak text
  signatures, geometry buckets, and optional heading context.
- Tagged-PDF structure summaries when `pdf_core` parses a simple
  `/StructTreeRoot`.
- High-confidence tagged nodes when explicit `/MCID` values map structure
  elements to text runs.

## Pipeline Context

`pdf_semantic` consumes text runs from `pdf_text` and produces semantic documents
for `diff_core`:

```text
pdf_text TextRun values -> pdf_semantic SemanticDocument -> diff_core changes
```

The output keeps normalized text, page/bounding-box evidence, provenance, and
confidence so reports can explain why a PDF comparison changed.

## Current Compatibility Boundary

The layout heuristics are deterministic and conservative, but they are not a
general document-understanding engine. Complex multi-column flow, renderer-grade
table reconstruction from arbitrary drawing geometry, style semantics, OCR
cleanup, and full tagged-PDF parent tree behavior remain incremental
compatibility work.
Uncertain content should stay as paragraph or candidate evidence rather than
being promoted to a false high-confidence semantic structure.
