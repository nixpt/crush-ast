module.exports = grammar({
    name: 'crush',

    extras: $ => [
        /\s/,
        $.comment,
    ],

    rules: {
        program: $ => repeat($._statement),

        _statement: $ => choice(
            $.var_decl,
            $.fn_def,
            $.struct_def,
            $.if_statement,
            $.while_statement,
            $.for_statement,
            $.return_statement,
            $.try_statement,
            $.throw_statement,
            $.assignment,
            $.expr_stmt,
            $.lang_block,
            $.annotation,
            $.capability_decl,
            $.semantic_switch_statement,
            $.break_statement,
            $.continue_statement,
            $.import_statement,
        ),

        import_statement: $ => choice(
            $.module_import,
            $.mcp_import,
            $.capability_import,
            $.polyglot_import,
            $.external_import
        ),

        // `prec.right` because `import foo { bar }` is otherwise ambiguous with
        // the `{ … }`-starting expressions added below (object/struct literals).
        module_import: $ => prec.right(2, seq(
            'import',
            field('path', $.import_path),
            optional(seq('as', field('alias', $.identifier))),
            optional(seq('{', sepBy(',', field('selective', $.identifier)), '}')),
            optional(';')
        )),

        import_path: $ => sepBy1('.', $.identifier),

        mcp_import: $ => seq(
            'use',
            '@mcp',
            field('url', $.string_literal),
            optional(seq('{', field('tools', sepBy(',', $.string_literal)), '}')),
            optional(seq('as', field('alias', $.identifier))),
            optional(';')
        ),

        capability_import: $ => seq(
            'use',
            '@cap',
            field('path', $.string_literal),
            optional(seq('{', field('permissions', sepBy(',', $.string_literal)), '}')),
            optional(seq('as', field('alias', $.identifier))),
            optional(';')
        ),

        polyglot_import: $ => seq(
            'use',
            '@lang',
            field('language', $.identifier),
            field('path', $.string_literal),
            optional(seq('{', field('selective', sepBy(',', $.string_literal)), '}')),
            optional(seq('as', field('alias', $.identifier))),
            optional(';')
        ),

        external_import: $ => seq(
            'import',
            field('type', choice('@git', '@http', '@file')),
            field('uri', $.string_literal),
            optional(seq('as', field('alias', $.identifier))),
            optional(';')
        ),

        // The frontend lexer reads `//` as the primary line comment, `#` as an
        // alias for it, and `/* … */` as a block comment (lexer.rs `read_comment`).
        // The grammar previously knew only `#`, so almost every real `.crush`
        // file (which opens with `//`) produced an ERROR node at line 1.
        comment: $ => token(choice(
            /\/\/[^\n]*/,
            /\/\*[^*]*\*+([^/*][^*]*\*+)*\//,
            /#[^\n]*/,
        )),

        var_decl: $ => seq(
            'let',
            field('name', $.identifier),
            '=',
            field('value', $._value_expression),
            optional(';'),
        ),

        fn_def: $ => seq(
            optional('pub'),
            optional('async'),
            'fn',
            field('name', $.identifier),
            field('parameters', $.parameter_list),
            // `fn fib(n: Int) -> Int { … }` — the return type was absent from
            // the grammar entirely (parser/mod.rs `parse_function`).
            optional(seq('->', field('return_type', $.type))),
            field('body', $.block),
        ),

        type: $ => $.identifier,

        lambda: $ => prec(20, seq(
            '|',
            optional(sepBy(',', field('parameters', $.lambda_parameter))),
            '|',
            choice(
                field('body', $.block),
                seq('=>', field('expression', $.expression))
            )
        )),

        lambda_parameter: $ => seq(
            field('name', $.identifier),
            optional(seq(':', field('type', $.identifier)))
        ),

        struct_def: $ => seq(
            'struct',
            field('name', $.identifier),
            '{',
            sepBy(',', field('fields', $.struct_field)),
            '}',
        ),

        struct_field: $ => seq(
            field('name', $.identifier),
            optional(seq(':', field('type', $.identifier)))
        ),

        parameter_list: $ => seq(
            '(',
            sepBy(',', $.lambda_parameter),
            ')',
        ),

        if_statement: $ => seq(
            'if',
            field('condition', $.expression),
            field('consequence', $.block),
            optional(seq(
                'else',
                field('alternative', choice($.block, $.if_statement)),
            )),
        ),

        while_statement: $ => seq(
            'while',
            field('condition', $.expression),
            field('body', $.block),
        ),

        return_statement: $ => seq(
            'return',
            optional(field('value', $._value_expression)),
            optional(';'),
        ),

        for_statement: $ => seq(
            'for',
            field('variable', $.identifier),
            'in',
            field('iterable', $.expression),
            field('body', $.block),
        ),

        throw_statement: $ => seq(
            'throw',
            field('value', $.expression),
            optional(';'),
        ),

        try_statement: $ => seq(
            'try',
            field('body', $.block),
            'catch',
            field('error_var', $.identifier),
            field('handler', $.block),
        ),

        block: $ => seq(
            '{',
            repeat($._statement),
            '}',
        ),

        // `x = 1`, `p.x = 10`, `xs[0] = 9` — one rule for all three targets
        // (parser/mod.rs `parse_expression_statement`). This used to be
        // `field_assignment`, which only accepted `a.b = c`, so plain
        // reassignment was an ERROR in ~a dozen examples.
        assignment: $ => seq(
            field('target', choice(
                $.field_access,
                $.index_expression,
                $.identifier,
            )),
            '=',
            field('value', $._value_expression),
            optional(';'),
        ),

        expr_stmt: $ => seq(
            $.expression,
            optional(';'),
        ),

        expression: $ => choice(
            $.pipeline,
            $._primary_expression,
        ),

        // A `{`-opening literal may only appear where a value is expected —
        // after `=`, `return`, in arguments and array elements. Keeping it out
        // of `expression` proper is what stops a statement from beginning with
        // `{`, which is what made `if x { … }` and `@module { … }` ambiguous.
        _value_expression: $ => choice(
            $.object_literal,
            $.struct_literal,
            $.expression,
        ),

        pipeline: $ => prec.left(1, seq(
            $._primary_expression,
            repeat1(seq('|', $._primary_expression)),
        )),

        _primary_expression: $ => choice(
            $.binary_op,
            $.unary_op,
            $.capability_call,
            $.spawn_expr,
            $.yield_expr,
            $.await_expr,
            $.new_struct,
            $.field_access,
            $.index_expression,
            $.range_expression,
            $.identifier,
            $._literal,
            $.array_literal,
            $.lambda,
            $.match_expression,
            seq('(', $.expression, ')'),
        ),

        binary_op: $ => choice(
            ...[
                ['||', 1], ['&&', 2],
                ['==', 3], ['!=', 3], ['<', 4], ['<=', 4], ['>', 4], ['>=', 4],
                ['+', 5], ['-', 5], ['*', 6], ['/', 6], ['%', 6],
            ].map(([op, precedence]) => prec.left(precedence, seq(
                field('left', $._primary_expression),
                field('operator', op),
                field('right', $._primary_expression),
            ))),
        ),

        // `!ok`, `not ok`, `-x` — the word forms are documented as identical to
        // the punctuation ones (lexer.rs keyword table). `-x` matters: negation
        // of a variable (`if -x == 5`) was an ERROR (arith_stress.crush).
        unary_op: $ => prec(7, seq(
            field('operator', choice('!', 'not', '-')),
            field('operand', $._primary_expression),
        )),

        capability_call: $ => choice(
            // Standard call: @ns.method(args) or method(args). The `@` form is
            // its own alternative so the token binds unambiguously — with
            // `optional('@')` a following binary operator left two parses.
            prec(3, seq(
                '@',
                field('name', choice(
                    $.identifier,
                    $.field_access,
                )),
                field('arguments', $.argument_list),
            )),
            prec(2, seq(
                field('name', choice(
                    $.identifier,
                    $.field_access,
                )),
                field('arguments', $.argument_list),
            )),
            // Sugar call: cmd arg1 arg2
            prec(1, seq(
                field('name', choice(
                    $.identifier,
                    $.field_access,
                )),
                repeat1($._literal),
            )),
        ),

        argument_list: $ => seq(
            '(',
            sepBy(',', choice($.named_argument, $._value_expression)),
            ')',
        ),

        // `ai_synthesize("X", constraints=["polite"])` — the frontend strips a
        // named-argument key and uses only the value (parser/mod.rs
        // `parse_arguments`).
        named_argument: $ => seq(
            field('name', $.identifier),
            '=',
            field('value', $._value_expression),
        ),

        spawn_expr: $ => prec(3, seq(
            'spawn',
            field('function', $.identifier),
            field('arguments', $.argument_list),
        )),

        yield_expr: $ => 'yield',

        await_expr: $ => seq(
            'await',
            field('expression', $._primary_expression)
        ),

        new_struct: $ => prec.right(seq(
            'new',
            field('name', $.identifier),
            optional(field('arguments', $.argument_list)),
        )),

        range_expression: $ => prec.left(1, seq(
            field('start', $._primary_expression),
            '..',
            field('end', $._primary_expression),
        )),

        // `r.origin.x` — a chain, not just one hop (struct_literals.crush).
        field_access: $ => prec.left(10, seq(
            field('target', choice(
                $.field_access,
                $.index_expression,
                $.identifier,
            )),
            '.',
            field('field', $.identifier),
        )),

        lang_block: $ => seq(
            '@',
            field('language', $.identifier),
            '{',
            field('content', $.block_content),
            '}',
        ),

        // `capability system readonly` (parser/mod.rs) — a top-level
        // declaration of a capability the program uses.
        capability_decl: $ => seq(
            'capability',
            field('name', $.identifier),
            field('access', $.identifier),
            optional(';'),
        ),

        // `semantic_switch <expr> { case "concept": … fallback: … }` — a soft
        // keyword, lexed as an identifier (parser/mod.rs
        // `parse_semantic_switch_statement`).
        semantic_switch_statement: $ => seq(
            'semantic_switch',
            field('value', $.expression),
            '{',
            repeat(choice($.semantic_case, $.semantic_fallback)),
            '}',
        ),

        semantic_case: $ => seq(
            'case',
            field('concept', $.string_literal),
            ':',
            repeat($._statement),
        ),

        semantic_fallback: $ => seq(
            'fallback',
            ':',
            repeat($._statement),
        ),

        // The AI-native annotation family: `@module { … }`, `@invariant "id"
        // { … }`, `@decision "id" { … }`, `@errors { … }`, `@wip { … }`. The
        // lexer treats exactly these `@`-names as annotations and every other
        // `@name { … }` as a polyglot block (lexer.rs `IMPORT_KEYWORDS`), so
        // the grammar mirrors that list rather than guessing from shape.
        annotation: $ => seq(
            '@',
            field('name', choice(
                'module', 'invariant',
                'errors', 'reads', 'writes', 'does-not-write',
                'covers', 'relies-on', 'complexity',
                'wip', 'temporary', 'decision',
                'invalidates', 'must-call-before', 'must-call-after',
            )),
            optional(field('id', $.string_literal)),
            field('body', $.annotation_body),
        ),

        annotation_body: $ => seq('{', repeat($.annotation_entry), '}'),

        // Line-oriented `key: value` entries; keys may contain `-`
        // (`expires-when`, `revisit-if`).
        annotation_entry: $ => seq(
            field('key', $.annotation_key),
            optional(seq(':', field('value', $.annotation_value))),
            optional(','),
        ),

        annotation_key: $ => /[a-zA-Z_][a-zA-Z0-9_-]*/,

        annotation_value: $ => choice(
            $.string_literal,
            $.float_literal,
            $.int_literal,
            $.bool_literal,
            $.null_literal,
            $.array_literal,
            $.identifier,
            $.annotation_body,
        ),

        null_literal: $ => 'null',

        // A polyglot body is opaque text, but its braces must balance:
        // `@javascript { const o = {a: 1, b: {c: 2}}; }` ends at the *outer*
        // `}` (polyglot_braces.crush). The old `/[^}]*/` stopped at the first
        // one. Quoted strings containing bare braces are still beyond a
        // context-free grammar (see that example's case 4).
        block_content: $ => repeat1(choice(
            /[^{}]+/,
            seq('{', optional($.block_content), '}'),
        )),

        match_expression: $ => seq(
            'match',
            field('expression', $.expression),
            '{',
            repeat(field('arms', $.match_arm)),
            '}'
        ),

        match_arm: $ => seq(
            field('pattern', $.pattern),
            '=>',
            field('body', choice(
                $.block,
                $.match_arm_expression,
            )),
        ),

        match_arm_expression: $ => seq(
            field('expression', $.expression),
            optional(',')
        ),

        pattern: $ => choice(
            $._literal_pattern,
            field('identifier', $.identifier),
            $.struct_pattern,
            $.wildcard_pattern
        ),

        _literal_pattern: $ => choice(
            $.string_literal,
            $.int_literal,
            $.bool_literal
        ),

        struct_pattern: $ => seq(
            field('name', $.identifier),
            '{',
            sepBy(',', $.struct_field_pattern),
            '}'
        ),

        struct_field_pattern: $ => seq(
            field('name', $.identifier),
            ':',
            field('pattern', $.pattern)
        ),

        wildcard_pattern: $ => '_',

        identifier: $ => /[a-zA-Z_]\w*/,

        array_literal: $ => seq(
            '[',
            sepBy(',', $._value_expression),
            optional(','),
            ']',
        ),

        // `{ "k": 1, name: v }` — object/map literal (parser/mod.rs
        // `parse_object_literal`).
        object_literal: $ => seq(
            '{',
            sepBy(',', $.object_property),
            optional(','),
            '}',
        ),

        object_property: $ => seq(
            field('key', choice($.string_literal, $.identifier)),
            ':',
            field('value', $._value_expression),
        ),

        // `Point { x: 2, y: 3 }` and `new Point { x: 7, y: 8 }`
        // (struct_literals.crush) — struct-literal construction.
        struct_literal: $ => prec(5, seq(
            optional('new'),
            field('name', $.identifier),
            '{',
            sepBy(',', $.struct_field_init),
            optional(','),
            '}',
        )),

        struct_field_init: $ => seq(
            field('name', $.identifier),
            ':',
            field('value', $._value_expression),
        ),

        index_expression: $ => prec(11, seq(
            field('target', $._primary_expression),
            '[',
            choice(field('index', $.expression), field('slice', $.slice)),
            ']',
        )),

        // `[start:stop:step]` with any bound omitted — `s[0:5]`, `s[-5:]`,
        // `xs[:]`, `u[::-1]`, as crush-frontend's parser reads them (bracket
        // slices desugar to seq.slice). `::` lexes as two `:` here.
        slice: $ => seq(
            optional(field('start', $.expression)),
            ':',
            optional(field('stop', $.expression)),
            optional(seq(':', optional(field('step', $.expression)))),
        ),

        _literal: $ => choice(
            $.int_literal,
            $.float_literal,
            $.string_literal,
            $.bool_literal,
            $.null_literal,
        ),

        // Decimal, plus the hex (0x/0X) and binary (0b/0B) forms crush-frontend's lexer reads.
        int_literal: $ => token(choice(/-?0[xX][0-9a-fA-F]+/, /-?0[bB][01]+/, /-?\d+/)),
        float_literal: $ => /-?\d+\.\d+/,
        // Escapes matter: `"a\"b"` and `"a\nb"` are ordinary in the corpus, and
        // the old `/[^"]*/` stopped at the escaped quote (CRUSH-207).
        string_literal: $ => choice(
            token(seq('"', /([^"\\]|\\.)*/, '"')),
            token(seq("'", /([^'\\]|\\.)*/, "'")),
        ),
        bool_literal: $ => choice('true', 'false'),

        break_statement: $ => seq('break', optional(';')),
        continue_statement: $ => seq('continue', optional(';')),
    },

    conflicts: $ => [
        [$.new_struct],
        [$.return_statement],
    ],
});

function sepBy(sep, rule) {
    return optional(seq(rule, repeat(seq(sep, rule))));
}

function sepBy1(sep, rule) {
    return seq(rule, repeat(seq(sep, rule)));
}
