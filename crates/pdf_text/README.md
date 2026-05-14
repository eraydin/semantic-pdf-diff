# pdf_text

Font decoding, `/ToUnicode` handling, positioned glyphs, and text-run grouping.

Current vertical-slice extraction consumes `Tj` and `TJ` text operations,
tracks simple text position, leading, character spacing, word spacing, and
horizontal scaling, and emits `MISSING_TOUNICODE` when it falls back to literal
or hex string bytes.
