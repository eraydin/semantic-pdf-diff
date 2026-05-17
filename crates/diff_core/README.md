# diff_core

Semantic block matching, text hunks, move detection, confidence, and severity defaults.

Current matching uses deterministic exact normalized-text anchors, emits
inserted/deleted/modified changes, relabels matching insert+delete pairs as
moves, and reports layout-only changes when exact text moves beyond the
configured page/bounding-box tolerance. Ordered fuzzy matching uses token-LCS
similarity inside unmatched exact-anchor windows and honors `min_match_score`.
Modified changes include deterministic text hunks with token ranges. Small
non-numeric word replacements also include character-level hunks so reports can
show localized spelling or identifier edits without turning numeric value
changes into noisy character diffs.
