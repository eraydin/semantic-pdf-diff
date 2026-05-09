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

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedIndirectObject {
    pub id: ObjectId,
    pub value: PdfPrimitive,
    pub stream: Option<PdfStream>,
    pub byte_range: ByteRange,
    pub value_byte_range: ByteRange,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedPrimitive {
    pub value: PdfPrimitive,
    pub byte_range: ByteRange,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PdfPrimitive {
    Null,
    Boolean(bool),
    Integer(i64),
    Real(f64),
    Name(String),
    LiteralString(Vec<u8>),
    HexString(Vec<u8>),
    Array(Vec<PdfPrimitive>),
    Dictionary(Vec<(String, PdfPrimitive)>),
    Reference(ObjectId),
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

pub fn parse_primitive(bytes: &[u8], config: ParseConfig) -> Result<ParsedPrimitive, PdfDiffError> {
    config.limits.check_file_size(bytes.len())?;
    let mut parser = PrimitiveParser::new(bytes, config);
    let start = parser.skip_whitespace_and_comments();
    let value = parser.parse_value(0)?;
    let end = parser.index;
    parser.skip_whitespace_and_comments();
    if !parser.is_eof() {
        return Err(PdfDiffError::InvalidInput(
            "trailing bytes after PDF primitive".into(),
        ));
    }
    Ok(ParsedPrimitive {
        value,
        byte_range: ByteRange::new(start, end),
    })
}

pub fn parse_indirect_object(
    bytes: &[u8],
    config: ParseConfig,
) -> Result<ParsedIndirectObject, PdfDiffError> {
    config.limits.check_file_size(bytes.len())?;
    let mut parser = PrimitiveParser::new(bytes, config);
    let object_start = parser.skip_whitespace_and_comments();
    let object_number = parser.parse_unsigned_object_number("object number")?;
    parser.skip_whitespace_and_comments();
    let generation = parser.parse_unsigned_generation()?;
    parser.skip_whitespace_and_comments();
    parser.expect_keyword(b"obj")?;
    let value_start = parser.skip_whitespace_and_comments();

    let parts = split_indirect_object_value_and_stream(bytes, value_start, config)?;
    let parsed = parse_primitive(parts.value_bytes, config)?;
    let mut end_parser = PrimitiveParser::new(&bytes[parts.value_end..], config);
    end_parser.skip_whitespace_and_comments();
    end_parser.expect_keyword(b"endobj")?;
    let object_end = parts.value_end + end_parser.index;
    end_parser.skip_whitespace_and_comments();
    if parts.value_end + end_parser.index < bytes.len() {
        return Err(PdfDiffError::InvalidInput(
            "trailing bytes after indirect object".into(),
        ));
    }

    Ok(ParsedIndirectObject {
        id: ObjectId {
            number: object_number,
            generation,
        },
        value: parsed.value,
        stream: parts.stream,
        byte_range: ByteRange::new(object_start, object_end),
        value_byte_range: ByteRange::new(
            parts.value_offset + parsed.byte_range.start,
            parts.value_offset + parsed.byte_range.end,
        ),
    })
}

struct IndirectValueParts<'a> {
    value_bytes: &'a [u8],
    value_offset: usize,
    value_end: usize,
    stream: Option<PdfStream>,
}

fn split_indirect_object_value_and_stream(
    bytes: &[u8],
    value_start: usize,
    config: ParseConfig,
) -> Result<IndirectValueParts<'_>, PdfDiffError> {
    let Some(relative_endobj) = find_keyword(bytes, value_start, b"endobj") else {
        return Err(PdfDiffError::InvalidInput(
            "indirect object is missing endobj".into(),
        ));
    };
    let body_end = relative_endobj;
    let Some(stream_marker) =
        find_keyword(bytes, value_start, b"stream").filter(|marker| *marker < body_end)
    else {
        let (value_bytes, leading_trim) = trim_ascii(&bytes[value_start..body_end]);
        return Ok(IndirectValueParts {
            value_bytes,
            value_offset: value_start + leading_trim,
            value_end: body_end,
            stream: None,
        });
    };
    let (value_bytes, leading_trim) = trim_ascii(&bytes[value_start..stream_marker]);
    let stream_data_start = match bytes.get(stream_marker + b"stream".len()) {
        Some(b'\r') if bytes.get(stream_marker + b"stream".len() + 1) == Some(&b'\n') => {
            stream_marker + b"stream".len() + 2
        }
        Some(b'\n') => stream_marker + b"stream".len() + 1,
        _ => stream_marker + b"stream".len(),
    };
    let Some(endstream_marker) =
        find_keyword(bytes, stream_data_start, b"endstream").filter(|marker| *marker < body_end)
    else {
        return Err(PdfDiffError::InvalidInput(
            "stream object is missing endstream".into(),
        ));
    };
    let stream_bytes = trim_trailing_ascii(&bytes[stream_data_start..endstream_marker]);
    if stream_bytes.len() > config.limits.max_stream_bytes {
        return Err(PdfDiffError::ResourceLimitExceeded(format!(
            "stream has {} bytes, limit is {}",
            stream_bytes.len(),
            config.limits.max_stream_bytes
        )));
    }

    Ok(IndirectValueParts {
        value_bytes,
        value_offset: value_start + leading_trim,
        value_end: body_end,
        stream: Some(PdfStream {
            bytes: stream_bytes.to_vec(),
            byte_range: ByteRange::new(stream_data_start, stream_data_start + stream_bytes.len()),
        }),
    })
}

struct PrimitiveParser<'a> {
    bytes: &'a [u8],
    index: usize,
    config: ParseConfig,
}

