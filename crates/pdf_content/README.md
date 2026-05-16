# pdf_content

Content stream tokenization and operator interpretation for page drawing
operations.

Current vertical-slice coverage recognizes the MVP text operators `BT`, `ET`,
`Tf`, `Tj`, `TJ`, `Td`, `TD`, `Tm`, `T*`, `Tc`, `Tw`, `Tz`, `TL`, plus the
graphics-state operators `q`, `Q`, and `cm`. It also recognizes common non-text
drawing, color, clipping, marked-content, and XObject operators so digitally
generated PDF visuals do not create diagnostic noise during text extraction.
`BMC`/`BDC`/`EMC` marked-content operators preserve controlled tag and `/MCID`
evidence for downstream tagged-PDF mapping.
Truly unknown operators are still preserved as `CONTENT_OPERATOR_UNKNOWN`
diagnostics.
