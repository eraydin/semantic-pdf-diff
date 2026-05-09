use spdfdiff_types::Diagnostic;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentProgram {
    pub operations: Vec<ContentOp>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentOp {
    BeginText,
    EndText,
    Unknown { operator: String },
}

#[must_use]
pub fn parse_content_stream(_bytes: &[u8]) -> ContentProgram {
    ContentProgram {
        operations: Vec::new(),
        diagnostics: Vec::new(),
    }
}
