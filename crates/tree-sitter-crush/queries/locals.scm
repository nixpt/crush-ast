; Crush scopes and bindings (CRUSH-366), nvim-treesitter locals vocabulary.

; scopes
(program) @local.scope
(fn_def) @local.scope
(lambda) @local.scope
(block) @local.scope
(match_arm) @local.scope
(for_statement) @local.scope

; definitions
(fn_def name: (identifier) @local.definition.function)
(var_decl name: (identifier) @local.definition.var)
(lambda_parameter name: (identifier) @local.definition.parameter)
(for_statement variable: (identifier) @local.definition.var)
(struct_def name: (identifier) @local.definition.type)
(module_import alias: (identifier) @local.definition.import)
(try_statement error_var: (identifier) @local.definition.var)
(pattern identifier: (identifier) @local.definition.var)

; references
(identifier) @local.reference
