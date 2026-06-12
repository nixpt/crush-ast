use tree_sitter_language::LanguageFn;

extern "C" {
    fn tree_sitter_crush() -> *const ();
}

/// The tree-sitter [`LanguageFn`] for this grammar.
pub const LANGUAGE: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_crush) };

/// The content of the node-types.json file for this grammar.
pub const NODE_TYPES: &str = include_str!("node-types.json");
