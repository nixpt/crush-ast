use serde::{Deserialize, Serialize};
use indexmap::IndexMap;
use crate::manifest::{Invariant, WipNode, TemporaryNode, DecisionNode};

/// Represents a key in a CSON Object.
/// It can be a standard exact-match string, or a semantic fuzzy-match anchor.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
#[serde(tag = "type")]
pub enum CsonKey {
    /// Standard exact string key
    Exact { value: String },
    /// Semantic fuzzy anchor key (e.g. ~"Billing or refund issues")
    Semantic { value: String },
}

/// Core values supported by CSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
#[serde(tag = "type")]
pub enum CsonValue {
    Null,
    Bool { value: bool },
    Int { value: i64 },
    Float { value: f64 },
    String { value: String },
    Array { elements: Vec<CsonNode> },
    Object { properties: IndexMap<String, CsonNode> }, // We use String internally for serialization ease, but maybe map semantic keys differently in runtime
    SemanticObject { properties: Vec<(CsonKey, CsonNode)> },
    /// Synthesized placeholder (e.g. `@synthesize("a complimentary color")`)
    Synthesize { prompt: String },
}

/// A node in the CSON tree, wrapping a value with optional AI metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct CsonNode {
    pub value: CsonValue,
    /// Probability/Confidence weight (e.g. ~0.95)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<f64>,
    /// Inline invariants bound to this node
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invariants: Vec<Invariant>,
}

/// The root document structure for a `.cson` file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct CsonDocument {
    /// The root value of the document (typically a SemanticObject or Object)
    pub root: CsonNode,
    
    // --- Global Annotations ---
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wip: Option<WipNode>,
    
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub temporaries: Vec<TemporaryNode>,
    
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub decisions: Vec<DecisionNode>,
}
