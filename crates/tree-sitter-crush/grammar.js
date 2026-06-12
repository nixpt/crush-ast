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
            $.field_assignment,
            $.expr_stmt,
            $.lang_block,
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

        module_import: $ => seq(
            'import',
            field('path', $.import_path),
            optional(seq('as', field('alias', $.identifier))),
            optional(seq('{', sepBy(',', field('selective', $.identifier)), '}')),
            optional(';')
        ),

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

        comment: $ => /#.*/,

        var_decl: $ => seq(
            'let',
            field('name', $.identifier),
            '=',
            field('value', $.expression),
            optional(';'),
        ),

        fn_def: $ => seq(
            'fn',
            field('name', $.identifier),
            field('parameters', $.parameter_list),
            field('body', $.block),
        ),

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
            optional(field('value', $.expression)),
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

        field_assignment: $ => seq(
            field('target', $.field_access),
            '=',
            field('value', $.expression),
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

        pipeline: $ => prec.left(1, seq(
            $._primary_expression,
            repeat1(seq('|', $._primary_expression)),
        )),

        _primary_expression: $ => choice(
            $.binary_op,
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
                ['==', 1], ['!=', 1], ['<', 2], ['<=', 2], ['>', 2], ['>=', 2],
                ['+', 3], ['-', 3], ['*', 4], ['/', 4], ['%', 4],
            ].map(([op, precedence]) => prec.left(precedence, seq(
                field('left', $._primary_expression),
                field('operator', op),
                field('right', $._primary_expression),
            ))),
        ),

        capability_call: $ => choice(
            // Standard call: @ns.method(args) or method(args)
            prec(2, seq(
                optional('@'),
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
            sepBy(',', $.expression),
            ')',
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

        field_access: $ => prec.left(10, seq(
            field('target', $.identifier),
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

        block_content: $ => /[^}]*/,

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
            sepBy(',', $.expression),
            optional(','),
            ']',
        ),

        index_expression: $ => prec(11, seq(
            field('target', $._primary_expression),
            '[',
            field('index', $.expression),
            ']',
        )),

        _literal: $ => choice(
            $.int_literal,
            $.float_literal,
            $.string_literal,
            $.bool_literal,
        ),

        int_literal: $ => /-?\d+/,
        float_literal: $ => /-?\d+\.\d+/,
        string_literal: $ => choice(
            seq('"', /[^"]*/, '"'),
            seq("'", /[^']*/, "'"),
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
