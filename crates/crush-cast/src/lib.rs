use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub mod ai;
pub mod caison;
/// Deprecated alias for [`caison`] (renamed from CSON; removed in 0.5).
#[deprecated(note = "renamed to crush_cast::caison")]
pub mod cson {
    pub use super::caison::*;
}
pub mod diff;
pub mod format;
pub mod manifest;
pub mod pack;
pub mod types;
pub mod validate;
pub use manifest::{
    Annotation, ChangelogEntry, CoverageAnnotation, DecisionNode, ErrorAnnotation, ErrorLikelihood,
    ExhaustiveMatchSite, FunctionAnnotations, Invariant, ModuleManifest, ReadAnnotation,
    SourceLoc, TemporaryNode, WeightedError, WriteAnnotation, WipNode,
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
    /// COSM declarations a walker lowered alongside the code (CRUSH-277):
    /// the object-side sibling of `functions`, merged by crush-frontend
    /// with anything it derives itself and carried into
    /// `casm::Program::cosm`. `None` for a program that declares no kinds,
    /// and for CAST written before the field existed (it is omitted when
    /// `None`, so such a program serializes exactly as before).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-export", ts(skip))]
    pub cosm: Option<crush_cosm::CosmModule>,
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
    Assign {
        target: String,
        value: Expression,
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
        /// Explicit output declarations from the `-> name` / `-> (a, b)` block
        /// header (CRUSH-270). Empty means "no `->` written": the pre-CRUSH-270
        /// implicit rule (last top-level assignment) applies, plus a compile
        /// warning. Every name here becomes a Crush variable at this line.
        #[serde(default)]
        outputs: Vec<String>,
        /// Import statements within the block
        #[serde(default)]
        imports: Vec<ImportStatement>,
        /// `@lang[dep1, dep2]` annotation: additional `buckets` package specs
        /// to provision alongside the language itself. Each entry is an OPAQUE
        /// spec string — a bare bottle alias (`openssl@^1.1`, CRUSH-20) or a
        /// `pypi:`/`npm:` registry spec (`pypi:numpy@1.26`, `npm:is-number@7`,
        /// CRUSH-66). Registry specs install into buckets' shared cellar
        /// HOST-side and reach the network-isolated guest via RO-bind +
        /// `PYTHONPATH`/`NODE_PATH` (no in-sandbox pip/npm). v1 rule: NO silent
        /// auto-prefix — bare `numpy` stays a bottle lookup and fails loudly if
        /// unknown; callers must write `pypi:numpy`. Scoped npm
        /// (`npm:@scope/name`) is rejected up front (buckets v1 is unscoped
        /// only). Provisioned only under the `sandboxed-polyglot` feature;
        /// without it, deps are silently unused (CRUSH-20 behavior).
        #[serde(default)]
        deps: Vec<String>,
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
        /// `@kind(key: value, …)` written on the struct (COSM K1.2,
        /// CRUSH-zorro-41): overrides Crush's default KindDecl strategies.
        /// Kept syntactic here; checked against the COSM catalogue when the
        /// program is lowered.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        kind: Option<KindAnnotation>,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    /// `impl P { … }` or `impl T for P { … }` (COSM K1.2, CRUSH-zorro-41).
    /// Every `impl` of a type in a file merges into that type's one COSM
    /// `KindDecl`; the methods are lifted to ordinary functions with `self`
    /// (an ordinary parameter name) first.
    ImplBlock {
        /// The type the methods belong to (`P`).
        type_name: String,
        /// `T` in `impl T for P`; `None` for an inherent impl.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        trait_name: Option<String>,
        methods: Vec<MethodDef>,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    /// `trait T { fn m(self) -> R; fn d(self) { … } }` (COSM K1.2): method
    /// signatures (no body) and default methods (with a body).
    TraitDef {
        name: String,
        methods: Vec<MethodDef>,
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
    VectorMath {
        operator: String,
        args: Vec<Expression>,
        #[serde(default)]
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
    /// Construct an instance of the COSM Kind `kind` with `args`
    /// (CRUSH-277 D3): JS `new C(…)` — distinct from `Call`, which a
    /// class must refuse (a class called without `new` is an error, so
    /// the two are never interchangeable). The compiler emits the CASM
    /// `construct` op; the Kind's `construct` strategy decides what runs.
    Construct {
        kind: String,
        args: Vec<Expression>,
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
    TupleLiteral {
        elements: Vec<Expression>,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    ListLiteral {
        elements: Vec<Expression>,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    VectorLiteral {
        elements: Vec<Expression>,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    SetLiteral {
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

impl Statement {
    /// A plain `struct` declaration: no `@kind(…)` annotation. Producers
    /// (walkers, tests) should build `StructDef` through this rather than a
    /// field-by-field literal, so optional parts added later (like `kind`,
    /// COSM K1.2) don't break them.
    pub fn struct_def(
        name: impl Into<String>,
        fields: Vec<(String, CastType)>,
        meta: HashMap<String, serde_json::Value>,
    ) -> Self {
        Statement::StructDef { name: name.into(), fields, kind: None, meta }
    }
}

/// A method in an `impl` block or a `trait` (COSM K1.2, CRUSH-zorro-41).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct MethodDef {
    pub name: String,
    /// All parameters, `self` included when present. A method whose first
    /// parameter is not `self` is a static member (`fn new(…)`).
    pub params: Vec<(String, CastType)>,
    /// The body. `None` only for a trait method signature without a default
    /// (`fn area(self);`); impl methods always have one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<Vec<Statement>>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_async: bool,
    #[serde(default)]
    pub meta: HashMap<String, serde_json::Value>,
}

impl MethodDef {
    /// Whether the method takes a receiver (`self` as its first parameter).
    pub fn has_self(&self) -> bool {
        self.params.first().is_some_and(|(n, _)| n == "self")
    }
}

/// `@kind(key: value, …)` on a `struct` (COSM K1.2, CRUSH-zorro-41).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct KindAnnotation {
    pub args: Vec<KindArg>,
    #[serde(default)]
    pub meta: HashMap<String, serde_json::Value>,
}

/// One `key: value` entry of a `@kind(…)` annotation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct KindArg {
    pub key: String,
    pub value: KindValue,
    #[serde(default)]
    pub meta: HashMap<String, serde_json::Value>,
}

/// The value of a `@kind` entry, kept as written: `sealed`, `value(copy)`,
/// `[Base, Mixin]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
#[serde(tag = "type")]
pub enum KindValue {
    /// A bare name: `sealed`, `c3`, `Base`.
    Name { name: String },
    /// A name applied to arguments: `value(copy)`, `ctor(make)`.
    Apply { name: String, args: Vec<KindValue> },
    /// A list: `[Base, Mixin]`.
    List { items: Vec<KindValue> },
}

impl std::fmt::Display for KindValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let join = |items: &[KindValue]| {
            items.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(", ")
        };
        match self {
            KindValue::Name { name } => write!(f, "{name}"),
            KindValue::Apply { name, args } => write!(f, "{name}({})", join(args)),
            KindValue::List { items } => write!(f, "[{}]", join(items)),
        }
    }
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
