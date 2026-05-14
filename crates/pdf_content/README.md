# pdf_content

Content stream tokenization and operator interpretation for page drawing
operations.

Current vertical-slice coverage recognizes the MVP text operators `BT`, `ET`,
`Tf`, `Tj`, `TJ`, `Td`, `TD`, `Tm`, `T*`, `Tc`, `Tw`, `Tz`, `TL`, plus the
graphics-state operators `q`, `Q`, and `cm`. Unknown operators are preserved as
`CONTENT_OPERATOR_UNKNOWN` diagnostics.
