# Semantic PDF Diff Application: Master Test Suite (v5 - Final Frontier)

This master document contains 17 advanced test scenarios engineered to validate, benchmark, and break semantic PDF diff applications, culminating in the "Final Frontier" of PDF edge cases.

*(Scenarios 1-14 are documented in previous versions and cover text, images, tables, layouts, page boundaries, forms, and outlines.)*

---

## [NEW] The Final Frontier: Advanced Layering & Graphics

### 15. Annotations & Markups (Visually Simulated)
**Files:** `annotations_base_v1.pdf`, `annotations_visual_markup_v2.pdf`
* **Detailed Purpose:** Evaluate the engine's ability to distinguish between body text flow and collaborative overlay objects (review markups).
* **Technical Challenge:** PDF Annotations (sticky notes, highlights) exist in a completely separate object stream from the static page content.
* **Specific Triggers:**
    * *Markup Insertion:* Version 2 introduces a visually simulated text highlight box overlaying the identical text in Version 1.
    * *Semantic Value:* A standard visual diff will flag the whole paragraph as "modified". A semantic diff engine should isolate the *visual overlay* layer and output: `No change to body text. Highlighting added to paragraph 2.` (Note: This scenario visually simulates markups via DOM styles. A production test must create actual `Annot` PDF dictionaries).

### 16. Native Vector Paths & Graphic Operations (Not Rasters)
**Files:** `vector_paths_graphic_v1.pdf`, `vector_paths_graphic_v2.pdf`
* **Detailed Purpose:** Assess extraction and comparison of native vector graphics (drawn shapes, paths, curves).
* **Technical Challenge:** Many charts and separators in PDFs are drawn natively using graphics operators (MoveTo, LineTo, Fill, Stroke) in the page stream. They are not binary PNG/JPEG images.
* **Specific Triggers:**
    * *Graphic Vertex Shift:* Version 2 contains a native vector line chart where the data point on a `polyline` has been physically moved from `y=100` to `y=120`. A corresponding native vector `circle` at that coordinate was also moved.
    * *Diff Engine Failure State:* If the app only extracts text, it will output "No changes detected," completely missing the data point alteration. A feature-rich app must compare vector paths to detect graphic modifications.

### 17. OCR / Scanned Image Document (No Text Layer)
**Files:** `scanned_document_v1.pdf`, `scanned_document_v2.pdf`
* **Detailed Purpose:** The ultimate benchmark of "robustness": how the app handles PDFs without actual text data.
* **Technical Challenge:** Scanned contracts or faxes arrive as images wrapped in a PDF envelope. There are no text operators, no font dictionaries—only a single binary `XObject` image stream covering the whole page.
* **Specific Triggers:**
    * *The OCR Barrier:* Version 1 and Version 2 are image-only PDFs. They look visually different because one sentence inside the raster graphic was edited.
    * *Feature or Failure:* A standard CLI app will report "No content." A robust, feature-rich app must integrate an OCR engine (e.g., Tesseract) and output a semantic diff of the OCRed text.
