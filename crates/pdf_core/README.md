# pdf_core

Low-level PDF parsing, object graph, stream handling, diagnostics, and resource-limit
enforcement.

Current stream decoding supports no-filter streams, `FlateDecode`,
`ASCIIHexDecode`, and `RunLengthDecode`. Unsupported filters and failed decodes
produce stable diagnostics while preserving raw bytes when possible.

Current page resolution traverses the catalog `/Pages` tree, honors ordered
`/Kids`, carries inherited page `/Resources`, `/MediaBox`, `/CropBox`, and
`/Rotate` values, and supports a single `/Contents` stream reference or an
ordered `/Contents [...]` array of stream references. Ordered content streams
are exposed across all parsed pages for CLI extraction.

Current tagged-PDF support parses simple `/StructTreeRoot` trees into
deterministic structure elements with structure type names, MCID references, and
controlled `/ParentTree` number-tree entries. Full parent-tree use in semantic
node construction remains later work.
