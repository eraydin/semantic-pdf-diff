# pdf_core

Low-level PDF parsing, object graph, stream handling, diagnostics, and resource-limit
enforcement.

Current stream decoding supports no-filter streams, `FlateDecode`,
`ASCIIHexDecode`, and `RunLengthDecode`. Unsupported filters and failed decodes
produce stable diagnostics while preserving raw bytes when possible.

Current page content resolution supports a single `/Contents` stream reference
or an ordered `/Contents [...]` array of stream references for controlled
vertical-slice fixtures, and exposes ordered content streams across all parsed
pages for CLI extraction.

Current tagged-PDF support parses simple `/StructTreeRoot` trees into
deterministic structure elements with structure type names and MCID references.
Parent tree resolution and tagged reading-order replacement remain later
semantic work.
