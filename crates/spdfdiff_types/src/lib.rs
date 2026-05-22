use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const DIFF_SCHEMA_VERSION: &str = "0.1.0";

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PdfDiffError {
    #[error("input exceeds configured resource limit: {0}")]
    ResourceLimitExceeded(String),
    #[error("input is not a supported PDF: {0}")]
    UnsupportedPdf(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("internal invariant failed: {0}")]
    InternalInvariant(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ObjectId {
    pub number: u32,
    pub generation: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ByteRange {
    pub start: usize,
    pub end: usize,
}

impl ByteRange {
    #[must_use]
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileRole {
    Old,
    New,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    pub file_role: Option<FileRole>,
    pub object_id: Option<ObjectId>,
    pub page_index: Option<usize>,
    pub stream_object_id: Option<ObjectId>,
    pub content_op_index: Option<usize>,
    pub byte_range: Option<ByteRange>,
}

impl Provenance {
    #[must_use]
    pub const fn unknown() -> Self {
        Self {
            file_role: None,
            object_id: None,
            page_index: None,
            stream_object_id: None,
            content_op_index: None,
            byte_range: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl Rect {
    #[must_use]
    pub fn width(self) -> f32 {
        self.x1 - self.x0
    }

    #[must_use]
    pub fn height(self) -> f32 {
        self.y1 - self.y0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Matrix {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}

impl Matrix {
    pub const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LineSegment {
    pub start: Point,
    pub end: Point,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosticSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub severity: DiagnosticSeverity,
    pub code: String,
    pub message: String,
    pub object: Option<ObjectId>,
    pub page_index: Option<usize>,
}

impl Diagnostic {
    #[must_use]
    pub fn new(
        severity: DiagnosticSeverity,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity,
            code: code.into(),
            message: message.into(),
            object: None,
            page_index: None,
        }
    }

    #[must_use]
    pub fn info(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(DiagnosticSeverity::Info, code, message)
    }

    #[must_use]
    pub fn warning(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(DiagnosticSeverity::Warning, code, message)
    }

    #[must_use]
    pub fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(DiagnosticSeverity::Error, code, message)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceLimits {
    pub max_file_bytes: usize,
    pub max_objects: usize,
    pub max_indirect_depth: usize,
    pub max_stream_bytes: usize,
    pub max_decoded_stream_bytes: usize,
    pub max_content_ops_per_page: usize,
    pub max_pages: usize,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_file_bytes: 100 * 1024 * 1024,
            max_objects: 250_000,
            max_indirect_depth: 64,
            max_stream_bytes: 50 * 1024 * 1024,
            max_decoded_stream_bytes: 200 * 1024 * 1024,
            max_content_ops_per_page: 1_000_000,
            max_pages: 10_000,
        }
    }
}

impl ResourceLimits {
    pub fn check_file_size(self, byte_len: usize) -> Result<(), PdfDiffError> {
        if byte_len > self.max_file_bytes {
            return Err(PdfDiffError::ResourceLimitExceeded(format!(
                "file has {byte_len} bytes, limit is {}",
                self.max_file_bytes
            )));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ParseConfig {
    pub limits: ResourceLimits,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangeKind {
    Inserted,
    Deleted,
    Modified,
    Moved,
    LayoutChanged,
    StyleChanged,
    MetadataChanged,
    AnnotationChanged,
    FormFieldChanged,
    ObjectChanged,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangeSeverity {
    Critical,
    Major,
    Minor,
    Info,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DiffSummary {
    pub inserted: usize,
    pub deleted: usize,
    pub modified: usize,
    pub moved: usize,
    pub layout_changed: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SemanticNodeEvidence {
    pub node_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semantic_role: Option<String>,
    pub page: usize,
    pub bbox: Option<Rect>,
    pub text: Option<String>,
    pub source: Vec<Provenance>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutDiff {
    pub old_bbox: Option<Rect>,
    pub new_bbox: Option<Rect>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delta_x: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delta_y: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delta_width: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delta_height: Option<f32>,
    pub page_changed: bool,
    pub reading_order_changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextHunkKind {
    Equal,
    Inserted,
    Deleted,
    Replaced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextRange {
    pub start: usize,
    pub end: usize,
}

impl TextRange {
    #[must_use]
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextHunkGranularity {
    Token,
    Character,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextHunk {
    pub kind: TextHunkKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub granularity: Option<TextHunkGranularity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_range: Option<TextRange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_range: Option<TextRange>,
    pub old_text: Option<String>,
    pub new_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SemanticChange {
    pub id: String,
    pub kind: ChangeKind,
    pub severity: ChangeSeverity,
    pub old_node: Option<SemanticNodeEvidence>,
    pub new_node: Option<SemanticNodeEvidence>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub text_hunks: Vec<TextHunk>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout_diff: Option<LayoutDiff>,
    pub confidence: f32,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiffDocument {
    pub schema_version: String,
    pub old_fingerprint: String,
    pub new_fingerprint: String,
    pub summary: DiffSummary,
    pub changes: Vec<SemanticChange>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AiReviewReport {
    pub schema_version: String,
    pub source_schema_version: String,
    pub old_fingerprint: String,
    pub new_fingerprint: String,
    pub summary: AiReviewSummary,
    pub question_hints: Vec<AiReviewQuestionHint>,
    pub review_items: Vec<AiReviewItem>,
    pub diagnostic_summary: Vec<AiDiagnosticCount>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiReviewSummary {
    pub total_changes: usize,
    pub inserted: usize,
    pub deleted: usize,
    pub modified: usize,
    pub moved: usize,
    pub layout_changed: usize,
    pub diagnostic_count: usize,
    pub low_confidence_change_count: usize,
    pub unsupported_surface_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiReviewQuestionHint {
    pub question: String,
    pub answer: AiReviewAnswer,
    pub supporting_change_ids: Vec<String>,
    pub rationale: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AiReviewAnswer {
    Yes,
    No,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AiReviewItem {
    pub change_id: String,
    pub kind: ChangeKind,
    pub severity: ChangeSeverity,
    pub confidence: f32,
    pub confidence_bucket: AiConfidenceBucket,
    pub tags: Vec<AiReviewTag>,
    pub explanation: String,
    pub evidence: AiEvidenceBundle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AiConfidenceBucket {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AiReviewTag {
    TextChanged,
    ContentInserted,
    ContentDeleted,
    ContentMoved,
    LayoutOnly,
    RepeatedPageRegion,
    PaymentTermsCandidate,
    DateOrDurationCandidate,
    PartyNameCandidate,
    NumericValueChanged,
    AnnotationOrLinkChanged,
    FormFieldChanged,
    MetadataChanged,
    VisualSurfaceChanged,
    UnsupportedSurface,
    LowConfidence,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AiEvidenceBundle {
    pub old_node_id: Option<String>,
    pub new_node_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_semantic_role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_semantic_role: Option<String>,
    pub section_hint: Option<String>,
    pub old_page: Option<usize>,
    pub new_page: Option<usize>,
    pub old_bbox: Option<Rect>,
    pub new_bbox: Option<Rect>,
    pub old_text: Option<String>,
    pub new_text: Option<String>,
    pub text_hunks: Vec<TextHunk>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout_diff: Option<LayoutDiff>,
    pub provenance: Vec<Provenance>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiDiagnosticCount {
    pub code: String,
    pub count: usize,
}

impl DiffDocument {
    #[must_use]
    pub fn empty(old_fingerprint: impl Into<String>, new_fingerprint: impl Into<String>) -> Self {
        Self {
            schema_version: DIFF_SCHEMA_VERSION.to_owned(),
            old_fingerprint: old_fingerprint.into(),
            new_fingerprint: new_fingerprint.into(),
            summary: DiffSummary::default(),
            changes: Vec::new(),
            diagnostics: Vec::new(),
        }
    }
}
