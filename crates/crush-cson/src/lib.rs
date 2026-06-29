pub mod vm_cap;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CsonKey {
    Exact(String),
    Semantic(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum CsonValue {
    String(String),
    Number(f64),
    Boolean(bool),
    Object(HashMap<CsonKey, CsonNode>),
    Array(Vec<CsonNode>),
    Synthesize(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct CsonDocument {
    pub version: String,
    pub root: CsonNode,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CsonNode {
    pub value: CsonValue,
    pub confidence: Option<f64>,
    pub annotations: Vec<CsonAnnotation>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CsonAnnotation {
    pub name: String,
    pub args: Option<String>,
    pub properties: HashMap<String, String>,
}

/// A very basic, MVP parser for Crush Semantic Object Notation (CSON).
pub struct CsonParser<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> CsonParser<'a> {
    pub fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }

    fn skip_whitespace_and_comments(&mut self) {
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

    pub fn parse(&mut self) -> Result<CsonDocument, String> {
        let mut root_obj = HashMap::new();
        let mut current_section = None;
        let mut pending_annotations = Vec::new();
        let mut document_version = "1.0".to_string();

        while self.pos < self.input.len() {
            self.skip_whitespace_and_comments();
            if self.pos >= self.input.len() { break; }

            let rest = &self.input[self.pos..];

            // 1. Annotations (e.g. `@wip { owner: "foreman" }`)
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

            // 2. Sections (e.g. `[routing_table]`)
            if rest.starts_with('[') {
                self.pos += 1; // skip '['
                let end = self.input[self.pos..].find(']').ok_or("Unclosed section")?;
                let section_name = self.input[self.pos..self.pos+end].trim().to_string();
                self.pos += end + 1; // skip ']'
                current_section = Some(section_name);
                continue;
            }

            // 3. Key-Value pairs
            if let Some((key, mut node)) = self.parse_kv_pair()? {
                node.annotations.extend(pending_annotations.drain(..));
                
                if let Some(ref sec) = current_section {
                    // It belongs to a section. If section object doesn't exist, create it.
                    let sec_key = CsonKey::Exact(sec.clone());
                    let section_node = root_obj.entry(sec_key).or_insert_with(|| CsonNode {
                        value: CsonValue::Object(HashMap::new()),
                        confidence: None,
                        annotations: vec![],
                    });
                    
                    if let CsonValue::Object(ref mut map) = section_node.value {
                        map.insert(key, node);
                    }
                } else {
                    root_obj.insert(key, node);
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
                annotations: pending_annotations, // Leftovers applied to root
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
        if self.input[self.pos..].starts_with('(') {
            self.pos += 1;
            let end = self.input[self.pos..].find(')').ok_or("Unclosed annotation args")?;
            args = Some(self.input[self.pos..self.pos+end].to_string());
            self.pos += end + 1;
        }

        self.skip_whitespace_and_comments();
        
        let mut properties = HashMap::new();
        if self.input[self.pos..].starts_with('{') {
            self.pos += 1; // skip '{'
            // Very naive property parsing
            let end = self.input[self.pos..].find('}').ok_or("Unclosed annotation properties")?;
            let props_str = &self.input[self.pos..self.pos+end];
            for pair in props_str.split(',') {
                if let Some((k, v)) = pair.split_once(':') {
                    properties.insert(k.trim().to_string(), v.trim().trim_matches('"').to_string());
                }
            }
            self.pos += end + 1;
        }

        Ok(CsonAnnotation { name, args, properties })
    }

    fn parse_kv_pair(&mut self) -> Result<Option<(CsonKey, CsonNode)>, String> {
        self.skip_whitespace_and_comments();
        if self.pos >= self.input.len() { return Ok(None); }
        if self.input[self.pos..].starts_with('[') || self.input[self.pos..].starts_with('@') {
            return Ok(None);
        }

        let mut is_semantic = false;
        if self.input[self.pos..].starts_with('~') {
            is_semantic = true;
            self.pos += 1;
        }

        // Parse key
        let key_str;
        if self.input[self.pos..].starts_with('"') {
            self.pos += 1;
            let end = self.input[self.pos..].find('"').ok_or("Unclosed string key")?;
            key_str = self.input[self.pos..self.pos+end].to_string();
            self.pos += end + 1;
        } else {
            let end = self.input[self.pos..].find(':').ok_or("Missing colon in kv pair")?;
            key_str = self.input[self.pos..self.pos+end].trim().to_string();
            self.pos += end;
        }

        let key = if is_semantic { CsonKey::Semantic(key_str) } else { CsonKey::Exact(key_str) };

        self.skip_whitespace_and_comments();
        if !self.input[self.pos..].starts_with(':') {
            return Err("Expected ':' after key".to_string());
        }
        self.pos += 1; // skip ':'
        
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
        
        let value;
        if self.input[self.pos..].starts_with('@') {
            let ann = self.parse_annotation()?;
            if ann.name == "synthesize" {
                value = CsonValue::Synthesize(ann.args.unwrap_or_default().trim_matches('"').to_string());
            } else {
                return Err("Only @synthesize is supported as a value annotation".to_string());
            }
        } else if self.input[self.pos..].starts_with('"') {
            self.pos += 1;
            let end = self.input[self.pos..].find('"').ok_or("Unclosed string value")?;
            value = CsonValue::String(self.input[self.pos..self.pos+end].to_string());
            self.pos += end + 1;
        } else if self.input[self.pos..].starts_with('{') {
            self.pos += 1;
            let mut map = HashMap::new();
            while !self.input[self.pos..].starts_with('}') {
                if let Some((k, n)) = self.parse_kv_pair()? {
                    map.insert(k, n);
                } else {
                    break;
                }
                self.skip_whitespace_and_comments();
            }
            self.pos += 1;
            value = CsonValue::Object(map);
        } else {
            // Unquoted string or number
            let end = self.input[self.pos..].find(|c: char| c == '\n' || c == '~').unwrap_or(self.input.len() - self.pos);
            let raw = self.input[self.pos..self.pos+end].trim().to_string();
            self.pos += end;
            
            if let Ok(num) = raw.parse::<f64>() {
                value = CsonValue::Number(num);
            } else if raw == "true" {
                value = CsonValue::Boolean(true);
            } else if raw == "false" {
                value = CsonValue::Boolean(false);
            } else {
                value = CsonValue::String(raw);
            }
        }

        // Check for confidence modifier `~0.95`
        let mut confidence = None;
        self.skip_whitespace_and_comments();
        if self.pos < self.input.len() && self.input[self.pos..].starts_with('~') {
            self.pos += 1;
            let end = self.input[self.pos..].find(|c: char| c.is_whitespace()).unwrap_or(self.input.len() - self.pos);
            let conf_str = self.input[self.pos..self.pos+end].trim();
            if let Ok(c) = conf_str.parse::<f64>() {
                confidence = Some(c);
                self.pos += end;
            } else {
                self.pos -= 1; // rollback if it's not a valid number
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
        @module { purpose: "Config" }
        
        name: "SupportBot"
        temperature: 0.7
        
        [routing_table]
        ~"cancel subscription": { action: "retention" }
        ~"mobile bug": { action: "engineering" } ~0.9
        "#;
        
        let mut parser = CsonParser::new(input);
        let doc = parser.parse().unwrap();
        
        let CsonValue::Object(map) = doc.root.value else { panic!() };
        
        let name_node = map.get(&CsonKey::Exact("name".to_string())).unwrap();
        assert_eq!(name_node.annotations.len(), 1);
        assert_eq!(name_node.annotations[0].name, "module");
        assert_eq!(name_node.annotations[0].properties.get("purpose").unwrap(), "Config");
        
        assert_eq!(name_node.value, CsonValue::String("SupportBot".to_string()));
        
        let temp_node = map.get(&CsonKey::Exact("temperature".to_string())).unwrap();
        assert_eq!(temp_node.value, CsonValue::Number(0.7));
        
        let routing_node = map.get(&CsonKey::Exact("routing_table".to_string())).unwrap();
        let CsonValue::Object(routing_map) = &routing_node.value else { panic!() };
        
        let bug_node = routing_map.get(&CsonKey::Semantic("mobile bug".to_string())).unwrap();
        assert_eq!(bug_node.confidence, Some(0.9));
    }

    #[test]
    fn test_version_parsing() {
        let input = r#"
        @cson { version: "1.5" }
        name: "Test"
        "#;
        let mut parser = CsonParser::new(input);
        let doc = parser.parse().unwrap();
        assert_eq!(doc.version, "1.5");
    }
}
