use spdfdiff_types::{Diagnostic, ParseConfig, PdfDiffError, ResourceLimits};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfVersion {
    pub major: u8,
    pub minor: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfDocument {
    pub version: PdfVersion,
    pub diagnostics: Vec<Diagnostic>,
}

impl PdfDocument {
    pub fn parse(bytes: &[u8]) -> Result<Self, PdfDiffError> {
        Self::parse_with_config(bytes, ParseConfig::default())
    }

    pub fn parse_with_config(bytes: &[u8], config: ParseConfig) -> Result<Self, PdfDiffError> {
        config.limits.check_file_size(bytes.len())?;
        parse_header(bytes)
    }
}

#[must_use]
pub fn default_resource_limits() -> ResourceLimits {
    ResourceLimits::default()
}

fn parse_header(bytes: &[u8]) -> Result<PdfDocument, PdfDiffError> {
    let header = bytes
        .get(..8)
        .ok_or_else(|| PdfDiffError::InvalidInput("file is too short for a PDF header".into()))?;

    if !header.starts_with(b"%PDF-") {
        return Err(PdfDiffError::UnsupportedPdf("missing %PDF- header".into()));
    }

    let major = header[5];
    let minor = header[7];
    if !major.is_ascii_digit() || header[6] != b'.' || !minor.is_ascii_digit() {
        return Err(PdfDiffError::InvalidInput(
            "malformed PDF version header".into(),
        ));
    }

    Ok(PdfDocument {
        version: PdfVersion {
            major: major - b'0',
            minor: minor - b'0',
        },
        diagnostics: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pdf_header() {
        let document = PdfDocument::parse(b"%PDF-1.7\n").expect("header should parse");
        assert_eq!(document.version, PdfVersion { major: 1, minor: 7 });
    }

    #[test]
    fn rejects_non_pdf_header() {
        let error = PdfDocument::parse(b"not a pdf").expect_err("header should be rejected");
        assert!(matches!(error, PdfDiffError::UnsupportedPdf(_)));
    }

    #[test]
    fn enforces_file_size_limit() {
        let config = ParseConfig {
            limits: ResourceLimits {
                max_file_bytes: 4,
                ..ResourceLimits::default()
            },
        };
        let error =
            PdfDocument::parse_with_config(b"%PDF-1.7\n", config).expect_err("limit should fail");
        assert!(matches!(error, PdfDiffError::ResourceLimitExceeded(_)));
    }
}
