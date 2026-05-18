# spdfdiff_types

Shared data model for semantic PDF diff and PDF comparison tools.

`spdfdiff_types` contains the stable, serializable types used across the
`semantic-pdf-diff` workspace: geometry, provenance, diagnostics, resource
limits, semantic change evidence, text hunks, layout-diff evidence, and the
AI-review report IR. It is useful if you want to build custom PDF comparison
pipelines or report consumers while reusing the same deterministic evidence
model as the `spdfdiff` CLI.

## What This Crate Provides

- `PdfDiffError`, `Diagnostic`, and `DiagnosticSeverity` for explicit failure
  and degraded-mode reporting.
- `ResourceLimits` and `ParseConfig` for parser and extraction safety.
- `Provenance`, `ObjectId`, `ByteRange`, `Rect`, `Point`, and `Matrix` for
  preserving where evidence came from in a PDF.
- `DiffDocument`, `SemanticChange`, `SemanticNodeEvidence`, `LayoutDiff`, and
  `TextHunk` for stable semantic PDF comparison output.
- `AiReviewReport` and related review item/tag/confidence types for
  prompt-ready, evidence-preserving review JSON.

## Pipeline Context

The workspace pipeline is:

```text
pdf_core -> pdf_content -> pdf_text -> pdf_semantic -> diff_core -> diff_report
```

This crate sits underneath all of those crates. Downstream crates may re-export
these types for convenience, but `spdfdiff_types` is the canonical owner of the
public report-facing model.

## Determinism And Compatibility

The types are designed for deterministic JSON output. Report-facing output must
not depend on timestamps, random IDs, pointer addresses, absolute local paths, or
unordered map iteration.

The current diff schema version is `0.1.0`. The crate version may change without
implying a schema change; use the `schema_version` field in `DiffDocument` and
AI review reports when validating serialized reports.

## When To Use It

Use `spdfdiff_types` when you need:

- a stable Rust model for semantic PDF comparison reports;
- explicit diagnostics for unsupported PDF features;
- provenance-aware evidence for text, layout, and object-level changes;
- resource-limit configuration shared with the parser stack.

If you only need a command-line PDF diff tool, use the `spdfdiff_cli` crate and
the `spdfdiff` binary instead.
