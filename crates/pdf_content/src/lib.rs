use spdfdiff_types::{Diagnostic, ObjectId, Provenance, ResourceLimits};

#[derive(Debug, Clone, PartialEq)]
pub struct ContentProgram {
    pub operations: Vec<ContentOp>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ContentOp {
    BeginText {
        source: Provenance,
    },
    EndText {
        source: Provenance,
    },
    SetFont {
        name: String,
        size: f32,
        source: Provenance,
    },
    MoveTextPosition {
        tx: f32,
        ty: f32,
        set_leading: Option<f32>,
        source: Provenance,
    },
    MoveToNextLine {
        source: Provenance,
    },
    SetTextLeading {
        leading: f32,
        source: Provenance,
    },
    SetCharacterSpacing {
        spacing: f32,
        source: Provenance,
    },
    SetWordSpacing {
        spacing: f32,
        source: Provenance,
    },
    SetHorizontalScaling {
        scale: f32,
        source: Provenance,
    },
    SetTextMatrix {
        a: f32,
        b: f32,
        c: f32,
        d: f32,
        e: f32,
        f: f32,
        source: Provenance,
    },
    ShowText {
        text: String,
        raw_bytes: Vec<u8>,
        source: Provenance,
    },
    ShowAdjustedText {
        text: String,
        raw_bytes: Vec<u8>,
        adjustments: Vec<f32>,
        source: Provenance,
    },
    SaveGraphicsState {
        source: Provenance,
    },
    RestoreGraphicsState {
        source: Provenance,
    },
    ConcatMatrix {
        a: f32,
        b: f32,
        c: f32,
        d: f32,
        e: f32,
        f: f32,
        source: Provenance,
    },
    RecognizedNonText {
        operator: String,
        source: Provenance,
    },
    Unknown {
        operator: String,
        source: Provenance,
    },
}

#[must_use]
pub fn parse_content_stream(bytes: &[u8]) -> ContentProgram {
    parse_content_stream_with_limits(bytes, 0, None, ResourceLimits::default())
}

#[must_use]
pub fn parse_content_stream_with_limits(
    bytes: &[u8],
    page_index: usize,
    stream_object_id: Option<ObjectId>,
    limits: ResourceLimits,
) -> ContentProgram {
    if bytes.len() > limits.max_stream_bytes {
        return ContentProgram {
            operations: Vec::new(),
            diagnostics: vec![Diagnostic::error(
                "RESOURCE_LIMIT_STREAM_BYTES",
                format!(
                    "content stream has {} bytes, limit is {}",
                    bytes.len(),
                    limits.max_stream_bytes
                ),
            )],
        };
    }

    let tokens = tokenize(bytes);
    let mut stack = Vec::new();
    let mut operations = Vec::new();
    let mut diagnostics = Vec::new();

    for token in tokens {
        match token {
            Token::Operator(operator) => {
                let op_index = operations.len();
                if op_index >= limits.max_content_ops_per_page {
                    diagnostics.push(Diagnostic::error(
                        "RESOURCE_LIMIT_CONTENT_OPERATORS",
                        format!(
                            "content stream exceeds operator limit of {}",
                            limits.max_content_ops_per_page
                        ),
                    ));
                    break;
                }
                let source = Provenance {
                    page_index: Some(page_index),
                    stream_object_id,
                    content_op_index: Some(op_index),
                    ..Provenance::unknown()
                };
                if let Some(operation) = build_operation(&operator, &stack, source.clone()) {
                    operations.push(operation);
                } else {
                    diagnostics.push(
                        Diagnostic::warning(
                            "CONTENT_OPERATOR_UNKNOWN",
                            format!("unsupported content operator {operator}"),
                        )
                        .with_page(page_index)
                        .with_object(stream_object_id),
                    );
                    operations.push(ContentOp::Unknown { operator, source });
                }
                stack.clear();
            }
            operand => stack.push(operand),
        }
    }

    ContentProgram {
        operations,
        diagnostics,
    }
}

fn build_operation(operator: &str, stack: &[Token], source: Provenance) -> Option<ContentOp> {
    match operator {
        "BT" => Some(ContentOp::BeginText { source }),
        "ET" => Some(ContentOp::EndText { source }),
        "Tf" => Some(ContentOp::SetFont {
            name: stack
                .get(stack.len().checked_sub(2)?)
                .and_then(Token::as_name)?
                .to_owned(),
            size: stack.last().and_then(Token::as_number)?,
            source,
        }),
        "Td" | "TD" => {
            let ty = stack.last().and_then(Token::as_number)?;
            Some(ContentOp::MoveTextPosition {
                tx: stack
                    .get(stack.len().checked_sub(2)?)
                    .and_then(Token::as_number)?,
                ty,
                set_leading: if operator == "TD" { Some(-ty) } else { None },
                source,
            })
        }
        "T*" => Some(ContentOp::MoveToNextLine { source }),
        "TL" => Some(ContentOp::SetTextLeading {
            leading: stack.last().and_then(Token::as_number)?,
            source,
        }),
        "Tc" => Some(ContentOp::SetCharacterSpacing {
            spacing: stack.last().and_then(Token::as_number)?,
            source,
        }),
        "Tw" => Some(ContentOp::SetWordSpacing {
            spacing: stack.last().and_then(Token::as_number)?,
            source,
        }),
        "Tz" => Some(ContentOp::SetHorizontalScaling {
            scale: stack.last().and_then(Token::as_number)?,
            source,
        }),
        "Tm" => Some(ContentOp::SetTextMatrix {
            a: stack
                .get(stack.len().checked_sub(6)?)
                .and_then(Token::as_number)?,
            b: stack
                .get(stack.len().checked_sub(5)?)
                .and_then(Token::as_number)?,
            c: stack
                .get(stack.len().checked_sub(4)?)
                .and_then(Token::as_number)?,
            d: stack
                .get(stack.len().checked_sub(3)?)
                .and_then(Token::as_number)?,
            e: stack
                .get(stack.len().checked_sub(2)?)
                .and_then(Token::as_number)?,
            f: stack.last().and_then(Token::as_number)?,
            source,
        }),
        "Tj" => {
            let text = stack.last().and_then(Token::as_text)?;
            Some(ContentOp::ShowText {
                raw_bytes: text.raw_bytes,
                text: text.text,
                source,
            })
        }
        "TJ" => {
            let array = stack.last().and_then(Token::as_array)?;
            let mut text = String::new();
            let mut raw_bytes = Vec::new();
            let mut adjustments = Vec::new();
            for item in array {
                if let Some(segment) = item.as_text() {
                    text.push_str(&segment.text);
                    raw_bytes.extend_from_slice(&segment.raw_bytes);
                } else if let Some(adjustment) = item.as_number() {
                    adjustments.push(adjustment);
                }
            }
            Some(ContentOp::ShowAdjustedText {
                text,
                raw_bytes,
                adjustments,
                source,
            })
        }
        "q" => Some(ContentOp::SaveGraphicsState { source }),
        "Q" => Some(ContentOp::RestoreGraphicsState { source }),
        "cm" => Some(ContentOp::ConcatMatrix {
            a: stack
                .get(stack.len().checked_sub(6)?)
                .and_then(Token::as_number)?,
            b: stack
                .get(stack.len().checked_sub(5)?)
                .and_then(Token::as_number)?,
            c: stack
                .get(stack.len().checked_sub(4)?)
                .and_then(Token::as_number)?,
            d: stack
                .get(stack.len().checked_sub(3)?)
                .and_then(Token::as_number)?,
            e: stack
                .get(stack.len().checked_sub(2)?)
                .and_then(Token::as_number)?,
            f: stack.last().and_then(Token::as_number)?,
            source,
        }),
        _ if is_recognized_non_text_operator(operator) => Some(ContentOp::RecognizedNonText {
            operator: operator.to_owned(),
            source,
        }),
        _ => None,
    }
}

fn is_recognized_non_text_operator(operator: &str) -> bool {
    matches!(
        operator,
        // Graphics state, path construction, painting, clipping, color, shading,
        // XObject, and marked-content operators that are common in generated PDFs.
        "w" | "J"
            | "j"
            | "M"
            | "d"
            | "ri"
            | "i"
            | "gs"
            | "m"
            | "l"
            | "c"
            | "v"
            | "y"
            | "h"
            | "re"
            | "S"
            | "s"
            | "f"
            | "F"
            | "f*"
            | "B"
            | "B*"
            | "b"
            | "b*"
            | "n"
            | "W"
            | "W*"
            | "CS"
            | "cs"
            | "SC"
            | "SCN"
            | "sc"
            | "scn"
            | "G"
            | "g"
            | "RG"
            | "rg"
            | "K"
            | "k"
            | "sh"
            | "Do"
            | "MP"
            | "DP"
            | "BMC"
            | "BDC"
            | "EMC"
            | "BX"
            | "EX"
    )
}

#[derive(Debug, Clone, PartialEq)]
struct TextOperand {
    text: String,
    raw_bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f32),
    Name(String),
    LiteralString(Vec<u8>),
    HexString(Vec<u8>),
    Array(Vec<Token>),
    Operator(String),
}

