# pdf_semantic

Layout segmentation, semantic nodes, reading order, and semantic anchors.

Current vertical-slice extraction clusters positioned text runs into deterministic
paragraph nodes using page index, baseline proximity, x ordering, and vertical
gap thresholds. It can mark controlled larger short paragraphs as
`HeadingCandidate`, basic bullet/numbered items as `ListCandidate`, and simple
grid-like short text runs as `TableCandidate`. Each node also receives
deterministic semantic anchors: strong normalized-text hash, weak text signature,
geometry bucket, and optional heading context.

`SemanticDocument` can also carry a tagged-structure summary when `pdf_core`
successfully parses a simple `/StructTreeRoot`. This is evidence for later
tagged reading-order work; text-node construction still falls back to layout
heuristics until MCID-to-text mapping is explicit and confidence-bearing.
