//! Crush Language Parser
//!
//! Handwritten recursive descent parser for Crush source code.
//! Converts text into AST for compilation.

use thiserror::Error;

#[derive(Debug, Error, Clone)]
pub enum ParseError {
    #[error("Unexpected token at line {line}, column {col}: {msg}")]
    UnexpectedToken {
        line: usize,
        col: usize,
        msg: String,
    },

    #[error("Expected {expected} but found {found} at line {line}, column {col}")]
    Expected {
        line: usize,
        col: usize,
        expected: String,
        found: String,
    },

    #[error("Unexpected end of input at line {line}, column {col}")]
    UnexpectedEOF { line: usize, col: usize },

    #[error("Invalid number literal at line {line}, column {col}: {value}")]
    InvalidNumber {
        line: usize,
        col: usize,
        value: String,
    },

    #[error("Unterminated string at line {line}, column {col}")]
    UnterminatedString { line: usize, col: usize },
}

/// Source location information
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SourceLocation {
    pub line: usize,
    pub col: usize,
}

/// Token types for Crush lexer with source location tracking
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // Literals
    Int(i64, SourceLocation),
    Float(f64, SourceLocation),
    String(String, SourceLocation),
    Bool(bool, SourceLocation),
    Null(SourceLocation),

    // Keywords
    Let(SourceLocation),
    Mut(SourceLocation),
    Fn(SourceLocation),
    If(SourceLocation),
    Else(SourceLocation),
    While(SourceLocation),
    For(SourceLocation),
    In(SourceLocation),
    Return(SourceLocation),
    Try(SourceLocation),
    Catch(SourceLocation),
    Throw(SourceLocation),
    Break(SourceLocation),
    Continue(SourceLocation),
    Struct(SourceLocation),
    Use(SourceLocation),
    Capability(SourceLocation),
    Async(SourceLocation),
    Await(SourceLocation),
    Spawn(SourceLocation),
    Yield(SourceLocation),
    Export(SourceLocation),
    Lang(SourceLocation),
    Import(SourceLocation),
    Match(SourceLocation),

    // Identifiers
    Ident(String, SourceLocation),

    // Operators
    Plus(SourceLocation),        // +
    Minus(SourceLocation),       // -
    Star(SourceLocation),        // *
    Slash(SourceLocation),       // /
    Percent(SourceLocation),     // %
    Eq(SourceLocation),          // ==
    Neq(SourceLocation),         // !=
    Lt(SourceLocation),          // <
    Gt(SourceLocation),          // >
    Lte(SourceLocation),         // <=
    Gte(SourceLocation),         // >=
    And(SourceLocation),         // &&
    Or(SourceLocation),          // ||
    Not(SourceLocation),         // !
    Assign(SourceLocation),      // =
    Pipe(SourceLocation),        // |>
    Arrow(SourceLocation),       // ->
    FatArrow(SourceLocation),    // =>  (match arm separator)
    DoubleColon(SourceLocation), // ::

    // Delimiters
    LParen(SourceLocation),    // (
    RParen(SourceLocation),    // )
    LBrace(SourceLocation),    // {
    RBrace(SourceLocation),    // }
    LBracket(SourceLocation),  // [
    RBracket(SourceLocation),  // ]
    Comma(SourceLocation),     // ,
    Colon(SourceLocation),     // :
    Semicolon(SourceLocation), // ;
    Dot(SourceLocation),       // .
    DotDot(SourceLocation),    // ..
    Question(SourceLocation),  // ?

    // Special
    Newline(SourceLocation),
    EOF(SourceLocation),
    Comment(String, SourceLocation),
    AtIdent(String, SourceLocation),  // @mcp, @cap, @lang, etc
    LangBody(String, SourceLocation), // Raw body of @python { ... }, @javascript { ... }, etc
}