impl Token {
    fn as_number(&self) -> Option<f32> {
        match self {
            Self::Number(value) => Some(*value),
            Self::Name(_)
            | Self::LiteralString(_)
            | Self::HexString(_)
            | Self::Array(_)
            | Self::Operator(_) => None,
        }
    }

    fn as_name(&self) -> Option<&str> {
        match self {
            Self::Name(value) => Some(value),
            Self::Number(_)
            | Self::LiteralString(_)
            | Self::HexString(_)
            | Self::Array(_)
            | Self::Operator(_) => None,
        }
    }

    fn as_text(&self) -> Option<TextOperand> {
        match self {
            Self::LiteralString(value) | Self::HexString(value) => Some(TextOperand {
                text: String::from_utf8_lossy(value).into_owned(),
                raw_bytes: value.clone(),
            }),
            Self::Number(_) | Self::Name(_) | Self::Array(_) | Self::Operator(_) => None,
        }
    }

    fn as_array(&self) -> Option<&[Token]> {
        match self {
            Self::Array(value) => Some(value),
            Self::Number(_)
            | Self::Name(_)
            | Self::LiteralString(_)
            | Self::HexString(_)
            | Self::Operator(_) => None,
        }
    }
}

fn tokenize(bytes: &[u8]) -> Vec<Token> {
    let mut index = 0;
    tokenize_until(bytes, &mut index, None)
}