impl<'a> PrimitiveParser<'a> {
    fn new(bytes: &'a [u8], config: ParseConfig) -> Self {
        Self {
            bytes,
            index: 0,
            config,
        }
    }

    fn parse_value(&mut self, depth: usize) -> Result<PdfPrimitive, PdfDiffError> {
        if depth > self.config.limits.max_indirect_depth {
            return Err(PdfDiffError::ResourceLimitExceeded(format!(
                "primitive nesting depth exceeds limit of {}",
                self.config.limits.max_indirect_depth
            )));
        }

        self.skip_whitespace_and_comments();
        match self.peek() {
            Some(b'/') => self.parse_name().map(PdfPrimitive::Name),
            Some(b'(') => self.parse_literal_string().map(PdfPrimitive::LiteralString),
            Some(b'[') => self.parse_array(depth),
            Some(b'<') if self.peek_next() == Some(b'<') => self.parse_dictionary(depth),
            Some(b'<') => self.parse_hex_string().map(PdfPrimitive::HexString),
            Some(byte) if starts_keyword_byte(byte) => self.parse_keyword(),
            Some(byte) if starts_number_byte(byte) => self.parse_number_or_reference(),
            Some(_) => Err(PdfDiffError::InvalidInput(format!(
                "unexpected byte 0x{:02x} while parsing PDF primitive",
                self.bytes[self.index]
            ))),
            None => Err(PdfDiffError::InvalidInput(
                "expected PDF primitive, found end of input".into(),
            )),
        }
    }

    fn parse_unsigned_object_number(&mut self, label: &str) -> Result<u32, PdfDiffError> {
        let word = self.read_word();
        let NumericPrimitive::Integer(value) = parse_number_token(&word)? else {
            return Err(PdfDiffError::InvalidInput(format!(
                "{label} must be an integer"
            )));
        };
        u32::try_from(value).map_err(|_| {
            PdfDiffError::InvalidInput(format!("{label} is outside the supported range"))
        })
    }

    fn parse_unsigned_generation(&mut self) -> Result<u16, PdfDiffError> {
        let word = self.read_word();
        let NumericPrimitive::Integer(value) = parse_number_token(&word)? else {
            return Err(PdfDiffError::InvalidInput(
                "generation number must be an integer".into(),
            ));
        };
        u16::try_from(value).map_err(|_| {
            PdfDiffError::InvalidInput("generation number is outside the supported range".into())
        })
    }

    fn expect_keyword(&mut self, expected: &[u8]) -> Result<(), PdfDiffError> {
        let word = self.read_word();
        if word == expected {
            return Ok(());
        }
        Err(PdfDiffError::InvalidInput(format!(
            "expected keyword {}",
            String::from_utf8_lossy(expected)
        )))
    }

