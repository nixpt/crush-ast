use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub mod ai;
pub mod cson;
pub mod diff;
pub mod format;
pub mod manifest;
pub mod pack;
pub mod types;
pub mod validate;
pub use manifest::{
    ChangelogEntry, DecisionNode, ErrorLikelihood, ExhaustiveMatchSite, FunctionAnnotations,
    Invariant, ModuleManifest, SourceLoc, TemporaryNode, WeightedError, WipNode,
};
pub use pack::{CAST_VERSION, Format, PackError};
pub use types::CastType;
pub use validate::{ValidationError, validate_json};

/// Helper for `#[serde(skip_serializing_if)]` on bool fields.
fn is_false(b: &bool) -> bool {
    !b
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct Program {
    pub cast_version: String,
    pub entry: String,
    pub lang: Option<String>,
    pub functions: HashMap<String, Function>,
    /// AI execution metadata (goals, tool-chains, delegation).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai_meta: Option<ai::AIMetadata>,
    /// Navigation manifest (@module annotation): purpose, exports, invariants,
    /// related modules. Consumed by crush-index to build the queryable index.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest: Option<manifest::ModuleManifest>,
    /// Exhaustive match sites for tracked sum types (compiler-populated).
    /// Set for each type listed in `manifest.exhaustive_types`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exhaustive_sites: Vec<manifest::ExhaustiveMatchSite>,
    /// Work-in-progress node, if a `@wip { ... }` block was declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wip: Option<manifest::WipNode>,
    /// Technical-debt nodes from `@temporary { ... }` blocks.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub temporaries: Vec<manifest::TemporaryNode>,
    /// Architectural decision records from `@decision "name" { ... }` blocks.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub decisions: Vec<manifest::DecisionNode>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct Function {
    pub params: Vec<(String, CastType)>,
    pub body: Vec<Statement>,
    pub meta: HashMap<String, serde_json::Value>,
    /// Whether this is an async function (marked with `async` keyword).
    /// Used for tooling and frontend lowering; spawn/await behavior is
    /// explicit via the `spawn` expression and `AWAIT` opcode.
    #[serde(default)]
    #[serde(skip_serializing_if = "is_false")]
    pub is_async: bool,
    /// Semantic annotations (@errors, @reads, @writes, @covers, @relies-on).
    /// Absent when no annotations were written; all sub-fields are optional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<manifest::FunctionAnnotations>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
#[serde(tag = "type")]
pub enum Statement {
    VarDecl {
        name: String,
        value: Expression,
        #[serde(default)]
        type_hint: CastType,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    Export {
        name: String,
        value: Expression,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    ExprStmt {
        expr: Expression,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    If {
        condition: Expression,
        then_body: Vec<Statement>,
        else_body: Option<Vec<Statement>>,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    While {
        condition: Box<Expression>,
        body: Vec<Statement>,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    For {
        variable: String,
        iterable: Box<Expression>,
        body: Vec<Statement>,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    Return {
        value: Option<Expression>,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    TryCatch {
        body: Vec<Statement>,
        error_var: String,
        handler: Vec<Statement>,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    Throw {
        value: Expression,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    FunctionDef {
        name: String,
        params: Vec<(String, CastType)>,
        body: Vec<Statement>,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    SetField {
        target: Expression,
        field: String,
        value: Expression,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    /// Execute code in a language sandbox
    LangBlock {
        /// Language name: python, javascript, rust, etc.
        lang: String,
        /// Raw source code to execute
        code: String,
        /// Variables to inject into the sandbox
        #[serde(default)]
        variables: Vec<String>,
        /// Import statements within the block
        #[serde(default)]
        imports: Vec<ImportStatement>,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    /// Import statement
    Import {
        /// The import to resolve
        #[serde(alias = "import_")]
        import: ImportStatement,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    StructDef {
        name: String,
        fields: Vec<(String, CastType)>,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    Break {
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    Continue {
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    DomMutate {
        target: Expression,
        mutation_type: DomMutationType,
        value: Option<Expression>,
        value2: Option<Expression>,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    DomEventListener {
        target: Expression,
        event: String,
        callback: Expression,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    /// AI-specific orchestration statement
    AI(ai::AIStatement),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
#[serde(tag = "type")]
pub enum DomMutationType {
    SetTextContent,
    SetAttribute,
    RemoveAttribute,
    SetStyle,
    SetInnerHtml,
    AppendHtml,
    Remove,
    AddClass,
    RemoveClass,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
#[serde(tag = "type")]
pub enum DomQueryType {
    QuerySelector,
    QuerySelectorAll,
    GetElementById,
    GetElementsByClassName,
    GetElementsByTagName,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
#[serde(tag = "type")]
pub enum Expression {
    IntLiteral {
        value: i64,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    FloatLiteral {
        value: f64,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    StringLiteral {
        value: String,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    BoolLiteral {
        value: bool,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    NullLiteral {
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    Var {
        name: String,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    BinaryOp {
        operator: String,
        left: Box<Expression>,
        right: Box<Expression>,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    UnaryOp {
        operator: String,
        operand: Box<Expression>,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    Call {
        function: String,
        args: Vec<Expression>,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    CapabilityCall {
        name: String,
        args: Vec<Expression>,
        meta: HashMap<String, serde_json::Value>,
    },
    Pipeline {
        segments: Vec<Expression>,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    Spawn {
        function: String,
        args: Vec<Expression>,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    Lambda {
        params: Vec<(String, CastType)>,
        body: Vec<Statement>,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    Yield {
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    NewStruct {
        name: String,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    GetField {
        target: Box<Expression>,
        field: String,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    Range {
        start: Box<Expression>,
        end: Box<Expression>,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    Await {
        expression: Box<Expression>,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    ArrayLiteral {
        elements: Vec<Expression>,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    ObjectLiteral {
        properties: Vec<(String, Expression)>,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    Index {
        target: Box<Expression>,
        index: Box<Expression>,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    DomQuery {
        query_type: DomQueryType,
        selector: Box<Expression>,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    Match {
        expression: Box<Expression>,
        arms: Vec<MatchArm>,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    /// AI-specific expression
    AI(ai::AIExpression),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
#[serde(tag = "type")]
pub enum Pattern {
    Literal {
        value: Expression,
    },
    Identifier {
        name: String,
    },
    Struct {
        name: String,
        fields: Vec<(String, Pattern)>,
    },
    Wildcard,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
#[serde(tag = "type")]
pub enum ImportStatement {
    CrushModule {
        module_path: String,
        alias: Option<String>,
        #[serde(default)]
        selective: Vec<String>,
    },
    PolyglotModule {
        language: String,
        module_path: String,
        alias: Option<String>,
        #[serde(default)]
        selective: Vec<String>,
    },
    MCPImport {
        server_url: String,
        tools: Vec<String>,
        alias: Option<String>,
    },
    Capability {
        capability_path: String,
        permissions: Vec<String>,
        alias: Option<String>,
    },
    External {
        uri: String,
        resource_type: ExternalResourceType,
        alias: Option<String>,
    },
    /// Import from secure-env (encrypted environment variable storage)
    ///
    /// # Example
    /// ```crush
    /// // Import specific secrets
    /// import secrets { DATABASE_URL, API_KEY }
    ///
    /// // Import all secrets with alias
    /// import secrets as env
    ///
    /// // Access secrets
    /// let db_url = secrets.DATABASE_URL
    /// ```
    SecureEnv {
        /// Specific keys to import (empty = all keys)
        #[serde(default)]
        keys: Vec<String>,
        /// Alias for the imported secrets module
        alias: Option<String>,
        /// Path to the secrets database (optional, uses default if not specified)
        #[serde(default)]
        db_path: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum ExternalResourceType {
    Http,
    Git,
    File,
    Database,
    API { format: String },
}