fn tokenize_until(bytes: &[u8], index: &mut usize, stop_byte: Option<u8>) -> Vec<Token> {
    let mut tokens = Vec::new();
    while *index < bytes.len() {
        if Some(bytes[*index]) == stop_byte {
            *index += 1;
            break;
        }
        if bytes[*index].is_ascii_whitespace() {
            *index += 1;
            continue;
        }
        match bytes[*index] {
            b'(' => {
                let (value, next) = parse_literal_string(bytes, *index + 1);
                tokens.push(Token::LiteralString(value));
                *index = next;
            }
            b'<' if bytes.get(*index + 1) != Some(&b'<') => {
                let (value, next) = parse_hex_string(bytes, *index + 1);
                tokens.push(Token::HexString(value));
                *index = next;
            }
            b'[' => {
                *index += 1;
                tokens.push(Token::Array(tokenize_until(bytes, index, Some(b']'))));
            }
            b'/' => {
                let (value, next) = parse_word(bytes, *index + 1);
                tokens.push(Token::Name(value));
                *index = next;
            }
            b']' if stop_byte.is_none() => {
                tokens.push(Token::Operator("]".into()));
                *index += 1;
            }
            b')' | b'>' if stop_byte.is_none() => {
                tokens.push(Token::Operator(char::from(bytes[*index]).to_string()));
                *index += 1;
            }
            _ => {
                let (word, next) = parse_word(bytes, *index);
                if next == *index {
                    tokens.push(Token::Operator(char::from(bytes[*index]).to_string()));
                    *index += 1;
                    continue;
                }
                if let Ok(value) = word.parse::<f32>() {
                    tokens.push(Token::Number(value));
                } else {
                    tokens.push(Token::Operator(word));
                }
                *index = next;
            }
        }
    }
    tokens
}

