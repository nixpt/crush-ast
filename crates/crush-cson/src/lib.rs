//! Crush Semantic Object Notation (CSON) �� canonical types.
//!
//! CSON is a human-readable configuration and serialization format that
//! blends JSON structure with AI-native primitives: confidence weights,
//! semantic keys, annotations, and synthesized values.
//!
//! These types are the single source of truth for CSON across the crush
//! ecosystem. They implement serde's `Serialize` and `Deserialize` for
//! zero-code JSON/MessagePack/CBOR interop.

pub mod parser;
pub mod vm_cap;
pub use parser::CsonParser;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Core Types ─────────────────────────────────────────────────────────────

/// A key in a CSON object — either an exact match or a semantic intent anchor.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CsonKey {
    /// Standard exact-match string key: `name: value`
    Exact(String),
    /// Semantic fuzzy-match anchor: `~"Billing or refund issues": handler`
    #[serde(rename = "~")]
    Semantic(String),
}

/// Core value types in the CSON data model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CsonValue {
    /// String literal: `"hello"`
    String(String),
    /// Numeric value (all numbers are f64): `42`, `3.14`
    Number(f64),
    /// Boolean: `true`, `false`
    Boolean(bool),
    /// Null placeholder: `null`
    Null,
    /// Key-value object: `{ key: value, ... }`
    Object(HashMap<String, CsonNode>),
    /// Ordered sequence: `[a, b, c]`
    Array(Vec<CsonNode>),
    /// AI-synthesized placeholder: `@synthesize("a complimentary color")`
    #[serde(rename = "@synthesize")]
    Synthesize(String),
}

/// A node in the CSON tree — a value with optional AI metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CsonNode {
    /// The node's value.
    pub value: CsonValue,
    /// Confidence/probability weight: `value ~0.95`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    /// Annotations attached to this node: `@wip { owner: "foreman" }`
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub annotations: Vec<CsonAnnotation>,
}

/// Metadata annotation attached to a node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CsonAnnotation {
    /// Annotation name: `@wip`, `@temporary`, `@decision`, etc.
    pub name: String,
    /// Optional parenthesized argument: `@cson("1.5")`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<String>,
    /// Key-value properties: `@wip { owner: "foreman" }`
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub properties: HashMap<String, String>,
}

/// The root document structure for a `.cson` file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CsonDocument {
    /// Document format version (from `@cson { version: "1.0" }`).
    #[serde(default = "default_version")]
    pub version: String,
    /// The root value node.
    pub root: CsonNode,
}

fn default_version() -> String { "1.0".to_string() }

// ── Convenience constructors ───────────────────────────────────────────────

impl CsonNode {
    pub fn new(value: CsonValue) -> Self {
        Self { value, confidence: None, annotations: vec![] }
    }

    pub fn with_confidence(mut self, c: f64) -> Self {
        self.confidence = Some(c);
        self
    }

    pub fn with_annotation(mut self, ann: CsonAnnotation) -> Self {
        self.annotations.push(ann);
        self
    }
}

impl CsonValue {
    /// Convenience: create a string value.
    pub fn string(s: impl Into<String>) -> Self { CsonValue::String(s.into()) }
    /// Convenience: create a number value.
    pub fn number(n: f64) -> Self { CsonValue::Number(n) }
    /// Convenience: create a boolean value.
    pub fn bool_value(b: bool) -> Self { CsonValue::Boolean(b) }
}

// ── JSON serialization ─────────────────────────────────────────────────────

impl CsonDocument {
    /// Serialize this CSON document to a JSON string.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Serialize this CSON document to compact JSON bytes.
    pub fn to_json_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }

    /// Deserialize a CSON document from a JSON string.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Deserialize a CSON document from JSON bytes.
    pub fn from_json_bytes(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
}

// ── Display ────────────────────────────────────────────────────────────────

impl std::fmt::Display for CsonValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CsonValue::String(s) => write!(f, "\"{s}\""),
            CsonValue::Number(n) => write!(f, "{n}"),
            CsonValue::Boolean(b) => write!(f, "{b}"),
            CsonValue::Null => write!(f, "null"),
            CsonValue::Object(_) => write!(f, "{{object}}"),
            CsonValue::Array(_) => write!(f, "[array]"),
            CsonValue::Synthesize(p) => write!(f, "@synthesize({p:?})"),
        }
    }
}

impl std::fmt::Display for CsonNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value)?;
        if let Some(c) = self.confidence {
            write!(f, " ~{c}")?;
        }
        Ok(())
    }
}

impl std::fmt::Display for CsonKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CsonKey::Exact(s) => write!(f, "{s}"),
            CsonKey::Semantic(s) => write!(f, "~\"{s}\""),
        }
    }
}

