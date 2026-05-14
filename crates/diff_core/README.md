# diff_core

Semantic block matching, text hunks, move detection, confidence, and severity defaults.

Current matching uses deterministic exact normalized-text anchors, emits
inserted/deleted/modified changes, and relabels matching insert+delete pairs as
moves. Fuzzy matching, text hunk extraction, and richer layout-only change
detection remain later MVP work.
