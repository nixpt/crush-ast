//! Export Python dataclasses for the full CAST AST.
//!
//! Run with:
//!   cargo run -p crush-cast --bin export-py
//!
//! Output is written to `crates/core/crush-cast/python/cast_types.py`.

use std::fmt::Write;

// ---------------------------------------------------------------------------
// Declarative schema mirroring the Rust types in lib.rs / ai.rs / types.rs
// ---------------------------------------------------------------------------

#[derive(Clone)]
enum PyType {
    Str,
    Int,
    Float,
    Bool,
    /// Python `Any` (maps to serde_json::Value)
    Any,
    /// `Optional<T>`
    Opt(Box<PyType>),
    /// `List<T>`
    List(Box<PyType>),
    /// `Dict[str, T]` (T defaults to Any)
    Dict { value: Box<PyType> },
    /// A reference to another named type
    Named(String),
    /// `Literal["x", "y"]`
    Literal(Vec<String>),
    /// Tuple type: `Tuple[T1, T2]`
    Tuple(Vec<PyType>),
}

impl PyType {
    fn render(&self) -> String {
        match self {
            PyType::Str => "str".to_string(),
            PyType::Int => "int".to_string(),
            PyType::Float => "float".to_string(),
            PyType::Bool => "bool".to_string(),
            PyType::Any => "Any".to_string(),
            PyType::Opt(t) => format!("Optional[{}]", t.render()),
            PyType::List(t) => format!("List[{}]", t.render()),
            PyType::Dict { value } => format!("Dict[str, {}]", value.render()),
            PyType::Named(n) => n.clone(),
            PyType::Literal(vals) => {
                if vals.len() == 1 {
                    format!("Literal[\"{}\"]", vals[0])
                } else {
                    let inner: Vec<String> = vals.iter().map(|v| format!("\"{}\"", v)).collect();
                    format!("Literal[{}]", inner.join(", "))
                }
            }
            PyType::Tuple(parts) => {
                let inner: Vec<String> = parts.iter().map(|p| p.render()).collect();
                format!("Tuple[{}]", inner.join(", "))
            }
        }
    }
}

#[derive(Clone)]
struct PyField {
    name: String,
    ty: PyType,
    /// Python expression for the default value, e.g. `None`, `"Any"`, `field(default_factory=dict)`
    default: Option<String>,
}

#[derive(Clone)]
struct PyVariant {
    /// Rust variant name (e.g. "VarDecl")
    name: String,
    /// serde tag value, if internally tagged
    tag: Option<String>,
    fields: Vec<PyField>,
}

#[derive(Clone)]
enum PyDef {
    /// Plain struct dataclass
    Struct {
        name: String,
        fields: Vec<PyField>,
    },
    /// Internally tagged enum — each variant is a dataclass, plus a Union alias
    TaggedUnion {
        name: String,
        tag_field: String,
        variants: Vec<PyVariant>,
    },
    /// Externally tagged enum — some variants are string literals, some are structs.
    /// We generate helper dataclasses for struct variants and a Union alias.
    ExternalUnion {
        name: String,
        variants: Vec<ExtVariant>,
    },
    /// Simple type alias (e.g. `JsonValue = Any`)
    Alias { name: String, target: PyType },
}

#[derive(Clone)]
enum ExtVariant {
    /// Unit variant → string literal
    Unit(String),
    /// Single unnamed field → wrapper dataclass with one field
    Wrapper { rust_name: String, inner: PyType },
    /// Struct variant → dataclass with the given fields, wrapped under rust_name
    Struct { rust_name: String, fields: Vec<PyField> },
}

// ---------------------------------------------------------------------------
// Renderer
// ---------------------------------------------------------------------------

fn render_field(field: &PyField) -> String {
    let ty = field.ty.render();
    match &field.default {
        Some(d) if d.starts_with("field(") => {
            format!("    {}: {} = {}", field.name, ty, d)
        }
        Some(d) => {
            format!("    {}: {} = {}", field.name, ty, d)
        }
        None => {
            format!("    {}: {}", field.name, ty)
        }
    }
}

fn render_dataclass(name: &str, fields: &[PyField], doc: Option<&str>) -> String {
    let mut out = String::new();
    if let Some(d) = doc {
        writeln!(out, "@dataclass").unwrap();
        writeln!(out, "class {}:", name).unwrap();
        for line in d.lines() {
            writeln!(out, "    \"\"\"{}\"\"\"", line).unwrap();
        }
    } else {
        writeln!(out, "@dataclass").unwrap();
        writeln!(out, "class {}:", name).unwrap();
    }
    if fields.is_empty() {
        writeln!(out, "    pass").unwrap();
    } else {
        // Separate required fields (no default) from optional fields
        let (required, optional): (Vec<_>, Vec<_>) =
            fields.iter().partition(|f| f.default.is_none());
        for f in &required {
            writeln!(out, "{}", render_field(f)).unwrap();
        }
        for f in &optional {
            writeln!(out, "{}", render_field(f)).unwrap();
        }
    }
    out
}

