# pdf_semantic

Layout segmentation, semantic nodes, reading order, and semantic anchors.

Current vertical-slice extraction clusters positioned text runs into deterministic
paragraph nodes using page index, baseline proximity, x ordering, and vertical
gap thresholds. It can mark controlled larger short paragraphs as
`HeadingCandidate`. Each node also receives deterministic semantic anchors:
strong normalized-text hash, weak text signature, geometry bucket, and optional
heading context. List and table heuristics remain later MVP work.
