//! Crush Language Parser
//!
//! Recursive descent parser that converts tokens into AST.
//! Uses Pratt parsing for expressions.

mod lexer;
pub mod cson;
pub use lexer::{Lexer, ParseError, SourceLocation, Token};

use crush_cast::manifest::{
    DecisionNode, ErrorLikelihood, FunctionAnnotations, Invariant, ModuleManifest, TemporaryNode,
    WeightedError, WipNode,
};
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
            Token::Arrow(loc) | Token::FatArrow(loc) => (loc.line, loc.col),
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

    /// Optionally consume a semicolon (statement terminator)
    fn maybe_semicolon(&mut self) {
        if matches!(self.peek(), Token::Semicolon(_)) {
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

        // Accumulated annotation state
        let mut pending_manifest: Option<ModuleManifest> = None;
        let mut pending_invariants: Vec<Invariant> = Vec::new();
        let mut pending_exhaustive_types: Vec<String> = Vec::new();
        let mut pending_fn_annotations: Option<FunctionAnnotations> = None;
        let mut pending_wip: Option<WipNode> = None;
        let mut pending_temporaries: Vec<TemporaryNode> = Vec::new();
        let mut pending_decisions: Vec<DecisionNode> = Vec::new();

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

            // @-annotation tokens at top level
            if let Token::AtIdent(id, _) = self.peek().clone() {
                // Skip if followed by LangBody — that's a polyglot block, handled below
                let next_is_lang_body =
                    matches!(self.tokens.get(self.pos + 1), Some(Token::LangBody(_, _)));
                if !next_is_lang_body {
                    match id.as_str() {
                        "module" => {
                            self.advance();
                            if let Ok(m) = self.parse_module_block() {
                                pending_manifest = Some(m);
                            }
                            continue;
                        }
                        "invariant" => {
                            self.advance();
                            if let Ok(inv) = self.parse_invariant_block() {
                                pending_invariants.push(inv);
                            }
                            continue;
                        }
                        "exhaustive-match-sites" => {
                            self.advance();
                            let items = self.parse_at_items();
                            pending_exhaustive_types.extend(items);
                            continue;
                        }
                        "errors" | "reads" | "writes" | "does-not-write" | "covers"
                        | "relies-on" | "complexity"
                        | "invalidates" | "must-call-before" | "must-call-after" => {
                            self.advance();
                            let ann = pending_fn_annotations.get_or_insert_with(Default::default);
                            self.parse_fn_annotation_body(&id, ann);
                            continue;
                        }
                        "wip" => {
                            self.advance();
                            pending_wip = Some(self.parse_wip_block());
                            continue;
                        }
                        "temporary" => {
                            self.advance();
                            pending_temporaries.push(self.parse_temporary_block());
                            continue;
                        }
                        "decision" => {
                            self.advance();
                            pending_decisions.push(self.parse_decision_block());
                            continue;
                        }
                        _ => {}
                    }
                }
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
                match self.parse_function(false) {
                    Ok((name, mut func)) => {
                        if let Some(ann) = pending_fn_annotations.take() {
                            func.annotations = Some(ann);
                        }
                        functions.insert(name, func);
                    }
                    Err(_) => {
                        pending_fn_annotations = None;
                        // Error recovery: synchronize to next statement
                        self.synchronize();
                    }
                }
                continue;
            }

            // async fn — consume 'async' and delegate to parse_function
            if self.pos + 1 < self.tokens.len() {
                if matches!(self.peek(), Token::Async(_))
                    && matches!(&self.tokens[self.pos + 1], Token::Fn(_))
                {
                    self.advance(); // consume 'async'
                    match self.parse_function(true) {
                        Ok((name, mut func)) => {
                            if let Some(ann) = pending_fn_annotations.take() {
                                func.annotations = Some(ann);
                            }
                            functions.insert(name, func);
                        }
                        Err(_) => {
                            pending_fn_annotations = None;
                            self.synchronize();
                        }
                    }
                    continue;
                }
            }

            // pub async fn — consume 'pub' 'async' and delegate to parse_function
            let is_pub_async_fn = if self.pos + 2 < self.tokens.len() {
                let ident_is_pub = match &self.tokens[self.pos] {
                    Token::Ident(s, _) => s == "pub",
                    _ => false,
                };
                let next_is_async = matches!(&self.tokens[self.pos + 1], Token::Async(_));
                let next_is_fn = matches!(&self.tokens[self.pos + 2], Token::Fn(_));
                ident_is_pub && next_is_async && next_is_fn
            } else {
                false
            };
            if is_pub_async_fn {
                self.advance(); // consume 'pub'
                self.advance(); // consume 'async'
                match self.parse_function(true) {
                    Ok((name, mut func)) => {
                        if let Some(ann) = pending_fn_annotations.take() {
                            func.annotations = Some(ann);
                        }
                        functions.insert(name, func);
                    }
                    Err(_) => {
                        pending_fn_annotations = None;
                        self.synchronize();
                    }
                }
                continue;
            }

            // pub fn — consume 'pub' and delegate to parse_function
            let is_pub_fn = if self.pos + 1 < self.tokens.len() {
                let ident_is_pub = match &self.tokens[self.pos] {
                    Token::Ident(s, _) => s == "pub",
                    _ => false,
                };
                let next_is_fn = match &self.tokens[self.pos + 1] {
                    Token::Fn(_) => true,
                    _ => false,
                };
                ident_is_pub && next_is_fn
            } else {
                false
            };
            if is_pub_fn {
                self.advance(); // consume 'pub'
                match self.parse_function(false) {
                    Ok((name, mut func)) => {
                        if let Some(ann) = pending_fn_annotations.take() {
                            func.annotations = Some(ann);
                        }
                        functions.insert(name, func);
                    }
                    Err(_) => {
                        pending_fn_annotations = None;
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
                ..Default::default()
            };
            functions.insert("main".to_string(), main_func);
        }

        // Build module manifest from accumulated @module / @invariant / @exhaustive-match-sites
        let manifest = if pending_manifest.is_some()
            || !pending_invariants.is_empty()
            || !pending_exhaustive_types.is_empty()
        {
            let mut m = pending_manifest.unwrap_or_default();
            m.invariants.extend(pending_invariants);
            m.exhaustive_types.extend(pending_exhaustive_types);
            Some(m)
        } else {
            None
        };

        Ok(Program {
            cast_version: "1.0.0".to_string(),
            entry: "main".to_string(),
            lang: Some("crush".to_string()),
            functions,
            ai_meta: None,
            manifest,
            wip: pending_wip,
            temporaries: pending_temporaries,
            decisions: pending_decisions,
            ..Default::default()
        })
    }

    /// Parse a function definition with error recovery
    fn parse_function(&mut self, is_async: bool) -> Result<(String, Function), ()> {
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
            is_async,
            ..Default::default()
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
                self.maybe_semicolon();
                Ok(Statement::Break {
                    meta: HashMap::new(),
                })
            }
            Token::Continue(_) => {
                self.advance();
                self.maybe_semicolon();
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
                if matches!(self.tokens.get(self.pos + 1), Some(Token::LangBody(_, _))) =>
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

        self.maybe_semicolon();

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

        self.maybe_semicolon();

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

        self.maybe_semicolon();

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

        self.maybe_semicolon();

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
                    Expression::GetField { target, field, .. } => match *target {
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
        self.skip_newlines();

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
        // Accept either `=>` (FatArrow) or `->` (Arrow) as the match arm separator.
        // FatArrow is preferred; Arrow is kept for backward compat with existing fixtures.
        match self.peek() {
            Token::FatArrow(_) | Token::Arrow(_) => { self.advance(); }
            _ => {
                let (line, col) = self.get_location(self.peek());
                self.errors.push(ParseError::Expected {
                    line, col,
                    expected: "=> or ->".to_string(),
                    found: format!("{:?}", self.peek()),
                });
                return Err(());
            }
        }

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
                        // name { field: pattern, ... } — named struct pattern
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
                    } else if matches!(self.peek(), Token::LParen(_)) {
                        // name(binding) — tuple-style variant pattern; treated as
                        // a Struct with the binding captured as a wildcard field.
                        // The binding variable name is stored in a synthetic "_" field
                        // so callers only see the variant name.
                        self.advance(); // consume (
                        // Skip the binding variable name (if any)
                        while !matches!(self.peek(), Token::RParen(_) | Token::EOF(_)) {
                            self.advance();
                        }
                        if matches!(self.peek(), Token::RParen(_)) {
                            self.advance(); // consume )
                        }
                        Ok(Pattern::Struct { name: n, fields: Vec::new() })
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
        if let Token::Ident(s, _) = self.peek()
            && s == "as"
        {
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
        if let Token::Ident(s, _) = self.peek()
            && s == "as"
        {
            self.advance();
            if let Token::Ident(a, _) = self.peek() {
                alias = Some(a.clone());
                self.advance();
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
        if let Token::Ident(s, _) = self.peek()
            && s == "as"
        {
            self.advance();
            if let Token::Ident(a, _) = self.peek() {
                alias = Some(a.clone());
                self.advance();
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
        if let Token::Ident(s, _) = self.peek()
            && s == "as"
        {
            self.advance();
            if let Token::Ident(a, _) = self.peek() {
                alias = Some(a.clone());
                self.advance();
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
        if let Token::Ident(s, _) = self.peek()
            && s == "as"
        {
            self.advance();
            if let Token::Ident(a, _) = self.peek() {
                alias = Some(a.clone());
                self.advance();
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

    // ── Annotation helpers ───────────────────────────────────────────────────

    /// Parse `@module { purpose: "...", exports: [...], ... }` → `ModuleManifest`.
    fn parse_module_block(&mut self) -> Result<ModuleManifest, ()> {
        self.skip_newlines();
        if !matches!(self.peek(), Token::LBrace(_)) {
            let (line, col) = self.get_location(self.peek());
            self.errors.push(ParseError::Expected {
                line,
                col,
                expected: "{ after @module".to_string(),
                found: format!("{:?}", self.peek()),
            });
            return Err(());
        }
        self.advance(); // consume {

        let mut manifest = ModuleManifest::default();
        self.skip_newlines();

        while !matches!(self.peek(), Token::RBrace(_)) && !matches!(self.peek(), Token::EOF(_)) {
            let key = match self.peek() {
                Token::Ident(k, _) => {
                    let k = k.clone();
                    self.advance();
                    k
                }
                _ => break,
            };

            // Optional colon
            if matches!(self.peek(), Token::Colon(_)) {
                self.advance();
            }

            match key.as_str() {
                "purpose" => {
                    manifest.purpose = self.parse_at_string_value();
                }
                "exports" | "related" | "invariants" | "exhaustive_types" | "changelog" => {
                    let items = self.parse_at_list();
                    match key.as_str() {
                        "exports" => manifest.exports = items,
                        "related" => manifest.related = items,
                        "invariants" => manifest.invariants.extend(items.into_iter().map(|n| {
                            Invariant {
                                name: n,
                                description: String::new(),
                                applies_to: Vec::new(),
                                consequence: None,
                                check_source: None,
                            }
                        })),
                        "exhaustive_types" => manifest.exhaustive_types = items,
                        _ => {} // changelog as string list is a no-op (needs richer type)
                    }
                }
                _ => {
                    // Unknown key — skip value token(s)
                    self.skip_at_value();
                }
            }
            self.skip_newlines();
        }

        if matches!(self.peek(), Token::RBrace(_)) {
            self.advance(); // consume }
        }
        Ok(manifest)
    }

    /// Parse `@invariant "name" { description: "...", applies_to: [...], consequence: "..." }`.
    fn parse_invariant_block(&mut self) -> Result<Invariant, ()> {
        self.skip_newlines();

        let name = self.parse_at_string_value();
        self.skip_newlines();

        if !matches!(self.peek(), Token::LBrace(_)) {
            // Bare `@invariant "name"` with no block is valid — treat as name-only
            return Ok(Invariant {
                name,
                description: String::new(),
                applies_to: Vec::new(),
                consequence: None,
                check_source: None,
            });
        }
        self.advance(); // consume {

        let mut inv = Invariant {
            name,
            description: String::new(),
            applies_to: Vec::new(),
            consequence: None,
            check_source: None,
        };
        self.skip_newlines();

        while !matches!(self.peek(), Token::RBrace(_)) && !matches!(self.peek(), Token::EOF(_)) {
            let key = match self.peek() {
                Token::Ident(k, _) => {
                    let k = k.clone();
                    self.advance();
                    k
                }
                _ => break,
            };

            if matches!(self.peek(), Token::Colon(_)) {
                self.advance();
            }

            match key.as_str() {
                "description" | "reason" => {
                    inv.description = self.parse_at_string_value();
                }
                "applies_to" | "applies-to" => {
                    inv.applies_to = self.parse_at_list();
                }
                "consequence" => {
                    let s = self.parse_at_string_value();
                    inv.consequence = if s.is_empty() { None } else { Some(s) };
                }
                "check" => {
                    let s = self.parse_at_string_value();
                    inv.check_source = if s.is_empty() { None } else { Some(s) };
                }
                _ => {
                    self.skip_at_value();
                }
            }
            self.skip_newlines();
        }

        if matches!(self.peek(), Token::RBrace(_)) {
            self.advance();
        }
        Ok(inv)
    }

    /// Parse the body of a function-level annotation after the `@name` has been consumed.
    /// Writes into `ann`.
    fn parse_fn_annotation_body(&mut self, name: &str, ann: &mut FunctionAnnotations) {
        match name {
            "errors" => {
                self.skip_newlines();
                if matches!(self.peek(), Token::LBrace(_)) {
                    ann.errors_weighted.extend(self.parse_weighted_errors());
                } else {
                    ann.errors.extend(self.parse_at_items());
                }
            }
            "reads" => ann.reads.extend(self.parse_at_items()),
            "writes" => ann.writes.extend(self.parse_at_items()),
            "does-not-write" => ann.does_not_write.extend(self.parse_at_items()),
            "covers" => ann.covers.extend(self.parse_at_items()),
            "relies-on" => ann.relies_on.extend(self.parse_at_items()),
            "complexity" => {
                if let Token::Int(n, _) = self.peek() {
                    let v = (*n).clamp(0, 100) as u8;
                    self.advance();
                    ann.complexity = Some(v);
                }
            }
            "invalidates" => ann.invalidates.extend(self.parse_at_items()),
            "must-call-before" => ann.must_call_before.extend(self.parse_at_items()),
            "must-call-after" => ann.must_call_after.extend(self.parse_at_items()),
            _ => {}
        }
    }

    /// Parse a `@decision "name" { chose: ..., over: [...], because: ..., revisit_if: [...] }` block.
    fn parse_decision_block(&mut self) -> DecisionNode {
        self.skip_newlines();
        let name = self.parse_at_string_value();
        self.skip_newlines();
        if !matches!(self.peek(), Token::LBrace(_)) {
            return DecisionNode {
                name,
                chose: String::new(),
                over: Vec::new(),
                because: String::new(),
                revisit_if: Vec::new(),
            };
        }
        self.advance(); // consume {
        let mut node = DecisionNode {
            name,
            chose: String::new(),
            over: Vec::new(),
            because: String::new(),
            revisit_if: Vec::new(),
        };
        self.skip_newlines();
        while !matches!(self.peek(), Token::RBrace(_)) && !matches!(self.peek(), Token::EOF(_)) {
            let key = self.read_annotation_key();
            if matches!(self.peek(), Token::Colon(_)) {
                self.advance();
            }
            match key.as_str() {
                "chose" => node.chose = self.parse_at_string_value(),
                "over" => node.over = self.parse_at_list(),
                "because" | "reason" => node.because = self.parse_at_string_value(),
                "revisit_if" | "revisit-if" => node.revisit_if = self.parse_at_list(),
                _ => {
                    self.skip_at_value();
                }
            }
            self.skip_newlines();
        }
        if matches!(self.peek(), Token::RBrace(_)) {
            self.advance();
        }
        node
    }

    /// Parse an annotation item list: either `[a, b, c]` or a single bare item.
    /// Returns the items as strings. Items may be qualified (`thread.ip`, `Foo::Bar`).
    fn parse_at_items(&mut self) -> Vec<String> {
        self.skip_newlines();
        if matches!(self.peek(), Token::LBracket(_)) {
            self.parse_at_list()
        } else {
            // Single bare item
            let s = self.parse_at_qualified_ident();
            if s.is_empty() {
                Vec::new()
            } else {
                vec![s]
            }
        }
    }

    /// Parse `[ item, item, ... ]` — items are strings or qualified identifiers.
    fn parse_at_list(&mut self) -> Vec<String> {
        self.skip_newlines();
        if !matches!(self.peek(), Token::LBracket(_)) {
            return Vec::new();
        }
        self.advance(); // consume [

        let mut items = Vec::new();
        self.skip_newlines();

        while !matches!(self.peek(), Token::RBracket(_)) && !matches!(self.peek(), Token::EOF(_)) {
            let item = match self.peek() {
                Token::String(s, _) => {
                    let s = s.clone();
                    self.advance();
                    s
                }
                Token::Ident(_, _) => self.parse_at_qualified_ident(),
                _ => {
                    self.advance(); // skip unexpected token
                    continue;
                }
            };
            if !item.is_empty() {
                items.push(item);
            }
            self.skip_newlines();
            if matches!(self.peek(), Token::Comma(_)) {
                self.advance();
                self.skip_newlines();
            }
        }

        if matches!(self.peek(), Token::RBracket(_)) {
            self.advance(); // consume ]
        }
        items
    }

    /// Read a single string value: either a `"string"` token or a qualified identifier.
    fn parse_at_string_value(&mut self) -> String {
        self.skip_newlines();
        match self.peek() {
            Token::String(s, _) => {
                let s = s.clone();
                self.advance();
                s
            }
            Token::Ident(_, _) => self.parse_at_qualified_ident(),
            _ => String::new(),
        }
    }

    /// Read a qualified identifier for annotation values.
    ///
    /// Handles:
    /// - `thread.ip`, `vm.types` — dot-qualified
    /// - `VmError::StackUnderflow` — double-colon qualified
    /// - `rc-refcell-not-send` — kebab-case (hyphen chains, annotation names only)
    ///
    /// The hyphen handling is safe here because this method is called exclusively
    /// from annotation parsing helpers where `-` as subtraction cannot appear.
    fn parse_at_qualified_ident(&mut self) -> String {
        let base = match self.peek() {
            Token::Ident(s, _) => {
                let s = s.clone();
                self.advance();
                s
            }
            _ => return String::new(),
        };

        let mut result = base;
        loop {
            match self.peek() {
                Token::Dot(_) => {
                    self.advance();
                    if let Token::Ident(part, _) = self.peek() {
                        let part = part.clone();
                        self.advance();
                        result.push('.');
                        result.push_str(&part);
                    } else {
                        break;
                    }
                }
                Token::DoubleColon(_) => {
                    self.advance();
                    if let Token::Ident(part, _) = self.peek() {
                        let part = part.clone();
                        self.advance();
                        result.push_str("::");
                        result.push_str(&part);
                    } else {
                        break;
                    }
                }
                Token::Minus(_) => {
                    // Peek two ahead: if next is Ident, treat as kebab-case hyphen
                    if matches!(self.tokens.get(self.pos + 1), Some(Token::Ident(_, _))) {
                        self.advance(); // consume -
                        if let Token::Ident(part, _) = self.peek() {
                            let part = part.clone();
                            self.advance();
                            result.push('-');
                            result.push_str(&part);
                        }
                    } else {
                        break;
                    }
                }
                _ => break,
            }
        }
        result
    }

    /// Parse `@wip { intent: "...", todo: [...], ... }` → `WipNode`.
    fn parse_wip_block(&mut self) -> WipNode {
        self.skip_newlines();
        if !matches!(self.peek(), Token::LBrace(_)) {
            return WipNode::default();
        }
        self.advance(); // consume {

        let mut wip = WipNode::default();
        self.skip_newlines();

        while !matches!(self.peek(), Token::RBrace(_)) && !matches!(self.peek(), Token::EOF(_)) {
            let key = self.read_annotation_key();
            if key.is_empty() {
                break;
            }
            if matches!(self.peek(), Token::Colon(_)) {
                self.advance();
            }
            match key.as_str() {
                "intent" => {
                    wip.intent = self.parse_at_string_value();
                }
                "started_by" | "started-by" => {
                    let s = self.parse_at_string_value();
                    wip.started_by = if s.is_empty() { None } else { Some(s) };
                }
                "done" => {
                    wip.done = self.parse_at_list();
                }
                "todo" => {
                    wip.todo = self.parse_at_list();
                }
                "unresolved" => {
                    wip.unresolved = self.parse_at_list();
                }
                _ => {
                    self.skip_at_value();
                }
            }
            self.skip_newlines();
        }

        if matches!(self.peek(), Token::RBrace(_)) {
            self.advance();
        }
        wip
    }

    /// Parse `@temporary { reason: "...", expires_when: "...", ... }` → `TemporaryNode`.
    fn parse_temporary_block(&mut self) -> TemporaryNode {
        self.skip_newlines();
        if !matches!(self.peek(), Token::LBrace(_)) {
            return TemporaryNode::default();
        }
        self.advance(); // consume {

        let mut tmp = TemporaryNode::default();
        self.skip_newlines();

        while !matches!(self.peek(), Token::RBrace(_)) && !matches!(self.peek(), Token::EOF(_)) {
            let key = self.read_annotation_key();
            if key.is_empty() {
                break;
            }
            if matches!(self.peek(), Token::Colon(_)) {
                self.advance();
            }
            match key.as_str() {
                "reason" => {
                    tmp.reason = self.parse_at_string_value();
                }
                "expires_when" | "expires-when" => {
                    let s = self.parse_at_string_value();
                    tmp.expires_when = if s.is_empty() { None } else { Some(s) };
                }
                "owner" => {
                    let s = self.parse_at_string_value();
                    tmp.owner = if s.is_empty() { None } else { Some(s) };
                }
                "added" => {
                    let s = self.parse_at_string_value();
                    tmp.added = if s.is_empty() { None } else { Some(s) };
                }
                _ => {
                    self.skip_at_value();
                }
            }
            self.skip_newlines();
        }

        if matches!(self.peek(), Token::RBrace(_)) {
            self.advance();
        }
        tmp
    }

    /// Parse `{ Variant: likely, ... }` — a weighted-errors block.
    fn parse_weighted_errors(&mut self) -> Vec<WeightedError> {
        if !matches!(self.peek(), Token::LBrace(_)) {
            return Vec::new();
        }
        self.advance(); // consume {

        let mut items = Vec::new();
        self.skip_newlines();

        while !matches!(self.peek(), Token::RBrace(_)) && !matches!(self.peek(), Token::EOF(_)) {
            let variant = self.read_annotation_key();
            if variant.is_empty() {
                break;
            }
            if matches!(self.peek(), Token::Colon(_)) {
                self.advance();
            }
            self.skip_newlines();
            let likelihood_str = self.read_annotation_key();
            let likelihood = match likelihood_str.as_str() {
                "likely" => ErrorLikelihood::Likely,
                "possible" => ErrorLikelihood::Possible,
                "rare" => ErrorLikelihood::Rare,
                _ => ErrorLikelihood::Possible, // default
            };
            items.push(WeightedError { variant, likelihood });
            self.skip_newlines();
            if matches!(self.peek(), Token::Comma(_)) {
                self.advance();
                self.skip_newlines();
            }
        }

        if matches!(self.peek(), Token::RBrace(_)) {
            self.advance();
        }
        items
    }

    /// Read a single identifier key from an annotation block (bare ident, no qualifiers).
    fn read_annotation_key(&mut self) -> String {
        self.skip_newlines();
        match self.peek() {
            Token::Ident(k, _) => {
                let k = k.clone();
                self.advance();
                k
            }
            _ => String::new(),
        }
    }

    /// Skip over an unknown annotation value (string, list, or single token).
    fn skip_at_value(&mut self) {
        self.skip_newlines();
        match self.peek() {
            Token::String(_, _) | Token::Int(_, _) | Token::Float(_, _) => {
                self.advance();
            }
            Token::LBracket(_) => {
                self.advance();
                let mut depth = 1;
                while depth > 0 && !matches!(self.peek(), Token::EOF(_)) {
                    match self.peek() {
                        Token::LBracket(_) => {
                            depth += 1;
                            self.advance();
                        }
                        Token::RBracket(_) => {
                            depth -= 1;
                            self.advance();
                        }
                        _ => {
                            self.advance();
                        }
                    }
                }
            }
            Token::LBrace(_) => {
                self.advance();
                let mut depth = 1;
                while depth > 0 && !matches!(self.peek(), Token::EOF(_)) {
                    match self.peek() {
                        Token::LBrace(_) => {
                            depth += 1;
                            self.advance();
                        }
                        Token::RBrace(_) => {
                            depth -= 1;
                            self.advance();
                        }
                        _ => {
                            self.advance();
                        }
                    }
                }
            }
            Token::Ident(_, _) => {
                self.advance();
            }
            _ => {}
        }
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
            if let ParseError::Expected { line, col, .. } = error {
                assert!(*line > 0);
                assert!(*col > 0);
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

    #[test]
    fn test_module_annotation() {
        let source = r#"
@module {
    purpose: "unit test module"
    exports: [run, stop]
    related: [vm.types]
}

fn run() {
    return 0
}
"#;
        let program = Parser::parse(source).unwrap();
        let manifest = program.manifest.expect("manifest should be set");
        assert_eq!(manifest.purpose, "unit test module");
        assert_eq!(manifest.exports, vec!["run", "stop"]);
        assert_eq!(manifest.related, vec!["vm.types"]);
        assert!(program.functions.contains_key("run"));
    }

    #[test]
    fn test_invariant_annotation() {
        let source = r#"
@invariant "no-reenter" {
    description: "no re-entrancy allowed"
    applies_to: [dispatch, execute_one]
    consequence: "deadlock"
}

fn dispatch() {
    return 0
}
"#;
        let program = Parser::parse(source).unwrap();
        let manifest = program.manifest.expect("manifest should be set");
        assert_eq!(manifest.invariants.len(), 1);
        let inv = &manifest.invariants[0];
        assert_eq!(inv.name, "no-reenter");
        assert_eq!(inv.description, "no re-entrancy allowed");
        assert_eq!(inv.applies_to, vec!["dispatch", "execute_one"]);
        assert_eq!(inv.consequence.as_deref(), Some("deadlock"));
    }

    #[test]
    fn test_exhaustive_match_sites_annotation() {
        let source = r#"
@exhaustive-match-sites [Value, StepAction]

fn main() {
    return 0
}
"#;
        let program = Parser::parse(source).unwrap();
        let manifest = program.manifest.expect("manifest should be set");
        assert!(manifest.exhaustive_types.contains(&"Value".to_string()));
        assert!(manifest.exhaustive_types.contains(&"StepAction".to_string()));
    }

    #[test]
    fn test_function_annotations() {
        let source = r#"
@errors [VmError::StackUnderflow, VmError::StepQuota]
@reads [thread.ip, thread.stack]
@writes [thread.ip, thread.out_parts]
@does-not-write [program]
@covers VmError::StackUnderflow
@relies-on rc-refcell-not-send
fn execute_one() {
    return 0
}
"#;
        let program = Parser::parse(source).unwrap();
        let func = program.functions.get("execute_one").expect("function should exist");
        let ann = func.annotations.as_ref().expect("annotations should be set");
        assert_eq!(ann.errors, vec!["VmError::StackUnderflow", "VmError::StepQuota"]);
        assert_eq!(ann.reads, vec!["thread.ip", "thread.stack"]);
        assert_eq!(ann.writes, vec!["thread.ip", "thread.out_parts"]);
        assert_eq!(ann.does_not_write, vec!["program"]);
        assert_eq!(ann.covers, vec!["VmError::StackUnderflow"]);
        assert_eq!(ann.relies_on, vec!["rc-refcell-not-send"]);
    }

    #[test]
    fn test_annotations_do_not_apply_to_next_function() {
        let source = r#"
@errors [VmError::Foo]
fn first() {
    return 0
}
fn second() {
    return 0
}
"#;
        let program = Parser::parse(source).unwrap();
        let first = program.functions.get("first").expect("first should exist");
        let second = program.functions.get("second").expect("second should exist");
        assert!(first.annotations.is_some());
        assert!(second.annotations.is_none());
    }

    #[test]
    fn test_complexity_annotation() {
        let source = r#"
@complexity 75
fn heavy() {
    return 0
}
"#;
        let program = Parser::parse(source).unwrap();
        let func = program.functions.get("heavy").expect("heavy should exist");
        let ann = func.annotations.as_ref().expect("annotations should be set");
        assert_eq!(ann.complexity, Some(75));
    }
}
