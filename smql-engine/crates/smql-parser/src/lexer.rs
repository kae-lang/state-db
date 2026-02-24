use smql_ast::{SmqlError, SmqlResult};

/// A single token from the SMQL input.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub text: String,
    pub offset: usize,
}

/// Token classification.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Keyword(String),
    Identifier(String),
    StringLiteral(String),
    IntLiteral(i64),
    FloatLiteral(f64),
    DurationLiteral(u64), // seconds
    Operator(String),
    Punctuation(String),
}

/// All SMQL keywords (case-insensitive).
const KEYWORDS: &[&str] = &[
    "DEFINE",
    "MACHINE",
    "DATA",
    "STATES",
    "INITIAL",
    "STATE",
    "TERMINAL",
    "TRANSITIONS",
    "TRANSITION",
    "GUARD",
    "ACTION",
    "TIMEOUT",
    "MUTATE",
    "CHILDREN",
    "PARENT",
    "HOOKS",
    "ROLES",
    "SPAWN",
    "FIND",
    "GET",
    "AGGREGATE",
    "TRAIL",
    "PATHS",
    "FUNNEL",
    "COMPARE",
    "WHERE",
    "SORT",
    "LIMIT",
    "OFFSET",
    "GROUP",
    "BY",
    "MEASURE",
    "OF",
    "FROM",
    "TO",
    "AS",
    "WITH",
    "MEMO",
    "THROUGH",
    "TRY",
    "BATCH",
    "ALTER",
    "ADD",
    "REMOVE",
    "MODIFY",
    "BACKFILL",
    "MIGRATE",
    "ANY",
    "ALL",
    "EXCEPT",
    "AND",
    "OR",
    "NOT",
    "IN",
    "IS",
    "SET",
    "NULL",
    "REQUIRED",
    "OPTIONAL",
    "DEFAULT",
    "MAX",
    "MIN",
    "RANGE",
    "UNIQUE",
    "PATTERN",
    "ENUM",
    "REF",
    "LIST",
    "MAP",
    "THEN",
    "ON",
    "BEFORE",
    "AFTER",
    "EACH",
    "ENTER",
    "EXIT",
    "DWELL",
    "SIGNAL",
    "EMIT",
    "NOTIFY",
    "LOG",
    "WEBHOOK",
    "CASCADE",
    "SUBSCRIBE",
    "DELIVER",
    "SEGMENT",
    "STUCK_IN",
    "TIMEOUT_REMAINING",
    "HAS_VISITED",
    "NEVER_VISITED",
    "ALIVE",
    "TERMINATED",
    "CONTAINS",
    "SELF",
    "ACTOR",
    "MONEY",
    "UUID",
    "TEXT",
    "INT",
    "FLOAT",
    "BOOL",
    "DATE",
    "DATETIME",
    "DURATION",
    "BLOB",
    "JSON",
    "TRUE",
    "FALSE",
    "ASC",
    "DESC",
    "COUNT",
    "SUM",
    "AVG",
    "PERCENTILE",
    "NOW",
    "TODAY",
    "OR_STAY",
    "DELETED",
    "CAN",
    "EXPLAIN",
    "FOR",
    "SELECT",
    "SINCE",
    "UNTIL",
    "IDEMPOTENCY_KEY",
    "TAGS",
    "TAG",
    "CLAIM",
    "RELEASE",
    "LEASE",
    "EVENTS",
    "TTL",
    "WATCH",
    "BEGIN",
    "COMMIT",
    "STORE",
    "ON_FAILURE",
    "TEMPLATE",
    "EXTENDS",
];