/// Lexer for Crush language
pub struct Lexer {
    input: Vec<char>,
    pos: usize,
    line: usize,
    col: usize,
    /// Buffered next token. Used by the polyglot block path: after emitting
    /// `@python` (AtIdent) the lexer pre-extracts the raw body and buffers
    /// a `LangBody` here to hand back on the next call.
    pending: Option<Token>,
}

impl Lexer {
    pub fn new(input: &str) -> Self {
        Self {
            input: input.chars().collect(),
            pos: 0,
            line: 1,
            col: 1,
            pending: None,
        }
    }

    fn peek(&self) -> Option<char> {
        self.input.get(self.pos).copied()
    }

    fn peek_ahead(&self, n: usize) -> Option<char> {
        self.input.get(self.pos + n).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.input.get(self.pos).copied();
        if let Some(c) = ch {
            self.pos += 1;
            if c == '\n' {
                self.line += 1;
                self.col = 1;
            } else {
                self.col += 1;
            }
        }
        ch
    }

    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.peek() {
            if ch.is_whitespace() && ch != '\n' {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn read_string(&mut self) -> Result<Token, ParseError> {
        let start_line = self.line;
        let start_col = self.col;
        self.advance(); // Skip opening quote

        let mut value = String::new();
        while let Some(ch) = self.peek() {
            if ch == '"' {
                self.advance(); // Skip closing quote
                return Ok(Token::String(
                    value,
                    SourceLocation {
                        line: start_line,
                        col: start_col,
                    },
                ));
            } else if ch == '\\' {
                self.advance();
                match self.advance() {
                    Some('n') => value.push('\n'),
                    Some('t') => value.push('\t'),
                    Some('r') => value.push('\r'),
                    Some('\\') => value.push('\\'),
                    Some('"') => value.push('"'),
                    Some(c) => value.push(c),
                    None => {
                        return Err(ParseError::UnterminatedString {
                            line: start_line,
                            col: start_col,
                        });
                    }
                }
            } else {
                value.push(ch);
                self.advance();
            }
        }

        Err(ParseError::UnterminatedString {
            line: start_line,
            col: start_col,
        })
    }

    fn read_number(&mut self) -> Result<Token, ParseError> {
        let start_line = self.line;
        let start_col = self.col;
        let mut value = String::new();
        let mut is_float = false;

        while let Some(ch) = self.peek() {
            if ch.is_ascii_digit() {
                value.push(ch);
                self.advance();
            } else if ch == '.' && !is_float {
                if let Some(next) = self.peek_ahead(1) {
                    if next.is_ascii_digit() {
                        is_float = true;
                        value.push(ch);
                        self.advance();
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        if is_float {
            match value.parse::<f64>() {
                Ok(f) => Ok(Token::Float(
                    f,
                    SourceLocation {
                        line: start_line,
                        col: start_col,
                    },
                )),
                Err(_) => Err(ParseError::InvalidNumber {
                    line: start_line,
                    col: start_col,
                    value,
                }),
            }
        } else {
            match value.parse::<i64>() {
                Ok(i) => Ok(Token::Int(
                    i,
                    SourceLocation {
                        line: start_line,
                        col: start_col,
                    },
                )),
                Err(_) => Err(ParseError::InvalidNumber {
                    line: start_line,
                    col: start_col,
                    value,
                }),
            }
        }
    }

    /// Like `read_identifier` but also accepts `-` (hyphen) for kebab-case `@annotation-names`.
    /// Returns the raw string — callers convert to a Token::Ident by themselves.
    fn read_at_identifier(&mut self) -> String {
        let mut value = String::new();
        while let Some(ch) = self.peek() {
            if ch.is_alphanumeric() || ch == '_' || ch == '-' {
                value.push(ch);
                self.advance();
            } else {
                break;
            }
        }
        value
    }

    fn read_identifier(&mut self) -> Token {
        let start_line = self.line;
        let start_col = self.col;
        let mut value = String::new();

        while let Some(ch) = self.peek() {
            if ch.is_alphanumeric() || ch == '_' {
                value.push(ch);
                self.advance();
            } else {
                break;
            }
        }

        let location = SourceLocation {
            line: start_line,
            col: start_col,
        };

        // Check for keywords
        match value.as_str() {
            "let" => Token::Let(location),
            "mut" => Token::Mut(location),
            "fn" => Token::Fn(location),
            "if" => Token::If(location),
            "else" => Token::Else(location),
            "while" => Token::While(location),
            "for" => Token::For(location),
            "in" => Token::In(location),
            "return" => Token::Return(location),
            "try" => Token::Try(location),
            "catch" => Token::Catch(location),
            "throw" => Token::Throw(location),
            "break" => Token::Break(location),
            "continue" => Token::Continue(location),
            "struct" => Token::Struct(location),
            "use" => Token::Use(location),
            "capability" => Token::Capability(location),
            "async" => Token::Async(location),
            "await" => Token::Await(location),
            "spawn" => Token::Spawn(location),
            "yield" => Token::Yield(location),
            "export" => Token::Export(location),
            "lang" => Token::Lang(location),
            "import" => Token::Import(location),
            "match" => Token::Match(location),
            "true" => Token::Bool(true, location),
            "false" => Token::Bool(false, location),
            "null" => Token::Null(location),
            _ => Token::Ident(value, location),
        }
    }

    fn read_comment(&mut self) -> Token {
        let start_line = self.line;
        let start_col = self.col;
        let mut value = String::new();
        self.advance(); // Skip second / (first was skipped in next_token)

        while let Some(ch) = self.peek() {
            if ch == '\n' {
                break;
            } else {
                value.push(ch);
                self.advance();
            }
        }

        Token::Comment(
            value,
            SourceLocation {
                line: start_line,
                col: start_col,
            },
        )
    }

    /// After reading `@<id>`, decide whether to enter polyglot raw-body mode.
    ///
    /// `@mcp`, `@cap`, `@lang`, `@git`, `@http`, `@file` are import keywords
    /// — they stay regular AtIdent tokens so the normal import parser can
    /// pick them up. Any other identifier (`@python`, `@javascript`, `@rust`,
    /// etc.) is treated as a polyglot block: if the next non-space char is
    /// `{`, the body up to the matching closing `}` is consumed as a single
    /// `LangBody` token. String literals inside the body (single/double/
    /// backtick quotes, plus Python triple quotes) are tracked so braces
    /// inside strings don't unbalance the count.
    fn maybe_consume_polyglot_body(
        &mut self,
        id: String,
        at_location: SourceLocation,
    ) -> Result<Token, ParseError> {
        // These @-names are NOT polyglot block starters — they are either import
        // keywords or AI-native manifest/annotation keywords.  Their `{` (if any)
        // must be parsed by the normal token stream, not consumed as a LangBody.
        const IMPORT_KEYWORDS: &[&str] = &[
            // import / capability keywords
            "mcp", "cap", "lang", "git", "http", "file",
            // AI-native annotation keywords (Step 2 — manifest + function annotations)
            "module", "invariant", "exhaustive-match-sites",
            "errors", "reads", "writes", "does-not-write",
            "covers", "relies-on", "complexity",
        ];
        if IMPORT_KEYWORDS.contains(&id.as_str()) {
            return Ok(Token::AtIdent(id, at_location));
        }

        let saved = (self.pos, self.line, self.col);
        while let Some(ch) = self.peek() {
            if ch.is_whitespace() {
                self.advance();
            } else {
                break;
            }
        }

        if self.peek() != Some('{') {
            self.pos = saved.0;
            self.line = saved.1;
            self.col = saved.2;
            return Ok(Token::AtIdent(id, at_location));
        }

        let brace_loc = SourceLocation {
            line: self.line,
            col: self.col,
        };
        self.advance(); // consume opening '{'
        let body = self.read_lang_body(brace_loc)?;
        self.pending = Some(Token::LangBody(body, brace_loc));
        Ok(Token::AtIdent(id, at_location))
    }

    /// Read the raw body of a polyglot block starting at depth 1 (caller
    /// has already consumed the opening `{`). Returns the body text with
    /// the closing `}` consumed but not included.
    fn read_lang_body(&mut self, start: SourceLocation) -> Result<String, ParseError> {
        let mut body = String::new();
        let mut depth: usize = 1;

        while let Some(ch) = self.peek() {
            match ch {
                '{' => {
                    depth += 1;
                    body.push(ch);
                    self.advance();
                }
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        self.advance();
                        return Ok(body);
                    }
                    body.push(ch);
                    self.advance();
                }
                '"' | '\'' => {
                    let quote = ch;
                    if self.peek_ahead(1) == Some(quote) && self.peek_ahead(2) == Some(quote) {
                        self.consume_triple_quoted(&mut body, quote)?;
                    } else {
                        self.consume_quoted(&mut body, quote)?;
                    }
                }
                '`' => {
                    self.consume_quoted(&mut body, '`')?;
                }
                _ => {
                    body.push(ch);
                    self.advance();
                }
            }
        }

        Err(ParseError::UnexpectedEOF {
            line: start.line,
            col: start.col,
        })
    }

    fn consume_quoted(&mut self, body: &mut String, quote: char) -> Result<(), ParseError> {
        let start_line = self.line;
        let start_col = self.col;
        body.push(quote);
        self.advance(); // opening quote
        while let Some(ch) = self.peek() {
            if ch == '\\' {
                body.push(ch);
                self.advance();
                if let Some(escaped) = self.peek() {
                    body.push(escaped);
                    self.advance();
                }
                continue;
            }
            body.push(ch);
            self.advance();
            if ch == quote {
                return Ok(());
            }
        }
        Err(ParseError::UnterminatedString {
            line: start_line,
            col: start_col,
        })
    }

    fn consume_triple_quoted(&mut self, body: &mut String, quote: char) -> Result<(), ParseError> {
        let start_line = self.line;
        let start_col = self.col;
        for _ in 0..3 {
            body.push(quote);
            self.advance();
        }
        while let Some(ch) = self.peek() {
            if ch == quote && self.peek_ahead(1) == Some(quote) && self.peek_ahead(2) == Some(quote)
            {
                for _ in 0..3 {
                    body.push(quote);
                    self.advance();
                }
                return Ok(());
            }
            body.push(ch);
            self.advance();
        }
        Err(ParseError::UnterminatedString {
            line: start_line,
            col: start_col,
        })
    }

    pub fn next_token(&mut self) -> Result<Token, ParseError> {
        if let Some(buffered) = self.pending.take() {
            return Ok(buffered);
        }

        self.skip_whitespace();

        let ch = match self.peek() {
            Some(c) => c,
            None => {
                return Ok(Token::EOF(SourceLocation {
                    line: self.line,
                    col: self.col,
                }));
            }
        };

        let location = SourceLocation {
            line: self.line,
            col: self.col,
        };

        match ch {
            '\n' => {
                self.advance();
                Ok(Token::Newline(location))
            }
            '"' => self.read_string(),
            '0'..='9' => self.read_number(),
            'a'..='z' | 'A'..='Z' | '_' => Ok(self.read_identifier()),
            '+' => {
                self.advance();
                Ok(Token::Plus(location))
            }
            '-' => {
                self.advance();
                if self.peek() == Some('>') {
                    self.advance();
                    Ok(Token::Arrow(location))
                } else {
                    Ok(Token::Minus(location))
                }
            }
            '*' => {
                self.advance();
                Ok(Token::Star(location))
            }
            '/' => {
                self.advance();
                if self.peek() == Some('/') {
                    Ok(self.read_comment())
                } else {
                    Ok(Token::Slash(location))
                }
            }
            '%' => {
                self.advance();
                Ok(Token::Percent(location))
            }
            '(' => {
                self.advance();
                Ok(Token::LParen(location))
            }
            ')' => {
                self.advance();
                Ok(Token::RParen(location))
            }
            '{' => {
                self.advance();
                Ok(Token::LBrace(location))
            }
            '}' => {
                self.advance();
                Ok(Token::RBrace(location))
            }
            '[' => {
                self.advance();
                Ok(Token::LBracket(location))
            }
            ']' => {
                self.advance();
                Ok(Token::RBracket(location))
            }
            ',' => {
                self.advance();
                Ok(Token::Comma(location))
            }
            ';' => {
                self.advance();
                Ok(Token::Semicolon(location))
            }
            '?' => {
                self.advance();
                Ok(Token::Question(location))
            }
            ':' => {
                self.advance();
                if self.peek() == Some(':') {
                    self.advance();
                    Ok(Token::DoubleColon(location))
                } else {
                    Ok(Token::Colon(location))
                }
            }
            '.' => {
                self.advance();
                if self.peek() == Some('.') {
                    self.advance();
                    Ok(Token::DotDot(location))
                } else {
                    Ok(Token::Dot(location))
                }
            }
            '=' => {
                self.advance();
                if self.peek() == Some('=') {
                    self.advance();
                    Ok(Token::Eq(location))
                } else if self.peek() == Some('>') {
                    self.advance();
                    Ok(Token::FatArrow(location))
                } else {
                    Ok(Token::Assign(location))
                }
            }
            '!' => {
                self.advance();
                if self.peek() == Some('=') {
                    self.advance();
                    Ok(Token::Neq(location))
                } else {
                    Ok(Token::Not(location))
                }
            }
            '<' => {
                self.advance();
                if self.peek() == Some('=') {
                    self.advance();
                    Ok(Token::Lte(location))
                } else {
                    Ok(Token::Lt(location))
                }
            }
            '>' => {
                self.advance();
                if self.peek() == Some('=') {
                    self.advance();
                    Ok(Token::Gte(location))
                } else {
                    Ok(Token::Gt(location))
                }
            }
            '&' => {
                self.advance();
                if self.peek() == Some('&') {
                    self.advance();
                    Ok(Token::And(location))
                } else {
                    Ok(Token::Ident("&".to_string(), location)) // Single & as ident for now
                }
            }
            '|' => {
                self.advance();
                if self.peek() == Some('>') {
                    self.advance();
                    Ok(Token::Pipe(location))
                } else if self.peek() == Some('|') {
                    self.advance();
                    Ok(Token::Or(location))
                } else {
                    Ok(Token::Ident("|".to_string(), location)) // Single | as ident for now
                }
            }
            '#' => {
                // Single-line comment (same as //)
                self.advance();
                let mut value = String::new();
                while let Some(ch) = self.peek() {
                    if ch == '\n' {
                        break;
                    }
                    value.push(ch);
                    self.advance();
                }
                Ok(Token::Comment(value, location))
            }
            '@' => {
                self.advance();
                let id = self.read_at_identifier();
                if id.is_empty() {
                    let line = self.line;
                    let col = self.col;
                    Err(ParseError::UnexpectedToken {
                        line,
                        col,
                        msg: "Expected identifier after @".to_string(),
                    })
                } else {
                    self.maybe_consume_polyglot_body(id, location)
                }
            }
            _ => {
                let line = self.line;
                let col = self.col;
                self.advance();
                Err(ParseError::UnexpectedToken {
                    line,
                    col,
                    msg: format!("Unexpected character: {}", ch),
                })
            }
        }
    }

    /// Tokenize entire input into a vector
    pub fn tokenize(&mut self) -> Result<Vec<Token>, ParseError> {
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token()?;
            if matches!(token, Token::EOF(_)) {
                tokens.push(token);
                break;
            }
            tokens.push(token);
        }
        Ok(tokens)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lex_int() {
        let mut lexer = Lexer::new("42");
        assert_eq!(
            lexer.next_token().unwrap(),
            Token::Int(42, SourceLocation { line: 1, col: 1 })
        );
        assert_eq!(
            lexer.next_token().unwrap(),
            Token::EOF(SourceLocation { line: 1, col: 3 })
        );
    }

    #[test]
    fn test_lex_float() {
        let mut lexer = Lexer::new("3.14");
        assert_eq!(
            lexer.next_token().unwrap(),
            Token::Float(3.14, SourceLocation { line: 1, col: 1 })
        );
    }

    #[test]
    fn test_lex_string() {
        let mut lexer = Lexer::new("\"hello world\"");
        assert_eq!(
            lexer.next_token().unwrap(),
            Token::String(
                "hello world".to_string(),
                SourceLocation { line: 1, col: 1 }
            )
        );
    }

    #[test]
    fn test_lex_keywords() {
        let mut lexer = Lexer::new("let fn if else return");
        assert_eq!(
            lexer.next_token().unwrap(),
            Token::Let(SourceLocation { line: 1, col: 1 })
        );
        assert_eq!(
            lexer.next_token().unwrap(),
            Token::Fn(SourceLocation { line: 1, col: 5 })
        );
        assert_eq!(
            lexer.next_token().unwrap(),
            Token::If(SourceLocation { line: 1, col: 8 })
        );
        assert_eq!(
            lexer.next_token().unwrap(),
            Token::Else(SourceLocation { line: 1, col: 11 })
        );
        assert_eq!(
            lexer.next_token().unwrap(),
            Token::Return(SourceLocation { line: 1, col: 16 })
        );
    }

    #[test]
    fn test_lex_operators() {
        let mut lexer = Lexer::new("+ - * / == != < > <= >= && || |> ->");
        assert_eq!(
            lexer.next_token().unwrap(),
            Token::Plus(SourceLocation { line: 1, col: 1 })
        );
        assert_eq!(
            lexer.next_token().unwrap(),
            Token::Minus(SourceLocation { line: 1, col: 3 })
        );
        assert_eq!(
            lexer.next_token().unwrap(),
            Token::Star(SourceLocation { line: 1, col: 5 })
        );
        assert_eq!(
            lexer.next_token().unwrap(),
            Token::Slash(SourceLocation { line: 1, col: 7 })
        );
        assert_eq!(
            lexer.next_token().unwrap(),
            Token::Eq(SourceLocation { line: 1, col: 9 })
        );
        assert_eq!(
            lexer.next_token().unwrap(),
            Token::Neq(SourceLocation { line: 1, col: 12 })
        );
        assert_eq!(
            lexer.next_token().unwrap(),
            Token::Lt(SourceLocation { line: 1, col: 15 })
        );
        assert_eq!(
            lexer.next_token().unwrap(),
            Token::Gt(SourceLocation { line: 1, col: 17 })
        );
        assert_eq!(
            lexer.next_token().unwrap(),
            Token::Lte(SourceLocation { line: 1, col: 19 })
        );
        assert_eq!(
            lexer.next_token().unwrap(),
            Token::Gte(SourceLocation { line: 1, col: 22 })
        );
        assert_eq!(
            lexer.next_token().unwrap(),
            Token::And(SourceLocation { line: 1, col: 25 })
        );
        assert_eq!(
            lexer.next_token().unwrap(),
            Token::Or(SourceLocation { line: 1, col: 28 })
        );
        assert_eq!(
            lexer.next_token().unwrap(),
            Token::Pipe(SourceLocation { line: 1, col: 31 })
        );
        assert_eq!(
            lexer.next_token().unwrap(),
            Token::Arrow(SourceLocation { line: 1, col: 34 })
        );
    }

    #[test]
    fn test_lex_comment() {
        let mut lexer = Lexer::new("// this is a comment\n42");
        match lexer.next_token().unwrap() {
            Token::Comment(s, _) => assert_eq!(s, " this is a comment"),
            _ => panic!("Expected comment"),
        }
        assert_eq!(
            lexer.next_token().unwrap(),
            Token::Newline(SourceLocation { line: 1, col: 21 })
        );
        assert_eq!(
            lexer.next_token().unwrap(),
            Token::Int(42, SourceLocation { line: 2, col: 1 })
        );
    }
}
