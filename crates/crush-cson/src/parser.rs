//! CSON text parser — hand-written recursive descent.
//!
//! Parses the Crush Semantic Object Notation text format into the
//! canonical `CsonDocument` type from the parent module.

use super::{CsonAnnotation, CsonDocument, CsonKey, CsonNode, CsonValue};
use std::collections::HashMap;

/// A basic MVP parser for CSON text format.
pub struct CsonParser<'a> {
    pub input: &'a str,
    pub pos: usize,
}

impl<'a> CsonParser<'a> {
    pub fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }

    pub fn error(&self, msg: &str) -> String {
        let mut line = 1;
        let mut col = 1;
        for c in self.input[..self.pos].chars() {
            if c == '\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }
        format!("{} at line {}, col {}", msg, line, col)
    }

    pub fn skip_whitespace_and_comments(&mut self) {
        while self.pos < self.input.len() {
            let rest = &self.input[self.pos..];
            if rest.starts_with("//") || rest.starts_with("#") {
                let end = rest.find('\n').unwrap_or(rest.len());
                self.pos += end;
            } else if rest.starts_with(char::is_whitespace) {
                self.pos += rest.chars().next().unwrap().len_utf8();
            } else {
                break;
            }
        }
    }

    fn parse_quoted_string(&mut self) -> Result<String, String> {
        self.pos += 1; // skip '"'
        let mut s = String::new();
        let mut escaped = false;
        while self.pos < self.input.len() {
            let c = self.input[self.pos..].chars().next().unwrap();
            self.pos += c.len_utf8();
            if escaped {
                s.push(match c {
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    '"' => '"',
                    '\\' => '\\',
                    _ => c,
                });
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                return Ok(s);
            } else {
                s.push(c);
            }
        }
        Err(self.error("Unclosed string"))
    }

    pub fn parse(&mut self) -> Result<CsonDocument, String> {
        let mut root_obj = HashMap::new();
        let mut current_section = None;
        let mut pending_annotations = Vec::new();
        let mut document_version = "1.0".to_string();

        while self.pos < self.input.len() {
            self.skip_whitespace_and_comments();
            if self.pos >= self.input.len() { break; }

            let rest = &self.input[self.pos..];

            // 1. Annotations
            if rest.starts_with('@') {
                let ann = self.parse_annotation()?;
                if ann.name == "cson" {
                    if let Some(v) = ann.properties.get("version") {
                        document_version = v.clone();
                    } else if let Some(ref arg) = ann.args {
                        document_version = arg.trim_matches('"').to_string();
                    }
                }
                pending_annotations.push(ann);
                continue;
            }

            // 2. Sections
            if rest.starts_with('[') {
                self.pos += 1;
                let end = self.input[self.pos..].find(']').ok_or_else(|| self.error("Unclosed section"))?;
                let section_name = self.input[self.pos..self.pos+end].trim().to_string();
                self.pos += end + 1;
                current_section = Some(section_name);
                continue;
            }

            // 3. Key-Value pairs
            if let Some((key, mut node)) = self.parse_kv_pair()? {
                node.annotations.extend(pending_annotations.drain(..));

                if let Some(ref sec) = current_section {
                    let sec_key = sec.clone();
                    let section_node = root_obj.entry(sec_key).or_insert_with(|| CsonNode {
                        value: CsonValue::Object(HashMap::new()),
                        confidence: None,
                        annotations: vec![],
                    });

                    if let CsonValue::Object(ref mut map) = section_node.value {
                        let k_str = key.to_string();
                        if map.contains_key(&k_str) {
                            return Err(self.error(&format!("Duplicate key: {}", k_str)));
                        }
                        map.insert(k_str, node);
                    }
                } else {
                    let k_str = key.to_string();
                    if root_obj.contains_key(&k_str) {
                        return Err(self.error(&format!("Duplicate key: {}", k_str)));
                    }
                    root_obj.insert(k_str, node);
                }
            } else {
                break;
            }
        }

        Ok(CsonDocument {
            version: document_version,
            root: CsonNode {
                value: CsonValue::Object(root_obj),
                confidence: None,
                annotations: pending_annotations,
            }
        })
    }

    fn parse_annotation(&mut self) -> Result<CsonAnnotation, String> {
        self.pos += 1; // skip '@'
        let name_end = self.input[self.pos..].find(|c: char| !c.is_alphanumeric() && c != '_').unwrap_or(self.input.len() - self.pos);
        let name = self.input[self.pos..self.pos+name_end].to_string();
        self.pos += name_end;

        self.skip_whitespace_and_comments();

        let mut args = None;
        if self.pos < self.input.len() && self.input[self.pos..].starts_with('(') {
            self.pos += 1;
            let end = self.input[self.pos..].find(')').ok_or_else(|| self.error("Unclosed annotation args"))?;
            args = Some(self.input[self.pos..self.pos+end].to_string());
            self.pos += end + 1;
        }

        self.skip_whitespace_and_comments();

        let mut properties = HashMap::new();
        if self.pos < self.input.len() && self.input[self.pos..].starts_with('{') {
            self.pos += 1;
            loop {
                self.skip_whitespace_and_comments();
                if self.pos >= self.input.len() {
                    return Err(self.error("Unclosed annotation properties"));
                }
                if self.input[self.pos..].starts_with('}') {
                    self.pos += 1;
                    break;
                }
                
                let key_end = self.input[self.pos..].find(':').ok_or_else(|| self.error("Missing colon in annotation property"))?;
                let k = self.input[self.pos..self.pos+key_end].trim().to_string();
                self.pos += key_end + 1;
                
                self.skip_whitespace_and_comments();
                let v = if self.input[self.pos..].starts_with('"') {
                    self.parse_quoted_string()?
                } else {
                    let val_end = self.input[self.pos..].find(|c: char| c == ',' || c == '}').unwrap_or(self.input.len() - self.pos);
                    let v_str = self.input[self.pos..self.pos+val_end].trim().to_string();
                    self.pos += val_end;
                    v_str
                };
                properties.insert(k, v);
                
                self.skip_whitespace_and_comments();
                if self.pos < self.input.len() && self.input[self.pos..].starts_with(',') {
                    self.pos += 1;
                }
            }
        }

        Ok(CsonAnnotation { name, args, properties })
    }

    fn parse_kv_pair(&mut self) -> Result<Option<(CsonKey, CsonNode)>, String> {
        self.skip_whitespace_and_comments();
        if self.pos >= self.input.len() { return Ok(None); }
        if self.input[self.pos..].starts_with('[') || self.input[self.pos..].starts_with('@') || self.input[self.pos..].starts_with('}') || self.input[self.pos..].starts_with(']') {
            return Ok(None);
        }

        let mut is_semantic = false;
        if self.input[self.pos..].starts_with('~') {
            is_semantic = true;
            self.pos += 1;
        }

        let key_str;
        if self.input[self.pos..].starts_with('"') {
            key_str = self.parse_quoted_string()?;
        } else {
            let end = self.input[self.pos..].find(':').ok_or_else(|| self.error("Missing colon in kv pair"))?;
            key_str = self.input[self.pos..self.pos+end].trim().to_string();
            self.pos += end;
        }

        let key = if is_semantic { CsonKey::Semantic(key_str) } else { CsonKey::Exact(key_str) };

        self.skip_whitespace_and_comments();
        if !self.input[self.pos..].starts_with(':') {
            return Err(self.error("Expected ':' after key"));
        }
        self.pos += 1;

        self.skip_whitespace_and_comments();

        let (value, confidence) = self.parse_value()?;

        Ok(Some((key, CsonNode {
            value,
            confidence,
            annotations: vec![],
        })))
    }

    fn parse_value(&mut self) -> Result<(CsonValue, Option<f64>), String> {
        self.skip_whitespace_and_comments();
        if self.pos >= self.input.len() {
            return Err(self.error("Unexpected end of input when parsing value"));
        }

        let value;
        if self.input[self.pos..].starts_with('@') {
            let ann = self.parse_annotation()?;
            if ann.name == "synthesize" {
                value = CsonValue::Synthesize(ann.args.unwrap_or_default().trim_matches('"').to_string());
            } else {
                return Err(self.error("Only @synthesize is supported as a value annotation"));
            }
        } else if self.input[self.pos..].starts_with('"') {
            value = CsonValue::String(self.parse_quoted_string()?);
        } else if self.input[self.pos..].starts_with("true") && (self.pos + 4 == self.input.len() || !self.input[self.pos+4..self.pos+5].chars().next().unwrap().is_alphanumeric()) {
            value = CsonValue::Boolean(true);
            self.pos += 4;
        } else if self.input[self.pos..].starts_with("false") && (self.pos + 5 == self.input.len() || !self.input[self.pos+5..self.pos+6].chars().next().unwrap().is_alphanumeric()) {
            value = CsonValue::Boolean(false);
            self.pos += 5;
        } else if self.input[self.pos..].starts_with("null") && (self.pos + 4 == self.input.len() || !self.input[self.pos+4..self.pos+5].chars().next().unwrap().is_alphanumeric()) {
            value = CsonValue::Null;
            self.pos += 4;
        } else if self.input[self.pos..].starts_with('{') {
            self.pos += 1;
            let mut map = HashMap::new();
            loop {
                self.skip_whitespace_and_comments();
                if self.pos >= self.input.len() { return Err(self.error("Unclosed object")); }
                if self.input[self.pos..].starts_with('}') { self.pos += 1; break; }
                if let Some((k, v)) = self.parse_kv_pair()? {
                    let k_str = k.to_string();
                    if map.contains_key(&k_str) {
                        return Err(self.error(&format!("Duplicate key: {}", k_str)));
                    }
                    map.insert(k_str, v);
                }
                self.skip_whitespace_and_comments();
                if self.pos < self.input.len() && self.input[self.pos..].starts_with(',') { self.pos += 1; }
            }
            value = CsonValue::Object(map);
        } else if self.input[self.pos..].starts_with('[') {
            self.pos += 1;
            let mut arr = Vec::new();
            loop {
                self.skip_whitespace_and_comments();
                if self.pos >= self.input.len() { return Err(self.error("Unclosed array")); }
                if self.input[self.pos..].starts_with(']') { self.pos += 1; break; }
                let (v, c) = self.parse_value()?;
                arr.push(CsonNode { value: v, confidence: c, annotations: vec![] });
                self.skip_whitespace_and_comments();
                if self.pos < self.input.len() && self.input[self.pos..].starts_with(',') { self.pos += 1; }
            }
            value = CsonValue::Array(arr);
        } else {
            let mut end = 0;
            let rest = &self.input[self.pos..];
            for (i, c) in rest.char_indices() {
                if c == '\n' || c == ',' || c == '}' || c == ']' || c == '~' {
                    break;
                }
                if rest[i..].starts_with('#') || rest[i..].starts_with("//") {
                    break;
                }
                end += c.len_utf8();
            }
            let raw_str = self.input[self.pos..self.pos+end].trim();
            value = if let Ok(n) = raw_str.parse::<f64>() {
                CsonValue::Number(n)
            } else if raw_str == "true" {
                CsonValue::Boolean(true)
            } else if raw_str == "false" {
                CsonValue::Boolean(false)
            } else if raw_str == "null" {
                CsonValue::Null
            } else {
                CsonValue::String(raw_str.to_string())
            };
            self.pos += end;
        }

        self.skip_whitespace_and_comments();
        let mut confidence = None;
        if self.pos < self.input.len() && self.input[self.pos..].starts_with('~') {
            // Check if it's a semantic key start, not confidence
            let after_tilde = &self.input[self.pos+1..];
            if !after_tilde.starts_with('"') && !after_tilde.trim_start().starts_with('"') {
                self.pos += 1;
                let end = self.input[self.pos..].find(|c: char| c.is_whitespace() || c == ',' || c == '}' || c == ']').unwrap_or(self.input.len() - self.pos);
                if let Ok(c) = self.input[self.pos..self.pos+end].parse::<f64>() {
                    confidence = Some(c);
                    self.pos += end;
                }
            }
        }

        Ok((value, confidence))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_parse() {
        let input = r#"
            name: "test-agent"
            version: 1.0
            active: true
            port: 8080  # the port
            
            ~"routing rules": { 
                priority: 5 ~0.9
                handler: "default"
            }
        "#;
        let mut parser = CsonParser::new(input);
        let doc = parser.parse().unwrap();
        assert_eq!(doc.version, "1.0");

        if let CsonValue::Object(ref map) = doc.root.value {
            assert!(map.contains_key("name"));
            assert!(map.contains_key("version"));
            assert!(map.contains_key("active"));
            assert_eq!(map.get("port").unwrap().value, CsonValue::Number(8080.0));
        } else {
            panic!("Expected Object root");
        }
    }

    #[test]
    fn test_version_parsing() {
        let input = r#"@cson { version: "2.0" }
            key: "value"
        "#;
        let mut parser = CsonParser::new(input);
        let doc = parser.parse().unwrap();
        assert_eq!(doc.version, "2.0");
    }

    #[test]
    fn test_issue_escaped_string() {
        let input = r#"msg: "he said \"hi\"""#;
        let mut parser = CsonParser::new(input);
        let doc = parser.parse().unwrap();
        if let CsonValue::Object(map) = doc.root.value {
            assert_eq!(map.get("msg").unwrap().value, CsonValue::String("he said \"hi\"".to_string()));
        } else {
            panic!();
        }
    }

    #[test]
    fn test_issue_annotation_comma() {
        let input = r#"@module { purpose: "parse a, b, c" }
        key: 1"#;
        let mut parser = CsonParser::new(input);
        let doc = parser.parse().unwrap();
        if let CsonValue::Object(map) = doc.root.value {
            assert_eq!(map.get("key").unwrap().annotations[0].properties.get("purpose").unwrap(), "parse a, b, c");
        } else {
            panic!();
        }
    }
}
