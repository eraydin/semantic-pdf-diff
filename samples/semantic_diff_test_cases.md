# Semantic PDF Diff Sample Test Scenarios

This document is the canonical scenario index for the sample PDFs in this
folder. It consolidates the earlier basic, detailed, v3, v4, and v5 scenario
documents into one place so integration tests and manual CLI checks have a
single source of truth.

The samples are designed to evaluate a semantic PDF diff engine across text,
layout, table, image, metadata, annotation, form, vector graphic, and scanned
document surfaces. Some scenarios describe capabilities that are intentionally
diagnostic-backed or deferred by the current CLI rather than fully implemented.

## 1. Basic Text And Versioning

**Files:** `document_v1.pdf`, `document_v2.pdf`

**Purpose:** Establish a baseline for text extraction, character/word-level
diffing, list changes, and version bumps.

**Expected scenario triggers:**

- Word replacement such as "robust backend" to "scalable backend".
- Inline wording changes in the main paragraph, including added technology
  references such as "Redis".
- List item value changes such as "200ms" to "150ms".
- Header version update from 1.0 to 1.1.
- Bullet encoding should not break paragraph continuity.

## 2. Basic Images And Text Variations

**Files:** `report_with_images_v1.pdf`, `report_with_images_v2.pdf`

**Purpose:** Evaluate image payload changes alongside surrounding text edits.

**Expected scenario triggers:**

- Image payload replacement from a blue bar chart to a red line chart.
- Draft/final financial summary text changes.
- Text before and after the image changes without falsely treating the image as
  moved solely because nearby text length changed.

## 3. Structural Shifts And Tabular Data

**Files:** `semantic_contract_v1.pdf`, `semantic_contract_v2.pdf`

**Purpose:** Test logical document structure reconstruction, block movement,
table data changes, and style changes in a legal-contract-like document.

**Expected scenario triggers:**

- Inline text edits such as "TechCorp" to "TechCorp LLC.", "30 days notice" to
  "30 days written notice", and "30 days of invoice" to "15 days of invoice
  receipt".
- The "Liability and Indemnification" section moves from Section 4 to Section 2.
  A semantic diff should ideally classify this as moved content instead of a
  large delete/insert pair.
- Table values change, kickoff and beta delivery amounts are adjusted, and an
  "Annual Maintenance" row is added.
- The table width, alignment, and border styling change.
- Liability wording changes from "total amount" to "50% of the total amount" and
  is visually emphasized with bold/orange/tinted styling.

## 4. Image Layout And Wrapping Variations

**Files:** `semantic_images_v1.pdf`, `semantic_images_v2.pdf`

**Purpose:** Evaluate image replacement, image layout changes, image resizing,
text wrapping, and image additions.

**Expected scenario triggers:**

- Product image replacement from a blue "Widget Gen 1" circle to a green
  bordered "Widget Gen 2" circle.
- The product image changes from centered on its own line to smaller and floated
  left, causing description text to wrap around it.
- Description text changes such as "streamlined design" to "upgraded,
  reinforced design", "50Hz" to "60Hz", and "12V" to "24V".
- The schematic image payload remains the same while its presentation changes:
  larger size, right alignment, dashed container, and background color.
- A new yellow ISO certification badge is added in version 2.
- Header border color changes from gray to green.

## 5. Complex Pagination, Math And Nested Lists

**Files:** `complex_semantic_diff_v1.pdf`, `complex_semantic_diff_v2.pdf`

**Purpose:** Stress-test cross-page flowing content, special character
extraction, complex tables, and nested list changes.

**Expected scenario triggers:**

- Insertions on page 1 push content onto page 2. A semantic diff should compare
  logical flow instead of treating page-boundary shifts as massive unrelated
  edits.
- Table structure changes include colspan/rowspan behavior and a new column
  under a merged header.
- Mathematical equations include subscripts, superscripts, and math symbols
  such as gamma, Sigma, and Delta.
- Nested list mutations include unordered-to-ordered list conversion, item
  reordering, and new nested sub-bullets.

## 6. Ultimate Structural And Image Blending

**Files:** `ultimate_semantic_diff_v1.pdf`, `ultimate_semantic_diff_v2.pdf`

**Purpose:** Combine structural, text, table, image relocation, image payload,
and layout changes in one stress test.

**Expected scenario triggers:**

- An identical schematic image moves from a floating text-wrap context to a
  standalone block near the bottom of the section.
