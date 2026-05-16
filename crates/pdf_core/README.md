# pdf_core

Low-level PDF parsing, object graph, stream handling, diagnostics, and resource-limit
enforcement.

Current page content resolution supports a single `/Contents` stream reference
or an ordered `/Contents [...]` array of stream references for controlled
vertical-slice fixtures, and exposes ordered content streams across all parsed
pages for CLI extraction.
