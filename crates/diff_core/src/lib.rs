use spdfdiff_types::{ChangeKind, ChangeSeverity, SemanticChange};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DiffConfig {
    pub ignore_whitespace: bool,
    pub ignore_case: bool,
    pub detect_moves: bool,
    pub layout_tolerance_pt: f32,
    pub min_match_score: f32,
}

impl Default for DiffConfig {
    fn default() -> Self {
        Self {
            ignore_whitespace: true,
            ignore_case: false,
            detect_moves: true,
            layout_tolerance_pt: 2.0,
            min_match_score: 0.8,
        }
    }
}

pub trait SeverityClassifier {
    fn classify(&self, change: &SemanticChange) -> ChangeSeverity;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DefaultSeverityClassifier;

impl SeverityClassifier for DefaultSeverityClassifier {
    fn classify(&self, change: &SemanticChange) -> ChangeSeverity {
        match change.kind {
            ChangeKind::Inserted | ChangeKind::Deleted | ChangeKind::Modified => {
                ChangeSeverity::Major
            }
            ChangeKind::Moved | ChangeKind::LayoutChanged | ChangeKind::StyleChanged => {
                ChangeSeverity::Minor
            }
            ChangeKind::MetadataChanged | ChangeKind::ObjectChanged => ChangeSeverity::Info,
            ChangeKind::AnnotationChanged | ChangeKind::FormFieldChanged | ChangeKind::Unknown => {
                ChangeSeverity::Major
            }
        }
    }
}
