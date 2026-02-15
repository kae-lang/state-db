// SMQL Parser — SMQL language parser (winnow-based)

mod lexer;
mod expr;
mod machine;
mod commands;
mod queries;
mod common;

#[cfg(test)]
mod tests;

use smql_ast::{SmqlError, SmqlResult};
use smql_ast::command::Statement;
use smql_ast::machine::MachineDefinition;

pub use lexer::Token;

/// Parse a complete SMQL input string, which may contain one or more statements.
/// Returns machine definitions and standalone statements.
pub fn parse(input: &str) -> SmqlResult<Vec<Statement>> {
    let tokens = lexer::tokenize(input)?;
    let mut parser = Parser::new(&tokens, input);
    let mut statements = Vec::new();
    while !parser.is_eof() {
        let stmt = parser.parse_statement()?;
        statements.push(stmt);
    }
    Ok(statements)
}

/// Parse a single DEFINE MACHINE statement (convenience for .smql files).
pub fn parse_machine(input: &str) -> SmqlResult<MachineDefinition> {
    let tokens = lexer::tokenize(input)?;
    let mut parser = Parser::new(&tokens, input);
    let machine = machine::parse_define_machine(&mut parser)?;
    Ok(machine)
}

/// Parse multiple machine definitions from a single input (e.g. order.smql with Order + LineItem + Shipment).
pub fn parse_machines(input: &str) -> SmqlResult<Vec<MachineDefinition>> {
    let tokens = lexer::tokenize(input)?;
    let mut parser = Parser::new(&tokens, input);
    let mut machines = Vec::new();
    while !parser.is_eof() {
        let machine = machine::parse_define_machine(&mut parser)?;
        machines.push(machine);
    }
    Ok(machines)
}

/// Internal parser state, walks over a token stream.
pub(crate) struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
    source: &'a str,
}

impl<'a> Parser<'a> {
    pub fn new(tokens: &'a [Token], source: &'a str) -> Self {
        Self {
            tokens,
            pos: 0,
            source,
        }
    }