fn render_pydef(def: &PyDef) -> String {
    let mut out = String::new();
    match def {
        PyDef::Struct { name, fields } => {
            out.push_str(&render_dataclass(name, fields, None));
        }
        PyDef::TaggedUnion {
            name,
            tag_field,
            variants,
        } => {
            // Render each variant as its own dataclass
            for v in variants {
                let mut fields = v.fields.clone();
                // Append the tag field at the end with a default
                fields.push(PyField {
                    name: tag_field.clone(),
                    ty: PyType::Literal(vec![v.tag.clone().unwrap_or_else(|| v.name.clone())]),
                    default: Some(format!("\"{}\"", v.tag.clone().unwrap_or_else(|| v.name.clone()))),
                });
                out.push_str(&render_dataclass(&v.name, &fields, None));
                out.push('\n');
            }
            // Union alias
            let parts: Vec<String> = variants.iter().map(|v| v.name.clone()).collect();
            writeln!(out, "{} = Union[{}]", name, parts.join(", ")).unwrap();
        }
        PyDef::ExternalUnion { name, variants } => {
            let mut union_parts: Vec<String> = Vec::new();
            for v in variants {
                match v {
                    ExtVariant::Unit(s) => {
                        union_parts.push(format!("Literal[\"{}\"]", s));
                    }
                    ExtVariant::Wrapper { rust_name, inner } => {
                        let cls_name = format!("_{}{}", name, rust_name);
                        let field = PyField {
                            name: rust_name.clone(),
                            ty: inner.clone(),
                            default: None,
                        };
                        out.push_str(&render_dataclass(&cls_name, &[field], None));
                        out.push('\n');
                        union_parts.push(cls_name);
                    }
                    ExtVariant::Struct { rust_name, fields } => {
                        let cls_name = format!("_{}{}", name, rust_name);
                        out.push_str(&render_dataclass(&cls_name, fields, None));
                        out.push('\n');
                        union_parts.push(cls_name);
                    }
                }
            }
            writeln!(out, "{} = Union[{}]", name, union_parts.join(", ")).unwrap();
        }
        PyDef::Alias { name, target } => {
            writeln!(out, "{} = {}", name, target.render()).unwrap();
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Schema definition
// ---------------------------------------------------------------------------

fn schema() -> Vec<PyDef> {
    let any_dict = PyType::Dict { value: Box::new(PyType::Any) };
    let any_opt = PyType::Opt(Box::new(PyType::Any));
    let str_opt = PyType::Opt(Box::new(PyType::Str));
    let expr_opt = PyType::Opt(Box::new(PyType::Named("Expression".to_string())));
    let stmt_list = PyType::List(Box::new(PyType::Named("Statement".to_string())));
    let stmt_list_opt = PyType::Opt(Box::new(stmt_list.clone()));
    let expr = PyType::Named("Expression".to_string());
    let cast_type = PyType::Named("CastType".to_string());
    let cast_type_list = PyType::List(Box::new(cast_type.clone()));

    vec![
        // --- Type aliases ---
        PyDef::Alias {
            name: "JsonValue".to_string(),
            target: PyType::Any,
        },
        // --- CastType (externally tagged) ---
        PyDef::ExternalUnion {
            name: "CastType".to_string(),
            variants: vec![
                ExtVariant::Unit("Int".to_string()),
                ExtVariant::Unit("Float".to_string()),
                ExtVariant::Unit("String".to_string()),
                ExtVariant::Unit("Bool".to_string()),
                ExtVariant::Unit("Null".to_string()),
                ExtVariant::Unit("Any".to_string()),
                ExtVariant::Wrapper {
                    rust_name: "Array".to_string(),
                    inner: cast_type.clone(),
                },
                ExtVariant::Wrapper {
                    rust_name: "Map".to_string(),
                    inner: cast_type.clone(),
                },
                ExtVariant::Wrapper {
                    rust_name: "Struct".to_string(),
                    inner: PyType::Str,
                },
                ExtVariant::Struct {
                    rust_name: "Lambda".to_string(),
                    fields: vec![
                        PyField { name: "params".to_string(), ty: cast_type_list.clone(), default: Some("field(default_factory=list)".to_string()) },
                        PyField { name: "returns".to_string(), ty: cast_type.clone(), default: None },
                    ],
                },
                ExtVariant::Wrapper {
                    rust_name: "TypeRef".to_string(),
                    inner: PyType::Str,
                },
            ],
        },
        // --- Program ---
        PyDef::Struct {
            name: "Program".to_string(),
            fields: vec![
                PyField { name: "cast_version".to_string(), ty: PyType::Str, default: None },
                PyField { name: "entry".to_string(), ty: PyType::Str, default: None },
                PyField { name: "lang".to_string(), ty: str_opt.clone(), default: Some("None".to_string()) },
                PyField { name: "functions".to_string(), ty: PyType::Dict { value: Box::new(PyType::Named("Function".to_string())) }, default: Some("field(default_factory=dict)".to_string()) },
                PyField { name: "ai_meta".to_string(), ty: PyType::Opt(Box::new(PyType::Named("AIMetadata".to_string()))), default: Some("None".to_string()) },
            ],
        },
        // --- Function ---
        PyDef::Struct {
            name: "Function".to_string(),
            fields: vec![
                PyField { name: "params".to_string(), ty: PyType::List(Box::new(PyType::Tuple(vec![PyType::Str, cast_type.clone()]))), default: Some("field(default_factory=list)".to_string()) },
                PyField { name: "body".to_string(), ty: stmt_list.clone(), default: Some("field(default_factory=list)".to_string()) },
                PyField { name: "meta".to_string(), ty: any_dict.clone(), default: Some("field(default_factory=dict)".to_string()) },
            ],
        },
        // --- Statement (tagged with "type") ---
        PyDef::TaggedUnion {
            name: "Statement".to_string(),
            tag_field: "type".to_string(),
            variants: vec![
                PyVariant {
                    name: "VarDecl".to_string(),
                    tag: Some("VarDecl".to_string()),
                    fields: vec![
                        PyField { name: "name".to_string(), ty: PyType::Str, default: None },
                        PyField { name: "value".to_string(), ty: expr.clone(), default: None },
                        PyField { name: "type_hint".to_string(), ty: cast_type.clone(), default: Some("\"Any\"".to_string()) },
                        PyField { name: "meta".to_string(), ty: any_dict.clone(), default: Some("field(default_factory=dict)".to_string()) },
                    ],
                },
                PyVariant {
                    name: "Export".to_string(),
                    tag: Some("Export".to_string()),
                    fields: vec![
                        PyField { name: "name".to_string(), ty: PyType::Str, default: None },
                        PyField { name: "value".to_string(), ty: expr.clone(), default: None },
                        PyField { name: "meta".to_string(), ty: any_dict.clone(), default: Some("field(default_factory=dict)".to_string()) },
                    ],
                },
                PyVariant {
                    name: "ExprStmt".to_string(),
                    tag: Some("ExprStmt".to_string()),
                    fields: vec![
                        PyField { name: "expr".to_string(), ty: expr.clone(), default: None },
                        PyField { name: "meta".to_string(), ty: any_dict.clone(), default: Some("field(default_factory=dict)".to_string()) },
                    ],
                },
                PyVariant {
                    name: "If".to_string(),
                    tag: Some("If".to_string()),
                    fields: vec![
                        PyField { name: "condition".to_string(), ty: expr.clone(), default: None },
                        PyField { name: "then_body".to_string(), ty: stmt_list.clone(), default: Some("field(default_factory=list)".to_string()) },
                        PyField { name: "else_body".to_string(), ty: stmt_list_opt.clone(), default: Some("None".to_string()) },
                        PyField { name: "meta".to_string(), ty: any_dict.clone(), default: Some("field(default_factory=dict)".to_string()) },
                    ],
                },
                PyVariant {
                    name: "While".to_string(),
                    tag: Some("While".to_string()),
                    fields: vec![
                        PyField { name: "condition".to_string(), ty: expr.clone(), default: None },
                        PyField { name: "body".to_string(), ty: stmt_list.clone(), default: Some("field(default_factory=list)".to_string()) },
                        PyField { name: "meta".to_string(), ty: any_dict.clone(), default: Some("field(default_factory=dict)".to_string()) },
                    ],
                },
                PyVariant {
                    name: "For".to_string(),
                    tag: Some("For".to_string()),
                    fields: vec![
                        PyField { name: "variable".to_string(), ty: PyType::Str, default: None },
                        PyField { name: "iterable".to_string(), ty: expr.clone(), default: None },
                        PyField { name: "body".to_string(), ty: stmt_list.clone(), default: Some("field(default_factory=list)".to_string()) },
                        PyField { name: "meta".to_string(), ty: any_dict.clone(), default: Some("field(default_factory=dict)".to_string()) },
                    ],
                },
                PyVariant {
                    name: "Return".to_string(),
                    tag: Some("Return".to_string()),
                    fields: vec![
                        PyField { name: "value".to_string(), ty: expr_opt.clone(), default: Some("None".to_string()) },
                        PyField { name: "meta".to_string(), ty: any_dict.clone(), default: Some("field(default_factory=dict)".to_string()) },
                    ],
                },
                PyVariant {
                    name: "TryCatch".to_string(),
                    tag: Some("TryCatch".to_string()),
                    fields: vec![
                        PyField { name: "body".to_string(), ty: stmt_list.clone(), default: Some("field(default_factory=list)".to_string()) },
                        PyField { name: "error_var".to_string(), ty: PyType::Str, default: None },
                        PyField { name: "handler".to_string(), ty: stmt_list.clone(), default: Some("field(default_factory=list)".to_string()) },
                        PyField { name: "meta".to_string(), ty: any_dict.clone(), default: Some("field(default_factory=dict)".to_string()) },
                    ],
                },
                PyVariant {
                    name: "Throw".to_string(),
                    tag: Some("Throw".to_string()),
                    fields: vec![
                        PyField { name: "value".to_string(), ty: expr.clone(), default: None },
                        PyField { name: "meta".to_string(), ty: any_dict.clone(), default: Some("field(default_factory=dict)".to_string()) },
                    ],
                },
                PyVariant {
                    name: "FunctionDef".to_string(),
                    tag: Some("FunctionDef".to_string()),
                    fields: vec![
                        PyField { name: "name".to_string(), ty: PyType::Str, default: None },
                        PyField { name: "params".to_string(), ty: PyType::List(Box::new(PyType::Tuple(vec![PyType::Str, cast_type.clone()]))), default: Some("field(default_factory=list)".to_string()) },
                        PyField { name: "body".to_string(), ty: stmt_list.clone(), default: Some("field(default_factory=list)".to_string()) },
                        PyField { name: "meta".to_string(), ty: any_dict.clone(), default: Some("field(default_factory=dict)".to_string()) },
                    ],
                },
                PyVariant {
                    name: "SetField".to_string(),
                    tag: Some("SetField".to_string()),
                    fields: vec![
                        PyField { name: "target".to_string(), ty: expr.clone(), default: None },
                        PyField { name: "field".to_string(), ty: PyType::Str, default: None },
                        PyField { name: "value".to_string(), ty: expr.clone(), default: None },
                        PyField { name: "meta".to_string(), ty: any_dict.clone(), default: Some("field(default_factory=dict)".to_string()) },
                    ],
                },
                PyVariant {
                    name: "LangBlock".to_string(),
                    tag: Some("LangBlock".to_string()),
                    fields: vec![
                        PyField { name: "lang".to_string(), ty: PyType::Str, default: None },
                        PyField { name: "code".to_string(), ty: PyType::Str, default: None },
                        PyField { name: "variables".to_string(), ty: PyType::List(Box::new(PyType::Str)), default: Some("field(default_factory=list)".to_string()) },
                        PyField { name: "imports".to_string(), ty: PyType::List(Box::new(PyType::Named("ImportStatement".to_string()))), default: Some("field(default_factory=list)".to_string()) },
                        PyField { name: "meta".to_string(), ty: any_dict.clone(), default: Some("field(default_factory=dict)".to_string()) },
                    ],
                },
                PyVariant {
                    name: "Import".to_string(),
                    tag: Some("Import".to_string()),
                    fields: vec![
                        PyField { name: "import_".to_string(), ty: PyType::Named("ImportStatement".to_string()), default: None },
                        PyField { name: "meta".to_string(), ty: any_dict.clone(), default: Some("field(default_factory=dict)".to_string()) },
                    ],
                },
                PyVariant {
                    name: "StructDef".to_string(),
                    tag: Some("StructDef".to_string()),
                    fields: vec![
                        PyField { name: "name".to_string(), ty: PyType::Str, default: None },
                        PyField { name: "fields".to_string(), ty: PyType::List(Box::new(PyType::Tuple(vec![PyType::Str, cast_type.clone()]))), default: Some("field(default_factory=list)".to_string()) },
                        PyField { name: "meta".to_string(), ty: any_dict.clone(), default: Some("field(default_factory=dict)".to_string()) },
                    ],
                },
                PyVariant {
                    name: "Break".to_string(),
                    tag: Some("Break".to_string()),
                    fields: vec![
                        PyField { name: "meta".to_string(), ty: any_dict.clone(), default: Some("field(default_factory=dict)".to_string()) },
                    ],
                },
                PyVariant {
                    name: "Continue".to_string(),
                    tag: Some("Continue".to_string()),
                    fields: vec![
                        PyField { name: "meta".to_string(), ty: any_dict.clone(), default: Some("field(default_factory=dict)".to_string()) },
                    ],
                },
                PyVariant {
                    name: "DomMutate".to_string(),
                    tag: Some("DomMutate".to_string()),
                    fields: vec![
                        PyField { name: "target".to_string(), ty: expr.clone(), default: None },
                        PyField { name: "mutation_type".to_string(), ty: PyType::Named("DomMutationType".to_string()), default: None },
                        PyField { name: "value".to_string(), ty: expr_opt.clone(), default: Some("None".to_string()) },
                        PyField { name: "value2".to_string(), ty: expr_opt.clone(), default: Some("None".to_string()) },
                        PyField { name: "meta".to_string(), ty: any_dict.clone(), default: Some("field(default_factory=dict)".to_string()) },
                    ],
                },
                PyVariant {
                    name: "DomEventListener".to_string(),
                    tag: Some("DomEventListener".to_string()),
                    fields: vec![
                        PyField { name: "target".to_string(), ty: expr.clone(), default: None },
                        PyField { name: "event".to_string(), ty: PyType::Str, default: None },
                        PyField { name: "callback".to_string(), ty: expr.clone(), default: None },
                        PyField { name: "meta".to_string(), ty: any_dict.clone(), default: Some("field(default_factory=dict)".to_string()) },
                    ],
                },
                PyVariant {
                    name: "AIStmt".to_string(),
                    tag: Some("AI".to_string()),
                    fields: vec![
                        PyField { name: "ai".to_string(), ty: PyType::Named("AIStatement".to_string()), default: None },
                    ],
                },
            ],
        },
        // --- DomMutationType ---
        PyDef::TaggedUnion {
            name: "DomMutationType".to_string(),
            tag_field: "type".to_string(),
            variants: vec![
                PyVariant { name: "SetTextContent".to_string(), tag: Some("SetTextContent".to_string()), fields: vec![] },
                PyVariant { name: "SetAttribute".to_string(), tag: Some("SetAttribute".to_string()), fields: vec![] },
                PyVariant { name: "RemoveAttribute".to_string(), tag: Some("RemoveAttribute".to_string()), fields: vec![] },
                PyVariant { name: "SetStyle".to_string(), tag: Some("SetStyle".to_string()), fields: vec![] },
                PyVariant { name: "SetInnerHtml".to_string(), tag: Some("SetInnerHtml".to_string()), fields: vec![] },
                PyVariant { name: "AppendHtml".to_string(), tag: Some("AppendHtml".to_string()), fields: vec![] },
                PyVariant { name: "Remove".to_string(), tag: Some("Remove".to_string()), fields: vec![] },
                PyVariant { name: "AddClass".to_string(), tag: Some("AddClass".to_string()), fields: vec![] },
                PyVariant { name: "RemoveClass".to_string(), tag: Some("RemoveClass".to_string()), fields: vec![] },
            ],
        },
        // --- DomQueryType ---
        PyDef::TaggedUnion {
            name: "DomQueryType".to_string(),
            tag_field: "type".to_string(),
            variants: vec![
                PyVariant { name: "QuerySelector".to_string(), tag: Some("QuerySelector".to_string()), fields: vec![] },
                PyVariant { name: "QuerySelectorAll".to_string(), tag: Some("QuerySelectorAll".to_string()), fields: vec![] },
                PyVariant { name: "GetElementById".to_string(), tag: Some("GetElementById".to_string()), fields: vec![] },
                PyVariant { name: "GetElementsByClassName".to_string(), tag: Some("GetElementsByClassName".to_string()), fields: vec![] },
                PyVariant { name: "GetElementsByTagName".to_string(), tag: Some("GetElementsByTagName".to_string()), fields: vec![] },
            ],
        },
        // --- Expression (tagged with "type") ---
        PyDef::TaggedUnion {
            name: "Expression".to_string(),
            tag_field: "type".to_string(),
            variants: vec![
                PyVariant {
                    name: "IntLiteral".to_string(),
                    tag: Some("IntLiteral".to_string()),
                    fields: vec![
                        PyField { name: "value".to_string(), ty: PyType::Int, default: None },
                        PyField { name: "meta".to_string(), ty: any_dict.clone(), default: Some("field(default_factory=dict)".to_string()) },
                    ],
                },
                PyVariant {
                    name: "FloatLiteral".to_string(),
                    tag: Some("FloatLiteral".to_string()),
                    fields: vec![
                        PyField { name: "value".to_string(), ty: PyType::Float, default: None },
                        PyField { name: "meta".to_string(), ty: any_dict.clone(), default: Some("field(default_factory=dict)".to_string()) },
                    ],
                },
                PyVariant {
                    name: "StringLiteral".to_string(),
                    tag: Some("StringLiteral".to_string()),
                    fields: vec![
                        PyField { name: "value".to_string(), ty: PyType::Str, default: None },
                        PyField { name: "meta".to_string(), ty: any_dict.clone(), default: Some("field(default_factory=dict)".to_string()) },
                    ],
                },
                PyVariant {
                    name: "BoolLiteral".to_string(),
                    tag: Some("BoolLiteral".to_string()),
                    fields: vec![
                        PyField { name: "value".to_string(), ty: PyType::Bool, default: None },
                        PyField { name: "meta".to_string(), ty: any_dict.clone(), default: Some("field(default_factory=dict)".to_string()) },
                    ],
                },
                PyVariant {
                    name: "NullLiteral".to_string(),
                    tag: Some("NullLiteral".to_string()),
                    fields: vec![
                        PyField { name: "meta".to_string(), ty: any_dict.clone(), default: Some("field(default_factory=dict)".to_string()) },
                    ],
                },
                PyVariant {
                    name: "Var".to_string(),
                    tag: Some("Var".to_string()),
                    fields: vec![
                        PyField { name: "name".to_string(), ty: PyType::Str, default: None },
                        PyField { name: "meta".to_string(), ty: any_dict.clone(), default: Some("field(default_factory=dict)".to_string()) },
                    ],
                },
                PyVariant {
                    name: "BinaryOp".to_string(),
                    tag: Some("BinaryOp".to_string()),
                    fields: vec![
                        PyField { name: "operator".to_string(), ty: PyType::Str, default: None },
                        PyField { name: "left".to_string(), ty: expr.clone(), default: None },
                        PyField { name: "right".to_string(), ty: expr.clone(), default: None },
                        PyField { name: "meta".to_string(), ty: any_dict.clone(), default: Some("field(default_factory=dict)".to_string()) },
                    ],
                },
                PyVariant {
                    name: "UnaryOp".to_string(),
                    tag: Some("UnaryOp".to_string()),
                    fields: vec![
                        PyField { name: "operator".to_string(), ty: PyType::Str, default: None },
                        PyField { name: "operand".to_string(), ty: expr.clone(), default: None },
                        PyField { name: "meta".to_string(), ty: any_dict.clone(), default: Some("field(default_factory=dict)".to_string()) },
                    ],
                },
                PyVariant {
                    name: "Call".to_string(),
                    tag: Some("Call".to_string()),
                    fields: vec![
                        PyField { name: "function".to_string(), ty: PyType::Str, default: None },
                        PyField { name: "args".to_string(), ty: PyType::List(Box::new(expr.clone())), default: Some("field(default_factory=list)".to_string()) },
                        PyField { name: "meta".to_string(), ty: any_dict.clone(), default: Some("field(default_factory=dict)".to_string()) },
                    ],
                },
                PyVariant {
                    name: "CapabilityCall".to_string(),
                    tag: Some("CapabilityCall".to_string()),
                    fields: vec![
                        PyField { name: "name".to_string(), ty: PyType::Str, default: None },
                        PyField { name: "args".to_string(), ty: PyType::List(Box::new(expr.clone())), default: Some("field(default_factory=list)".to_string()) },
                        PyField { name: "meta".to_string(), ty: any_dict.clone(), default: Some("field(default_factory=dict)".to_string()) },
                    ],
                },
                PyVariant {
                    name: "Pipeline".to_string(),
                    tag: Some("Pipeline".to_string()),
                    fields: vec![
                        PyField { name: "segments".to_string(), ty: PyType::List(Box::new(expr.clone())), default: Some("field(default_factory=list)".to_string()) },
                        PyField { name: "meta".to_string(), ty: any_dict.clone(), default: Some("field(default_factory=dict)".to_string()) },
                    ],
                },
                PyVariant {
                    name: "Spawn".to_string(),
                    tag: Some("Spawn".to_string()),
                    fields: vec![
                        PyField { name: "function".to_string(), ty: PyType::Str, default: None },
                        PyField { name: "args".to_string(), ty: PyType::List(Box::new(expr.clone())), default: Some("field(default_factory=list)".to_string()) },
                        PyField { name: "meta".to_string(), ty: any_dict.clone(), default: Some("field(default_factory=dict)".to_string()) },
                    ],
                },
                PyVariant {
                    name: "Lambda".to_string(),
                    tag: Some("Lambda".to_string()),
                    fields: vec![
                        PyField { name: "params".to_string(), ty: PyType::List(Box::new(PyType::Tuple(vec![PyType::Str, cast_type.clone()]))), default: Some("field(default_factory=list)".to_string()) },
                        PyField { name: "body".to_string(), ty: stmt_list.clone(), default: Some("field(default_factory=list)".to_string()) },
                        PyField { name: "meta".to_string(), ty: any_dict.clone(), default: Some("field(default_factory=dict)".to_string()) },
                    ],
                },
                PyVariant {
                    name: "Yield".to_string(),
                    tag: Some("Yield".to_string()),
                    fields: vec![
                        PyField { name: "meta".to_string(), ty: any_dict.clone(), default: Some("field(default_factory=dict)".to_string()) },
                    ],
                },
                PyVariant {
                    name: "NewStruct".to_string(),
                    tag: Some("NewStruct".to_string()),
                    fields: vec![
                        PyField { name: "name".to_string(), ty: PyType::Str, default: None },
                        PyField { name: "meta".to_string(), ty: any_dict.clone(), default: Some("field(default_factory=dict)".to_string()) },
                    ],
                },
                PyVariant {
                    name: "GetField".to_string(),
                    tag: Some("GetField".to_string()),
                    fields: vec![
                        PyField { name: "target".to_string(), ty: expr.clone(), default: None },
                        PyField { name: "field".to_string(), ty: PyType::Str, default: None },
                        PyField { name: "meta".to_string(), ty: any_dict.clone(), default: Some("field(default_factory=dict)".to_string()) },
                    ],
                },
                PyVariant {
                    name: "Range".to_string(),
                    tag: Some("Range".to_string()),
                    fields: vec![
                        PyField { name: "start".to_string(), ty: expr.clone(), default: None },
                        PyField { name: "end".to_string(), ty: expr.clone(), default: None },
                        PyField { name: "meta".to_string(), ty: any_dict.clone(), default: Some("field(default_factory=dict)".to_string()) },
                    ],
                },
                PyVariant {
                    name: "Await".to_string(),
                    tag: Some("Await".to_string()),
                    fields: vec![
                        PyField { name: "expression".to_string(), ty: expr.clone(), default: None },
                        PyField { name: "meta".to_string(), ty: any_dict.clone(), default: Some("field(default_factory=dict)".to_string()) },
                    ],
                },
                PyVariant {
                    name: "ArrayLiteral".to_string(),
                    tag: Some("ArrayLiteral".to_string()),
                    fields: vec![
                        PyField { name: "elements".to_string(), ty: PyType::List(Box::new(expr.clone())), default: Some("field(default_factory=list)".to_string()) },
                        PyField { name: "meta".to_string(), ty: any_dict.clone(), default: Some("field(default_factory=dict)".to_string()) },
                    ],
                },
                PyVariant {
                    name: "ObjectLiteral".to_string(),
                    tag: Some("ObjectLiteral".to_string()),
                    fields: vec![
                        PyField { name: "properties".to_string(), ty: PyType::List(Box::new(PyType::Tuple(vec![PyType::Str, expr.clone()]))), default: Some("field(default_factory=list)".to_string()) },
                        PyField { name: "meta".to_string(), ty: any_dict.clone(), default: Some("field(default_factory=dict)".to_string()) },
                    ],
                },
                PyVariant {
                    name: "Index".to_string(),
                    tag: Some("Index".to_string()),
                    fields: vec![
                        PyField { name: "target".to_string(), ty: expr.clone(), default: None },
                        PyField { name: "index".to_string(), ty: expr.clone(), default: None },
                        PyField { name: "meta".to_string(), ty: any_dict.clone(), default: Some("field(default_factory=dict)".to_string()) },
                    ],
                },
                PyVariant {
                    name: "DomQuery".to_string(),
                    tag: Some("DomQuery".to_string()),
                    fields: vec![
                        PyField { name: "query_type".to_string(), ty: PyType::Named("DomQueryType".to_string()), default: None },
                        PyField { name: "selector".to_string(), ty: expr.clone(), default: None },
                        PyField { name: "meta".to_string(), ty: any_dict.clone(), default: Some("field(default_factory=dict)".to_string()) },
                    ],
                },
                PyVariant {
                    name: "Match".to_string(),
                    tag: Some("Match".to_string()),
                    fields: vec![
                        PyField { name: "expression".to_string(), ty: expr.clone(), default: None },
                        PyField { name: "arms".to_string(), ty: PyType::List(Box::new(PyType::Named("MatchArm".to_string()))), default: Some("field(default_factory=list)".to_string()) },
                        PyField { name: "meta".to_string(), ty: any_dict.clone(), default: Some("field(default_factory=dict)".to_string()) },
                    ],
                },
                PyVariant {
                    name: "AIExpr".to_string(),
                    tag: Some("AI".to_string()),
                    fields: vec![
                        PyField { name: "ai".to_string(), ty: PyType::Named("AIExpression".to_string()), default: None },
                    ],
                },
            ],
        },
        // --- MatchArm ---
        PyDef::Struct {
            name: "MatchArm".to_string(),
            fields: vec![
                PyField { name: "pattern".to_string(), ty: PyType::Named("Pattern".to_string()), default: None },
                PyField { name: "body".to_string(), ty: stmt_list.clone(), default: Some("field(default_factory=list)".to_string()) },
            ],
        },
        // --- Pattern (tagged with "type") ---
        PyDef::TaggedUnion {
            name: "Pattern".to_string(),
            tag_field: "type".to_string(),
            variants: vec![
                PyVariant {
                    name: "LiteralPattern".to_string(),
                    tag: Some("Literal".to_string()),
                    fields: vec![
                        PyField { name: "value".to_string(), ty: expr.clone(), default: None },
                    ],
                },
                PyVariant {
                    name: "IdentifierPattern".to_string(),
                    tag: Some("Identifier".to_string()),
                    fields: vec![
                        PyField { name: "name".to_string(), ty: PyType::Str, default: None },
                    ],
                },
                PyVariant {
                    name: "StructPattern".to_string(),
                    tag: Some("Struct".to_string()),
                    fields: vec![
                        PyField { name: "name".to_string(), ty: PyType::Str, default: None },
                        PyField { name: "fields".to_string(), ty: PyType::List(Box::new(PyType::Tuple(vec![PyType::Str, PyType::Named("Pattern".to_string())]))), default: Some("field(default_factory=list)".to_string()) },
                    ],
                },
                PyVariant {
                    name: "WildcardPattern".to_string(),
                    tag: Some("Wildcard".to_string()),
                    fields: vec![],
                },
            ],
        },
        // --- ImportStatement (tagged with "type") ---
        PyDef::TaggedUnion {
            name: "ImportStatement".to_string(),
            tag_field: "type".to_string(),
            variants: vec![
                PyVariant {
                    name: "CrushModule".to_string(),
                    tag: Some("CrushModule".to_string()),
                    fields: vec![
                        PyField { name: "module_path".to_string(), ty: PyType::Str, default: None },
                        PyField { name: "alias".to_string(), ty: str_opt.clone(), default: Some("None".to_string()) },
                        PyField { name: "selective".to_string(), ty: PyType::List(Box::new(PyType::Str)), default: Some("field(default_factory=list)".to_string()) },
                    ],
                },
                PyVariant {
                    name: "PolyglotModule".to_string(),
                    tag: Some("PolyglotModule".to_string()),
                    fields: vec![
                        PyField { name: "language".to_string(), ty: PyType::Str, default: None },
                        PyField { name: "module_path".to_string(), ty: PyType::Str, default: None },
                        PyField { name: "alias".to_string(), ty: str_opt.clone(), default: Some("None".to_string()) },
                        PyField { name: "selective".to_string(), ty: PyType::List(Box::new(PyType::Str)), default: Some("field(default_factory=list)".to_string()) },
                    ],
                },
                PyVariant {
                    name: "MCPImport".to_string(),
                    tag: Some("MCPImport".to_string()),
                    fields: vec![
                        PyField { name: "server_url".to_string(), ty: PyType::Str, default: None },
                        PyField { name: "tools".to_string(), ty: PyType::List(Box::new(PyType::Str)), default: Some("field(default_factory=list)".to_string()) },
                        PyField { name: "alias".to_string(), ty: str_opt.clone(), default: Some("None".to_string()) },
                    ],
                },
                PyVariant {
                    name: "CapabilityImport".to_string(),
                    tag: Some("Capability".to_string()),
                    fields: vec![
                        PyField { name: "capability_path".to_string(), ty: PyType::Str, default: None },
                        PyField { name: "permissions".to_string(), ty: PyType::List(Box::new(PyType::Str)), default: Some("field(default_factory=list)".to_string()) },
                        PyField { name: "alias".to_string(), ty: str_opt.clone(), default: Some("None".to_string()) },
                    ],
                },
                PyVariant {
                    name: "ExternalImport".to_string(),
                    tag: Some("External".to_string()),
                    fields: vec![
                        PyField { name: "uri".to_string(), ty: PyType::Str, default: None },
                        PyField { name: "resource_type".to_string(), ty: PyType::Named("ExternalResourceType".to_string()), default: None },
                        PyField { name: "alias".to_string(), ty: str_opt.clone(), default: Some("None".to_string()) },
                    ],
                },
                PyVariant {
                    name: "SecureEnvImport".to_string(),
                    tag: Some("SecureEnv".to_string()),
                    fields: vec![
                        PyField { name: "keys".to_string(), ty: PyType::List(Box::new(PyType::Str)), default: Some("field(default_factory=list)".to_string()) },
                        PyField { name: "alias".to_string(), ty: str_opt.clone(), default: Some("None".to_string()) },
                        PyField { name: "db_path".to_string(), ty: str_opt.clone(), default: Some("None".to_string()) },
                    ],
                },
            ],
        },
        // --- ExternalResourceType (externally tagged) ---
        PyDef::ExternalUnion {
            name: "ExternalResourceType".to_string(),
            variants: vec![
                ExtVariant::Unit("Http".to_string()),
                ExtVariant::Unit("Git".to_string()),
                ExtVariant::Unit("File".to_string()),
                ExtVariant::Unit("Database".to_string()),
                ExtVariant::Struct {
                    rust_name: "API".to_string(),
                    fields: vec![
                        PyField { name: "format".to_string(), ty: PyType::Str, default: None },
                    ],
                },
            ],
        },
        // --- AIExpression (tagged with "ai_type") ---
        PyDef::TaggedUnion {
            name: "AIExpression".to_string(),
            tag_field: "ai_type".to_string(),
            variants: vec![
                PyVariant {
                    name: "Query".to_string(),
                    tag: Some("Query".to_string()),
                    fields: vec![
                        PyField { name: "query".to_string(), ty: PyType::Str, default: None },
                        PyField { name: "result_type".to_string(), ty: str_opt.clone(), default: Some("None".to_string()) },
                        PyField { name: "context".to_string(), ty: any_dict.clone(), default: Some("field(default_factory=dict)".to_string()) },
                    ],
                },
                PyVariant {
                    name: "ToolChain".to_string(),
                    tag: Some("ToolChain".to_string()),
                    fields: vec![
                        PyField { name: "tools".to_string(), ty: PyType::List(Box::new(PyType::Named("ToolCall".to_string()))), default: Some("field(default_factory=list)".to_string()) },
                        PyField { name: "strategy".to_string(), ty: PyType::Named("ExecutionStrategy".to_string()), default: None },
                        PyField { name: "error_handling".to_string(), ty: PyType::Named("ErrorHandling".to_string()), default: None },
                    ],
                },
                PyVariant {
                    name: "AgentDelegation".to_string(),
                    tag: Some("AgentDelegation".to_string()),
                    fields: vec![
                        PyField { name: "task".to_string(), ty: PyType::Str, default: None },
                        PyField { name: "agents".to_string(), ty: PyType::List(Box::new(PyType::Str)), default: Some("field(default_factory=list)".to_string()) },
                        PyField { name: "delegation_strategy".to_string(), ty: PyType::Named("DelegationStrategy".to_string()), default: None },
                        PyField { name: "expected_format".to_string(), ty: str_opt.clone(), default: Some("None".to_string()) },
                    ],
                },
                PyVariant {
                    name: "LearningLoop".to_string(),
                    tag: Some("LearningLoop".to_string()),
                    fields: vec![
                        PyField { name: "learning_target".to_string(), ty: PyType::Named("LearningTarget".to_string()), default: None },
                        PyField { name: "strategy".to_string(), ty: PyType::Named("LearningStrategy".to_string()), default: None },
                        PyField { name: "adaptations".to_string(), ty: PyType::List(Box::new(PyType::Named("AdaptationAction".to_string()))), default: Some("field(default_factory=list)".to_string()) },
                    ],
                },
                PyVariant {
                    name: "ContextAware".to_string(),
                    tag: Some("ContextAware".to_string()),
                    fields: vec![
                        PyField { name: "expression".to_string(), ty: expr.clone(), default: None },
                        PyField { name: "requires_context".to_string(), ty: PyType::List(Box::new(PyType::Str)), default: Some("field(default_factory=list)".to_string()) },
                        PyField { name: "provides_context".to_string(), ty: PyType::List(Box::new(PyType::Str)), default: Some("field(default_factory=list)".to_string()) },
                    ],
                },
            ],
        },
        // --- AIStatement (tagged with "ai_type") ---
        PyDef::TaggedUnion {
            name: "AIStatement".to_string(),
            tag_field: "ai_type".to_string(),
            variants: vec![
                PyVariant {
                    name: "GoalDeclaration".to_string(),
                    tag: Some("GoalDeclaration".to_string()),
                    fields: vec![
                        PyField { name: "goal_id".to_string(), ty: PyType::Str, default: None },
                        PyField { name: "description".to_string(), ty: PyType::Str, default: None },
                        PyField { name: "success_criteria".to_string(), ty: PyType::List(Box::new(PyType::Str)), default: Some("field(default_factory=list)".to_string()) },
                        PyField { name: "priority".to_string(), ty: PyType::Named("Priority".to_string()), default: None },
                        PyField { name: "deadline".to_string(), ty: str_opt.clone(), default: Some("None".to_string()) },
                    ],
                },
                PyVariant {
                    name: "ProgressUpdate".to_string(),
                    tag: Some("ProgressUpdate".to_string()),
                    fields: vec![
                        PyField { name: "goal_id".to_string(), ty: PyType::Str, default: None },
                        PyField { name: "progress".to_string(), ty: PyType::Float, default: None },
                        PyField { name: "status_message".to_string(), ty: PyType::Str, default: None },
                        PyField { name: "metrics".to_string(), ty: PyType::Dict { value: Box::new(PyType::Float) }, default: Some("field(default_factory=dict)".to_string()) },
                    ],
                },
                PyVariant {
                    name: "KnowledgeSharing".to_string(),
                    tag: Some("KnowledgeSharing".to_string()),
                    fields: vec![
                        PyField { name: "knowledge_type".to_string(), ty: PyType::Named("KnowledgeType".to_string()), default: None },
                        PyField { name: "content".to_string(), ty: PyType::Any, default: None },
                        PyField { name: "recipients".to_string(), ty: PyType::List(Box::new(PyType::Str)), default: Some("field(default_factory=list)".to_string()) },
                        PyField { name: "retention_policy".to_string(), ty: PyType::Named("RetentionPolicy".to_string()), default: None },
                    ],
                },
                PyVariant {
                    name: "CapabilityDiscovery".to_string(),
                    tag: Some("CapabilityDiscovery".to_string()),
                    fields: vec![
                        PyField { name: "domain".to_string(), ty: PyType::Str, default: None },
                        PyField { name: "requirements".to_string(), ty: PyType::List(Box::new(PyType::Str)), default: Some("field(default_factory=list)".to_string()) },
                        PyField { name: "discovery_strategy".to_string(), ty: PyType::Named("DiscoveryStrategy".to_string()), default: None },
                    ],
                },
                PyVariant {
                    name: "AdaptationRequest".to_string(),
                    tag: Some("AdaptationRequest".to_string()),
                    fields: vec![
                        PyField { name: "adaptation_type".to_string(), ty: PyType::Named("AdaptationType".to_string()), default: None },
                        PyField { name: "reason".to_string(), ty: PyType::Str, default: None },
                        PyField { name: "parameters".to_string(), ty: any_dict.clone(), default: Some("field(default_factory=dict)".to_string()) },
                    ],
                },
            ],
        },
        // --- AIMetadata ---
        PyDef::Struct {
            name: "AIMetadata".to_string(),
            fields: vec![
                PyField { name: "description".to_string(), ty: PyType::Str, default: None },
                PyField { name: "ai_tags".to_string(), ty: PyType::List(Box::new(PyType::Str)), default: Some("field(default_factory=list)".to_string()) },
                PyField { name: "required_capabilities".to_string(), ty: PyType::List(Box::new(PyType::Str)), default: Some("field(default_factory=list)".to_string()) },
                PyField { name: "execution_context".to_string(), ty: PyType::Named("ExecutionContext".to_string()), default: None },
                PyField { name: "learning_objectives".to_string(), ty: PyType::List(Box::new(PyType::Str)), default: Some("field(default_factory=list)".to_string()) },
                PyField { name: "collaboration_patterns".to_string(), ty: PyType::List(Box::new(PyType::Named("CollaborationPattern".to_string()))), default: Some("field(default_factory=list)".to_string()) },
                PyField { name: "inputs".to_string(), ty: PyType::List(Box::new(PyType::Named("ParameterSchema".to_string()))), default: Some("field(default_factory=list)".to_string()) },
                PyField { name: "outputs".to_string(), ty: PyType::List(Box::new(PyType::Named("ParameterSchema".to_string()))), default: Some("field(default_factory=list)".to_string()) },
                PyField { name: "complexity".to_string(), ty: PyType::Int, default: Some("0".to_string()) },
            ],
        },
        // --- ToolCall ---
        PyDef::Struct {
            name: "ToolCall".to_string(),
            fields: vec![
                PyField { name: "tool_name".to_string(), ty: PyType::Str, default: None },
                PyField { name: "parameters".to_string(), ty: any_dict.clone(), default: Some("field(default_factory=dict)".to_string()) },
                PyField { name: "result_binding".to_string(), ty: str_opt.clone(), default: Some("None".to_string()) },
                PyField { name: "condition".to_string(), ty: str_opt.clone(), default: Some("None".to_string()) },
            ],
        },
        // --- ExecutionStrategy (tagged with "type") ---
        PyDef::TaggedUnion {
            name: "ExecutionStrategy".to_string(),
            tag_field: "type".to_string(),
            variants: vec![
                PyVariant { name: "Sequential".to_string(), tag: Some("Sequential".to_string()), fields: vec![] },
                PyVariant { name: "Parallel".to_string(), tag: Some("Parallel".to_string()), fields: vec![] },
                PyVariant {
                    name: "Conditional".to_string(),
                    tag: Some("Conditional".to_string()),
                    fields: vec![
                        PyField { name: "conditions".to_string(), ty: PyType::List(Box::new(PyType::Str)), default: Some("field(default_factory=list)".to_string()) },
                    ],
                },
                PyVariant {
                    name: "RetryStrategy".to_string(),
                    tag: Some("Retry".to_string()),
                    fields: vec![
                        PyField { name: "max_attempts".to_string(), ty: PyType::Int, default: None },
                        PyField { name: "backoff_strategy".to_string(), ty: PyType::Named("BackoffStrategy".to_string()), default: None },
                    ],
                },
            ],
        },
        // --- ErrorHandling (tagged with "type") ---
        PyDef::TaggedUnion {
            name: "ErrorHandling".to_string(),
            tag_field: "type".to_string(),
            variants: vec![
                PyVariant { name: "FailFast".to_string(), tag: Some("FailFast".to_string()), fields: vec![] },
                PyVariant { name: "ContinueOnError".to_string(), tag: Some("ContinueOnError".to_string()), fields: vec![] },
                PyVariant {
                    name: "RetryError".to_string(),
                    tag: Some("Retry".to_string()),
                    fields: vec![
                        PyField { name: "max_retries".to_string(), ty: PyType::Int, default: None },
                        PyField { name: "retry_condition".to_string(), ty: str_opt.clone(), default: Some("None".to_string()) },
                    ],
                },
                PyVariant {
                    name: "Fallback".to_string(),
                    tag: Some("Fallback".to_string()),
                    fields: vec![
                        PyField { name: "fallback_tools".to_string(), ty: PyType::List(Box::new(PyType::Named("ToolCall".to_string()))), default: Some("field(default_factory=list)".to_string()) },
                    ],
                },
            ],
        },
        // --- BackoffStrategy (tagged with "type") ---
        PyDef::TaggedUnion {
            name: "BackoffStrategy".to_string(),
            tag_field: "type".to_string(),
            variants: vec![
                PyVariant {
                    name: "FixedBackoff".to_string(),
                    tag: Some("Fixed".to_string()),
                    fields: vec![
                        PyField { name: "delay_ms".to_string(), ty: PyType::Int, default: None },
                    ],
                },
                PyVariant {
                    name: "ExponentialBackoff".to_string(),
                    tag: Some("Exponential".to_string()),
                    fields: vec![
                        PyField { name: "base_delay_ms".to_string(), ty: PyType::Int, default: None },
                        PyField { name: "max_delay_ms".to_string(), ty: PyType::Int, default: None },
                    ],
                },
                PyVariant {
                    name: "LinearBackoff".to_string(),
                    tag: Some("Linear".to_string()),
                    fields: vec![
                        PyField { name: "increment_ms".to_string(), ty: PyType::Int, default: None },
                    ],
                },
            ],
        },
        // --- DelegationStrategy (externally tagged) ---
        PyDef::ExternalUnion {
            name: "DelegationStrategy".to_string(),
            variants: vec![
                ExtVariant::Unit("FirstAvailable".to_string()),
                ExtVariant::Unit("CapabilityMatch".to_string()),
                ExtVariant::Unit("ParallelSplit".to_string()),
                ExtVariant::Unit("Hierarchical".to_string()),
                ExtVariant::Struct {
                    rust_name: "Consensus".to_string(),
                    fields: vec![
                        PyField { name: "threshold".to_string(), ty: PyType::Float, default: None },
                    ],
                },
                ExtVariant::Unit("Broadcast".to_string()),
                ExtVariant::Unit("Best".to_string()),
                ExtVariant::Unit("RoundRobin".to_string()),
            ],
        },
        // --- Priority (simple unit enum → string literals) ---
        PyDef::Alias {
            name: "Priority".to_string(),
            target: PyType::Literal(vec![
                "Low".to_string(),
                "Medium".to_string(),
                "High".to_string(),
                "Critical".to_string(),
            ]),
        },
        // --- KnowledgeType ---
        PyDef::Alias {
            name: "KnowledgeType".to_string(),
            target: PyType::Literal(vec![
                "Pattern".to_string(),
                "Solution".to_string(),
                "BestPractice".to_string(),
                "Warning".to_string(),
                "Insight".to_string(),
            ]),
        },
        // --- RetentionPolicy (externally tagged) ---
        PyDef::ExternalUnion {
            name: "RetentionPolicy".to_string(),
            variants: vec![
                ExtVariant::Unit("Ephemeral".to_string()),
                ExtVariant::Unit("Session".to_string()),
                ExtVariant::Unit("Persistent".to_string()),
                ExtVariant::Struct {
                    rust_name: "Conditional".to_string(),
                    fields: vec![
                        PyField { name: "condition".to_string(), ty: PyType::Str, default: None },
                    ],
                },
            ],
        },
        // --- DiscoveryStrategy ---
        PyDef::Alias {
            name: "DiscoveryStrategy".to_string(),
            target: PyType::Literal(vec![
                "Broadcast".to_string(),
                "Targeted".to_string(),
                "Hierarchical".to_string(),
                "LearningBased".to_string(),
            ]),
        },
        // --- AdaptationType ---
        PyDef::Alias {
            name: "AdaptationType".to_string(),
            target: PyType::Literal(vec![
                "Performance".to_string(),
                "Reliability".to_string(),
                "Usability".to_string(),
                "Compatibility".to_string(),
                "Learning".to_string(),
            ]),
        },
        // --- LearningTarget ---
        PyDef::Alias {
            name: "LearningTarget".to_string(),
            target: PyType::Literal(vec![
                "UserBehavior".to_string(),
                "ExecutionPatterns".to_string(),
                "ErrorPatterns".to_string(),
                "PerformanceMetrics".to_string(),
                "ToolUsage".to_string(),
            ]),
        },
        // --- LearningStrategy ---
        PyDef::Alias {
            name: "LearningStrategy".to_string(),
            target: PyType::Literal(vec![
                "PatternRecognition".to_string(),
                "StatisticalAnalysis".to_string(),
                "MachineLearning".to_string(),
                "RuleBased".to_string(),
            ]),
        },
        // --- AdaptationAction ---
        PyDef::Alias {
            name: "AdaptationAction".to_string(),
            target: PyType::Literal(vec![
                "OptimizeToolChain".to_string(),
                "ImproveErrorHandling".to_string(),
                "UpdateAgentSelection".to_string(),
                "ModifyExecutionStrategy".to_string(),
                "LearnNewPatterns".to_string(),
            ]),
        },
        // --- ParameterSchema ---
        PyDef::Struct {
            name: "ParameterSchema".to_string(),
            fields: vec![
                PyField { name: "name".to_string(), ty: PyType::Str, default: None },
                PyField { name: "description".to_string(), ty: PyType::Str, default: None },
                PyField { name: "type_hint".to_string(), ty: PyType::Str, default: None },
                PyField { name: "required".to_string(), ty: PyType::Bool, default: Some("True".to_string()) },
                PyField { name: "default_value".to_string(), ty: any_opt.clone(), default: Some("None".to_string()) },
            ],
        },
        // --- ToolSchema ---
        PyDef::Struct {
            name: "ToolSchema".to_string(),
            fields: vec![
                PyField { name: "name".to_string(), ty: PyType::Str, default: None },
                PyField { name: "description".to_string(), ty: PyType::Str, default: None },
                PyField { name: "parameters".to_string(), ty: PyType::Dict { value: Box::new(PyType::Named("ParameterSchema".to_string())) }, default: Some("field(default_factory=dict)".to_string()) },
                PyField { name: "return_type".to_string(), ty: PyType::Str, default: None },
                PyField { name: "mcp_server".to_string(), ty: str_opt.clone(), default: Some("None".to_string()) },
                PyField { name: "mcp_method".to_string(), ty: str_opt.clone(), default: Some("None".to_string()) },
            ],
        },
        // --- LearningSource (externally tagged) ---
        PyDef::ExternalUnion {
            name: "LearningSource".to_string(),
            variants: vec![
                ExtVariant::Unit("ExecutionResults".to_string()),
                ExtVariant::Unit("UserFeedback".to_string()),
                ExtVariant::Unit("EnvironmentObservations".to_string()),
                ExtVariant::Struct {
                    rust_name: "PeerAgents".to_string(),
                    fields: vec![
                        PyField { name: "agent_ids".to_string(), ty: PyType::List(Box::new(PyType::Str)), default: Some("field(default_factory=list)".to_string()) },
                    ],
                },
            ],
        },
        // --- AdaptationStrategy ---
        PyDef::Alias {
            name: "AdaptationStrategy".to_string(),
            target: PyType::Literal(vec![
                "PerformanceOptimization".to_string(),
                "Personalization".to_string(),
                "CapabilityExpansion".to_string(),
                "CollaborationEnhancement".to_string(),
            ]),
        },
        // --- ExecutionContext ---
        PyDef::Struct {
            name: "ExecutionContext".to_string(),
            fields: vec![
                PyField { name: "environment".to_string(), ty: PyType::List(Box::new(PyType::Str)), default: Some("field(default_factory=list)".to_string()) },
                PyField { name: "resources".to_string(), ty: PyType::List(Box::new(PyType::Str)), default: Some("field(default_factory=list)".to_string()) },
                PyField { name: "permissions".to_string(), ty: PyType::List(Box::new(PyType::Str)), default: Some("field(default_factory=list)".to_string()) },
                PyField { name: "dependencies".to_string(), ty: PyType::List(Box::new(PyType::Str)), default: Some("field(default_factory=list)".to_string()) },
            ],
        },
        // --- CollaborationPattern ---
        PyDef::Struct {
            name: "CollaborationPattern".to_string(),
            fields: vec![
                PyField { name: "pattern_type".to_string(), ty: PyType::Str, default: None },
                PyField { name: "participants".to_string(), ty: PyType::List(Box::new(PyType::Str)), default: Some("field(default_factory=list)".to_string()) },
                PyField { name: "communication_style".to_string(), ty: PyType::Str, default: None },
                PyField { name: "decision_making".to_string(), ty: PyType::Str, default: None },
            ],
        },
    ]
}

fn main() {
    let defs = schema();

    let mut output = String::new();
    output.push_str("# Generated by export-py from crush-cast Rust types.\n");
    output.push_str("# Do not edit manually — re-run `cargo run -p crush-cast --bin export-py`.\n\n");

    output.push_str("from __future__ import annotations\n\n");
    output.push_str("import dataclasses\n");
    output.push_str("from dataclasses import dataclass, field\n");
    output.push_str("from typing import Any, Dict, List, Optional, Tuple, Union\n");
    output.push_str("try:\n");
    output.push_str("    from typing import Literal\n");
    output.push_str("except ImportError:\n");
    output.push_str("    from typing_extensions import Literal  # type: ignore\n\n");

    for def in &defs {
        output.push_str(&render_pydef(def));
        output.push('\n');
    }

    let crate_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out_path = crate_root.join("python").join("cast_types.py");
    std::fs::create_dir_all(out_path.parent().unwrap()).unwrap();
    std::fs::write(&out_path, output).unwrap();

    println!("Exported Python bindings to {}", out_path.display());
}
