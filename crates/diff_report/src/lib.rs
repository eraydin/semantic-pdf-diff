use spdfdiff_types::{DiffDocument, PdfDiffError};

pub fn to_json(document: &DiffDocument) -> Result<String, PdfDiffError> {
    serde_json::to_string_pretty(document)
        .map_err(|error| PdfDiffError::InternalInvariant(error.to_string()))
}

#[must_use]
pub fn to_markdown(document: &DiffDocument) -> String {
    format!(
        "# Semantic PDF Diff\n\nInserted: {}\nDeleted: {}\nModified: {}\nMoved: {}\nLayout changed: {}\n",
        document.summary.inserted,
        document.summary.deleted,
        document.summary.modified,
        document.summary.moved,
        document.summary.layout_changed
    )
}
