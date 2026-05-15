# Semantic PDF Diff Application: Comprehensive Test Suite Documentation

This document provides in-depth technical details for evaluating a semantic PDF diff engine. A true semantic diff must go beyond standard text comparison; it must parse layout geometry, reading order, embedded media, and interactive elements.

## 1. Basic Text & Versioning
**Files:** `document_v1.pdf`, `document_v2.pdf`
* **Detailed Purpose:** Establish a baseline for text extraction and character/word-level diffing. 
* **Technical Challenge:** The tool must isolate purely textual changes without breaking paragraph continuity. 
* **Specific Triggers:**
    * *Word Replacement:* "robust backend" -> "scalable backend".
    * *List Item Modification:* Changing "200ms" to "150ms". If the tool relies on raw text streams, it might treat the bullet point character (`•`) differently depending on font encoding.
    * *Header Bump:* Version 1.0 to 1.1.

## 2. Basic Images & Text Variations
**Files:** `report_with_images_v1.pdf`, `report_with_images_v2.pdf`
* **Detailed Purpose:** Evaluate the engine's ability to isolate image payload changes from surrounding text flow.
* **Technical Challenge:** In PDFs, images are often XObjects. The tool must hash or visually compare the XObject payload.
* **Specific Triggers:**
    * *Image Payload Alteration:* A blue bar chart (V1) is swapped for a red line chart (V2). The diff should highlight the image box itself as modified.
    * *Contextual Text Flow:* Text immediately preceding and following the image is altered. The diff must not falsely flag the image as "moved" just because the preceding text string length changed.

## 3. Structural Shifts & Tabular Data
**Files:** `semantic_contract_v1.pdf`, `semantic_contract_v2.pdf`
* **Detailed Purpose:** Test logical document structure parsing (DOM-like reconstruction).
* **Technical Challenge:** Tabular data in PDFs is just lines and text placed at specific (x,y) coordinates; there are no actual `<table>` tags unless the PDF is Tagged (PDF/UA). 
* **Specific Triggers:**
    * *Block Move:* "Section 4" is physically moved to become "Section 2". A naive diff shows massive deletes at the bottom and inserts at the top. A semantic diff highlights the block and flags it as *Moved*.
    * *Table Reconstruction:* Values changed inside table cells, and a row was added. The engine must successfully rebuild the grid layout to diff cell-by-cell rather than line-by-line.
    * *Style Changes:* Important text is wrapped in bold, colored highlighting in V2.

## 4. Image Layout & Wrapping Variations
**Files:** `semantic_images_v1.pdf`, `semantic_images_v2.pdf`
* **Detailed Purpose:** Test the bounding box and CSS-style layout shift detection.
* **Technical Challenge:** Distinguishing between an image that has *changed* vs. an image that has just been *moved or resized*.
* **Specific Triggers:**
    * *Text Wrapping:* An image centered on its own line (V1) is floated left (V2), forcing the paragraph to wrap around it. The engine needs to extract the logical paragraph (reading order) despite the lines being physically split by the image box.
    * *CSS Manipulation:* An identical schematic image is scaled up by 60% and given different borders. The binary image payload is identical, but the presentation changed.

## 5. Complex Pagination, Math & Nested Lists
**Files:** `complex_semantic_diff_v1.pdf`, `complex_semantic_diff_v2.pdf`
* **Detailed Purpose:** Stress-test page boundary flows and special character encoding.
* **Technical Challenge:** PDFs are fixed-layout. Pushing content to a new page breaks the logical flow across physical page objects.
* **Specific Triggers:**
    * *Cross-Page Flow:* An insertion on Page 1 pushes paragraphs onto Page 2. The semantic engine must stitch pages together *before* diffing to prevent false positives at page breaks.
    * *Table Colspan Manipulation:* A new column is inserted *under* a merged header. Extracting the correct column index for the diff is highly complex.
    * *Mathematical Symbols:* Standard fonts vs Math fonts. Changing `mc²` to `γmc² + ΣΔQi` tests Unicode extraction and subscript/superscript baseline shifts.

## 6. Ultimate Structural & Image Blending
**Files:** `ultimate_semantic_diff_v1.pdf`, `ultimate_semantic_diff_v2.pdf`
* **Detailed Purpose:** The culmination of structural and media changes in a single document.
* **Technical Challenge:** Concurrent processing of image payload changes, image relocation, and text flow.
* **Specific Triggers:**
    * *Image Ripping:* An image is pulled out of a text-wrap context and placed as a standalone block.
    * *Chart Alteration + Restyling:* A chart is replaced, shrunk, and heavily restyled with CSS borders and backgrounds.

---
## NEW SCENARIOS

## 7. Interactive Elements, Metadata & Hyperlinks
**Files:** `interactive_links_v1.pdf`, `interactive_links_v2.pdf`
* **Detailed Purpose:** Evaluate non-visual PDF annotations and metadata changes.
* **Technical Challenge:** Hyperlinks in PDFs are stored as Annotations (Link Annotations) over specific coordinate rectangles, completely separate from the text characters themselves.
* **Specific Triggers:**
    * *Hidden Link Alteration:* The text "visit the dashboard" remains exactly the same, but the underlying hyperlink changes from `.../db1` to `.../db2_cluster`. A purely visual diff will miss this completely. A semantic diff must extract and compare Link Annotations.
    * *Document Metadata:* The title embedded in the PDF metadata changes from "System Status (V1)" to "(V2 - Updated)".

## 8. Multi-column Layout & Reading Order Complexity
**Files:** `multicolumn_layout_v1.pdf`, `multicolumn_layout_v2.pdf`
* **Detailed Purpose:** The ultimate test of Reading Order extraction algorithms (e.g., XY-cut, heuristic block grouping).
* **Technical Challenge:** In a two-column layout, the text on the right side of the page is physically lower on the Y-axis but logically comes *after* the bottom of the left column.
* **Specific Triggers:**
    * *Column Overflow:* A new paragraph is inserted in the middle of Column 1. This pushes the bottom of Column 1 to the top of Column 2.
    * *Diff Engine Failure State:* If the diff engine reads left-to-right, top-to-bottom across the whole page, it will interleave the columns and output complete garbage when diffing. It must reconstruct the two-column flow before executing the comparison.
