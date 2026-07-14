//! CSON parser for the crush-frontend crate.
//!
//! Parses CSON text into the canonical `crush_cson` types.
//! Uses the unified type definitions from `crush-cson`.

use crush_cast::cson::{CsonDocument, CsonKey, CsonNode, CsonValue};
use std::collections::HashMap;

#[derive(Debug)]
pub enum CsonParseError {
    Unexpected(String),
}

pub struct CsonParser<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> CsonParser<'a> {
    pub fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.input.len() {
            let c = self.input[self.pos..].chars().next().unwrap();
            if c.is_whitespace() {
                self.pos += c.len_utf8();
            } else if self.input[self.pos..].starts_with('#') {
                while self.pos < self.input.len() && !self.input[self.pos..].starts_with('\n') {
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
    }

    fn peek(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }

    fn consume(&mut self) -> Option<char> {
        if let Some(c) = self.peek() {
            self.pos += c.len_utf8();
            Some(c)
        } else {
            None
        }
    }

    fn match_char(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.consume();
            true
        } else {
            false
        }
    }

    fn parse_string(&mut self) -> Result<String, CsonParseError> {
        self.skip_whitespace();
        let mut s = String::new();
        let mut in_quotes = false;
        if self.match_char('"') {
            in_quotes = true;
        }

        while let Some(c) = self.peek() {
            if in_quotes && c == '"' {
                self.consume();
                break;
            } else if !in_quotes && (c.is_whitespace() || c == ':' || c == '~' || c == ',' || c == '}') {
                break;
            } else {
                s.push(c);
                self.consume();
            }
        }
        Ok(s)
    }

    fn parse_weight(&mut self) -> Result<Option<f64>, CsonParseError> {
        self.skip_whitespace();
        if self.match_char('~') {
            let mut num_str = String::new();
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() || c == '.' {
                    num_str.push(c);
                    self.consume();
                } else {
                    break;
                }
            }
            if let Ok(w) = num_str.parse::<f64>() {
                return Ok(Some(w));
            } else {
                return Err(CsonParseError::Unexpected(format!("Invalid weight: ~{num_str}")));
            }
        }
        Ok(None)
    }

    pub fn parse_value(&mut self) -> Result<CsonNode, CsonParseError> {
        self.skip_whitespace();
        if self.match_char('{') {
            let mut properties = HashMap::new();
            let mut annotations = Vec::new();

            loop {
                self.skip_whitespace();
                if self.match_char('}') {
                    break;
                }

                // Parse key — semantic keys start with `~`
                let mut is_semantic = false;
                if self.match_char('~') {
                    is_semantic = true;
                }
                let raw_key = self.parse_string()?;
                let full_key = if is_semantic {
                    format!("~{raw_key}")
                } else {
                    raw_key
                };

                self.skip_whitespace();
                if !self.match_char(':') {
                    return Err(CsonParseError::Unexpected("Expected ':' after key".to_string()));
                }

                let val = self.parse_value()?;
                properties.insert(full_key, val);

                self.skip_whitespace();
                self.match_char(',');
            }

            Ok(CsonNode {
                value: CsonValue::Object(properties),
                confidence: None,
                annotations,
            })
        } else if self.match_char('[') {
            let mut arr = Vec::new();
            loop {
                self.skip_whitespace();
                if self.match_char(']') { break; }
                arr.push(self.parse_value()?);
                self.skip_whitespace();
                self.match_char(',');
            }
            Ok(CsonNode {
                value: CsonValue::Array(arr),
                confidence: None,
                annotations: vec![],
            })
        } else {
            let val_str = self.parse_string()?;
            let weight = self.parse_weight()?;

            let value = if val_str == "null" {
                CsonValue::Null
            } else if val_str == "true" {
                CsonValue::Boolean(true)
            } else if val_str == "false" {
                CsonValue::Boolean(false)
            } else if let Ok(i) = val_str.parse::<i64>() {
                CsonValue::Number(i as f64)
            } else if let Ok(f) = val_str.parse::<f64>() {
                CsonValue::Number(f)
            } else {
                CsonValue::String(val_str)
            };

            Ok(CsonNode {
                value,
                confidence: weight,
                annotations: vec![],
            })
        }
    }

    pub fn parse_document(&mut self) -> Result<CsonDocument, CsonParseError> {
        let root_node = self.parse_value()?;
        Ok(CsonDocument {
            version: "1.0".into(),
            root: root_node,
        })
    }
}

pub fn parse_cson(input: &str) -> Result<CsonDocument, CsonParseError> {
    let mut parser = CsonParser::new(input);
    parser.parse_document()
}
