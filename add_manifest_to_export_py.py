import sys

def insert_manifest_types():
    with open('crates/crush-cast/src/bin/export-py.rs', 'r') as f:
        content = f.read()

    manifest_types = """
        // --- ErrorLikelihood ---
        PyDef::Enum {
            name: "ErrorLikelihood".to_string(),
            variants: vec![
                PyVariant { name: "Likely".to_string(), inner: None },
                PyVariant { name: "Possible".to_string(), inner: None },
                PyVariant { name: "Rare".to_string(), inner: None },
            ],
        },
        // --- WeightedError ---
        PyDef::Struct {
            name: "WeightedError".to_string(),
            fields: vec![
                PyField {
                    name: "variant".to_string(),
                    ty: PyType::Str,
                    default: None,
                },
                PyField {
                    name: "likelihood".to_string(),
                    ty: PyType::Named("ErrorLikelihood".to_string()),
                    default: None,
                },
            ],
        },
        // --- FunctionAnnotations ---
        PyDef::Struct {
            name: "FunctionAnnotations".to_string(),
            fields: vec![
                PyField {
                    name: "errors".to_string(),
                    ty: PyType::List(Box::new(PyType::Str)),
                    default: Some("field(default_factory=list)".to_string()),
                },
                PyField {
                    name: "errors_weighted".to_string(),
                    ty: PyType::List(Box::new(PyType::Named("WeightedError".to_string()))),
                    default: Some("field(default_factory=list)".to_string()),
                },
                PyField {
                    name: "reads".to_string(),
                    ty: PyType::List(Box::new(PyType::Str)),
                    default: Some("field(default_factory=list)".to_string()),
                },
                PyField {
                    name: "writes".to_string(),
                    ty: PyType::List(Box::new(PyType::Str)),
                    default: Some("field(default_factory=list)".to_string()),
                },
                PyField {
                    name: "does_not_write".to_string(),
                    ty: PyType::List(Box::new(PyType::Str)),
                    default: Some("field(default_factory=list)".to_string()),
                },
                PyField {
                    name: "covers".to_string(),
                    ty: PyType::List(Box::new(PyType::Str)),
                    default: Some("field(default_factory=list)".to_string()),
                },
                PyField {
                    name: "relies_on".to_string(),
                    ty: PyType::List(Box::new(PyType::Str)),
                    default: Some("field(default_factory=list)".to_string()),
                },
                PyField {
                    name: "complexity".to_string(),
                    ty: PyType::Opt(Box::new(PyType::Int)),
                    default: Some("None".to_string()),
                },
            ],
        },
        // --- ChangelogEntry ---
        PyDef::Struct {
            name: "ChangelogEntry".to_string(),
            fields: vec![
                PyField {
                    name: "date".to_string(),
                    ty: PyType::Str,
                    default: None,
                },
                PyField {
                    name: "summary".to_string(),
                    ty: PyType::Str,
                    default: None,
                },
            ],
        },
        // --- Invariant ---
        PyDef::Struct {
            name: "Invariant".to_string(),
            fields: vec![
                PyField {
                    name: "name".to_string(),
                    ty: PyType::Str,
                    default: None,
                },
                PyField {
                    name: "description".to_string(),
                    ty: PyType::Str,
                    default: None,
                },
                PyField {
                    name: "applies_to".to_string(),
                    ty: PyType::List(Box::new(PyType::Str)),
                    default: Some("field(default_factory=list)".to_string()),
                },
                PyField {
                    name: "consequence".to_string(),
                    ty: PyType::Opt(Box::new(PyType::Str)),
                    default: Some("None".to_string()),
                },
                PyField {
                    name: "check_source".to_string(),
                    ty: PyType::Opt(Box::new(PyType::Str)),
                    default: Some("None".to_string()),
                },
            ],
        },
        // --- ExhaustiveMatchSite ---
        PyDef::Struct {
            name: "ExhaustiveMatchSite".to_string(),
            fields: vec![
                PyField {
                    name: "file".to_string(),
                    ty: PyType::Str,
                    default: None,
                },
                PyField {
                    name: "line".to_string(),
                    ty: PyType::Int,
                    default: None,
                },
                PyField {
                    name: "match_type".to_string(),
                    ty: PyType::Str,
                    default: None,
                },
            ],
        },
        // --- WipNode ---
        PyDef::Struct {
            name: "WipNode".to_string(),
            fields: vec![
                PyField {
                    name: "intent".to_string(),
                    ty: PyType::Str,
                    default: None,
                },
                PyField {
                    name: "started_by".to_string(),
                    ty: PyType::Opt(Box::new(PyType::Str)),
                    default: Some("None".to_string()),
                },
                PyField {
                    name: "done".to_string(),
                    ty: PyType::List(Box::new(PyType::Str)),
                    default: Some("field(default_factory=list)".to_string()),
                },
                PyField {
                    name: "todo".to_string(),
                    ty: PyType::List(Box::new(PyType::Str)),
                    default: Some("field(default_factory=list)".to_string()),
                },
                PyField {
                    name: "unresolved".to_string(),
                    ty: PyType::List(Box::new(PyType::Str)),
                    default: Some("field(default_factory=list)".to_string()),
                },
            ],
        },
        // --- TemporaryNode ---
        PyDef::Struct {
            name: "TemporaryNode".to_string(),
            fields: vec![
                PyField {
                    name: "reason".to_string(),
                    ty: PyType::Str,
                    default: None,
                },
                PyField {
                    name: "expires_when".to_string(),
                    ty: PyType::Opt(Box::new(PyType::Str)),
                    default: Some("None".to_string()),
                },
                PyField {
                    name: "owner".to_string(),
                    ty: PyType::Opt(Box::new(PyType::Str)),
                    default: Some("None".to_string()),
                },
                PyField {
                    name: "added".to_string(),
                    ty: PyType::Opt(Box::new(PyType::Str)),
                    default: Some("None".to_string()),
                },
            ],
        },
        // --- DecisionNode ---
        PyDef::Struct {
            name: "DecisionNode".to_string(),
            fields: vec![
                PyField {
                    name: "name".to_string(),
                    ty: PyType::Str,
                    default: None,
                },
                PyField {
                    name: "chose".to_string(),
                    ty: PyType::Str,
                    default: None,
                },
                PyField {
                    name: "over".to_string(),
                    ty: PyType::List(Box::new(PyType::Str)),
                    default: Some("field(default_factory=list)".to_string()),
                },
                PyField {
                    name: "because".to_string(),
                    ty: PyType::Str,
                    default: None,
                },
                PyField {
                    name: "revisit_if".to_string(),
                    ty: PyType::List(Box::new(PyType::Str)),
                    default: Some("field(default_factory=list)".to_string()),
                },
            ],
        },
        // --- ModuleManifest ---
        PyDef::Struct {
            name: "ModuleManifest".to_string(),
            fields: vec![
                PyField {
                    name: "purpose".to_string(),
                    ty: PyType::Str,
                    default: None,
                },
                PyField {
                    name: "exports".to_string(),
                    ty: PyType::List(Box::new(PyType::Str)),
                    default: Some("field(default_factory=list)".to_string()),
                },
                PyField {
                    name: "invariants".to_string(),
                    ty: PyType::List(Box::new(PyType::Named("Invariant".to_string()))),
                    default: Some("field(default_factory=list)".to_string()),
                },
                PyField {
                    name: "related".to_string(),
                    ty: PyType::List(Box::new(PyType::Str)),
                    default: Some("field(default_factory=list)".to_string()),
                },
                PyField {
                    name: "exhaustive_types".to_string(),
                    ty: PyType::List(Box::new(PyType::Str)),
                    default: Some("field(default_factory=list)".to_string()),
                },
                PyField {
                    name: "changelog".to_string(),
                    ty: PyType::List(Box::new(PyType::Named("ChangelogEntry".to_string()))),
                    default: Some("field(default_factory=list)".to_string()),
                },
            ],
        },
"""
    
    # Insert new defs before the closing bracket of schema()
    pos = content.rfind("    ]\n}")
    if pos != -1:
        content = content[:pos] + manifest_types + content[pos:]

    # Add missing fields to Program
    program_fields = """
                PyField {
                    name: "manifest".to_string(),
                    ty: PyType::Opt(Box::new(PyType::Named("ModuleManifest".to_string()))),
                    default: Some("None".to_string()),
                },
                PyField {
                    name: "exhaustive_sites".to_string(),
                    ty: PyType::List(Box::new(PyType::Named("ExhaustiveMatchSite".to_string()))),
                    default: Some("field(default_factory=list)".to_string()),
                },
                PyField {
                    name: "wip".to_string(),
                    ty: PyType::Opt(Box::new(PyType::Named("WipNode".to_string()))),
                    default: Some("None".to_string()),
                },
                PyField {
                    name: "temporaries".to_string(),
                    ty: PyType::List(Box::new(PyType::Named("TemporaryNode".to_string()))),
                    default: Some("field(default_factory=list)".to_string()),
                },
                PyField {
                    name: "decisions".to_string(),
                    ty: PyType::List(Box::new(PyType::Named("DecisionNode".to_string()))),
                    default: Some("field(default_factory=list)".to_string()),
                },"""
    
    # Find AIMetadata field in Program and insert after it
    aimeta_pos = content.find("name: \"ai_meta\".to_string()")
    if aimeta_pos != -1:
        # Find the end of the ai_meta PyField struct
        end_aimeta = content.find("},", aimeta_pos) + 2
        content = content[:end_aimeta] + program_fields + content[end_aimeta:]

    # Add annotations to Function
    func_annotations = """
                PyField {
                    name: "annotations".to_string(),
                    ty: PyType::Opt(Box::new(PyType::Named("FunctionAnnotations".to_string()))),
                    default: Some("None".to_string()),
                },"""
    
    body_pos = content.find("name: \"body\".to_string()", aimeta_pos)
    if body_pos != -1:
        # We need to find the end of the Function PyDef, or insert right after body
        # Let's find "name: \"meta\".to_string()" inside Function
        meta_pos = content.find("name: \"meta\".to_string()", body_pos)
        end_meta = content.find("},", meta_pos) + 2
        content = content[:end_meta] + func_annotations + content[end_meta:]

    with open('crates/crush-cast/src/bin/export-py.rs', 'w') as f:
        f.write(content)

insert_manifest_types()
