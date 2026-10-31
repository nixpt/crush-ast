; Crush indent rules (CRUSH-366): 4 spaces, matching polish's formatter.
; Captures follow nvim-treesitter's indent module (@indent.begin / .end / .branch).

[
  (block)
  (annotation_body)
  (object_literal)
  (struct_literal)
  (struct_def)
  (match_expression)
  (array_literal)
  (argument_list)
  (parameter_list)
  (lang_block)
] @indent.begin

[ "}" "]" ")" ] @indent.end @indent.branch

(comment) @indent.ignore
(string_literal) @indent.auto
(block_content) @indent.auto
