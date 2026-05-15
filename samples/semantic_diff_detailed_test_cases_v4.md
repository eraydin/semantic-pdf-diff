# Semantic PDF Diff Application: Master Test Suite (v4 - Enterprise Scale)

This master document contains 14 advanced test scenarios engineered to validate, benchmark, and break semantic PDF diff applications, specifically focusing on enterprise-scale document complexity.

*(Scenarios 1-11 are documented in previous versions and cover basic text, images, tables, layouts, headers/footers, and watermarks.)*

---

## [NEW] Enterprise & Complex Document Structures

### 12. Multi-Page Tables & Ripple Effects
**Files:** `multipage_table_v1.pdf`, `multipage_table_v2.pdf`
* **Detailed Purpose:** Test the engine's ability to handle logical tables that span across physical page boundaries and evaluate diff performance against cascading coordinate shifts.
* **Technical Challenge:** When a table spans multiple pages, PDFs simply draw lines and text on subsequent pages; they do not inherently link the grid on Page 2 back to the grid on Page 1. Furthermore, repeating `<thead>` elements are re-drawn on every page.
* **Specific Triggers:**
    * *The Ripple Shift:* Version 1 contains an 80-row table spanning 3 pages. In Version 2, **Row 15 is deleted**.
    * *Diff Engine Failure State:* Because Row 15 is gone, Rows 16-80 all shift UP by one row's height. If the diff engine relies heavily on Y-coordinates, it will mistakenly flag every single row on Pages 2 and 3 as modified/moved. A true semantic diff will identify that the logical table structure remains intact and only Row 15 was removed, completely ignoring the massive coordinate shift of the subsequent 65 rows.

### 13. Interactive Form Fields (AcroForms/XFA)
**Files:** `interactive_forms_v1.pdf`, `interactive_forms_v2.pdf`
* **Detailed Purpose:** Assess extraction and comparison of interactive PDF form widget states.
* **Technical Challenge:** Form fields in a PDF are stored in interactive dictionaries (AcroForms) separated from the static page content. The visual representation of a checked box is often an overlay applied by the PDF viewer.
* **Specific Triggers:**
    * *State Changes:* V1 is a blank employee onboarding form. V2 has the text fields filled out ("Jane Doe", "Engineering") and the "Laptop" checkbox toggled to the checked state.
    * *Semantic Value:* The CLI app should ideally not just say "visual change detected", but specifically output: `Field 'emp_name' changed from '' to 'Jane Doe'` and `Checkbox 'laptop' changed from unchecked to checked`.

### 14. Document Outlines (Bookmarks) & Deep Structure
**Files:** `document_outline_v1.pdf`, `document_outline_v2.pdf`
* **Detailed Purpose:** Test extraction of non-visible document hierarchy and navigation trees.
* **Technical Challenge:** PDFs can contain a hierarchical Outline dictionary (commonly called Bookmarks by users) which maps string titles to specific page destinations. 
* **Specific Triggers:**
    * *Hierarchy Alterations:* V2 introduces a new subsection ("1.2 Caching Layer"), renames a root section ("API Endpoints" to "API Specifications"), and deletes a subsection entirely.
    * *Cross-Reference Verification:* Changing the document structure forces the target pages of the remaining bookmarks to shift. The diff engine must distinguish between a bookmark *changing its name* vs. a bookmark *changing its destination target* due to content flowing onto new pages.