fn parse_word(bytes: &[u8], start: usize) -> (String, usize) {
    let mut end = start;
    while end < bytes.len()
        && !bytes[end].is_ascii_whitespace()
        && !matches!(bytes[end], b'(' | b')' | b'<' | b'>' | b'[' | b']')
    {
        end += 1;
    }
    (
        String::from_utf8_lossy(&bytes[start..end]).into_owned(),
        end,
    )
}

fn parse_hex_string(bytes: &[u8], start: usize) -> (Vec<u8>, usize) {
    let mut nybbles = Vec::new();
    let mut index = start;
    while index < bytes.len() {
        match bytes[index] {
            b'>' => {
                index += 1;
                break;
            }
            byte if byte.is_ascii_whitespace() => {
                index += 1;
            }
            byte => {
                nybbles.push(byte);
                index += 1;
            }
        }
    }

    if nybbles.len() % 2 == 1 {
        nybbles.push(b'0');
    }

    let mut output = Vec::new();
    for pair in nybbles.chunks(2) {
        let high = hex_value(pair[0]).unwrap_or(0);
        let low = hex_value(pair[1]).unwrap_or(0);
        output.push((high << 4) | low);
    }
    (output, index)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn parse_literal_string(bytes: &[u8], start: usize) -> (Vec<u8>, usize) {
    let mut output = Vec::new();
    let mut index = start;
    let mut depth = 1usize;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' if index + 1 < bytes.len() => {
                output.push(match bytes[index + 1] {
                    b'n' => b'\n',
                    b'r' => b'\r',
                    b't' => b'\t',
                    b'b' => 0x08,
                    b'f' => 0x0c,
                    escaped => escaped,
                });
                index += 2;
            }
            b'(' => {
                depth += 1;
                output.push(bytes[index]);
                index += 1;
            }
            b')' => {
                depth -= 1;
                index += 1;
                if depth == 0 {
                    break;
                }
                output.push(b')');
            }
            byte => {
                output.push(byte);
                index += 1;
            }
        }
    }
    (output, index)
}

trait DiagnosticExt {
    fn with_page(self, page_index: usize) -> Self;
    fn with_object(self, object_id: Option<ObjectId>) -> Self;
}

impl DiagnosticExt for Diagnostic {
    fn with_page(mut self, page_index: usize) -> Self {
        self.page_index = Some(page_index);
        self
    }

