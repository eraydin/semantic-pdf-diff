# pdf_core

Low-level PDF parser foundation for semantic PDF diff and PDF comparison.

`pdf_core` parses enough of the PDF object graph to support the
`semantic-pdf-diff` pipeline without depending on third-party PDF parser or
renderer libraries. It is aimed at evidence-preserving comparison of digitally
generated PDFs, where partial results and stable diagnostics are better than
silently ignoring unsupported features.

## What This Crate Provides

- PDF header and primitive object parsing.
- Indirect object and stream object parsing with byte-range provenance.
- Classic xref table and trailer parsing.
- Controlled `/Type /XRef` xref stream support.
- Controlled `/Type /ObjStm` object stream extraction through `ObjectStore`.
- Stream decoding for no-filter, `FlateDecode`, `ASCIIHexDecode`, and
  `RunLengthDecode`.
- Catalog `/Pages` traversal with ordered `/Kids`, inherited `/Resources`,
  `/MediaBox`, `/CropBox`, and `/Rotate`.
- Page content stream resolution for single `/Contents` streams and ordered
  `/Contents [...]` arrays.
- Simple `/StructTreeRoot` and parent-tree summaries with structure type names
  and MCID references.
- Resource-limit enforcement through `spdfdiff_types::ResourceLimits`.

## Pipeline Context

`pdf_core` is the first stage of the workspace pipeline:

```text
PDF bytes -> pdf_core object graph/pages/streams -> pdf_content operators
```

It intentionally does not perform semantic text comparison. Downstream crates
consume its page content streams, object provenance, tagged-structure summaries,
and diagnostics.

## Diagnostics Instead Of Hidden Failure

Unsupported filters, failed stream decodes, malformed object streams, encrypted
PDFs, damaged xrefs, and resource-limit violations produce stable diagnostics or
typed errors. Raw stream bytes are preserved when possible so later tooling can
still report partial evidence.

## Current Compatibility Boundary

This is a compatibility-gate parser foundation, not a claim of broad PDF
renderer compatibility. It currently focuses on parser constructs needed by the
semantic diff CLI and sample corpus. Native rendering, visual diffing, full
annotation semantics, JavaScript actions, and arbitrary damaged-PDF recovery are
outside this crate's current scope.

Use `ParseConfig` and `ResourceLimits` when parsing untrusted PDFs.