- A bar chart is replaced by a line chart with annotations, threshold lines, and
  legends.
- The chart container is restyled with different borders/backgrounds.
- Paragraph text, table rows, and image layouts change together.

## 7. Interactive Elements, Metadata And Hyperlinks

**Files:** `interactive_links_v1.pdf`, `interactive_links_v2.pdf`

**Purpose:** Evaluate non-visual annotation and metadata changes.

**Expected scenario triggers:**

- Visible text such as "visit the dashboard" remains unchanged while the
  underlying link annotation target changes from a `db1` style URL to a
  `db2_cluster` style URL.
- Document metadata title changes from "System Status (V1)" to a V2 updated
  title.
- A visual-only text diff should miss the link change; a semantic diff should
  extract and compare link annotations.

## 8. Multi-Column Layout And Reading Order Complexity

**Files:** `multicolumn_layout_v1.pdf`, `multicolumn_layout_v2.pdf`

**Purpose:** Test reading-order reconstruction for multi-column layouts.

**Expected scenario triggers:**

- A new paragraph is inserted in the middle of Column 1.
- The insertion pushes the bottom of Column 1 to the top of Column 2.
- A naive left-to-right/top-to-bottom extractor may interleave columns and
  produce noisy diffs; the desired semantic behavior reconstructs column flow
  before comparing content.

## 9. Headers, Footers And Repeated Page Regions

**Files:** `headers_footers_v1.pdf`, `headers_footers_v2.pdf`

**Purpose:** Test filtering or categorization of repeated page-region noise.

**Expected scenario triggers:**

- Repeated header text changes from `DocID: 994-A` to `DocID: 994-B`.
- Repeated footer year changes from 2025 to 2026.
- A single body-text change, "and verify firewall logs", appears on page 2.
- A good semantic diff should ideally help users separate repeated header/footer
  changes from core body changes.

## 10. Inline Formatting And Typographic Semantics

**Files:** `inline_formatting_v1.pdf`, `inline_formatting_v2.pdf`

**Purpose:** Test detection of style changes applied directly to text.

**Expected scenario triggers:**

- The word "must" becomes bold.
- "5 seconds" changes to a highlighted "2 seconds".
- "Should" is struck through and "shall" is added.
- PDFs express these changes through font dictionaries, painting, or positioned
  text rather than HTML-like tags, so style-aware semantic output should
  preserve both text and presentation evidence.

## 11. Watermarks And Overlapping Z-Index Elements

**Files:** `watermark_overlay_v1.pdf`, `watermark_overlay_v2.pdf`

**Purpose:** Assess handling of large overlapping graphic/text elements.

**Expected scenario triggers:**

- Version 2 adds a giant rotated low-opacity "VOIDED" overlay.
- The watermark physically intersects body text and invoice table coordinates.
- A naive coordinate-only extractor may interleave watermark letters into table
  data; the desired semantic behavior isolates the watermark layer and leaves
  unchanged table data alone.

## 12. Multi-Page Tables And Ripple Effects

**Files:** `multipage_table_v1.pdf`, `multipage_table_v2.pdf`

**Purpose:** Test logical tables spanning physical page boundaries and the
effect of cascading coordinate shifts.

**Expected scenario triggers:**

- Version 1 contains an 80-row table spanning three pages.
- Version 2 deletes Row 15.
- Rows 16-80 shift upward by one row height after the deletion.
- A geometry-heavy diff may mark many rows as moved or modified; a semantic table
  diff should identify only the removed logical row and preserve the remaining
  row identities.

## 13. Interactive Form Fields

**Files:** `interactive_forms_v1.pdf`, `interactive_forms_v2.pdf`

**Purpose:** Assess extraction and comparison of AcroForm or XFA-style form
field states.

**Expected scenario triggers:**

- Version 1 is a blank employee onboarding form.
- Version 2 fills text fields such as "Jane Doe" and "Engineering".
- Version 2 toggles the "Laptop" checkbox to checked.
- Ideal semantic output identifies field-level changes such as `emp_name` from
  empty to "Jane Doe" and `laptop` from unchecked to checked.

## 14. Document Outlines And Deep Structure

**Files:** `document_outline_v1.pdf`, `document_outline_v2.pdf`

**Purpose:** Test extraction and comparison of non-visible document hierarchy
and navigation trees.

**Expected scenario triggers:**