    fn parse_keyword(&mut self) -> Result<PdfPrimitive, PdfDiffError> {
        let word = self.read_word();
        match word.as_slice() {
            b"true" => Ok(PdfPrimitive::Boolean(true)),
            b"false" => Ok(PdfPrimitive::Boolean(false)),
            b"null" => Ok(PdfPrimitive::Null),
            _ => Err(PdfDiffError::InvalidInput(format!(
                "unsupported primitive keyword {}",
                String::from_utf8_lossy(&word)
            ))),
        }
    }

    fn parse_number_or_reference(&mut self) -> Result<PdfPrimitive, PdfDiffError> {
        let first = self.read_word();
        let first_number = parse_number_token(&first)?;
        let after_first = self.index;

        if let NumericPrimitive::Integer(number) = first_number {
            let restore = self.index;
            self.skip_whitespace_and_comments();
            if let Some(second) = self.try_read_integer_word() {
                self.skip_whitespace_and_comments();
                if self.peek() == Some(b'R') && self.peek_after_word() {
                    self.index += 1;
                    let object_number = u32::try_from(number).map_err(|_| {
                        PdfDiffError::InvalidInput("reference object number is out of range".into())
                    })?;
                    let generation = u16::try_from(second).map_err(|_| {
                        PdfDiffError::InvalidInput(
                            "reference generation number is out of range".into(),
                        )
                    })?;
                    return Ok(PdfPrimitive::Reference(ObjectId {
                        number: object_number,
                        generation,
                    }));
                }
            }
            self.index = restore;
        }

        self.index = after_first;
        match first_number {
            NumericPrimitive::Integer(value) => Ok(PdfPrimitive::Integer(value)),
            NumericPrimitive::Real(value) => Ok(PdfPrimitive::Real(value)),
        }
    }

    fn parse_name(&mut self) -> Result<String, PdfDiffError> {
        self.expect_byte(b'/')?;
        let mut output = Vec::new();
        while let Some(byte) = self.peek() {
            if is_delimiter_or_whitespace(byte) {
                break;
            }
            if byte == b'#' {
                self.index += 1;
                let high = self.next_hex_digit()?;
                let low = self.next_hex_digit()?;
                output.push((high << 4) | low);
            } else {
                output.push(byte);
                self.index += 1;
            }
        }
        if output.is_empty() {
            return Err(PdfDiffError::InvalidInput("PDF name is empty".into()));
        }
        Ok(String::from_utf8_lossy(&output).into_owned())
    }