/// Tokenize SMQL input into a token stream.
pub fn tokenize(input: &str) -> SmqlResult<Vec<Token>> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    // Map from char index → byte offset (for correct UTF-8 string slicing)
    let byte_offsets: Vec<usize> = input
        .char_indices()
        .map(|(i, _)| i)
        .chain(std::iter::once(input.len()))
        .collect();
    let mut pos = 0;

    while pos < chars.len() {
        // Skip whitespace
        if chars[pos].is_whitespace() {
            pos += 1;
            continue;
        }

        // Skip line comments: -- to end of line
        if pos + 1 < chars.len() && chars[pos] == '-' && chars[pos + 1] == '-' {
            while pos < chars.len() && chars[pos] != '\n' {
                pos += 1;
            }
            continue;
        }

        let start = pos;

        // String literals
        if chars[pos] == '"' {
            pos += 1;
            let mut s = String::new();
            while pos < chars.len() && chars[pos] != '"' {
                if chars[pos] == '\\' && pos + 1 < chars.len() {
                    pos += 1;
                    match chars[pos] {
                        'n' => s.push('\n'),
                        't' => s.push('\t'),
                        '\\' => s.push('\\'),
                        '"' => s.push('"'),
                        c => {
                            s.push('\\');
                            s.push(c);
                        }
                    }
                } else {
                    s.push(chars[pos]);
                }
                pos += 1;
            }
            if pos >= chars.len() {
                return Err(SmqlError::ParseError {
                    message: "Unterminated string literal".into(),
                    span: Some(smql_ast::Span::new(byte_offsets[start], byte_offsets[pos.min(chars.len())])),
                    hint: Some("Add a closing '\"'".into()),
                });
            }
            pos += 1; // skip closing "
            let text: String = input[byte_offsets[start]..byte_offsets[pos]].to_string();
            tokens.push(Token {
                kind: TokenKind::StringLiteral(s),
                text,
                offset: byte_offsets[start],
            });
            continue;
        }

        // Two-character operators
        if pos + 1 < chars.len() {
            let two: String = chars[pos..pos + 2].iter().collect();
            match two.as_str() {
                "==" | "!=" | ">=" | "<=" | "->" => {
                    tokens.push(Token {
                        kind: TokenKind::Operator(two.clone()),
                        text: two,
                        offset: byte_offsets[start],
                    });
                    pos += 2;
                    continue;
                }
                _ => {}
            }
        }

        // Single-character operators and punctuation
        match chars[pos] {
            '{' | '}' | '(' | ')' | ':' | ',' | '[' | ']' | '.' => {
                let c = chars[pos].to_string();
                tokens.push(Token {
                    kind: TokenKind::Punctuation(c.clone()),
                    text: c,
                    offset: byte_offsets[start],
                });
                pos += 1;
                continue;
            }
            '>' | '<' | '=' => {
                let c = chars[pos].to_string();
                tokens.push(Token {
                    kind: TokenKind::Operator(c.clone()),
                    text: c,
                    offset: byte_offsets[start],
                });
                pos += 1;
                continue;
            }
            '+' | '*' | '/' => {
                let c = chars[pos].to_string();
                tokens.push(Token {
                    kind: TokenKind::Operator(c.clone()),
                    text: c,
                    offset: byte_offsets[start],
                });
                pos += 1;
                continue;
            }
            _ => {}
        }

        // Numbers and duration literals
        if chars[pos].is_ascii_digit()
            || (chars[pos] == '-' && pos + 1 < chars.len() && chars[pos + 1].is_ascii_digit())
        {
            let negative = chars[pos] == '-';
            if negative {
                pos += 1;
            }
            let num_start = pos;
            while pos < chars.len() && chars[pos].is_ascii_digit() {
                pos += 1;
            }

            // Check for duration suffix
            if pos < chars.len()
                && (chars[pos] == 's'
                    || chars[pos] == 'm'
                    || chars[pos] == 'h'
                    || chars[pos] == 'd')
            {
                // Check it's not part of an identifier
                let next_after = if pos + 1 < chars.len() {
                    chars[pos + 1]
                } else {
                    ' '
                };
                if !next_after.is_alphanumeric() && next_after != '_' {
                    let num: u64 = input[byte_offsets[num_start]..byte_offsets[pos]]
                        .parse()
                        .map_err(|_| SmqlError::parse("Invalid number in duration"))?;
                    let suffix = chars[pos];
                    pos += 1;
                    let seconds = match suffix {
                        's' => num,
                        'm' => num.checked_mul(60).ok_or_else(|| SmqlError::parse("Duration value too large"))?,
                        'h' => num.checked_mul(3600).ok_or_else(|| SmqlError::parse("Duration value too large"))?,
                        'd' => num.checked_mul(86400).ok_or_else(|| SmqlError::parse("Duration value too large"))?,
                        _ => unreachable!(),
                    };
                    let text = input[byte_offsets[start]..byte_offsets[pos]].to_string();
                    tokens.push(Token {
                        kind: TokenKind::DurationLiteral(seconds),
                        text,
                        offset: byte_offsets[start],
                    });
                    continue;
                }
            }

            // Check for float
            if pos < chars.len()
                && chars[pos] == '.'
                && pos + 1 < chars.len()
                && chars[pos + 1].is_ascii_digit()
            {
                pos += 1;
                while pos < chars.len() && chars[pos].is_ascii_digit() {
                    pos += 1;
                }
                let text = input[byte_offsets[start]..byte_offsets[pos]].to_string();
                let val: f64 = text
                    .parse()
                    .map_err(|_| SmqlError::parse("Invalid float literal"))?;
                tokens.push(Token {
                    kind: TokenKind::FloatLiteral(val),
                    text,
                    offset: byte_offsets[start],
                });
                continue;
            }

            // Integer
            let text = input[byte_offsets[start]..byte_offsets[pos]].to_string();
            let val: i64 = text
                .parse()
                .map_err(|_| SmqlError::parse("Invalid integer literal"))?;
            tokens.push(Token {
                kind: TokenKind::IntLiteral(val),
                text,
                offset: byte_offsets[start],
            });
            continue;
        }

        // Identifiers and keywords
        if chars[pos].is_alphabetic() || chars[pos] == '_' {
            while pos < chars.len() && (chars[pos].is_alphanumeric() || chars[pos] == '_') {
                pos += 1;
            }
            let text = input[byte_offsets[start]..byte_offsets[pos]].to_string();
            let upper = text.to_uppercase();

            // Check if it's a keyword
            if KEYWORDS.contains(&upper.as_str()) {
                tokens.push(Token {
                    kind: TokenKind::Keyword(upper),
                    text,
                    offset: byte_offsets[start],
                });
            } else {
                tokens.push(Token {
                    kind: TokenKind::Identifier(text.clone()),
                    text,
                    offset: byte_offsets[start],
                });
            }
            continue;
        }

        // Minus sign (not part of a number)
        if chars[pos] == '-' {
            let c = chars[pos].to_string();
            tokens.push(Token {
                kind: TokenKind::Operator("-".into()),
                text: c,
                offset: byte_offsets[start],
            });
            pos += 1;
            continue;
        }

        // Unknown character
        return Err(SmqlError::ParseError {
            message: format!("Unexpected character '{}'", chars[pos]),
            span: Some(smql_ast::Span::new(byte_offsets[pos], byte_offsets[pos + 1])),
            hint: None,
        });
    }

    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_keywords() {
        let tokens = tokenize("DEFINE MACHINE").unwrap();
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].kind, TokenKind::Keyword("DEFINE".into()));
        assert_eq!(tokens[1].kind, TokenKind::Keyword("MACHINE".into()));
    }

    #[test]
    fn tokenize_case_insensitive_keywords() {
        let tokens = tokenize("define Machine").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Keyword("DEFINE".into()));
        assert_eq!(tokens[1].kind, TokenKind::Keyword("MACHINE".into()));
    }

    #[test]
    fn tokenize_identifiers() {
        let tokens = tokenize("customer_id myField").unwrap();
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].kind, TokenKind::Identifier("customer_id".into()));
        assert_eq!(tokens[1].kind, TokenKind::Identifier("myField".into()));
    }

    #[test]
    fn tokenize_string_literals() {
        let tokens = tokenize("\"hello world\" \"escape \\\"quote\\\"\"").unwrap();
        assert_eq!(tokens.len(), 2);
        assert_eq!(
            tokens[0].kind,
            TokenKind::StringLiteral("hello world".into())
        );
        assert_eq!(
            tokens[1].kind,
            TokenKind::StringLiteral("escape \"quote\"".into())
        );
    }

    #[test]
    fn tokenize_numbers() {
        let tokens = tokenize("42 3.125 -5").unwrap();
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0].kind, TokenKind::IntLiteral(42));
        assert_eq!(tokens[1].kind, TokenKind::FloatLiteral(3.125));
        assert_eq!(tokens[2].kind, TokenKind::IntLiteral(-5));
    }

    #[test]
    fn tokenize_duration_literals() {
        let tokens = tokenize("24h 7d 30m 60s").unwrap();
        assert_eq!(tokens.len(), 4);
        assert_eq!(tokens[0].kind, TokenKind::DurationLiteral(86400));
        assert_eq!(tokens[1].kind, TokenKind::DurationLiteral(604800));
        assert_eq!(tokens[2].kind, TokenKind::DurationLiteral(1800));
        assert_eq!(tokens[3].kind, TokenKind::DurationLiteral(60));
    }

    #[test]
    fn tokenize_operators() {
        let tokens = tokenize("== != >= <= > < -> + - * /").unwrap();
        assert_eq!(tokens.len(), 11);
        assert_eq!(tokens[0].kind, TokenKind::Operator("==".into()));
        assert_eq!(tokens[1].kind, TokenKind::Operator("!=".into()));
        assert_eq!(tokens[2].kind, TokenKind::Operator(">=".into()));
        assert_eq!(tokens[3].kind, TokenKind::Operator("<=".into()));
        assert_eq!(tokens[4].kind, TokenKind::Operator(">".into()));
        assert_eq!(tokens[5].kind, TokenKind::Operator("<".into()));
        assert_eq!(tokens[6].kind, TokenKind::Operator("->".into()));
    }

    #[test]
    fn tokenize_punctuation() {
        let tokens = tokenize("{ } ( ) : , [ ]").unwrap();
        assert_eq!(tokens.len(), 8);
        assert_eq!(tokens[0].kind, TokenKind::Punctuation("{".into()));
        assert_eq!(tokens[1].kind, TokenKind::Punctuation("}".into()));
    }

    #[test]
    fn tokenize_comments() {
        let tokens = tokenize("DEFINE -- this is a comment\nMACHINE").unwrap();
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].kind, TokenKind::Keyword("DEFINE".into()));
        assert_eq!(tokens[1].kind, TokenKind::Keyword("MACHINE".into()));
    }

    #[test]
    fn tokenize_unterminated_string() {
        let result = tokenize("\"hello");
        assert!(result.is_err());
    }

    #[test]
    fn tokenize_mixed() {
        let input = "GUARD : assignee IS SET";
        let tokens = tokenize(input).unwrap();
        assert_eq!(tokens.len(), 5);
        assert_eq!(tokens[0].kind, TokenKind::Keyword("GUARD".into()));
        assert_eq!(tokens[1].kind, TokenKind::Punctuation(":".into()));
        assert_eq!(tokens[2].kind, TokenKind::Identifier("assignee".into()));
        assert_eq!(tokens[3].kind, TokenKind::Keyword("IS".into()));
        assert_eq!(tokens[4].kind, TokenKind::Keyword("SET".into()));
    }

    #[test]
    fn tokenize_full_transition() {
        let input = r#"open -> triaged {
      GUARD  : assignee IS SET
      ACTION : NOTIFY(assignee, "ticket.assigned")
    }"#;
        let tokens = tokenize(input).unwrap();
        // Should tokenize without errors
        assert!(tokens.len() > 5);
    }
}
