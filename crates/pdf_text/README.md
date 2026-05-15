# pdf_text

Font decoding, `/ToUnicode` handling, positioned glyphs, and text-run grouping.

Current vertical-slice extraction consumes `Tj` and `TJ` text operations,
tracks simple text position, leading, character spacing, word spacing, and
horizontal scaling, and emits `MISSING_TOUNICODE` when it falls back to literal
or hex string bytes.

The current CLI performs a narrow `/ToUnicode` CMap application step before
calling `pdf_text` when a page resource font maps to a decoded CMap stream.
Longer term, font resource resolution belongs in this crate behind an explicit
resource model.