    fn parse_literal_string(&mut self) -> Result<Vec<u8>, PdfDiffError> {
        self.expect_byte(b'(')?;
        let mut output = Vec::new();
        let mut depth = 1usize;
        while let Some(byte) = self.peek() {
            self.index += 1;
            match byte {
                b'\\' => {
                    let Some(escaped) = self.peek() else {
                        return Err(PdfDiffError::InvalidInput(
                            "unterminated literal string escape".into(),
                        ));
                    };
                    self.index += 1;
                    match escaped {
                        b'n' => output.push(b'\n'),
                        b'r' => output.push(b'\r'),
                        b't' => output.push(b'\t'),
                        b'b' => output.push(0x08),
                        b'f' => output.push(0x0c),
                        b'\n' => {}
                        b'\r' => {
                            if self.peek() == Some(b'\n') {
                                self.index += 1;
                            }
                        }
                        b'0'..=b'7' => output.push(self.finish_octal_escape(escaped)),
                        other => output.push(other),
                    }
                }
                b'(' => {
                    depth += 1;
                    output.push(byte);
                }
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(output);
                    }
                    output.push(byte);
                }
                other => output.push(other),
            }
        }
        Err(PdfDiffError::InvalidInput(
            "unterminated literal string".into(),
        ))
    }

    fn parse_hex_string(&mut self) -> Result<Vec<u8>, PdfDiffError> {
        self.expect_byte(b'<')?;
        let mut nibbles = Vec::new();
        while let Some(byte) = self.peek() {
            self.index += 1;
            if byte == b'>' {
                if nibbles.len() % 2 == 1 {
                    nibbles.push(0);
                }
                return Ok(nibbles
                    .chunks(2)
                    .map(|pair| (pair[0] << 4) | pair[1])
                    .collect());
            }
            if byte.is_ascii_whitespace() {
                continue;
            }
            nibbles.push(hex_value(byte).ok_or_else(|| {
                PdfDiffError::InvalidInput(format!("invalid hex string digit 0x{byte:02x}"))
            })?);
        }
        Err(PdfDiffError::InvalidInput("unterminated hex string".into()))
    }

    fn parse_array(&mut self, depth: usize) -> Result<PdfPrimitive, PdfDiffError> {
        self.expect_byte(b'[')?;
        let mut values = Vec::new();
        loop {
            self.skip_whitespace_and_comments();
            match self.peek() {
                Some(b']') => {
                    self.index += 1;
                    return Ok(PdfPrimitive::Array(values));
                }
                Some(_) => values.push(self.parse_value(depth + 1)?),
                None => {
                    return Err(PdfDiffError::InvalidInput("unterminated array".into()));
                }
            }
        }
    }

    fn parse_dictionary(&mut self, depth: usize) -> Result<PdfPrimitive, PdfDiffError> {
        self.expect_byte(b'<')?;
        self.expect_byte(b'<')?;
        let mut entries = Vec::new();
        loop {
            self.skip_whitespace_and_comments();
            match (self.peek(), self.peek_next()) {
                (Some(b'>'), Some(b'>')) => {
                    self.index += 2;
                    return Ok(PdfPrimitive::Dictionary(entries));
                }
                (Some(b'/'), _) => {
                    let key = self.parse_name()?;
                    let value = self.parse_value(depth + 1)?;
                    entries.push((key, value));
                }
                (Some(_), _) => {
                    return Err(PdfDiffError::InvalidInput(
                        "dictionary key must be a PDF name".into(),
                    ));
                }
                (None, _) => {
                    return Err(PdfDiffError::InvalidInput("unterminated dictionary".into()));
                }
            }
        }
    }

    fn skip_whitespace_and_comments(&mut self) -> usize {
        loop {
            while self.peek().is_some_and(|byte| byte.is_ascii_whitespace()) {
                self.index += 1;
            }
            if self.peek() == Some(b'%') {
                while let Some(byte) = self.peek() {
                    self.index += 1;
                    if matches!(byte, b'\r' | b'\n') {
                        break;
                    }
                }
                continue;
            }
            return self.index;
        }
    }

    fn read_word(&mut self) -> Vec<u8> {
        let start = self.index;
        while let Some(byte) = self.peek() {
            if is_delimiter_or_whitespace(byte) {
                break;
            }
            self.index += 1;
        }
        self.bytes[start..self.index].to_vec()
    }

    fn try_read_integer_word(&mut self) -> Option<i64> {
        let start = self.index;
        let word = self.read_word();
        if word.is_empty() {
            self.index = start;
            return None;
        }
        match parse_number_token(&word).ok()? {
            NumericPrimitive::Integer(value) => Some(value),
            NumericPrimitive::Real(_) => {
                self.index = start;
                None
            }
        }
    }

    fn peek_after_word(&self) -> bool {
        self.bytes
            .get(self.index + 1)
            .is_none_or(|byte| is_delimiter_or_whitespace(*byte))
    }

    fn finish_octal_escape(&mut self, first: u8) -> u8 {
        let mut value = first - b'0';
        for _ in 0..2 {
            let Some(byte @ b'0'..=b'7') = self.peek() else {
                break;
            };
            self.index += 1;
            value = value.saturating_mul(8).saturating_add(byte - b'0');
        }
        value
    }

    fn expect_byte(&mut self, expected: u8) -> Result<(), PdfDiffError> {
        if self.peek() == Some(expected) {
            self.index += 1;
            return Ok(());
        }
        Err(PdfDiffError::InvalidInput(format!(
            "expected byte 0x{expected:02x}"
        )))
    }

    fn next_hex_digit(&mut self) -> Result<u8, PdfDiffError> {
        let Some(byte) = self.peek() else {
            return Err(PdfDiffError::InvalidInput(
                "name hex escape is truncated".into(),
            ));
        };
        self.index += 1;
        hex_value(byte)
            .ok_or_else(|| PdfDiffError::InvalidInput("name hex escape is invalid".into()))
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.index).copied()
    }

    fn peek_next(&self) -> Option<u8> {
        self.bytes.get(self.index + 1).copied()
    }

    fn is_eof(&self) -> bool {
        self.index >= self.bytes.len()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum NumericPrimitive {
    Integer(i64),
    Real(f64),
}

fn parse_number_token(bytes: &[u8]) -> Result<NumericPrimitive, PdfDiffError> {
    let token = std::str::from_utf8(bytes)
        .map_err(|_| PdfDiffError::InvalidInput("number token is not UTF-8".into()))?;
    if token.contains('.') {
        return token
            .parse::<f64>()
            .map(NumericPrimitive::Real)
            .map_err(|_| PdfDiffError::InvalidInput(format!("invalid real number {token}")));
    }
    token
        .parse::<i64>()
        .map(NumericPrimitive::Integer)
        .map_err(|_| PdfDiffError::InvalidInput(format!("invalid integer {token}")))
}

fn starts_keyword_byte(byte: u8) -> bool {
    matches!(byte, b't' | b'f' | b'n')
}

fn starts_number_byte(byte: u8) -> bool {
    byte.is_ascii_digit() || matches!(byte, b'+' | b'-' | b'.')
}

fn is_delimiter_or_whitespace(byte: u8) -> bool {
    byte.is_ascii_whitespace()
        || matches!(byte, b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'/' | b'%')
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn find_keyword(bytes: &[u8], start: usize, keyword: &[u8]) -> Option<usize> {
    bytes
        .get(start..)?
        .windows(keyword.len())
        .position(|window| window == keyword)
        .map(|index| start + index)
}

fn trim_ascii(bytes: &[u8]) -> (&[u8], usize) {
    let mut start = 0;
    let mut end = bytes.len();
    while start < end && bytes[start].is_ascii_whitespace() {
        start += 1;
    }
    while end > start && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    (&bytes[start..end], start)
}

fn trim_trailing_ascii(bytes: &[u8]) -> &[u8] {
    let mut end = bytes.len();
    while end > 0 && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    &bytes[..end]
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
    fn parses_primitive_scalars() {
        assert_eq!(
            parse_primitive(b" true ", ParseConfig::default())
                .expect("boolean should parse")
                .value,
            PdfPrimitive::Boolean(true)
        );
        assert_eq!(
            parse_primitive(b"null", ParseConfig::default())
                .expect("null should parse")
                .value,
            PdfPrimitive::Null
        );
        assert_eq!(
            parse_primitive(b"-42", ParseConfig::default())
                .expect("integer should parse")
                .value,
            PdfPrimitive::Integer(-42)
        );
        assert!(matches!(
            parse_primitive(b"3.25", ParseConfig::default())
                .expect("real should parse")
                .value,
            PdfPrimitive::Real(value) if value == 3.25
        ));
    }

    #[test]
    fn skips_comments_and_preserves_primitive_byte_range() {
        let parsed = parse_primitive(b"  % comment\n/Name", ParseConfig::default())
            .expect("name should parse");

        assert_eq!(parsed.value, PdfPrimitive::Name("Name".into()));
        assert_eq!(parsed.byte_range, ByteRange::new(12, 17));
    }

    #[test]
    fn parses_names_with_hex_escapes() {
        let parsed =
            parse_primitive(b"/A#20Name", ParseConfig::default()).expect("name should parse");

        assert_eq!(parsed.value, PdfPrimitive::Name("A Name".into()));
    }

    #[test]
    fn parses_literal_and_hex_strings() {
        assert_eq!(
            parse_primitive(br"(Hello \(PDF\)\n)", ParseConfig::default())
                .expect("literal string should parse")
                .value,
            PdfPrimitive::LiteralString(b"Hello (PDF)\n".to_vec())
        );
        assert_eq!(
            parse_primitive(b"<48656c6c6f2>", ParseConfig::default())
                .expect("hex string should parse")
                .value,
            PdfPrimitive::HexString(vec![b'H', b'e', b'l', b'l', b'o', 0x20])
        );
    }

    #[test]
    fn parses_arrays_dictionaries_and_references() {
        let parsed = parse_primitive(
            b"<< /Type /Page /Count 1 /Kids [3 0 R] >>",
            ParseConfig::default(),
        )
        .expect("dictionary should parse");

        let PdfPrimitive::Dictionary(entries) = parsed.value else {
            panic!("expected dictionary");
        };
        assert_eq!(
            entries[0],
            ("Type".into(), PdfPrimitive::Name("Page".into()))
        );
        assert_eq!(entries[1], ("Count".into(), PdfPrimitive::Integer(1)));
        assert_eq!(
            entries[2],
            (
                "Kids".into(),
                PdfPrimitive::Array(vec![PdfPrimitive::Reference(ObjectId {
                    number: 3,
                    generation: 0
                })])
            )
        );
    }

    #[test]
    fn rejects_malformed_primitives_without_panic() {
        assert!(matches!(
            parse_primitive(b"(unterminated", ParseConfig::default()),
            Err(PdfDiffError::InvalidInput(_))
        ));
        assert!(matches!(
            parse_primitive(b"<< /Name >>", ParseConfig::default()),
            Err(PdfDiffError::InvalidInput(_))
        ));
        assert!(matches!(
            parse_primitive(b"<zz>", ParseConfig::default()),
            Err(PdfDiffError::InvalidInput(_))
        ));
    }

    #[test]
    fn enforces_primitive_nesting_limit() {
        let config = ParseConfig {
            limits: ResourceLimits {
                max_indirect_depth: 1,
                ..ResourceLimits::default()
            },
        };

        assert!(matches!(
            parse_primitive(b"[[1]]", config),
            Err(PdfDiffError::ResourceLimitExceeded(_))
        ));
    }

    #[test]
    fn parses_indirect_object_with_dictionary() {
        let parsed = parse_indirect_object(
            b"7 0 obj\n<< /Type /Page /Contents 8 0 R >>\nendobj\n",
            ParseConfig::default(),
        )
        .expect("indirect object should parse");

        assert_eq!(
            parsed.id,
            ObjectId {
                number: 7,
                generation: 0
            }
        );
        assert_eq!(parsed.byte_range, ByteRange::new(0, 48));
        assert_eq!(parsed.value_byte_range, ByteRange::new(8, 41));
        let PdfPrimitive::Dictionary(entries) = parsed.value else {
            panic!("expected dictionary object");
        };
        assert_eq!(
            entries[1],
            (
                "Contents".into(),
                PdfPrimitive::Reference(ObjectId {
                    number: 8,
                    generation: 0
                })
            )
        );
        assert_eq!(parsed.stream, None);
    }

    #[test]
    fn parses_indirect_stream_object() {
        let parsed = parse_indirect_object(
            b"8 0 obj\n<< /Length 5 >>\nstream\nHello\nendstream\nendobj",
            ParseConfig::default(),
        )
        .expect("stream object should parse");

        assert_eq!(
            parsed.id,
            ObjectId {
                number: 8,
                generation: 0
            }
        );
        assert_eq!(
            parsed.value,
            PdfPrimitive::Dictionary(vec![("Length".into(), PdfPrimitive::Integer(5))])
        );
        let stream = parsed.stream.expect("stream should be present");
        assert_eq!(stream.bytes, b"Hello");
        assert_eq!(stream.byte_range, ByteRange::new(31, 36));
    }

    #[test]
    fn rejects_unterminated_indirect_object_gracefully() {
        assert!(matches!(
            parse_indirect_object(b"1 0 obj\n<< /Type /Page >>", ParseConfig::default()),
            Err(PdfDiffError::InvalidInput(_))
        ));
    }

    #[test]
    fn rejects_unterminated_stream_object_gracefully() {
        assert!(matches!(
            parse_indirect_object(
                b"1 0 obj\n<< /Length 5 >>\nstream\nHello\nendobj",
                ParseConfig::default()
            ),
            Err(PdfDiffError::InvalidInput(_))
        ));
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
