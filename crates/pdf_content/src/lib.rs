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
    Unknown {
        operator: String,
        source: Provenance,
    },
}

#[must_use]
pub fn parse_content_stream(_bytes: &[u8]) -> ContentProgram {
    parse_content_stream_with_limits(_bytes, 0, None, ResourceLimits::default())
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
        "Td" | "TD" => Some(ContentOp::MoveTextPosition {
            tx: stack
                .get(stack.len().checked_sub(2)?)
                .and_then(Token::as_number)?,
            ty: stack.last().and_then(Token::as_number)?,
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
            let text = stack.last().and_then(Token::as_literal_string)?;
            Some(ContentOp::ShowText {
                raw_bytes: text.as_bytes().to_vec(),
                text: text.to_owned(),
                source,
            })
        }
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f32),
    Name(String),
    LiteralString(String),
    Operator(String),
}

impl Token {
    fn as_number(&self) -> Option<f32> {
        match self {
            Self::Number(value) => Some(*value),
            Self::Name(_) | Self::LiteralString(_) | Self::Operator(_) => None,
        }
    }

    fn as_name(&self) -> Option<&str> {
        match self {
            Self::Name(value) => Some(value),
            Self::Number(_) | Self::LiteralString(_) | Self::Operator(_) => None,
        }
    }

    fn as_literal_string(&self) -> Option<&str> {
        match self {
            Self::LiteralString(value) => Some(value),
            Self::Number(_) | Self::Name(_) | Self::Operator(_) => None,
        }
    }
}

fn tokenize(bytes: &[u8]) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index].is_ascii_whitespace() {
            index += 1;
            continue;
        }
        match bytes[index] {
            b'(' => {
                let (value, next) = parse_literal_string(bytes, index + 1);
                tokens.push(Token::LiteralString(value));
                index = next;
            }
            b'/' => {
                let (value, next) = parse_word(bytes, index + 1);
                tokens.push(Token::Name(value));
                index = next;
            }
            _ => {
                let (word, next) = parse_word(bytes, index);
                if let Ok(value) = word.parse::<f32>() {
                    tokens.push(Token::Number(value));
                } else {
                    tokens.push(Token::Operator(word));
                }
                index = next;
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

fn parse_literal_string(bytes: &[u8], start: usize) -> (String, usize) {
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
    (String::from_utf8_lossy(&output).into_owned(), index)
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
            program.operations[3],
            ContentOp::ShowText { ref text, .. } if text == "Hello"
        ));
    }

    #[test]
    fn emits_diagnostic_for_unknown_operator() {
        let program = parse_content_stream(b"BT 1 2 XX ET");

        assert_eq!(program.diagnostics.len(), 1);
        assert_eq!(program.diagnostics[0].code, "CONTENT_OPERATOR_UNKNOWN");
        assert_eq!(program.diagnostics[0].severity, DiagnosticSeverity::Warning);
    }
}