    pub fn is_eof(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    pub fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    pub fn peek_kind(&self) -> Option<&lexer::TokenKind> {
        self.peek().map(|t| &t.kind)
    }

    pub fn peek_nth(&self, n: usize) -> Option<&Token> {
        self.tokens.get(self.pos + n)
    }

    pub fn advance(&mut self) -> SmqlResult<&Token> {
        if self.pos >= self.tokens.len() {
            return Err(SmqlError::parse("Unexpected end of input"));
        }
        let tok = &self.tokens[self.pos];
        self.pos += 1;
        Ok(tok)
    }

    pub fn expect_keyword(&mut self, kw: &str) -> SmqlResult<&Token> {
        let tok = self.advance()?;
        match &tok.kind {
            lexer::TokenKind::Keyword(k) if k.eq_ignore_ascii_case(kw) => Ok(tok),
            _ => Err(SmqlError::ParseError {
                message: format!("Expected keyword '{}', found '{}'", kw, tok.text),
                span: Some(smql_ast::Span::new(tok.offset, tok.offset + tok.text.len())),
                hint: None,
            }),
        }
    }

    pub fn expect_ident(&mut self) -> SmqlResult<String> {
        let tok = self.advance()?;
        match &tok.kind {
            lexer::TokenKind::Identifier(s) => Ok(s.clone()),
            // Keywords can also serve as identifiers in some contexts;
            // return original text to preserve case (e.g. "count" not "COUNT")
            lexer::TokenKind::Keyword(_) => Ok(tok.text.clone()),
            _ => Err(SmqlError::ParseError {
                message: format!("Expected identifier, found '{}'", tok.text),
                span: Some(smql_ast::Span::new(tok.offset, tok.offset + tok.text.len())),
                hint: None,
            }),
        }
    }

    /// Accept an identifier, keyword, or string literal — used for instance IDs
    /// which may be ULIDs starting with digits.
    pub fn expect_ident_or_string(&mut self) -> SmqlResult<String> {
        let tok = self.advance()?;
        match &tok.kind {
            lexer::TokenKind::Identifier(s) => Ok(s.clone()),
            lexer::TokenKind::Keyword(_) => Ok(tok.text.clone()),
            lexer::TokenKind::StringLiteral(s) => Ok(s.clone()),
            _ => Err(SmqlError::ParseError {
                message: format!("Expected identifier or string, found '{}'", tok.text),
                span: Some(smql_ast::Span::new(tok.offset, tok.offset + tok.text.len())),
                hint: None,
            }),
        }
    }

    pub fn expect_punct(&mut self, p: &str) -> SmqlResult<&Token> {
        let tok = self.advance()?;
        match &tok.kind {
            lexer::TokenKind::Punctuation(s) if s == p => Ok(tok),
            _ => Err(SmqlError::ParseError {
                message: format!("Expected '{}', found '{}'", p, tok.text),
                span: Some(smql_ast::Span::new(tok.offset, tok.offset + tok.text.len())),
                hint: None,
            }),
        }
    }

    pub fn expect_operator(&mut self, op: &str) -> SmqlResult<&Token> {
        let tok = self.advance()?;
        match &tok.kind {
            lexer::TokenKind::Operator(s) if s == op => Ok(tok),
            _ => Err(SmqlError::ParseError {
                message: format!("Expected '{}', found '{}'", op, tok.text),
                span: Some(smql_ast::Span::new(tok.offset, tok.offset + tok.text.len())),
                hint: None,
            }),
        }
    }

    pub fn check_keyword(&self, kw: &str) -> bool {
        matches!(self.peek_kind(), Some(lexer::TokenKind::Keyword(k)) if k.eq_ignore_ascii_case(kw))
    }

    pub fn check_punct(&self, p: &str) -> bool {
        matches!(self.peek_kind(), Some(lexer::TokenKind::Punctuation(s)) if s == p)
    }

    pub fn check_operator(&self, op: &str) -> bool {
        matches!(self.peek_kind(), Some(lexer::TokenKind::Operator(s)) if s == op)
    }

    pub fn try_keyword(&mut self, kw: &str) -> bool {
        if self.check_keyword(kw) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    pub fn try_punct(&mut self, p: &str) -> bool {
        if self.check_punct(p) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    pub fn try_operator(&mut self, op: &str) -> bool {
        if self.check_operator(op) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    pub fn current_offset(&self) -> usize {
        self.peek().map(|t| t.offset).unwrap_or(self.source.len())
    }

    fn parse_statement(&mut self) -> SmqlResult<Statement> {
        if self.check_keyword("DEFINE") {
            let machine = machine::parse_define_machine(self)?;
            Ok(Statement::Command(smql_ast::command::Command::DefineMachine(machine)))
        } else if self.check_keyword("SPAWN") {
            let cmd = commands::parse_spawn(self)?;
            Ok(Statement::Command(cmd))
        } else if self.check_keyword("TRANSITION") {
            let cmd = commands::parse_transition(self)?;
            Ok(Statement::Command(cmd))
        } else if self.check_keyword("TRY") {
            let cmd = commands::parse_try_transition(self)?;
            Ok(Statement::Command(cmd))
        } else if self.check_keyword("ALTER") {
            let cmd = commands::parse_alter_machine(self)?;
            Ok(Statement::Command(cmd))
        } else if self.check_keyword("GET") {
            let q = queries::parse_get(self)?;
            Ok(Statement::Query(q))
        } else if self.check_keyword("FIND") {
            let q = queries::parse_find(self)?;
            Ok(Statement::Query(q))
        } else if self.check_keyword("AGGREGATE") {
            let q = queries::parse_aggregate(self)?;
            Ok(Statement::Query(q))
        } else if self.check_keyword("TRAIL") {
            let q = queries::parse_trail(self)?;
            Ok(Statement::Query(q))
        } else if self.check_keyword("PATHS") {
            let q = queries::parse_paths(self)?;
            Ok(Statement::Query(q))
        } else if self.check_keyword("FUNNEL") {
            let q = queries::parse_funnel(self)?;
            Ok(Statement::Query(q))
        } else if self.check_keyword("COMPARE") {
            let q = queries::parse_compare_paths(self)?;
            Ok(Statement::Query(q))
        } else {
            let tok = self.peek();
            let msg = match tok {
                Some(t) => format!("Unexpected token '{}' at start of statement", t.text),
                None => "Unexpected end of input".to_string(),
            };
            Err(SmqlError::parse(msg))
        }
    }
}
