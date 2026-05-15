# Semantic PDF Diff Application - Test Suite Documentation

This document outlines the various test cases generated to evaluate and benchmark the semantic PDF diff application. The generated PDF pairs increase in complexity to stress-test different parsing and comparison capabilities.

---

## Test Case 1: Basic Text & Versioning
**Files:** `document_v1.pdf`, `document_v2.pdf`

**Purpose:** Baseline test for identifying minor text alterations and version bumps.
* **Inline Text Edits:** Minor wording changes in the main paragraph (e.g., adding "Redis").
* **List Modifications:** Changes to numeric values within bullet points (e.g., "200ms" to "150ms").
* **Header Updates:** Version number increment in the `<h1>` tag.

---

## Test Case 2: Basic Images & Text Variations
**Files:** `report_with_images_v1.pdf`, `report_with_images_v2.pdf`

**Purpose:** Introduction of raster images alongside text edits.
* **Image Replacement:** Version 1 contains a blue bar chart; Version 2 contains a red line chart.
* **Contextual Text Edits:** Summaries and bullet points altered to reflect "Draft" vs. "Final" financial states.

---

## Test Case 3: Structural Shifts & Tabular Data
**Files:** `semantic_contract_v1.pdf`, `semantic_contract_v2.pdf`

**Purpose:** Testing block-level movements, table data parsing, and visual styling changes.
* **Structural Movement (Block Shifting):** A whole section ("Liability and Indemnification") was moved from Section 4 to Section 2. A semantic diff should detect a *move*, not a delete/insert.
* **Tabular Data Modifications:** Cell values changed, a new row was appended, and the overall table width/alignment was altered.
* **Visual Formatting:** Plain text converted to a heavily styled warning block (bold, colored text, tinted background) to test if styling changes are flagged separately from semantic text changes.

---

## Test Case 4: Image Layout & Wrapping Variations
**Files:** `semantic_images_v1.pdf`, `semantic_images_v2.pdf`

**Purpose:** Evaluating how the diff engine handles image repositioning, resizing, and text wrapping.
* **Image Content Replacement:** A product image (blue circle) completely replaced by a new design (green circle with border).
* **Layout & Text Wrapping:** An image originally centered on its own line is scaled down and floated left, causing text to wrap around it.
* **CSS Shifts on Identical Images:** A schematic graphic remains byte-for-byte identical, but its CSS container changes (resized, aligned differently, new borders).
* **Image Addition:** A completely new graphic (ISO badge) is introduced in version 2.

---

## Test Case 5: Complex Pagination, Math & Nested Lists
**Files:** `complex_semantic_diff_v1.pdf`, `complex_semantic_diff_v2.pdf`

**Purpose:** Advanced test for academic/research papers with cross-page flowing content.
* **Pagination (Cross-Page Diffing):** Insertions on page 1 force content to flow differently onto page 2. The diff must recognize flowing content rather than registering massive deletions/insertions at page boundaries.
* **Complex Table Alterations:** Introduction of `colspan` and `rowspan` changes, adding a new column under a merged header.
* **Mathematical Equations:** Inline and block equations with subscripts, superscripts, and Unicode math symbols (γ, Σ, Δ).
* **Nested List Mutations:** Conversion of unordered (`<ul>`) to ordered (`<ol>`) lists, reordering of list items, and insertion of new nested sub-bullets.

---

## Test Case 6: Ultimate Structural & Image Blending
**Files:** `ultimate_semantic_diff_v1.pdf`, `ultimate_semantic_diff_v2.pdf`

**Purpose:** The final stress test combining complex document structures with dynamic image rendering and relocation.
* **Image Relocation:** An identical schematic image is ripped out of a floating text-wrap context and moved to a standalone block at the bottom of the section.
* **Chart Payload Alteration:** A bar chart is replaced by a line chart with new annotations (threshold lines, legends), while its CSS container is completely restyled.
* **Blended Complexities:** Simultaneous updates to paragraph text, table rows, and image layouts.