    fn with_object(mut self, object_id: Option<ObjectId>) -> Self {
        self.object = object_id;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spdfdiff_types::DiagnosticSeverity;

    #[test]
    fn parses_basic_text_operators() {
        let program = parse_content_stream(b"BT /F1 12 Tf 72 720 Td (Hello) Tj ET");

        assert_eq!(program.diagnostics, Vec::new());
        assert_eq!(program.operations.len(), 5);
        assert!(matches!(program.operations[0], ContentOp::BeginText { .. }));
        assert!(matches!(
            program.operations[1],
            ContentOp::SetFont { ref name, size, .. } if name == "F1" && size == 12.0
        ));
        assert!(matches!(
            program.operations[2],
            ContentOp::MoveTextPosition { tx, ty, set_leading: None, .. }
                if tx == 72.0 && ty == 720.0
        ));
        assert!(matches!(
            program.operations[3],
            ContentOp::ShowText { ref text, ref raw_bytes, .. }
                if text == "Hello" && raw_bytes == b"Hello"
        ));
    }

    #[test]
    fn parses_tj_array_text_and_adjustments() {
        let program = parse_content_stream(b"BT [(Hel) -120 <6c6f>] TJ ET");

        assert_eq!(program.diagnostics, Vec::new());
        assert!(matches!(
            program.operations[1],
            ContentOp::ShowAdjustedText {
                ref text,
                ref raw_bytes,
                ref adjustments,
                ..
            } if text == "Hello" && raw_bytes == b"Hello" && adjustments == &vec![-120.0]
        ));
    }

    #[test]
    fn parses_text_state_and_graphics_state_operators() {
        let program = parse_content_stream(b"q 1 0 0 1 10 20 cm BT 14 TL T* 2 Tc 3 Tw 90 Tz ET Q");

        assert_eq!(program.diagnostics, Vec::new());
        assert!(matches!(
            program.operations[0],
            ContentOp::SaveGraphicsState { .. }
        ));
        assert!(matches!(
            program.operations[1],
            ContentOp::ConcatMatrix {
                e: 10.0,
                f: 20.0,
                ..
            }
        ));
        assert!(matches!(
            program.operations[3],
            ContentOp::SetTextLeading { leading: 14.0, .. }
        ));
        assert!(matches!(
            program.operations[4],
            ContentOp::MoveToNextLine { .. }
        ));
        assert!(matches!(
            program.operations[5],
            ContentOp::SetCharacterSpacing { spacing: 2.0, .. }
        ));
        assert!(matches!(
            program.operations[6],
            ContentOp::SetWordSpacing { spacing: 3.0, .. }
        ));
        assert!(matches!(
            program.operations[7],
            ContentOp::SetHorizontalScaling { scale: 90.0, .. }
        ));
        assert!(matches!(
            program.operations[9],
            ContentOp::RestoreGraphicsState { .. }
        ));
    }

    #[test]
    fn recognizes_common_non_text_drawing_operators_without_diagnostics() {
        let program = parse_content_stream(b"0.1 0.2 0.3 rg 10 20 30 40 re f /Im1 Do");

        assert_eq!(program.diagnostics, Vec::new());
        assert_eq!(program.operations.len(), 4);
        assert!(matches!(
            program.operations[0],
            ContentOp::RecognizedNonText { ref operator, .. } if operator == "rg"
        ));
        assert!(matches!(
            program.operations[1],
            ContentOp::RecognizedNonText { ref operator, .. } if operator == "re"
        ));
        assert!(matches!(
            program.operations[2],
            ContentOp::RecognizedNonText { ref operator, .. } if operator == "f"
        ));
        assert!(matches!(
            program.operations[3],
            ContentOp::RecognizedNonText { ref operator, .. } if operator == "Do"
        ));
    }

    #[test]
    fn emits_diagnostic_for_unknown_operator() {
        let program = parse_content_stream(b"BT 1 2 XX ET");

        assert_eq!(program.diagnostics.len(), 1);
        assert_eq!(program.diagnostics[0].code, "CONTENT_OPERATOR_UNKNOWN");
        assert_eq!(program.diagnostics[0].severity, DiagnosticSeverity::Warning);
    }

    #[test]
    fn tokenizer_advances_over_unmatched_delimiters() {
        let program = parse_content_stream(b") > ]");

        assert_eq!(program.operations.len(), 3);
        assert_eq!(program.diagnostics.len(), 3);
        assert!(matches!(
            program.operations[0],
            ContentOp::Unknown { ref operator, .. } if operator == ")"
        ));
        assert!(matches!(
            program.operations[1],
            ContentOp::Unknown { ref operator, .. } if operator == ">"
        ));
        assert!(matches!(
            program.operations[2],
            ContentOp::Unknown { ref operator, .. } if operator == "]"
        ));
    }
}