// ── Tests ──��────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_roundtrip() {
        let doc = CsonDocument {
            version: "1.0".into(),
            root: CsonNode::new(CsonValue::Object(HashMap::from([
                ("name".into(), CsonNode::new(CsonValue::string("test")).with_confidence(0.95)),
                ("count".into(), CsonNode::new(CsonValue::number(42.0))),
            ]))),
        };

        let json = doc.to_json().unwrap();
        let doc2 = CsonDocument::from_json(&json).unwrap();
        assert_eq!(doc.version, doc2.version);
        assert_eq!(doc.root.value, doc2.root.value);
    }

    #[test]
    fn test_json_roundtrip_array() {
        let doc = CsonDocument {
            version: "1.0".into(),
            root: CsonNode::new(CsonValue::Array(vec![
                CsonNode::new(CsonValue::string("a")),
                CsonNode::new(CsonValue::string("b")),
            ])),
        };

        let json = doc.to_json().unwrap();
        let doc2 = CsonDocument::from_json(&json).unwrap();
        assert_eq!(doc.root.value, doc2.root.value);
    }

    #[test]
    fn test_json_roundtrip_annotations() {
        let mut props = HashMap::new();
        props.insert("owner".into(), "foreman".into());
        let doc = CsonDocument {
            version: "1.0".into(),
            root: CsonNode::new(CsonValue::Null)
                .with_annotation(CsonAnnotation { name: "wip".into(), args: None, properties: props }),
        };

        let json = doc.to_json().unwrap();
        let doc2 = CsonDocument::from_json(&json).unwrap();
        assert_eq!(doc2.root.annotations.len(), 1);
        assert_eq!(doc2.root.annotations[0].name, "wip");
    }

    #[test]
    fn test_synthesize() {
        let doc = CsonDocument {
            version: "1.0".into(),
            root: CsonNode::new(CsonValue::Synthesize("a warm color".into())),
        };
        let json = doc.to_json().unwrap();
        // Synthesize values serialise as plain strings with #[serde(untagged)],
        // so they deserialise as String. The semantic distinction is preserved
        // at the CSON level; JSON is a lossy transport for the @synthesize tag.
        let doc2 = CsonDocument::from_json(&json).unwrap();
        assert_eq!(doc2.root.value, CsonValue::String("a warm color".into()));
    }
}

// ── CSON serialization ─────────────────────────────────────────────────────

impl CsonDocument {
    pub fn to_cson(&self) -> String {
        let mut out = String::new();
        if self.version != "1.0" {
            out.push_str(&format!("@cson {{ version: \"{}\" }}\n", self.version));
        }
        
        if let CsonValue::Object(obj) = &self.root.value {
            let mut items: Vec<_> = obj.iter().collect();
            items.sort_by_key(|(k, _)| *k);
            for (k, v) in items {
                if k.starts_with('~') {
                    out.push_str(&format!("~\"{}\": ", k.trim_start_matches('~')));
                } else if k.chars().all(|c| c.is_alphanumeric() || c == '_') && !k.is_empty() {
                    out.push_str(&format!("{}: ", k));
                } else {
                    out.push_str(&format!("\"{}\": ", k));
                }
                out.push_str(&v.to_cson(0));
                out.push('\n');
            }
        } else {
            out.push_str(&self.root.to_cson(0));
            out.push('\n');
        }
        
        out
    }
}

impl CsonNode {
    pub fn to_cson(&self, indent: usize) -> String {
        let mut out = String::new();
        for ann in &self.annotations {
            out.push_str(&format!("@{}", ann.name));
            if let Some(args) = &ann.args {
                out.push_str(&format!("(\"{}\")", args));
            }
            if !ann.properties.is_empty() {
                out.push_str(" {");
                let mut props: Vec<_> = ann.properties.iter().collect();
                props.sort_by_key(|(k, _)| *k);
                for (i, (k, v)) in props.iter().enumerate() {
                    if i > 0 { out.push_str(","); }
                    out.push_str(&format!(" {}: \"{}\"", k, v.replace('"', "\\\"")));
                }
                out.push_str(" }");
            }
            out.push('\n');
            out.push_str(&" ".repeat(indent * 4));
        }
        
        out.push_str(&self.value.to_cson(indent));
        
        if let Some(c) = self.confidence {
            out.push_str(&format!(" ~{}", c));
        }
        out
    }
}

impl CsonValue {
    pub fn to_cson(&self, indent: usize) -> String {
        match self {
            CsonValue::String(s) => format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n").replace('\r', "\\r").replace('\t', "\\t")),
            CsonValue::Number(n) => n.to_string(),
            CsonValue::Boolean(b) => b.to_string(),
            CsonValue::Null => "null".to_string(),
            CsonValue::Synthesize(s) => format!("@synthesize(\"{}\")", s),
            CsonValue::Array(arr) => {
                if arr.is_empty() {
                    return "[]".to_string();
                }
                let mut out = String::from("[\n");
                let child_indent = indent + 1;
                for item in arr {
                    out.push_str(&" ".repeat(child_indent * 4));
                    out.push_str(&item.to_cson(child_indent));
                    out.push_str(",\n");
                }
                out.push_str(&" ".repeat(indent * 4));
                out.push(']');
                out
            }
            CsonValue::Object(obj) => {
                if obj.is_empty() {
                    return "{}".to_string();
                }
                let mut out = String::from("{\n");
                let child_indent = indent + 1;
                let mut items: Vec<_> = obj.iter().collect();
                items.sort_by_key(|(k, _)| *k);
                for (k, v) in items {
                    out.push_str(&" ".repeat(child_indent * 4));
                    if k.starts_with('~') {
                        out.push_str(&format!("~\"{}\": ", k.trim_start_matches('~')));
                    } else if k.chars().all(|c| c.is_alphanumeric() || c == '_') && !k.is_empty() {
                        out.push_str(&format!("{}: ", k));
                    } else {
                        out.push_str(&format!("\"{}\": ", k));
                    }
                    out.push_str(&v.to_cson(child_indent));
                    out.push_str(",\n");
                }
                out.push_str(&" ".repeat(indent * 4));
                out.push('}');
                out
            }
        }
    }
}
