# Semantic PDF Diff Application: Master Test Suite (v3)

This master document contains all 11 test scenarios engineered to validate, benchmark, and break semantic PDF diff applications. 

## [Core Text & Layout]
**1. Basic Text & Versioning (`document_v1/v2.pdf`)**
* **Focus:** Baseline text extraction, character-level diffs, and list modification detection.

**2. Basic Images & Text Variations (`report_with_images_v1/v2.pdf`)**
* **Focus:** Identifying replaced image payloads without breaking the text flow surrounding the image.

**3. Structural Shifts & Tabular Data (`semantic_contract_v1/v2.pdf`)**
* **Focus:** Logical DOM reconstruction. Detecting block moves (Section 4 shifted to Section 2) rather than massive deletes/inserts, and mapping table cell value changes.

**4. Image Layout & Wrapping Variations (`semantic_images_v1/v2.pdf`)**
* **Focus:** Bounding box and CSS shift detection. Tests if the engine can parse paragraphs split by floated images, and recognize when an image is just resized vs. replaced.

**5. Complex Pagination, Math & Nested Lists (`complex_semantic_diff_v1/v2.pdf`)**
* **Focus:** Cross-page text flows (text pushed to page 2), table `colspan` logic, and mathematical symbol extraction (Unicode, superscripts).

**6. Ultimate Structural & Image Blending (`ultimate_semantic_diff_v1/v2.pdf`)**
* **Focus:** Combined stress test. Ripping an image out of a text wrap and placing it as a standalone block, alongside data table changes.

## [Advanced Flow & Metadata]
**7. Interactive Elements, Metadata & Hyperlinks (`interactive_links_v1/v2.pdf`)**
* **Focus:** Extracting PDF Link Annotations and Metadata. The visual text is identical, but the underlying hyperlink `href` and document Title are modified.

**8. Multi-column Layout & Reading Order Complexity (`multicolumn_layout_v1/v2.pdf`)**
* **Focus:** XY-cut algorithms and vertical reading order. Inserting a paragraph in Column 1 pushes the bottom of Column 1 to the top of Column 2. Purely horizontal text extractors will fail entirely.

---

## [NEW] Edge Cases & Layering

### 9. Headers, Footers & Repeated Page Regions
**Files:** `headers_footers_v1.pdf`, `headers_footers_v2.pdf`
* **Detailed Purpose:** Evaluate the engine's ability to filter out "document noise" from actual content changes.
* **Technical Challenge:** Headers and footers are repeated on every single page. A small bump in a version number or year in the header will cause a naive diff to flag *every single page* as modified.
* **Specific Triggers:**
    * *Repeated Edits:* The header changes from `DocID: 994-A` to `994-B`, and the footer changes from `2025` to `2026` across all pages.
    * *Needle in a Haystack:* A single word change ("and verify firewall logs") is buried on page 2. A good semantic diff should ideally allow users to "ignore headers/footers" to highlight only the core body change.

### 10. Inline Formatting & Typographic Semantics
**Files:** `inline_formatting_v1.pdf`, `inline_formatting_v2.pdf`
* **Detailed Purpose:** Test detection of style changes applied directly to text elements.
* **Technical Challenge:** PDFs do not have `<b>` or `<i>` tags; they change the actual font dictionary (e.g., from `Times-Roman` to `Times-Bold`) for specific character byte sequences. 
* **Specific Triggers:**
    * *Formatting Addition:* The word "must" is changed to **must** (bolded). 
    * *Content & Style change:* "5 seconds" is changed to a highlighted "2 seconds". "Should" is crossed out (`<del>`) and "shall" is added. 
    * *Semantic Value:* The CLI app needs a way to express these style diffs, perhaps outputting them as markdown diffs or AST property changes.

### 11. Watermarks & Overlapping Z-Index Elements
**Files:** `watermark_overlay_v1.pdf`, `watermark_overlay_v2.pdf`
* **Detailed Purpose:** Assess how the parser handles massive overlapping graphic/text elements.
* **Technical Challenge:** Watermarks are often drawn as massive text objects rotated across the center of the page, physically intersecting the coordinates of the body text beneath it.
* **Specific Triggers:**
    * *Z-Index Intersections:* Version 2 has a giant, rotated, low-opacity "VOIDED" text overlay. 
    * *False Text Interleaving:* If the extractor relies purely on Y-coordinates, it might randomly insert the letters V-O-I-D-E-D into the middle of the invoice table data. The semantic engine must isolate the watermark to its own logical layer and leave the underlying table data completely un-diffed (as the table itself hasn't changed).
