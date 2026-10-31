; Crush highlights (CRUSH-366). Capture names follow the nvim-treesitter /
; Helix / Zed common vocabulary.

; ---- comments ---------------------------------------------------------------
(comment) @comment

; ---- literals ---------------------------------------------------------------
(string_literal) @string
(int_literal) @number
(float_literal) @number.float
(bool_literal) @boolean
(null_literal) @constant.builtin
(wildcard_pattern) @variable.builtin

; ---- types ------------------------------------------------------------------
(type) @type
(struct_def name: (identifier) @type)
(struct_field type: (identifier) @type)
(lambda_parameter type: (identifier) @type)
(new_struct name: (identifier) @type)
(struct_literal name: (identifier) @type)
(struct_pattern name: (identifier) @type)

; ---- functions and calls ----------------------------------------------------
(fn_def name: (identifier) @function)

; foo(x) and foo x y
(capability_call
  name: (identifier) @function.call)

; fs.read(...), net.http.get(...): a dotted callee is a capability call
(capability_call
  name: (field_access
    target: (_) @module
    field: (identifier) @function.builtin))

; @fs.read(...): explicit capability sigil
(capability_call
  "@" @attribute)

(yield_expr) @keyword
(spawn_expr function: (identifier) @function.call)
(named_argument name: (identifier) @variable.parameter)

; ---- parameters / fields (plain identifiers stay unstyled: patterns on the
; same node would otherwise be resolved differently by each editor) -------
(lambda_parameter name: (identifier) @variable.parameter)
(struct_field name: (identifier) @property)
(struct_field_init name: (identifier) @property)
(struct_field_pattern name: (identifier) @property)
(object_property key: (identifier) @property)

; ---- imports ----------------------------------------------------------------
(import_path) @module
(module_import alias: (identifier) @module)
(polyglot_import language: (identifier) @attribute)
(external_import type: _ @attribute)
(capability_decl name: (identifier) @module access: (identifier) @constant)

; ---- annotations: @module { … }, @invariant "id" { … } ---------------------
(annotation "@" @attribute)
(annotation name: _ @attribute)
(annotation id: (string_literal) @label)
(annotation_key) @property
(annotation_value (identifier) @constant)

; ---- polyglot blocks: @python { … } ----------------------------------------
(lang_block "@" @attribute)
(lang_block language: (identifier) @attribute)
(block_content) @embedded

; ---- keywords ---------------------------------------------------------------
[
  "fn" "let" "struct" "capability" "pub" "async"
] @keyword

[
  "import" "use" "as"
] @keyword.import

[
  "if" "else" "match" "case" "fallback"
] @keyword.conditional

[
  "while" "for" "in" "break" "continue"
] @keyword.repeat

[
  "return"
] @keyword.return

[
  "try" "catch" "throw"
] @keyword.exception

[
  "spawn" "await" "new"
] @keyword

[
  "not"
] @keyword.operator

; ---- operators and punctuation ---------------------------------------------
[
  "||" "&&" "==" "!=" "<" "<=" ">" ">=" "+" "-" "*" "/" "%" "!"
  "=" "|" ".." "->" "=>"
] @operator

[ "(" ")" "[" "]" "{" "}" ] @punctuation.bracket
[ "," ";" ":" "." ] @punctuation.delimiter
