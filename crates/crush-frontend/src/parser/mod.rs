//! Crush Language Parser
//!
//! Recursive descent parser that converts tokens into AST.
//! Uses Pratt parsing for expressions.

mod lexer;
pub use lexer::{Lexer, ParseError, SourceLocation, Token};

use crush_cast::*;
use crush_cast::{ExternalResourceType, ImportStatement};
use std::collections::HashMap;

/// Parser for Crush language with error recovery
pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    errors: Vec<ParseError>,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            pos: 0,
            errors: Vec::new(),
        }
    }

    /// Parse source code into a Program with error recovery
    pub fn parse(source: &str) -> Result<Program, Vec<ParseError>> {
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().map_err(|e| vec![e])?;
        let mut parser = Self::new(tokens);
        match parser.parse_program() {
            Ok(program) => {
                if parser.errors.is_empty() {
                    Ok(program)
                } else {
                    Err(parser.errors)
                }
            }
            Err(_) => {
                // Return collected errors instead of first error
                if parser.errors.is_empty() {
                    Err(vec![ParseError::UnexpectedEOF { line: 0, col: 0 }])
                } else {
                    Err(parser.errors)
                }
            }
        }
    }

    /// Parse an already-tokenized program.
    ///
    /// This exists for benchmark instrumentation that needs to time lexing and
    /// parsing separately without changing the production parser entry point.
    #[doc(hidden)]
    pub fn parse_program_for_benchmark(&mut self) -> Result<Program, Vec<ParseError>> {
        match self.parse_program() {
            Ok(program) => {
                if self.errors.is_empty() {
                    Ok(program)
                } else {
                    Err(self.errors.clone())
                }
            }
            Err(_) => {
                if self.errors.is_empty() {
                    Err(vec![ParseError::UnexpectedEOF { line: 0, col: 0 }])
                } else {
                    Err(self.errors.clone())
                }
            }
        }
    }

    fn peek(&self) -> &Token {
        self.tokens
            .get(self.pos)
            .unwrap_or(&Token::EOF(SourceLocation { line: 0, col: 0 }))
    }

    fn advance(&mut self) -> &Token {
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        self.peek()
    }

    /// Helper method to get source location from a token
    fn get_location(&self, token: &Token) -> (usize, usize) {
        match token {
            Token::Int(_, loc) => (loc.line, loc.col),
            Token::Float(_, loc) => (loc.line, loc.col),
            Token::String(_, loc) => (loc.line, loc.col),
            Token::Bool(_, loc) => (loc.line, loc.col),
            Token::Null(loc) => (loc.line, loc.col),
            Token::Let(loc) => (loc.line, loc.col),
            Token::Mut(loc) => (loc.line, loc.col),
            Token::Fn(loc) => (loc.line, loc.col),
            Token::If(loc) => (loc.line, loc.col),
            Token::Else(loc) => (loc.line, loc.col),
            Token::While(loc) => (loc.line, loc.col),
            Token::For(loc) => (loc.line, loc.col),
            Token::In(loc) => (loc.line, loc.col),
            Token::Return(loc) => (loc.line, loc.col),
            Token::Try(loc) => (loc.line, loc.col),
            Token::Catch(loc) => (loc.line, loc.col),
            Token::Throw(loc) => (loc.line, loc.col),
            Token::Break(loc) => (loc.line, loc.col),
            Token::Continue(loc) => (loc.line, loc.col),
            Token::Struct(loc) => (loc.line, loc.col),
            Token::Use(loc) => (loc.line, loc.col),
            Token::Capability(loc) => (loc.line, loc.col),
            Token::Async(loc) => (loc.line, loc.col),
            Token::Await(loc) => (loc.line, loc.col),
            Token::Spawn(loc) => (loc.line, loc.col),
            Token::Yield(loc) => (loc.line, loc.col),
            Token::Export(loc) => (loc.line, loc.col),
            Token::Lang(loc) => (loc.line, loc.col),
            Token::Import(loc) => (loc.line, loc.col),
            Token::Match(loc) => (loc.line, loc.col),
            Token::Ident(_, loc) => (loc.line, loc.col),
            Token::Plus(loc) => (loc.line, loc.col),
            Token::Minus(loc) => (loc.line, loc.col),
            Token::Star(loc) => (loc.line, loc.col),
            Token::Slash(loc) => (loc.line, loc.col),
            Token::Percent(loc) => (loc.line, loc.col),
            Token::Eq(loc) => (loc.line, loc.col),
            Token::Neq(loc) => (loc.line, loc.col),
            Token::Lt(loc) => (loc.line, loc.col),
            Token::Gt(loc) => (loc.line, loc.col),
            Token::Lte(loc) => (loc.line, loc.col),
            Token::Gte(loc) => (loc.line, loc.col),
            Token::And(loc) => (loc.line, loc.col),
            Token::Or(loc) => (loc.line, loc.col),
            Token::Not(loc) => (loc.line, loc.col),
            Token::Assign(loc) => (loc.line, loc.col),
            Token::Pipe(loc) => (loc.line, loc.col),
            Token::Arrow(loc) => (loc.line, loc.col),
            Token::DoubleColon(loc) => (loc.line, loc.col),
            Token::LParen(loc) => (loc.line, loc.col),
            Token::RParen(loc) => (loc.line, loc.col),
            Token::LBrace(loc) => (loc.line, loc.col),
            Token::RBrace(loc) => (loc.line, loc.col),
            Token::LBracket(loc) => (loc.line, loc.col),
            Token::RBracket(loc) => (loc.line, loc.col),
            Token::Comma(loc) => (loc.line, loc.col),
            Token::Colon(loc) => (loc.line, loc.col),
            Token::Semicolon(loc) => (loc.line, loc.col),
            Token::Dot(loc) => (loc.line, loc.col),
            Token::DotDot(loc) => (loc.line, loc.col),
            Token::Question(loc) => (loc.line, loc.col),
            Token::Newline(loc) => (loc.line, loc.col),
            Token::EOF(loc) => (loc.line, loc.col),
            Token::Comment(_, loc) => (loc.line, loc.col),
            Token::AtIdent(_, loc) => (loc.line, loc.col),
            Token::LangBody(_, loc) => (loc.line, loc.col),
        }
    }

    fn expect(&mut self, expected: Token) -> Result<(), ()> {
        let actual = self.peek();
        if std::mem::discriminant(actual) == std::mem::discriminant(&expected) {
            self.advance();
            Ok(())
        } else {
            let (line, col) = self.get_location(actual);

            self.errors.push(ParseError::Expected {
                line,
                col,
                expected: format!("{:?}", expected),
                found: format!("{:?}", actual),
            });
            Err(())
        }
    }

    fn skip_newlines(&mut self) {
        while matches!(self.peek(), Token::Newline(_) | Token::Comment(_, _)) {
            self.advance();
        }
    }

    /// Synchronize to the next statement boundary
    fn synchronize(&mut self) {
        let start_pos = self.pos;

        while !matches!(self.peek(), Token::EOF(_)) {
            match self.peek() {
                Token::Newline(_) => {
                    self.advance();
                    return;
                }
                Token::Let(_)
                | Token::If(_)
                | Token::While(_)
                | Token::For(_)
                | Token::Return(_)
                | Token::Struct(_)
                | Token::Import(_)
                | Token::Use(_)
                | Token::Export(_) => {
                    // Guarantee progress even if already at a boundary token.
                    if self.pos == start_pos {
                        self.advance();
                    }
                    return;
                }
                Token::RBrace(_) => {
                    self.advance();
                    return;
                }
                _ => {
                    self.advance();
                }
            }
        }
    }

    /// Parse a complete program with error recovery
    fn parse_program(&mut self) -> Result<Program, ()> {
        let mut functions = HashMap::new();
        let mut statements = Vec::new();
        let mut iterations = 0usize;

        self.skip_newlines();

        while !matches!(self.peek(), Token::EOF(_)) {
            iterations += 1;
            if iterations > (self.tokens.len().saturating_mul(4)).max(1024) {
                let (line, col) = self.get_location(self.peek());
                self.errors.push(ParseError::UnexpectedToken {
                    line,
                    col,
                    msg: "Parser iteration limit reached during recovery".to_string(),
                });
                break;
            }

            let loop_start = self.pos;
            self.skip_newlines();

            if matches!(self.peek(), Token::EOF(_)) {
                break;
            }

            // Try to parse capability declarations first
            if matches!(self.peek(), Token::Capability(_)) {
                // TODO: Parse capability declarations
                self.advance();
                if let Token::Ident(_name, _) = self.peek() {
                    self.advance();
                    // Parse permission (readonly, readwrite, etc.)
                    if let Token::Ident(_perm, _) = self.peek() {
                        self.advance();
                        // Store capability somewhere
                    }
                }
                continue;
            }

            // Try to parse function definition
            if matches!(self.peek(), Token::Fn(_)) {
                match self.parse_function() {
                    Ok((name, func)) => {
                        functions.insert(name, func);
                    }
                    Err(_) => {
                        // Error recovery: synchronize to next statement
                        self.synchronize();
                    }
                }
                continue;
            }

            // Parse statement
            match self.parse_statement() {
                Ok(stmt) => statements.push(stmt),
                Err(_) => {
                    // Error recovery: synchronize to next statement
                    self.synchronize();
                }
            }

            self.skip_newlines();

            // Defensive progress check to avoid infinite loops in error recovery.
            if self.pos == loop_start {
                let (line, col) = self.get_location(self.peek());
                self.errors.push(ParseError::UnexpectedToken {
                    line,
                    col,
                    msg: "Parser made no progress; skipping token during recovery".to_string(),
                });
                self.advance();
            }
        }

        // Create main function from top-level statements
        if !statements.is_empty() {
            let main_func = Function {
                params: Vec::new(),
                body: statements,
                meta: HashMap::new(),
            };
            functions.insert("main".to_string(), main_func);
        }

        Ok(Program {
            cast_version: "1.0.0".to_string(),
            entry: "main".to_string(),
            lang: Some("crush".to_string()),
            functions,
            ai_meta: None,
        })
    }

    /// Parse a function definition with error recovery
    fn parse_function(&mut self) -> Result<(String, Function), ()> {
        match self.peek() {
            Token::Fn(_) => {
                self.advance();
            }
            _ => {
                let (line, col) = self.get_location(self.peek());
                self.errors.push(ParseError::Expected {
                    line,
                    col,
                    expected: "fn keyword".to_string(),
                    found: format!("{:?}", self.peek()),
                });
                return Err(());
            }
        }

        let name = match self.peek() {
            Token::Ident(n, _) => {
                let name = n.clone();
                self.advance();
                name
            }
            _ => {
                let (line, col) = self.get_location(self.peek());
                self.errors.push(ParseError::Expected {
                    line,
                    col,
                    expected: "function name".to_string(),
                    found: format!("{:?}", self.peek()),
                });
                return Err(());
            }
        };

        match self.peek() {
            Token::LParen(_) => {
                self.advance();
            }
            _ => {
                let (line, col) = self.get_location(self.peek());
                self.errors.push(ParseError::Expected {
                    line,
                    col,
                    expected: "(".to_string(),
                    found: format!("{:?}", self.peek()),
                });
                return Err(());
            }
        }

        let params = self.parse_parameters()?;

        match self.peek() {
            Token::RParen(_) => {
                self.advance();
            }
            _ => {
                let (line, col) = self.get_location(self.peek());
                self.errors.push(ParseError::Expected {
                    line,
                    col,
                    expected: ")".to_string(),
                    found: format!("{:?}", self.peek()),
                });
                return Err(());
            }
        }

        // Optional return type (not stored in AST Function currently)
        if matches!(self.peek(), Token::Arrow(_)) {
            self.advance();
            if let Token::Ident(_, _) = self.peek() {
                self.advance();
            }
        }

        match self.peek() {
            Token::LBrace(_) => {
                self.advance();
            }
            _ => {
                let (line, col) = self.get_location(self.peek());
                self.errors.push(ParseError::Expected {
                    line,
                    col,
                    expected: "{".to_string(),
                    found: format!("{:?}", self.peek()),
                });
                return Err(());
            }
        }

        self.skip_newlines();

        let body = self.parse_block()?;

        match self.peek() {
            Token::RBrace(_) => {
                self.advance();
            }
            _ => {
                let (line, col) = self.get_location(self.peek());
                self.errors.push(ParseError::Expected {
                    line,
                    col,
                    expected: "}".to_string(),
                    found: format!("{:?}", self.peek()),
                });
                return Err(());
            }
        }

        let func = Function {
            params: params.into_iter().map(|p| (p.name, p.type_hint)).collect(),
            body,
            meta: HashMap::new(),
        };

        Ok((name, func))
    }

    /// Parse function parameters with error recovery
    fn parse_parameters(&mut self) -> Result<Vec<Parameter>, ()> {
        let mut params = Vec::new();

        while !matches!(self.peek(), Token::RParen(_)) {
            let name = match self.peek() {
                Token::Ident(n, _) => {
                    let name = n.clone();
                    self.advance();
                    name
                }
                _ => break,
            };

            let type_hint = if matches!(self.peek(), Token::Colon(_)) {
                self.advance();
                self.parse_cast_type()?
            } else {
                CastType::Any
            };

            params.push(Parameter { name, type_hint });

            if matches!(self.peek(), Token::Comma(_)) {
                self.advance();
            } else {
                break;
            }
        }

        Ok(params)
    }

    fn parse_cast_type(&mut self) -> Result<CastType, ()> {
        match self.peek() {
            Token::Ident(t, _) => {
                let name = t.clone();
                self.advance();
                match name.as_str() {
                    "Int" | "int" | "i64" => Ok(CastType::Int),
                    "Float" | "float" | "f64" => Ok(CastType::Float),
                    "String" | "string" | "str" => Ok(CastType::String),
                    "Bool" | "bool" | "boolean" => Ok(CastType::Bool),
                    "Null" | "null" | "unit" => Ok(CastType::Null),
                    "Any" | "any" => Ok(CastType::Any),
                    _ => Ok(CastType::TypeRef(name)),
                }
            }
            _ => {
                // Default to Any if no type specified after colon (though colon implies type)
                Ok(CastType::Any)
            }
        }
    }

    /// Parse a block of statements with error recovery
    fn parse_block(&mut self) -> Result<Vec<Statement>, ()> {
        let mut statements = Vec::new();

        while !matches!(self.peek(), Token::RBrace(_)) && !matches!(self.peek(), Token::EOF(_)) {
            self.skip_newlines();

            if matches!(self.peek(), Token::RBrace(_)) || matches!(self.peek(), Token::EOF(_)) {
                break;
            }

            match self.parse_statement() {
                Ok(stmt) => statements.push(stmt),
                Err(_) => {
                    // Error recovery: synchronize to next statement
                    self.synchronize();
                }
            }

            self.skip_newlines();
        }

        Ok(statements)
    }

    /// Parse a statement with error recovery
    fn parse_statement(&mut self) -> Result<Statement, ()> {
        self.skip_newlines();

        match self.peek() {
            Token::Let(_) => self.parse_let_statement(),
            Token::If(_) => self.parse_if_statement(),
            Token::While(_) => self.parse_while_statement(),
            Token::For(_) => self.parse_for_statement(),
            Token::Return(_) => self.parse_return_statement(),
            Token::Break(_) => {
                self.advance();
                Ok(Statement::Break {
                    meta: HashMap::new(),
                })
            }
            Token::Continue(_) => {
                self.advance();
                Ok(Statement::Continue {
                    meta: HashMap::new(),
                })
            }
            Token::Try(_) => self.parse_try_catch(),
            Token::Throw(_) => self.parse_throw_statement(),
            Token::Struct(_) => self.parse_struct_def(),
            Token::Import(_) | Token::Use(_) => self.parse_import_statement(),
            Token::Export(_) => self.parse_export_statement(),
            Token::AtIdent(_, _)
                if matches!(
                    self.tokens.get(self.pos + 1),
                    Some(Token::LangBody(_, _))
                ) =>
            {
                self.parse_lang_block()
            }
            _ => self.parse_expression_statement(),
        }
    }

    /// Parse let statement: let name = value
    fn parse_let_statement(&mut self) -> Result<Statement, ()> {
        self.expect(Token::Let(SourceLocation { line: 0, col: 0 }))?;

        let name = match self.peek() {
            Token::Ident(n, _) => {
                let name = n.clone();
                self.advance();
                name
            }
            _ => {
                let (line, col) = self.get_location(self.peek());
                self.errors.push(ParseError::Expected {
                    line,
                    col,
                    expected: "variable name".to_string(),
                    found: format!("{:?}", self.peek()),
                });
                return Err(());
            }
        };

        // Optional type annotation
        let type_hint = if matches!(self.peek(), Token::Colon(_)) {
            self.advance();
            Some(self.parse_cast_type()?)
        } else {
            None
        };

        self.expect(Token::Assign(SourceLocation { line: 0, col: 0 }))?;

        let value = self.parse_expression()?;

        Ok(Statement::VarDecl {
            name,
            value,
            type_hint: type_hint.unwrap_or(CastType::Any),
            meta: HashMap::new(),
        })
    }

    /// Parse if statement: if condition { body } else { body }
    fn parse_if_statement(&mut self) -> Result<Statement, ()> {
        self.expect(Token::If(SourceLocation { line: 0, col: 0 }))?;

        let condition = self.parse_expression()?;

        self.expect(Token::LBrace(SourceLocation { line: 0, col: 0 }))?;
        self.skip_newlines();
        let then_body = self.parse_block()?;
        self.expect(Token::RBrace(SourceLocation { line: 0, col: 0 }))?;

        let else_body = if matches!(self.peek(), Token::Else(_)) {
            self.advance();
            if matches!(self.peek(), Token::If(_)) {
                // else if
                let else_if = self.parse_if_statement()?;
                vec![else_if]
            } else {
                self.expect(Token::LBrace(SourceLocation { line: 0, col: 0 }))?;
                self.skip_newlines();
                let body = self.parse_block()?;
                self.expect(Token::RBrace(SourceLocation { line: 0, col: 0 }))?;
                body
            }
        } else {
            Vec::new()
        };

        Ok(Statement::If {
            condition,
            then_body,
            else_body: if else_body.is_empty() {
                None
            } else {
                Some(else_body)
            },
            meta: HashMap::new(),
        })
    }

    /// Parse while statement: while condition { body }
    fn parse_while_statement(&mut self) -> Result<Statement, ()> {
        self.expect(Token::While(SourceLocation { line: 0, col: 0 }))?;

        let condition = self.parse_expression()?;

        self.expect(Token::LBrace(SourceLocation { line: 0, col: 0 }))?;
        self.skip_newlines();
        let body = self.parse_block()?;
        self.expect(Token::RBrace(SourceLocation { line: 0, col: 0 }))?;

        Ok(Statement::While {
            condition: Box::new(condition),
            body,
            meta: HashMap::new(),
        })
    }

    /// Parse for statement: for item in iterable { body }
    fn parse_for_statement(&mut self) -> Result<Statement, ()> {
        self.expect(Token::For(SourceLocation { line: 0, col: 0 }))?;

        let variable = match self.peek() {
            Token::Ident(n, _) => {
                let name = n.clone();
                self.advance();
                name
            }
            _ => {
                let (line, col) = self.get_location(self.peek());
                self.errors.push(ParseError::Expected {
                    line,
                    col,
                    expected: "variable name".to_string(),
                    found: format!("{:?}", self.peek()),
                });
                return Err(());
            }
        };

        self.expect(Token::In(SourceLocation { line: 0, col: 0 }))?;

        let iterable = self.parse_expression()?;

        self.expect(Token::LBrace(SourceLocation { line: 0, col: 0 }))?;
        self.skip_newlines();
        let body = self.parse_block()?;
        self.expect(Token::RBrace(SourceLocation { line: 0, col: 0 }))?;

        Ok(Statement::For {
            variable,
            iterable: Box::new(iterable),
            body,
            meta: HashMap::new(),
        })
    }

    /// Parse return statement: return value
    fn parse_return_statement(&mut self) -> Result<Statement, ()> {
        self.expect(Token::Return(SourceLocation { line: 0, col: 0 }))?;

        let value = if self.is_expression_start() {
            Some(self.parse_expression()?)
        } else {
            None
        };

        Ok(Statement::Return {
            value,
            meta: HashMap::new(),
        })
    }

    /// Parse try-catch statement
    fn parse_try_catch(&mut self) -> Result<Statement, ()> {
        self.expect(Token::Try(SourceLocation { line: 0, col: 0 }))?;
        self.expect(Token::LBrace(SourceLocation { line: 0, col: 0 }))?;
        self.skip_newlines();
        let body = self.parse_block()?;
        self.expect(Token::RBrace(SourceLocation { line: 0, col: 0 }))?;

        self.expect(Token::Catch(SourceLocation { line: 0, col: 0 }))?;

        let error_var = match self.peek() {
            Token::Ident(n, _) => {
                let name = n.clone();
                self.advance();
                name
            }
            _ => "error".to_string(),
        };

        self.expect(Token::LBrace(SourceLocation { line: 0, col: 0 }))?;
        self.skip_newlines();
        let handler = self.parse_block()?;
        self.expect(Token::RBrace(SourceLocation { line: 0, col: 0 }))?;

        Ok(Statement::TryCatch {
            body,
            error_var,
            handler,
            meta: HashMap::new(),
        })
    }

    /// Parse throw statement
    fn parse_throw_statement(&mut self) -> Result<Statement, ()> {
        self.expect(Token::Throw(SourceLocation { line: 0, col: 0 }))?;
        let value = self.parse_expression()?;

        Ok(Statement::Throw {
            value,
            meta: HashMap::new(),
        })
    }

    /// Parse struct definition
    fn parse_struct_def(&mut self) -> Result<Statement, ()> {
        self.expect(Token::Struct(SourceLocation { line: 0, col: 0 }))?;

        let name = match self.peek() {
            Token::Ident(n, _) => {
                let name = n.clone();
                self.advance();
                name
            }
            _ => {
                let (line, col) = self.get_location(self.peek());
                self.errors.push(ParseError::Expected {
                    line,
                    col,
                    expected: "struct name".to_string(),
                    found: format!("{:?}", self.peek()),
                });
                return Err(());
            }
        };

        self.expect(Token::LBrace(SourceLocation { line: 0, col: 0 }))?;

        let mut fields = Vec::new();
        while !matches!(self.peek(), Token::RBrace(_)) && !matches!(self.peek(), Token::EOF(_)) {
            let field_name = match self.peek() {
                Token::Ident(n, _) => {
                    let name = n.clone();
                    self.advance();
                    name
                }
                _ => break,
            };

            let field_type = if matches!(self.peek(), Token::Colon(_)) {
                self.advance();
                self.parse_cast_type()?
            } else {
                CastType::Any
            };

            fields.push((field_name, field_type));

            if matches!(self.peek(), Token::Comma(_)) {
                self.advance();
            } else {
                break;
            }
        }

        self.expect(Token::RBrace(SourceLocation { line: 0, col: 0 }))?;

        Ok(Statement::StructDef {
            name,
            fields,
            meta: HashMap::new(),
        })
    }

    /// Parse export statement
    fn parse_export_statement(&mut self) -> Result<Statement, ()> {
        self.expect(Token::Export(SourceLocation { line: 0, col: 0 }))?;

        let name = match self.peek() {
            Token::Ident(n, _) => {
                let name = n.clone();
                self.advance();
                name
            }
            _ => {
                let (line, col) = self.get_location(self.peek());
                self.errors.push(ParseError::Expected {
                    line,
                    col,
                    expected: "name to export".to_string(),
                    found: format!("{:?}", self.peek()),
                });
                return Err(());
            }
        };

        let value = if matches!(self.peek(), Token::Assign(_)) {
            self.advance();
            self.parse_expression()?
        } else {
            Expression::Var {
                name: name.clone(),
                meta: HashMap::new(),
            }
        };

        Ok(Statement::Export {
            name,
            value,
            meta: HashMap::new(),
        })
    }

    /// Parse expression statement
    fn parse_expression_statement(&mut self) -> Result<Statement, ()> {
        let expr = self.parse_expression()?;

        Ok(Statement::ExprStmt {
            expr,
            meta: HashMap::new(),
        })
    }

    /// Check if current token can start an expression
    fn is_expression_start(&self) -> bool {
        match self.peek() {
            Token::Int(..)
            | Token::Float(..)
            | Token::String(..)
            | Token::Bool(..)
            | Token::Null(..)
            | Token::Ident(..)
            | Token::LParen(..)
            | Token::LBracket(..)
            | Token::LBrace(..)
            | Token::Minus(..)
            | Token::Not(..)
            | Token::Await(..)
            | Token::Spawn(..) => true,
            _ => false,
        }
    }

    /// Parse expression (Pratt parser)
    fn parse_expression(&mut self) -> Result<Expression, ()> {
        self.parse_expression_with_precedence(0)
    }

    /// Parse expression with given minimum precedence
    fn parse_expression_with_precedence(&mut self, min_prec: u8) -> Result<Expression, ()> {
        let mut left = self.parse_primary()?;

        loop {
            let (op, prec, right_assoc) = match self.peek() {
                Token::Pipe(_) => ("|>", 10, false),
                Token::Or(_) => ("||", 20, false),
                Token::And(_) => ("&&", 30, false),
                Token::Eq(_) => ("==", 40, false),
                Token::Neq(_) => ("!=", 40, false),
                Token::Lt(_) => ("<", 50, false),
                Token::Gt(_) => (">", 50, false),
                Token::Lte(_) => ("<=", 50, false),
                Token::Gte(_) => (">=", 50, false),
                Token::Plus(_) => ("+", 60, false),
                Token::Minus(_) => ("-", 60, false),
                Token::Star(_) => ("*", 70, false),
                Token::Slash(_) => ("/", 70, false),
                Token::Percent(_) => ("%", 70, false),
                Token::Dot(_) => (".", 80, false),
                Token::LBracket(_) => ("[]", 80, false),
                Token::LParen(_) => ("()", 90, false),
                _ => break,
            };

            if prec < min_prec {
                break;
            }

            self.advance();

            let next_min_prec = if right_assoc { prec } else { prec + 1 };

            left = if op == "|>" {
                // Pipeline: left |> right becomes right(left)
                let right = self.parse_expression_with_precedence(next_min_prec)?;
                match right {
                    Expression::Call {
                        function,
                        args,
                        meta,
                    } => {
                        let mut new_args = vec![left];
                        new_args.extend(args);
                        Expression::Call {
                            function,
                            args: new_args,
                            meta,
                        }
                    }
                    Expression::Var { name, meta } => Expression::Call {
                        function: name,
                        args: vec![left],
                        meta,
                    },
                    _ => {
                        let (line, col) = self.get_location(self.peek());
                        self.errors.push(ParseError::UnexpectedToken {
                            line,
                            col,
                            msg: "Pipeline right side must be function".to_string(),
                        });
                        return Err(());
                    }
                }
            } else if op == "." {
                // Field access
                let field = match self.peek() {
                    Token::Ident(n, _) => {
                        let name = n.clone();
                        self.advance();
                        name
                    }
                    _ => {
                        let (line, col) = self.get_location(self.peek());
                        self.errors.push(ParseError::Expected {
                            line,
                            col,
                            expected: "field name".to_string(),
                            found: format!("{:?}", self.peek()),
                        });
                        return Err(());
                    }
                };

                Expression::GetField {
                    target: Box::new(left),
                    field,
                    meta: HashMap::new(),
                }
            } else if op == "[]" {
                // Index access
                let index = self.parse_expression()?;
                self.expect(Token::RBracket(SourceLocation { line: 0, col: 0 }))?;

                Expression::Index {
                    target: Box::new(left),
                    index: Box::new(index),
                    meta: HashMap::new(),
                }
            } else if op == "()" {
                // Function call
                let args = self.parse_arguments()?;
                self.expect(Token::RParen(SourceLocation { line: 0, col: 0 }))?;

                match left {
                    Expression::Var { name, .. } => Expression::Call {
                        function: name,
                        args,
                        meta: HashMap::new(),
                    },
                    Expression::GetField { target, field, .. } => {
                        match *target {
                            Expression::Var { name: obj, .. } => Expression::CapabilityCall {
                                name: format!("{}.{}", obj, field),
                                args,
                                meta: HashMap::new(),
                            },
                            _ => {
                                let (line, col) = self.get_location(self.peek());
                                self.errors.push(ParseError::UnexpectedToken {
                                    line,
                                    col,
                                    msg: "Cannot call non-function".to_string(),
                                });
                                return Err(());
                            }
                        }
                    }
                    _ => {
                        let (line, col) = self.get_location(self.peek());
                        self.errors.push(ParseError::UnexpectedToken {
                            line,
                            col,
                            msg: "Cannot call non-function".to_string(),
                        });
                        return Err(());
                    }
                }
            } else {
                // Binary operator
                let right = self.parse_expression_with_precedence(next_min_prec)?;
                Expression::BinaryOp {
                    operator: op.to_string(),
                    left: Box::new(left),
                    right: Box::new(right),
                    meta: HashMap::new(),
                }
            };
        }

        Ok(left)
    }

    /// Parse primary expression
    fn parse_primary(&mut self) -> Result<Expression, ()> {
        match self.peek() {
            Token::Int(n, _) => {
                let value = *n;
                self.advance();
                Ok(Expression::IntLiteral {
                    value,
                    meta: HashMap::new(),
                })
            }
            Token::Float(n, _) => {
                let value = *n;
                self.advance();
                Ok(Expression::FloatLiteral {
                    value,
                    meta: HashMap::new(),
                })
            }
            Token::String(s, _) => {
                let value = s.clone();
                self.advance();
                Ok(Expression::StringLiteral {
                    value,
                    meta: HashMap::new(),
                })
            }
            Token::Pipe(_) => self.parse_lambda(),
            Token::Bool(b, _) => {
                let value = *b;
                self.advance();
                Ok(Expression::BoolLiteral {
                    value,
                    meta: HashMap::new(),
                })
            }
            Token::Null(_) => {
                self.advance();
                Ok(Expression::NullLiteral {
                    meta: HashMap::new(),
                })
            }
            Token::Ident(name, _) => {
                let n = name.clone();
                self.advance();
                Ok(Expression::Var {
                    name: n,
                    meta: HashMap::new(),
                })
            }
            Token::LParen(_) => {
                self.advance();
                let expr = self.parse_expression()?;
                self.expect(Token::RParen(SourceLocation { line: 0, col: 0 }))?;
                Ok(expr)
            }
            Token::LBracket(_) => self.parse_array_literal(),
            Token::LBrace(_) => self.parse_object_literal(),
            Token::Match(_) => self.parse_match_expression(),
            Token::Minus(_) => {
                self.advance();
                let operand = self.parse_primary()?;
                Ok(Expression::UnaryOp {
                    operator: "-".to_string(),
                    operand: Box::new(operand),
                    meta: HashMap::new(),
                })
            }
            Token::Not(_) => {
                self.advance();
                let operand = self.parse_primary()?;
                Ok(Expression::UnaryOp {
                    operator: "!".to_string(),
                    operand: Box::new(operand),
                    meta: HashMap::new(),
                })
            }
            Token::Await(_) => {
                self.advance();
                let expr = self.parse_expression()?;
                Ok(Expression::Await {
                    expression: Box::new(expr),
                    meta: HashMap::new(),
                })
            }
            Token::Spawn(_) => {
                self.advance();
                let func = match self.peek() {
                    Token::Ident(n, _) => {
                        let name = n.clone();
                        self.advance();
                        name
                    }
                    _ => {
                        let (line, col) = self.get_location(self.peek());
                        self.errors.push(ParseError::Expected {
                            line,
                            col,
                            expected: "function name".to_string(),
                            found: format!("{:?}", self.peek()),
                        });
                        return Err(());
                    }
                };

                self.expect(Token::LParen(SourceLocation { line: 0, col: 0 }))?;
                let args = self.parse_arguments()?;
                self.expect(Token::RParen(SourceLocation { line: 0, col: 0 }))?;

                Ok(Expression::Spawn {
                    function: func,
                    args,
                    meta: HashMap::new(),
                })
            }
            _ => {
                let (line, col) = self.get_location(self.peek());
                self.errors.push(ParseError::UnexpectedToken {
                    line,
                    col,
                    msg: format!("Unexpected token in expression: {:?}", self.peek()),
                });
                Err(())
            }
        }
    }

    /// Parse array literal: [1, 2, 3]
    fn parse_array_literal(&mut self) -> Result<Expression, ()> {
        self.expect(Token::LBracket(SourceLocation { line: 0, col: 0 }))?;

        let mut elements = Vec::new();
        while !matches!(self.peek(), Token::RBracket(_)) && !matches!(self.peek(), Token::EOF(_)) {
            let elem = self.parse_expression()?;
            elements.push(elem);

            if matches!(self.peek(), Token::Comma(_)) {
                self.advance();
            } else {
                break;
            }
        }

        self.expect(Token::RBracket(SourceLocation { line: 0, col: 0 }))?;

        Ok(Expression::ArrayLiteral {
            elements,
            meta: HashMap::new(),
        })
    }

    /// Parse object literal: {name: "value", age: 30}
    fn parse_object_literal(&mut self) -> Result<Expression, ()> {
        self.expect(Token::LBrace(SourceLocation { line: 0, col: 0 }))?;

        let mut properties = Vec::new();
        while !matches!(self.peek(), Token::RBrace(_)) && !matches!(self.peek(), Token::EOF(_)) {
            let key = match self.peek() {
                Token::Ident(n, _) => {
                    let name = n.clone();
                    self.advance();
                    name
                }
                Token::String(s, _) => {
                    let name = s.clone();
                    self.advance();
                    name
                }
                _ => {
                    let (line, col) = self.get_location(self.peek());
                    self.errors.push(ParseError::Expected {
                        line,
                        col,
                        expected: "property name".to_string(),
                        found: format!("{:?}", self.peek()),
                    });
                    return Err(());
                }
            };

            self.expect(Token::Colon(SourceLocation { line: 0, col: 0 }))?;

            let value = self.parse_expression()?;
            properties.push((key, value));

            if matches!(self.peek(), Token::Comma(_)) {
                self.advance();
            } else {
                break;
            }
        }

        self.expect(Token::RBrace(SourceLocation { line: 0, col: 0 }))?;

        Ok(Expression::ObjectLiteral {
            properties,
            meta: HashMap::new(),
        })
    }

    /// Parse lambda expression: |args| { body } or |args| => expr
    fn parse_lambda(&mut self) -> Result<Expression, ()> {
        self.expect(Token::Pipe(SourceLocation { line: 0, col: 0 }))?;

        let mut params = Vec::new();
        while !matches!(self.peek(), Token::Pipe(_)) && !matches!(self.peek(), Token::EOF(_)) {
            match self.peek() {
                Token::Ident(n, _) => {
                    let name = n.clone();
                    self.advance();

                    let type_hint = if matches!(self.peek(), Token::Colon(_)) {
                        self.advance();
                        self.parse_cast_type()?
                    } else {
                        CastType::Any
                    };
                    params.push((name, type_hint));
                }
                _ => {
                    let (line, col) = self.get_location(self.peek());
                    self.errors.push(ParseError::Expected {
                        line,
                        col,
                        expected: "parameter name".to_string(),
                        found: format!("{:?}", self.peek()),
                    });
                    return Err(());
                }
            }

            if matches!(self.peek(), Token::Comma(_)) {
                self.advance();
            }
        }

        self.expect(Token::Pipe(SourceLocation { line: 0, col: 0 }))?;

        let body = if matches!(self.peek(), Token::LBrace(_)) {
            self.expect(Token::LBrace(SourceLocation { line: 0, col: 0 }))?;
            let b = self.parse_block()?;
            self.expect(Token::RBrace(SourceLocation { line: 0, col: 0 }))?;
            b
        } else if matches!(self.peek(), Token::Arrow(_)) {
            self.advance();
            let expr = self.parse_expression()?;
            vec![Statement::Return {
                value: Some(expr),
                meta: HashMap::new(),
            }]
        } else {
            Vec::new()
        };

        Ok(Expression::Lambda {
            params,
            body,
            meta: HashMap::new(),
        })
    }

    fn parse_match_expression(&mut self) -> Result<Expression, ()> {
        self.expect(Token::Match(SourceLocation { line: 0, col: 0 }))?;
        let expression = Box::new(self.parse_expression()?);
        self.expect(Token::LBrace(SourceLocation { line: 0, col: 0 }))?;

        let mut arms = Vec::new();
        while !matches!(self.peek(), Token::RBrace(_)) && !matches!(self.peek(), Token::EOF(_)) {
            arms.push(self.parse_match_arm()?);
            self.skip_newlines();
        }

        self.expect(Token::RBrace(SourceLocation { line: 0, col: 0 }))?;

        Ok(Expression::Match {
            expression,
            arms,
            meta: HashMap::new(),
        })
    }

    fn parse_match_arm(&mut self) -> Result<MatchArm, ()> {
        let pattern = self.parse_pattern()?;
        self.expect(Token::Arrow(SourceLocation { line: 0, col: 0 }))?;

        let body = if matches!(self.peek(), Token::LBrace(_)) {
            self.expect(Token::LBrace(SourceLocation { line: 0, col: 0 }))?;
            let b = self.parse_block()?;
            self.expect(Token::RBrace(SourceLocation { line: 0, col: 0 }))?;
            b
        } else {
            let expr = self.parse_expression()?;
            // If there's a comma, skip it
            if matches!(self.peek(), Token::Comma(_)) {
                self.advance();
            }
            vec![Statement::ExprStmt {
                expr,
                meta: HashMap::new(),
            }]
        };

        Ok(MatchArm { pattern, body })
    }

    fn parse_pattern(&mut self) -> Result<Pattern, ()> {
        match self.peek() {
            Token::Int(n, _) => {
                let val = *n;
                self.advance();
                Ok(Pattern::Literal {
                    value: Expression::IntLiteral {
                        value: val,
                        meta: HashMap::new(),
                    },
                })
            }
            Token::String(s, _) => {
                let val = s.clone();
                self.advance();
                Ok(Pattern::Literal {
                    value: Expression::StringLiteral {
                        value: val,
                        meta: HashMap::new(),
                    },
                })
            }
            Token::Bool(b, _) => {
                let val = *b;
                self.advance();
                Ok(Pattern::Literal {
                    value: Expression::BoolLiteral {
                        value: val,
                        meta: HashMap::new(),
                    },
                })
            }
            Token::Ident(name, _) => {
                let n = name.clone();
                if n == "_" {
                    self.advance();
                    Ok(Pattern::Wildcard)
                } else {
                    self.advance();
                    if matches!(self.peek(), Token::LBrace(_)) {
                        self.advance();
                        let mut fields = Vec::new();
                        while !matches!(self.peek(), Token::RBrace(_))
                            && !matches!(self.peek(), Token::EOF(_))
                        {
                            let f_name = match self.peek() {
                                Token::Ident(fnm, _) => {
                                    let fnm = fnm.clone();
                                    self.advance();
                                    fnm
                                }
                                _ => break,
                            };
                            self.expect(Token::Colon(SourceLocation { line: 0, col: 0 }))?;
                            let f_pattern = self.parse_pattern()?;
                            fields.push((f_name, f_pattern));

                            if matches!(self.peek(), Token::Comma(_)) {
                                self.advance();
                            } else {
                                break;
                            }
                        }
                        self.expect(Token::RBrace(SourceLocation { line: 0, col: 0 }))?;
                        Ok(Pattern::Struct { name: n, fields })
                    } else {
                        Ok(Pattern::Identifier { name: n })
                    }
                }
            }
            _ => {
                let (line, col) = self.get_location(self.peek());
                self.errors.push(ParseError::Expected {
                    line,
                    col,
                    expected: "pattern".to_string(),
                    found: format!("{:?}", self.peek()),
                });
                Err(())
            }
        }
    }

    fn parse_import_statement(&mut self) -> Result<Statement, ()> {
        let is_use = matches!(self.peek(), Token::Use(_));
        self.advance();

        let import_stmt = if is_use {
            match self.peek() {
                Token::AtIdent(id, _) => {
                    let id = id.clone();
                    self.advance();
                    match id.as_str() {
                        "mcp" => self.parse_mcp_import()?,
                        "cap" => self.parse_capability_import()?,
                        "lang" => self.parse_polyglot_import()?,
                        _ => {
                            let (line, col) = self.get_location(self.peek());
                            self.errors.push(ParseError::UnexpectedToken {
                                line,
                                col,
                                msg: format!("Expected @mcp, @cap, or @lang, found @{}", id),
                            });
                            return Err(());
                        }
                    }
                }
                _ => {
                    let (line, col) = self.get_location(self.peek());
                    self.errors.push(ParseError::Expected {
                        line,
                        col,
                        expected: "identifier after 'use'".to_string(),
                        found: format!("{:?}", self.peek()),
                    });
                    return Err(());
                }
            }
        } else {
            match self.peek() {
                Token::AtIdent(id, _) => {
                    let id = id.clone();
                    self.advance();
                    self.parse_external_import(id)?
                }
                _ => self.parse_module_import()?,
            }
        };

        if matches!(self.peek(), Token::Semicolon(_)) {
            self.advance();
        }

        Ok(Statement::Import {
            import: import_stmt,
            meta: HashMap::new(),
        })
    }

    fn parse_module_import(&mut self) -> Result<ImportStatement, ()> {
        let mut path = Vec::new();
        match self.peek() {
            Token::Ident(id, _) => {
                path.push(id.clone());
                self.advance();
            }
            _ => {
                let (line, col) = self.get_location(self.peek());
                self.errors.push(ParseError::Expected {
                    line,
                    col,
                    expected: "module path".to_string(),
                    found: format!("{:?}", self.peek()),
                });
                return Err(());
            }
        }

        while matches!(self.peek(), Token::Dot(_)) {
            self.advance();
            match self.peek() {
                Token::Ident(id, _) => {
                    path.push(id.clone());
                    self.advance();
                }
                _ => break,
            }
        }

        let module_path = path.join(".");
        let mut alias = None;
        if let Token::Ident(s, _) = self.peek() {
            if s == "as" {
                self.advance();
                match self.peek() {
                    Token::Ident(a, _) => {
                        alias = Some(a.clone());
                        self.advance();
                    }
                    _ => {
                        let (line, col) = self.get_location(self.peek());
                        self.errors.push(ParseError::Expected {
                            line,
                            col,
                            expected: "alias name".to_string(),
                            found: format!("{:?}", self.peek()),
                        });
                        return Err(());
                    }
                }
            }
        }

        let mut selective = Vec::new();
        if matches!(self.peek(), Token::LBrace(_)) {
            self.advance();
            while !matches!(self.peek(), Token::RBrace(_)) && !matches!(self.peek(), Token::EOF(_))
            {
                match self.peek() {
                    Token::Ident(id, _) => {
                        selective.push(id.clone());
                        self.advance();
                    }
                    _ => break,
                }
                if matches!(self.peek(), Token::Comma(_)) {
                    self.advance();
                } else {
                    break;
                }
            }
            self.expect(Token::RBrace(SourceLocation { line: 0, col: 0 }))?;
        }

        Ok(ImportStatement::CrushModule {
            module_path,
            alias,
            selective,
        })
    }

    fn parse_mcp_import(&mut self) -> Result<ImportStatement, ()> {
        let server_url = match self.peek() {
            Token::String(s, _) => {
                let s = s.clone();
                self.advance();
                s
            }
            _ => {
                let (line, col) = self.get_location(self.peek());
                self.errors.push(ParseError::Expected {
                    line,
                    col,
                    expected: "server URL string".to_string(),
                    found: format!("{:?}", self.peek()),
                });
                return Err(());
            }
        };

        let mut tools = Vec::new();
        if matches!(self.peek(), Token::LBrace(_)) {
            self.advance();
            while !matches!(self.peek(), Token::RBrace(_)) && !matches!(self.peek(), Token::EOF(_))
            {
                match self.peek() {
                    Token::String(s, _) => {
                        tools.push(s.clone());
                        self.advance();
                    }
                    _ => break,
                }
                if matches!(self.peek(), Token::Comma(_)) {
                    self.advance();
                } else {
                    break;
                }
            }
            self.expect(Token::RBrace(SourceLocation { line: 0, col: 0 }))?;
        }

        let mut alias = None;
        if let Token::Ident(s, _) = self.peek() {
            if s == "as" {
                self.advance();
                match self.peek() {
                    Token::Ident(a, _) => {
                        alias = Some(a.clone());
                        self.advance();
                    }
                    _ => {}
                }
            }
        }

        Ok(ImportStatement::MCPImport {
            server_url,
            tools,
            alias,
        })
    }

    fn parse_capability_import(&mut self) -> Result<ImportStatement, ()> {
        let capability_path = match self.peek() {
            Token::String(s, _) => {
                let s = s.clone();
                self.advance();
                s
            }
            _ => {
                let (line, col) = self.get_location(self.peek());
                self.errors.push(ParseError::Expected {
                    line,
                    col,
                    expected: "capability path string".to_string(),
                    found: format!("{:?}", self.peek()),
                });
                return Err(());
            }
        };

        let mut permissions = Vec::new();
        if matches!(self.peek(), Token::LBrace(_)) {
            self.advance();
            while !matches!(self.peek(), Token::RBrace(_)) && !matches!(self.peek(), Token::EOF(_))
            {
                match self.peek() {
                    Token::String(s, _) => {
                        permissions.push(s.clone());
                        self.advance();
                    }
                    _ => break,
                }
                if matches!(self.peek(), Token::Comma(_)) {
                    self.advance();
                } else {
                    break;
                }
            }
            self.expect(Token::RBrace(SourceLocation { line: 0, col: 0 }))?;
        }

        let mut alias = None;
        if let Token::Ident(s, _) = self.peek() {
            if s == "as" {
                self.advance();
                match self.peek() {
                    Token::Ident(a, _) => {
                        alias = Some(a.clone());
                        self.advance();
                    }
                    _ => {}
                }
            }
        }

        Ok(ImportStatement::Capability {
            capability_path,
            permissions,
            alias,
        })
    }

    /// Parse a polyglot block: `@<lang> { <opaque body> }`.
    ///
    /// The lexer has already pre-extracted the body as a single `LangBody`
    /// token using string-aware brace counting, so the parser just stitches
    /// the language name and body together into a `Statement::LangBlock`.
    fn parse_lang_block(&mut self) -> Result<Statement, ()> {
        let lang = match self.peek() {
            Token::AtIdent(id, _) => {
                let id = id.clone();
                self.advance();
                id
            }
            _ => {
                let (line, col) = self.get_location(self.peek());
                self.errors.push(ParseError::Expected {
                    line,
                    col,
                    expected: "@<language>".to_string(),
                    found: format!("{:?}", self.peek()),
                });
                return Err(());
            }
        };

        let code = match self.peek() {
            Token::LangBody(body, _) => {
                let body = body.clone();
                self.advance();
                body
            }
            _ => {
                let (line, col) = self.get_location(self.peek());
                self.errors.push(ParseError::Expected {
                    line,
                    col,
                    expected: "polyglot block body".to_string(),
                    found: format!("{:?}", self.peek()),
                });
                return Err(());
            }
        };

        Ok(Statement::LangBlock {
            lang,
            code,
            variables: Vec::new(),
            imports: Vec::new(),
            meta: HashMap::new(),
        })
    }

    fn parse_polyglot_import(&mut self) -> Result<ImportStatement, ()> {
        let language = match self.peek() {
            Token::Ident(id, _) => {
                let id = id.clone();
                self.advance();
                id
            }
            _ => {
                let (line, col) = self.get_location(self.peek());
                self.errors.push(ParseError::Expected {
                    line,
                    col,
                    expected: "language identifier".to_string(),
                    found: format!("{:?}", self.peek()),
                });
                return Err(());
            }
        };

        let module_path = match self.peek() {
            Token::String(s, _) => {
                let s = s.clone();
                self.advance();
                s
            }
            _ => {
                let (line, col) = self.get_location(self.peek());
                self.errors.push(ParseError::Expected {
                    line,
                    col,
                    expected: "module path string".to_string(),
                    found: format!("{:?}", self.peek()),
                });
                return Err(());
            }
        };

        let mut selective = Vec::new();
        if matches!(self.peek(), Token::LBrace(_)) {
            self.advance();
            while !matches!(self.peek(), Token::RBrace(_)) && !matches!(self.peek(), Token::EOF(_))
            {
                match self.peek() {
                    Token::String(s, _) => {
                        selective.push(s.clone());
                        self.advance();
                    }
                    _ => break,
                }
                if matches!(self.peek(), Token::Comma(_)) {
                    self.advance();
                } else {
                    break;
                }
            }
            self.expect(Token::RBrace(SourceLocation { line: 0, col: 0 }))?;
        }

        let mut alias = None;
        if let Token::Ident(s, _) = self.peek() {
            if s == "as" {
                self.advance();
                match self.peek() {
                    Token::Ident(a, _) => {
                        alias = Some(a.clone());
                        self.advance();
                    }
                    _ => {}
                }
            }
        }

        Ok(ImportStatement::PolyglotModule {
            language,
            module_path,
            alias,
            selective,
        })
    }

    fn parse_external_import(&mut self, id: String) -> Result<ImportStatement, ()> {
        let resource_type = match id.as_str() {
            "http" => ExternalResourceType::Http,
            "git" => ExternalResourceType::Git,
            "file" => ExternalResourceType::File,
            _ => {
                let (line, col) = self.get_location(self.peek());
                self.errors.push(ParseError::UnexpectedToken {
                    line,
                    col,
                    msg: format!("Unknown external resource type: @{}", id),
                });
                return Err(());
            }
        };

        let uri = match self.peek() {
            Token::String(s, _) => {
                let s = s.clone();
                self.advance();
                s
            }
            _ => {
                let (line, col) = self.get_location(self.peek());
                self.errors.push(ParseError::Expected {
                    line,
                    col,
                    expected: "URI string".to_string(),
                    found: format!("{:?}", self.peek()),
                });
                return Err(());
            }
        };

        let mut alias = None;
        if let Token::Ident(s, _) = self.peek() {
            if s == "as" {
                self.advance();
                match self.peek() {
                    Token::Ident(a, _) => {
                        alias = Some(a.clone());
                        self.advance();
                    }
                    _ => {}
                }
            }
        }

        Ok(ImportStatement::External {
            uri,
            resource_type,
            alias,
        })
    }

    /// Parse function arguments
    fn parse_arguments(&mut self) -> Result<Vec<Expression>, ()> {
        let mut args = Vec::new();

        while !matches!(self.peek(), Token::RParen(_)) && !matches!(self.peek(), Token::EOF(_)) {
            let arg = self.parse_expression()?;
            args.push(arg);

            if matches!(self.peek(), Token::Comma(_)) {
                self.advance();
            } else {
                break;
            }
        }

        Ok(args)
    }
}