- Version 2 adds a subsection such as "1.2 Caching Layer".
- A root section is renamed from "API Endpoints" to "API Specifications".
- A subsection is deleted.
- Content flow may shift bookmark destinations; the diff should distinguish
  title changes from destination target changes.

## 15. Annotations And Markups

**Files:** `annotations_base_v1.pdf`, `annotations_visual_markup_v2.pdf`

**Purpose:** Evaluate the ability to separate body text from collaborative
overlay objects.

**Expected scenario triggers:**

- Version 2 introduces a visually simulated highlight box over otherwise
  identical body text.
- A standard visual diff may flag the paragraph as modified.
- A semantic diff should ideally report unchanged body text plus added markup.
- This sample visually simulates markups; a production annotation test should
  also cover real `Annot` dictionaries.

## 16. Native Vector Paths And Graphic Operations

**Files:** `vector_paths_graphic_v1.pdf`, `vector_paths_graphic_v2.pdf`

**Purpose:** Assess comparison of native vector graphics drawn by PDF path
operators rather than raster image XObjects.

**Expected scenario triggers:**

- Version 2 moves a native vector line-chart data point from `y=100` to `y=120`.
- A corresponding native vector circle moves to the new coordinate.
- A text-only diff may report no semantic text change; a vector-aware diff should
  diagnose or compare native graphic operations.

## 17. OCR / Scanned Image Document

**Files:** `scanned_document_v1.pdf`, `scanned_document_v2.pdf`

**Purpose:** Test robustness with PDFs that have no extractable text layer.

**Expected scenario triggers:**

- Both versions are image-only PDFs.
- The visible raster content differs because one sentence inside the scanned
  image changed.
- There are no text operators or font dictionaries to extract.
- Current text-layer-only behavior should report a diagnostic such as missing
  text layer; full semantic comparison would require OCR.

## 18. Layered Redaction And Hidden Text

**Files:** `layered_redaction_v1.pdf`, `layered_redaction_v2.pdf`

**Purpose:** Test mixed visible text edits, redaction-style vector overlays, and
tiny hidden text that remains extractable in the content stream.

**Expected scenario triggers:**

- Version 1 exposes `Customer SSN: 123-45-6789`; version 2 visibly replaces it
  with `Customer SSN: REDACTED`.
- Version 2 also leaves `123-45-6789 hidden legacy text` as very small white text
  behind the redaction area. Current text extraction should preserve this
  evidence instead of silently dropping it.
- The case package changes from baseline to revised redaction package.
- Patient risk changes from standard to elevated monitoring.
- The approval lane inserts a Privacy review step.
- Native rectangle/path painting creates a diagnostic-backed vector overlay
  surface until full redaction-layer semantics exist.

## 19. Tagged Table Reflow With MCID Markers

**Files:** `tagged_table_reflow_v1.pdf`, `tagged_table_reflow_v2.pdf`

**Purpose:** Exercise tagged-PDF structure markers, marked-content IDs, and
logical table-row reordering in one controlled fixture.

**Expected scenario triggers:**

- The document declares a `StructTreeRoot` and content streams include
  marked-content property names with `/MCID` identifiers.
- The title changes from `Tagged Control Matrix Q1` to `Tagged Control Matrix
  Q2`.
- Row A changes from `Planned` to `Complete`.
- Row C moves above Row B and changes from weekly to daily backups.
- Row B changes from SRE-owned partial logging to Security-owned `MFA Required`
  logging.
- Row D, `Evidence Export`, is inserted in version 2.
- Current behavior should emit tagged-PDF diagnostics while still extracting and
  diffing the visible row text.

## 20. Attachment, Link Target And Visible Evidence Bundle Changes

**Files:** `attachment_link_bundle_v1.pdf`, `attachment_link_bundle_v2.pdf`

**Purpose:** Combine visible evidence-package text edits with real link and
file-attachment annotation dictionaries.

**Expected scenario triggers:**

- The visible call to action remains `Download evidence bundle`, but the link
  annotation target changes from a v1 evidence URL to a v2 final URL.
- The embedded-file style attachment label changes from
  `control-evidence-v1.zip` to `control-evidence-v2.zip`.
- The checksum changes from `sha256: AAA111` to `sha256: BBB222`.
- Status text changes from draft/staging to final/production.
- Current behavior should report the visible text changes and emit
  `UNSUPPORTED_ANNOTATION_DIFF` for the annotation and attachment surfaces until
  field-level annotation comparison is implemented.
