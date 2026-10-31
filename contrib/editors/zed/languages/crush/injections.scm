; Crush injections (CRUSH-366): `@python { … }` etc. embed another language.
; The body is a `block_content` node; the language name is the identifier
; after the `@`.

((lang_block
   language: (identifier) @_lang
   content: (block_content) @injection.content)
 (#any-of? @_lang "python" "py")
 (#set! injection.language "python")
 (#set! injection.include-children))

((lang_block
   language: (identifier) @_lang
   content: (block_content) @injection.content)
 (#any-of? @_lang "javascript" "js" "node")
 (#set! injection.language "javascript")
 (#set! injection.include-children))

((lang_block
   language: (identifier) @_lang
   content: (block_content) @injection.content)
 (#any-of? @_lang "bash" "sh" "shell")
 (#set! injection.language "bash")
 (#set! injection.include-children))

; Every other `@lang { … }` falls back to the language named in the source,
; so `@ruby { … }` / `@lua { … }` work wherever that grammar is installed.
((lang_block
   language: (identifier) @injection.language
   content: (block_content) @injection.content)
 (#not-any-of? @injection.language "python" "py" "javascript" "js" "node" "bash" "sh" "shell")
 (#set! injection.include-children))