/// Simple parameter struct for parsing
#[derive(Debug, Clone)]
pub struct Parameter {
    pub name: String,
    pub type_hint: CastType,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_literal() {
        let source = "42";
        let program = Parser::parse(source).unwrap();
        assert!(program.functions.contains_key("main"));
    }

    #[test]
    fn test_parse_let_statement() {
        let source = "let x = 42";
        let program = Parser::parse(source).unwrap();
        assert!(program.functions.contains_key("main"));
    }

    #[test]
    fn test_parse_function() {
        let source = r#"
fn add(x: Int, y: Int) -> Int {
    return x + y
}
"#;
        let program = Parser::parse(source).unwrap();
        assert!(program.functions.contains_key("add"));
    }

    #[test]
    fn test_parse_if_statement() {
        let source = r#"
if x > 0 {
    print("positive")
} else {
    print("non-positive")
}
"#;
        let program = Parser::parse(source).unwrap();
        assert!(program.functions.contains_key("main"));
    }

    #[test]
    fn test_parse_array_literal() {
        let source = "[1, 2, 3]";
        let program = Parser::parse(source).unwrap();
        assert!(program.functions.contains_key("main"));
    }

    #[test]
    fn test_parse_object_literal() {
        let source = r#"{name: "test", value: 42}"#;
        let program = Parser::parse(source).unwrap();
        assert!(program.functions.contains_key("main"));
    }

    #[test]
    fn test_parse_pipeline() {
        let source = "data |> process |> output";
        let program = Parser::parse(source).unwrap();
        assert!(program.functions.contains_key("main"));
    }

    #[test]
    fn test_multi_error_reporting() {
        let source = r#"
let x = 42
let y =
let = 7
let z = 100
"#;
        let result = Parser::parse(source);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.len() > 1);
        // Check that errors have line/col information
        for error in &errors {
            match error {
                ParseError::Expected { line, col, .. } => {
                    assert!(*line > 0);
                    assert!(*col > 0);
                }
                _ => {}
            }
        }
    }

    #[test]
    fn test_error_recovery() {
        let source = r#"
let x = 42
invalid syntax here
let y = 100
"#;
        let result = Parser::parse(source);
        // Should recover and parse the valid statements
        assert!(result.is_ok());
        let program = result.unwrap();
        assert!(program.functions.contains_key("main"));
    }
}
