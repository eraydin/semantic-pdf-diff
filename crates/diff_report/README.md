# diff_report

Report generation for semantic PDF diff and comparison results.

`diff_report` renders a `spdfdiff_types::DiffDocument` into deterministic JSON,
AI review JSON, Markdown, and self-contained HTML. It is intended for tools that
need stable, evidence-preserving PDF comparison output rather than pixel
screenshots or nondeterministic prose summaries.

## What This Crate Provides

- Pretty JSON serialization of the stable diff report IR.
- AI review JSON with summary counts, question hints, neutral candidate tags,
  confidence buckets, evidence bundles, text hunks, and diagnostic summaries.
- AI review tags for repeated page-region changes when evidence is classified
  as a candidate header, footer, or page template.
- Markdown summaries with old/new evidence text and page references.
- Self-contained HTML reports with side-by-side evidence.
- Deterministic inline SVG evidence overlays when changed nodes include page and
  bounding-box evidence.
- HTML output without external network resources.

## Pipeline Context

`diff_report` is the final rendering stage:

```text
diff_core DiffDocument -> diff_report JSON / ai-json / Markdown / HTML
```

The crate does not parse PDFs and does not compute semantic matches. It renders
the evidence already present in `DiffDocument`.

## AI Review JSON Is Not An LLM

The AI review report is a prompt-ready, deterministic artifact. It groups
evidence and suggests neutral review questions, but it does not call an LLM and
does not make legal, medical, financial, or business conclusions.

## Current Compatibility Boundary

HTML overlays use PDF user-space bounding boxes from extraction and diff
evidence. They are useful for locating changed text/layout evidence, but they
are not a full visual renderer and should not be treated as pixel-perfect page
renderings.
