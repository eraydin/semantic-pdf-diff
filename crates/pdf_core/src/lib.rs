use spdfdiff_types::{ByteRange, Diagnostic, ObjectId, ParseConfig, PdfDiffError, ResourceLimits};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfVersion {
    pub major: u8,
    pub minor: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfDocument {
    pub version: PdfVersion,
    pub objects: Vec<PdfObject>,
    pub pages: Vec<PdfPage>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfObject {
    pub id: ObjectId,
    pub body: String,
    pub stream: Option<PdfStream>,
    pub byte_range: ByteRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfStream {
    pub bytes: Vec<u8>,
    pub byte_range: ByteRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfPage {
    pub page_index: usize,
    pub object_id: ObjectId,
    pub content_object_id: ObjectId,
}

impl PdfDocument {
    pub fn parse(bytes: &[u8]) -> Result<Self, PdfDiffError> {
        Self::parse_with_config(bytes, ParseConfig::default())
    }

    pub fn parse_with_config(bytes: &[u8], config: ParseConfig) -> Result<Self, PdfDiffError> {
        config.limits.check_file_size(bytes.len())?;
        let mut document = parse_header(bytes)?;
        document.objects = parse_indirect_objects(bytes, &config)?;
        if document.objects.len() > config.limits.max_objects {
            return Err(PdfDiffError::ResourceLimitExceeded(format!(
                "file has {} indirect objects, limit is {}",
                document.objects.len(),
                config.limits.max_objects
            )));
        }
        document
            .diagnostics
            .extend(scan_unsupported_features(&document.objects));
        document.pages =
            resolve_pages(&document.objects, config.limits, &mut document.diagnostics)?;
        Ok(document)
    }

    #[must_use]
    pub fn first_page_content(&self) -> Option<PageContent<'_>> {
        let page = self.pages.first()?;
        let object = self
            .objects
            .iter()
            .find(|object| object.id == page.content_object_id)?;
        let stream = object.stream.as_ref()?;
        Some(PageContent {
            page_index: page.page_index,
            page_object_id: page.object_id,
            stream_object_id: object.id,
            bytes: &stream.bytes,
            byte_range: stream.byte_range,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageContent<'a> {
    pub page_index: usize,
    pub page_object_id: ObjectId,
    pub stream_object_id: ObjectId,
    pub bytes: &'a [u8],
    pub byte_range: ByteRange,
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
        objects: Vec::new(),
        pages: Vec::new(),
        diagnostics: Vec::new(),
    })
}

fn parse_indirect_objects(
    bytes: &[u8],
    config: &ParseConfig,
) -> Result<Vec<PdfObject>, PdfDiffError> {
    let text = String::from_utf8_lossy(bytes);
    let mut objects = Vec::new();
    let mut cursor = 0;

    while let Some(relative_obj_start) = text[cursor..].find(" obj") {
        let marker_start = cursor + relative_obj_start;
        let Some(line_start) = text[..marker_start]
            .rfind('\n')
            .map_or(Some(0), |index| index.checked_add(1))
        else {
            break;
        };
        let header = text[line_start..marker_start].trim();
        let Some((number, generation)) = parse_object_header(header) else {
            cursor = marker_start + " obj".len();
            continue;
        };
        let body_start = marker_start + " obj".len();
        let Some(relative_end) = text[body_start..].find("endobj") else {
            return Err(PdfDiffError::InvalidInput(format!(
                "object {number} {generation} is missing endobj"
            )));
        };
        let object_end = body_start + relative_end + "endobj".len();
        let body = text[body_start..body_start + relative_end]
            .trim()
            .to_owned();
        let stream = parse_stream(bytes, &text, body_start, body_start + relative_end, config)?;
        objects.push(PdfObject {
            id: ObjectId { number, generation },
            body,
            stream,
            byte_range: ByteRange::new(line_start, object_end),
        });
        cursor = object_end;
    }

    objects.sort_by_key(|object| (object.id.number, object.id.generation));
    Ok(objects)
}

fn parse_object_header(header: &str) -> Option<(u32, u16)> {
    let mut parts = header.split_ascii_whitespace();
    let number = parts.next()?.parse().ok()?;
    let generation = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((number, generation))
}

fn parse_stream(
    bytes: &[u8],
    text: &str,
    body_start: usize,
    body_end: usize,
    config: &ParseConfig,
) -> Result<Option<PdfStream>, PdfDiffError> {
    let body = &text[body_start..body_end];
    let Some(relative_stream_marker) = body.find("stream") else {
        return Ok(None);
    };
    let stream_marker = body_start + relative_stream_marker;
    let stream_data_start = match bytes.get(stream_marker + "stream".len()) {
        Some(b'\r') if bytes.get(stream_marker + "stream".len() + 1) == Some(&b'\n') => {
            stream_marker + "stream".len() + 2
        }
        Some(b'\n') => stream_marker + "stream".len() + 1,
        _ => stream_marker + "stream".len(),
    };
    let Some(relative_endstream) = text[stream_data_start..body_end].find("endstream") else {
        return Err(PdfDiffError::InvalidInput(
            "stream is missing endstream marker".into(),
        ));
    };
    let mut stream_data_end = stream_data_start + relative_endstream;
    while stream_data_end > stream_data_start && matches!(bytes[stream_data_end - 1], b'\n' | b'\r')
    {
        stream_data_end -= 1;
    }
    let stream_len = stream_data_end.saturating_sub(stream_data_start);
    if stream_len > config.limits.max_stream_bytes {
        return Err(PdfDiffError::ResourceLimitExceeded(format!(
            "stream has {stream_len} bytes, limit is {}",
            config.limits.max_stream_bytes
        )));
    }
    Ok(Some(PdfStream {
        bytes: bytes[stream_data_start..stream_data_end].to_vec(),
        byte_range: ByteRange::new(stream_data_start, stream_data_end),
    }))
}

fn resolve_pages(
    objects: &[PdfObject],
    limits: ResourceLimits,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<PdfPage>, PdfDiffError> {
    let mut pages = Vec::new();
    for object in objects {
        if !is_page_object(&object.body) {
            continue;
        }
        let Some(content_object_id) = find_reference_after(&object.body, "/Contents") else {
            diagnostics.push(
                Diagnostic::warning(
                    "MISSING_CONTENT_STREAM",
                    "page does not reference /Contents",
                )
                .with_object(object.id),
            );
            continue;
        };
        pages.push(PdfPage {
            page_index: pages.len(),
            object_id: object.id,
            content_object_id,
        });
    }
    if pages.len() > limits.max_pages {
        return Err(PdfDiffError::ResourceLimitExceeded(format!(
            "file has {} pages, limit is {}",
            pages.len(),
            limits.max_pages
        )));
    }
    Ok(pages)
}

fn scan_unsupported_features(objects: &[PdfObject]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for object in objects {
        if object.body.contains("/Type /XRef") {
            diagnostics.push(
                Diagnostic::warning(
                    "UNSUPPORTED_XREF_STREAM",
                    "xref stream objects are not part of the vertical-slice parser",
                )
                .with_object(object.id),
            );
        }
        if object.body.contains("/Type /ObjStm") {
            diagnostics.push(
                Diagnostic::warning(
                    "UNSUPPORTED_OBJECT_STREAM",
                    "object streams are not part of the vertical-slice parser",
                )
                .with_object(object.id),
            );
        }
        if object.stream.is_some() && object.body.contains("/Filter") {
            diagnostics.push(
                Diagnostic::warning(
                    "UNSUPPORTED_STREAM_FILTER",
                    "filtered streams are not decoded by the vertical-slice parser",
                )
                .with_object(object.id),
            );
        }
    }
    diagnostics
}

fn is_page_object(body: &str) -> bool {
    body.contains("/Type /Page") && !body.contains("/Type /Pages")
}

fn find_reference_after(body: &str, key: &str) -> Option<ObjectId> {
    let start = body.find(key)? + key.len();
    let mut parts = body[start..].split_ascii_whitespace();
    let number = parts.next()?.parse().ok()?;
    let generation = parts.next()?.parse().ok()?;
    if parts.next()? != "R" {
        return None;
    }
    Some(ObjectId { number, generation })
}

trait DiagnosticExt {
    fn with_object(self, object_id: ObjectId) -> Self;
}

impl DiagnosticExt for Diagnostic {
    fn with_object(mut self, object_id: ObjectId) -> Self {
        self.object = Some(object_id);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spdfdiff_types::DiagnosticSeverity;

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

    #[test]
    fn resolves_first_page_content_stream() {
        let document = PdfDocument::parse(minimal_pdf()).expect("fixture should parse");
        let content = document
            .first_page_content()
            .expect("page content should resolve");

        assert_eq!(content.page_index, 0);
        assert_eq!(
            content.stream_object_id,
            ObjectId {
                number: 4,
                generation: 0
            }
        );
        assert_eq!(content.bytes, b"BT /F1 12 Tf 72 720 Td (Hello) Tj ET");
    }

    #[test]
    fn emits_diagnostic_for_page_without_contents() {
        let pdf = b"%PDF-1.7
1 0 obj
<< /Type /Page >>
endobj
";
        let document = PdfDocument::parse(pdf).expect("fixture should parse partially");
        assert_eq!(document.pages, Vec::new());
        assert_eq!(document.diagnostics.len(), 1);
        assert_eq!(document.diagnostics[0].code, "MISSING_CONTENT_STREAM");
        assert_eq!(
            document.diagnostics[0].severity,
            DiagnosticSeverity::Warning
        );
    }

    #[test]
    fn emits_diagnostic_for_filtered_stream() {
        let pdf = b"%PDF-1.7
1 0 obj
<< /Type /Page /Contents 2 0 R >>
endobj
2 0 obj
<< /Length 5 /Filter /FlateDecode >>
stream
abcde
endstream
endobj
";
        let document = PdfDocument::parse(pdf).expect("fixture should parse partially");

        assert!(document.diagnostics.iter().any(|diagnostic| diagnostic.code
            == "UNSUPPORTED_STREAM_FILTER"
            && diagnostic.object
                == Some(ObjectId {
                    number: 2,
                    generation: 0
                })));
    }

    fn minimal_pdf() -> &'static [u8] {
        b"%PDF-1.7
1 0 obj
<< /Type /Catalog /Pages 2 0 R >>
endobj
2 0 obj
<< /Type /Pages /Kids [3 0 R] /Count 1 >>
endobj
3 0 obj
<< /Type /Page /Parent 2 0 R /Contents 4 0 R >>
endobj
4 0 obj
<< /Length 38 >>
stream
BT /F1 12 Tf 72 720 Td (Hello) Tj ET
endstream
endobj
"
    }
}
