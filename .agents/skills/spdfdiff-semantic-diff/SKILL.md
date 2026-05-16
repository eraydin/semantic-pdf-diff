---
name: spdfdiff-semantic-diff
description: Implement or review semantic-pdf-diff semantic extraction and diff engine work in crates/pdf_semantic and crates/diff_core. Use for layout clustering, reading order, semantic nodes, headings, lists, table candidates, semantic anchors, matching, edit hunks, move detection, layout diffs, confidence scoring, and neutral severity classification.
---

# SPDFDiff Semantic Diff

## Workflow

1. Read `AGENTS.md`, then read `references/semantic-diff-plan.md`.
2. Keep semantic extraction separate from low-level parsing and report rendering.
3. Use `spdfdiff_types` for shared report-facing IR and diagnostics.
4. Make every heuristic confidence-bearing and deterministic.
5. Add synthetic semantic-document or text-run fixtures before relying on end-to-end PDFs.

## Semantic Extraction Rules

- Cluster lines and blocks deterministically using page index, reading order, and geometry.
- Prefer `Candidate`-style node kinds or `UnknownBlock` over false confidence.
- Preserve page spans, bounding boxes, normalized text, style hints, provenance, and confidence.
- Tagged-PDF structure and marked-content IDs are compatibility-gate surfaces:
  parse simple structure trees, keep diagnostics stable, and use tagged reading
  order only when MCID-to-text mapping is explicit and confidence-bearing.

## Diff Rules

- Match using stable anchors before fuzzy matching.
- Keep exact and fuzzy matching resource-bounded; emit stable diagnostics and
  use deterministic fallback matching instead of allocating unbounded matrices.
- Detect moved content separately from delete plus insert when confidence supports it.
- Keep layout-only changes separate from text modifications.
- The default engine classifier must never emit `Critical`; reserve that for caller-provided classifiers.
- Keep change ordering stable and snapshot-friendly.

## Verification

Run:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

For semantic or diff slices, include tests for identical docs, insert/delete/modify, moved block, layout-only change, stable anchors, and low-confidence unmatched cases.
