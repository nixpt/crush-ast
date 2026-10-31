use tree_sitter_language::LanguageFn;

extern "C" {
    fn tree_sitter_crush() -> *const ();
}

/// The tree-sitter [`LanguageFn`] for this grammar.
pub const LANGUAGE: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_crush) };

/// The content of the node-types.json file for this grammar.
pub const NODE_TYPES: &str = include_str!("node-types.json");

/// Syntax highlighting query (`queries/highlights.scm`).
pub const HIGHLIGHTS_QUERY: &str = include_str!("../queries/highlights.scm");
/// Indentation query (`queries/indents.scm`).
pub const INDENTS_QUERY: &str = include_str!("../queries/indents.scm");
/// Scope / binding query (`queries/locals.scm`).
pub const LOCALS_QUERY: &str = include_str!("../queries/locals.scm");
/// Language-injection query (`queries/injections.scm`): `@python { … }` etc.
pub const INJECTIONS_QUERY: &str = include_str!("../queries/injections.scm");
