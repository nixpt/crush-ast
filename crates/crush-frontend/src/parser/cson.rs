use crush_cast::cson::{CsonDocument, CsonKey, CsonNode, CsonValue};
// use crush_cast::manifest::{Invariant, TemporaryNode, WipNode, DecisionNode};
use indexmap::IndexMap;

// Basic CSON Parser Scaffold (Phase 2b Proof of Concept)
// Supports key-value pairs, string values, nested objects, probabilities, and semantic keys.

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
                // Skip comment
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
            } else if !in_quotes && (c.is_whitespace() || c == ':' || c == '~') {
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
                if c.is_digit(10) || c == '.' {
                    num_str.push(c);
                    self.consume();
                } else {
                    break;
                }
            }
            if let Ok(w) = num_str.parse::<f64>() {
                return Ok(Some(w));
            } else {
                return Err(CsonParseError::Unexpected(format!("Invalid weight: ~{}", num_str)));
            }
        }
        Ok(None)
    }

    pub fn parse_value(&mut self) -> Result<CsonNode, CsonParseError> {
        self.skip_whitespace();
        if self.match_char('{') {
            let mut properties = IndexMap::new();
            let mut semantic_properties = Vec::new();
            let mut has_semantic = false;

            loop {
                self.skip_whitespace();
                if self.match_char('}') {
                    break;
                }

                // Parse key
                let mut is_semantic = false;
                if self.match_char('~') {
                    is_semantic = true;
                }
                let key_str = self.parse_string()?;
                self.skip_whitespace();
                if !self.match_char(':') {
                    return Err(CsonParseError::Unexpected("Expected ':' after key".to_string()));
                }

                // Parse value
                let val = self.parse_value()?;

                if is_semantic {
                    has_semantic = true;
                    semantic_properties.push((CsonKey::Semantic { value: key_str }, val));
                } else {
                    properties.insert(key_str.clone(), val.clone());
                    semantic_properties.push((CsonKey::Exact { value: key_str }, val));
                }

                self.skip_whitespace();
                self.match_char(','); // optional comma
            }
            
            // For now, no weight on object blocks
            if has_semantic {
                Ok(CsonNode {
                    value: CsonValue::SemanticObject { properties: semantic_properties },
                    weight: None,
                    invariants: vec![],
                })
            } else {
                Ok(CsonNode {
                    value: CsonValue::Object { properties },
                    weight: None,
                    invariants: vec![],
                })
            }
        } else {
            // String or number value
            let val_str = self.parse_string()?;
            let weight = self.parse_weight()?;
            
            // Try parse int/float
            if let Ok(i) = val_str.parse::<i64>() {
                Ok(CsonNode {
                    value: CsonValue::Int { value: i },
                    weight,
                    invariants: vec![],
                })
            } else if let Ok(f) = val_str.parse::<f64>() {
                Ok(CsonNode {
                    value: CsonValue::Float { value: f },
                    weight,
                    invariants: vec![],
                })
            } else if val_str == "null" {
                Ok(CsonNode {
                    value: CsonValue::Null,
                    weight,
                    invariants: vec![],
                })
            } else if val_str == "true" {
                Ok(CsonNode {
                    value: CsonValue::Bool { value: true },
                    weight,
                    invariants: vec![],
                })
            } else if val_str == "false" {
                Ok(CsonNode {
                    value: CsonValue::Bool { value: false },
                    weight,
                    invariants: vec![],
                })
            } else {
                Ok(CsonNode {
                    value: CsonValue::String { value: val_str },
                    weight,
                    invariants: vec![],
                })
            }
        }
    }

    pub fn parse_document(&mut self) -> Result<CsonDocument, CsonParseError> {
        let root_node = self.parse_value()?;
        Ok(CsonDocument {
            root: root_node,
            wip: None,
            temporaries: vec![],
            decisions: vec![],
        })
    }
}

pub fn parse_cson(input: &str) -> Result<CsonDocument, CsonParseError> {
    let mut parser = CsonParser::new(input);
    parser.parse_document()
}
