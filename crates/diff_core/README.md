# diff_core

Semantic block matching, text hunks, move detection, confidence, and severity defaults.

Current matching uses deterministic exact normalized-text anchors, emits
inserted/deleted/modified changes, relabels matching insert+delete pairs as
moves, and reports layout-only changes when exact text moves beyond the
configured page/bounding-box tolerance. Fuzzy matching and richer text hunk
extraction remain later MVP work.
