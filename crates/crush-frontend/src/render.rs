use crush_cast::manifest::{FunctionAnnotations, Invariant, ModuleManifest};
use crush_cast::*;

pub fn render_program(program: &Program) -> String {
    let mut renderer = Renderer::new();
    renderer.render_program(program);
    renderer.output
}

/// Render a single named function (with its annotations) back to Crush source.
///
/// Useful for surgical reads: instead of reading a full 1000-line file, an
/// agent calls `extract_symbol(source, "fn_name")` and gets back just the
/// annotated function source.
pub fn render_function_standalone(name: &str, func: &Function) -> String {
    let mut renderer = Renderer::new();
    if let Some(ann) = &func.annotations {
        renderer.render_fn_annotations(ann);
    }
    renderer.render_function_header(name, func);
    renderer.output
}

/// Render a `@module { ... }` block from a `ModuleManifest`.
pub fn render_module_manifest(manifest: &ModuleManifest) -> String {
    let mut renderer = Renderer::new();
    renderer.render_manifest(manifest);
    renderer.output
}

/// Render a single `@invariant "name" { ... }` block.
pub fn render_invariant(inv: &Invariant) -> String {
    let mut renderer = Renderer::new();
    renderer.render_single_invariant(inv);
    renderer.output
}

struct Renderer {
    output: String,
    indent: usize,
}

impl Renderer {
    fn new() -> Self {
        Self {
            output: String::new(),
            indent: 0,
        }
    }

    fn write_indent(&mut self) {
        for _ in 0..self.indent {
            self.output.push_str("    ");
        }
    }

    fn push_str(&mut self, s: &str) {
        self.output.push_str(s);
    }

    fn newline(&mut self) {
        self.output.push('\n');
    }

    fn render_program(&mut self, program: &Program) {
        let mut names: Vec<&String> = program.functions.keys().collect();
        names.sort();

        let mut first = true;
        for name in &names {
            if name.as_str() == "main" {
                continue;
            }
            if !first {
                self.newline();
            }
            first = false;
            let func = program.functions.get(*name).unwrap();
            self.render_function_header(name, func);
        }

        if let Some(main) = program.functions.get("main")
            && !main.body.is_empty()
        {
            if !first {
                self.newline();
            }
            for stmt in &main.body {
                self.render_statement(stmt);
            }
        }
    }

    fn render_function_header(&mut self, name: &str, func: &Function) {
        self.write_indent();
        self.push_str("fn ");
        self.push_str(name);
        self.push_str("(");
        for (i, (p_name, p_type)) in func.params.iter().enumerate() {
            if i > 0 {
                self.push_str(", ");
            }
            self.push_str(p_name);
            if *p_type != CastType::Any {
                self.push_str(": ");
                self.push_str(&p_type.to_string());
            }
        }
        self.push_str(") {\n");
        self.indent += 1;
        for stmt in &func.body {
            self.render_statement(stmt);
        }
        self.indent -= 1;
        self.write_indent();
        self.push_str("}\n");
    }

    fn render_statement(&mut self, stmt: &Statement) {
        match stmt {
            Statement::VarDecl {
                name,
                value,
                type_hint,
                ..
            } => {
                self.write_indent();
                self.push_str("let ");
                self.push_str(name);
                if *type_hint != CastType::Any {
                    self.push_str(": ");
                    self.push_str(&type_hint.to_string());
                }
                self.push_str(" = ");
                self.render_expression(value, 0);
                self.newline();
            }
            Statement::Assign { target, value, .. } => {
                self.write_indent();
                self.push_str(target);
                self.push_str(" = ");
                self.render_expression(value, 0);
                self.newline();
            }
            Statement::Assign { target, value, .. } => {
                self.write_indent();
                self.push_str(target);
                self.push_str(" = ");
                self.render_expression(value, 0);
                self.newline();
            }
            Statement::Export { name, value, .. } => {
                self.write_indent();
                self.push_str("export ");
                self.push_str(name);
                match value {
                    Expression::Var { name: var_name, .. } if var_name == name => {}
                    _ => {
                        self.push_str(" = ");
                        self.render_expression(value, 0);
                    }
                }
                self.newline();
            }
            Statement::ExprStmt { expr, .. } => {
                self.write_indent();
                self.render_expression(expr, 0);
                self.newline();
            }
            Statement::If {
                condition,
                then_body,
                else_body,
                ..
            } => {
                self.write_indent();
                self.push_str("if ");
                self.render_expression(condition, 0);
                self.push_str(" {\n");
                self.indent += 1;
                for s in then_body {
                    self.render_statement(s);
                }
                self.indent -= 1;
                self.write_indent();
                self.push_str("}");
                if let Some(else_stmts) = else_body {
                    if else_stmts.len() == 1 && matches!(else_stmts[0], Statement::If { .. }) {
                        self.push_str(" else ");
                        self.render_else_if(&else_stmts[0]);
                    } else {
                        self.push_str(" else {\n");
                        self.indent += 1;
                        for s in else_stmts {
                            self.render_statement(s);
                        }
                        self.indent -= 1;
                        self.write_indent();
                        self.push_str("}");
                    }
                }
                self.newline();
            }
            Statement::While {
                condition, body, ..
            } => {
                self.write_indent();
                self.push_str("while ");
                self.render_expression(condition, 0);
                self.push_str(" {\n");
                self.indent += 1;
                for s in body {
                    self.render_statement(s);
                }
                self.indent -= 1;
                self.write_indent();
                self.push_str("}\n");
            }
            Statement::For {
                variable,
                iterable,
                body,
                ..
            } => {
                self.write_indent();
                self.push_str("for ");
                self.push_str(variable);
                self.push_str(" in ");
                self.render_expression(iterable, 0);
                self.push_str(" {\n");
                self.indent += 1;
                for s in body {
                    self.render_statement(s);
                }
                self.indent -= 1;
                self.write_indent();
                self.push_str("}\n");
            }
            Statement::Return { value, .. } => {
                self.write_indent();
                self.push_str("return");
                if let Some(v) = value {
                    self.push_str(" ");
                    self.render_expression(v, 0);
                }
                self.newline();
            }
            Statement::TryCatch {
                body,
                error_var,
                handler,
                ..
            } => {
                self.write_indent();
                self.push_str("try {\n");
                self.indent += 1;
                for s in body {
                    self.render_statement(s);
                }
                self.indent -= 1;
                self.write_indent();
                self.push_str("} catch ");
                self.push_str(error_var);
                self.push_str(" {\n");
                self.indent += 1;
                for s in handler {
                    self.render_statement(s);
                }
                self.indent -= 1;
                self.write_indent();
                self.push_str("}\n");
            }
            Statement::Throw { value, .. } => {
                self.write_indent();
                self.push_str("throw ");
                self.render_expression(value, 0);
                self.newline();
            }
            Statement::FunctionDef {
                name, params, body, ..
            } => {
                let func = Function {
                    params: params.clone(),
                    body: body.clone(),
                    meta: std::collections::HashMap::new(),
                    ..Default::default()
                };
                self.render_function_header(name, &func);
            }
            Statement::SetField {
                target,
                field,
                value,
                ..
            } => {
                self.write_indent();
                self.push_str("# NOTE: SetField is not parseable from text\n");
                self.write_indent();
                self.render_expression(target, 0);
                self.push_str(".");
                self.push_str(field);
                self.push_str(" = ");
                self.render_expression(value, 0);
                self.newline();
            }
            Statement::LangBlock {
                lang,
                code,
                variables,
                imports,
                ..
            } => {
                self.write_indent();
                self.push_str("lang \"");
                self.push_str(lang);
                self.push_str("\" {\n");
                self.indent += 1;
                for imp in imports {
                    self.write_indent();
                    self.push_str("# import: ");
                    self.render_import_inline(imp);
                    self.newline();
                }
                if !variables.is_empty() {
                    self.write_indent();
                    self.push_str("# variables: ");
                    self.push_str(&variables.join(", "));
                    self.newline();
                }
                for line in code.lines() {
                    self.write_indent();
                    self.push_str(line);
                    self.newline();
                }
                self.indent -= 1;
                self.write_indent();
                self.push_str("}\n");
            }
            Statement::Import { import, .. } => {
                self.write_indent();
                self.push_str("import ");
                self.render_import_inline(import);
                self.newline();
            }
            Statement::StructDef { name, fields, .. } => {
                self.write_indent();
                self.push_str("struct ");
                self.push_str(name);
                self.push_str(" {");
                for (i, (f_name, f_type)) in fields.iter().enumerate() {
                    if i > 0 {
                        self.push_str(",");
                    }
                    self.push_str(" ");
                    self.push_str(f_name);
                    if *f_type != CastType::Any {
                        self.push_str(": ");
                        self.push_str(&f_type.to_string());
                    }
                }
                if !fields.is_empty() {
                    self.push_str(" ");
                }
                self.push_str("}\n");
            }
            Statement::Break { .. } => {
                self.write_indent();
                self.push_str("break\n");
            }
            Statement::Continue { .. } => {
                self.write_indent();
                self.push_str("continue\n");
            }
            Statement::DomMutate {
                target,
                mutation_type,
                value,
                value2,
                ..
            } => {
                self.write_indent();
                self.push_str("# DOM mutation (not parseable from text)\n");
                self.write_indent();
                self.push_str("dom_mutate(");
                self.render_expression(target, 0);
                self.push_str(", \"");
                self.push_str(&format!("{:?}", mutation_type));
                self.push_str("\"");
                if let Some(v) = value {
                    self.push_str(", ");
                    self.render_expression(v, 0);
                }
                if let Some(v) = value2 {
                    self.push_str(", ");
                    self.render_expression(v, 0);
                }
                self.push_str(")\n");
            }
            Statement::DomEventListener {
                target,
                event,
                callback,
                ..
            } => {
                self.write_indent();
                self.push_str("# DOM event listener (not parseable from text)\n");
                self.write_indent();
                self.push_str("dom_on(");
                self.render_expression(target, 0);
                self.push_str(", \"");
                self.push_str(event);
                self.push_str("\", ");
                self.render_expression(callback, 0);
                self.push_str(")\n");
            }
            Statement::AI(ai_stmt) => {
                self.write_indent();
                self.push_str("# AI-NATIVE: read-only\n");
                self.render_ai_statement(ai_stmt);
            }
        }
    }

    fn render_else_if(&mut self, stmt: &Statement) {
        if let Statement::If {
            condition,
            then_body,
            else_body,
            ..
        } = stmt
        {
            self.push_str("if ");
            self.render_expression(condition, 0);
            self.push_str(" {\n");
            self.indent += 1;
            for s in then_body {
                self.render_statement(s);
            }
            self.indent -= 1;
            self.write_indent();
            self.push_str("}");
            if let Some(else_stmts) = else_body {
                if else_stmts.len() == 1 && matches!(else_stmts[0], Statement::If { .. }) {
                    self.push_str(" else ");
                    self.render_else_if(&else_stmts[0]);
                } else if !else_stmts.is_empty() {
                    self.push_str(" else {\n");
                    self.indent += 1;
                    for s in else_stmts {
                        self.render_statement(s);
                    }
                    self.indent -= 1;
                    self.write_indent();
                    self.push_str("}");
                }
            }
        }
    }

    fn render_import_inline(&mut self, import: &ImportStatement) {
        match import {
            ImportStatement::CrushModule {
                module_path,
                alias,
                selective,
            } => {
                self.push_str(module_path);
                if !selective.is_empty() {
                    self.push_str(" { ");
                    self.push_str(&selective.join(", "));
                    self.push_str(" }");
                }
                if let Some(a) = alias {
                    self.push_str(" as ");
                    self.push_str(a);
                }
            }
            ImportStatement::PolyglotModule {
                language,
                module_path,
                alias,
                selective,
            } => {
                self.push_str("@lang ");
                self.push_str(language);
                self.push_str(" \"");
                self.push_str(module_path);
                self.push_str("\"");
                if !selective.is_empty() {
                    self.push_str(" { ");
                    self.push_str(&selective.join(", "));
                    self.push_str(" }");
                }
                if let Some(a) = alias {
                    self.push_str(" as ");
                    self.push_str(a);
                }
            }
            ImportStatement::MCPImport {
                server_url,
                tools,
                alias,
            } => {
                self.push_str("@mcp \"");
                self.push_str(server_url);
                self.push_str("\"");
                if !tools.is_empty() {
                    self.push_str(" { ");
                    self.push_str(&tools.join(", "));
                    self.push_str(" }");
                }
                if let Some(a) = alias {
                    self.push_str(" as ");
                    self.push_str(a);
                }
            }
            ImportStatement::Capability {
                capability_path,
                permissions,
                alias,
            } => {
                self.push_str("@cap \"");
                self.push_str(capability_path);
                self.push_str("\"");
                if !permissions.is_empty() {
                    self.push_str(" { ");
                    self.push_str(&permissions.join(", "));
                    self.push_str(" }");
                }
                if let Some(a) = alias {
                    self.push_str(" as ");
                    self.push_str(a);
                }
            }
            ImportStatement::External {
                uri,
                resource_type,
                alias,
            } => {
                let prefix = match resource_type {
                    ExternalResourceType::Http => "@http",
                    ExternalResourceType::Git => "@git",
                    ExternalResourceType::File => "@file",
                    ExternalResourceType::Database => "@database",
                    ExternalResourceType::API { .. } => "@api",
                };
                self.push_str(prefix);
                self.push_str(" \"");
                self.push_str(uri);
                self.push_str("\"");
                if let Some(a) = alias {
                    self.push_str(" as ");
                    self.push_str(a);
                }
            }
            ImportStatement::SecureEnv { keys, alias, .. } => {
                self.push_str("secrets");
                if !keys.is_empty() {
                    self.push_str(" { ");
                    self.push_str(&keys.join(", "));
                    self.push_str(" }");
                }
                if let Some(a) = alias {
                    self.push_str(" as ");
                    self.push_str(a);
                }
            }
        }
    }

    fn render_expression(&mut self, expr: &Expression, parent_prec: u8) {
        let prec = expr_precedence(expr);
        let needs_parens = prec < parent_prec;
        if needs_parens {
            self.push_str("(");
        }

        match expr {
            Expression::IntLiteral { value, .. } => {
                self.push_str(&value.to_string());
            }
            Expression::FloatLiteral { value, .. } => {
                let s = value.to_string();
                self.push_str(&s);
                if s.parse::<i64>().is_ok() && !s.contains('.') && !s.contains('e') {
                    self.push_str(".0");
                }
            }
            Expression::StringLiteral { value, .. } => {
                self.push_str("\"");
                self.push_str(&escape_string(value));
                self.push_str("\"");
            }
            Expression::BoolLiteral { value, .. } => {
                self.push_str(if *value { "true" } else { "false" });
            }
            Expression::NullLiteral { .. } => {
                self.push_str("null");
            }
            Expression::Var { name, .. } => {
                self.push_str(name);
            }
            Expression::BinaryOp {
                operator,
                left,
                right,
                ..
            } => {
                self.render_expression(left, prec);
                self.push_str(" ");
                self.push_str(operator);
                self.push_str(" ");
                self.render_expression(right, prec + 1);
            }
            Expression::UnaryOp {
                operator, operand, ..
            } => {
                self.push_str(operator);
                self.render_expression(operand, prec);
            }
            Expression::Call { function, args, .. } => {
                self.push_str(function);
                self.push_str("(");
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        self.push_str(", ");
                    }
                    self.render_expression(arg, 0);
                }
                self.push_str(")");
            }
            Expression::CapabilityCall { name, args, .. } => {
                self.push_str("@");
                self.push_str(name);
                self.push_str("(");
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        self.push_str(", ");
                    }
                    self.render_expression(arg, 0);
                }
                self.push_str(")");
            }
            Expression::Pipeline { segments, .. } => {
                for (i, seg) in segments.iter().enumerate() {
                    if i > 0 {
                        self.push_str(" |> ");
                    }
                    self.render_expression(seg, prec);
                }
            }
            Expression::Spawn { function, args, .. } => {
                self.push_str("spawn ");
                self.push_str(function);
                self.push_str("(");
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        self.push_str(", ");
                    }
                    self.render_expression(arg, 0);
                }
                self.push_str(")");
            }
            Expression::Lambda { params, body, .. } => {
                self.push_str("|");
                for (i, (p_name, p_type)) in params.iter().enumerate() {
                    if i > 0 {
                        self.push_str(", ");
                    }
                    self.push_str(p_name);
                    if *p_type != CastType::Any {
                        self.push_str(": ");
                        self.push_str(&p_type.to_string());
                    }
                }
                self.push_str("|");
                if body.len() == 1 {
                    if let Statement::Return { value: Some(v), .. } = &body[0] {
                        self.push_str(" => ");
                        self.render_expression(v, 0);
                    } else {
                        self.push_str(" {\n");
                        self.indent += 1;
                        self.render_statement(&body[0]);
                        self.indent -= 1;
                        self.write_indent();
                        self.push_str("}");
                    }
                } else {
                    self.push_str(" {\n");
                    self.indent += 1;
                    for s in body {
                        self.render_statement(s);
                    }
                    self.indent -= 1;
                    self.write_indent();
                    self.push_str("}");
                }
            }
            Expression::Yield { .. } => {
                self.push_str("yield");
            }
            Expression::NewStruct { name, .. } => {
                self.push_str(name);
                self.push_str(" {}");
            }
            Expression::GetField { target, field, .. } => {
                self.render_expression(target, prec);
                self.push_str(".");
                self.push_str(field);
            }
            Expression::Range { start, end, .. } => {
                self.render_expression(start, prec);
                self.push_str("..");
                self.render_expression(end, prec);
            }
            Expression::Await { expression, .. } => {
                self.push_str("await ");
                self.render_expression(expression, prec);
            }
            Expression::ArrayLiteral { elements, .. } => {
                self.push_str("[");
                for (i, elem) in elements.iter().enumerate() {
                    if i > 0 {
                        self.push_str(", ");
                    }
                    self.render_expression(elem, 0);
                }
                self.push_str("]");
            }
            Expression::TupleLiteral { elements, .. } => {
                self.push_str("(");
                for (i, elem) in elements.iter().enumerate() {
                    if i > 0 {
                        self.push_str(", ");
                    }
                    self.render_expression(elem, 0);
                }
                self.push_str(")");
            }
            Expression::ListLiteral { elements, .. } => {
                self.push_str("List[");
                for (i, elem) in elements.iter().enumerate() {
                    if i > 0 {
                        self.push_str(", ");
                    }
                    self.render_expression(elem, 0);
                }
                self.push_str("]");
            }
            Expression::VectorLiteral { elements, .. } => {
                self.push_str("Vector[");
                for (i, elem) in elements.iter().enumerate() {
                    if i > 0 {
                        self.push_str(", ");
                    }
                    self.render_expression(elem, 0);
                }
                self.push_str("]");
            }
            Expression::SetLiteral { elements, .. } => {
                self.push_str("Set{");
                for (i, elem) in elements.iter().enumerate() {
                    if i > 0 {
                        self.push_str(", ");
                    }
                    self.render_expression(elem, 0);
                }
                self.push_str("}");
            }
            Expression::ObjectLiteral { properties, .. } => {
                self.push_str("{");
                for (i, (key, value)) in properties.iter().enumerate() {
                    if i > 0 {
                        self.push_str(", ");
                    }
                    if is_valid_ident(key) {
                        self.push_str(key);
                    } else {
                        self.push_str("\"");
                        self.push_str(&escape_string(key));
                        self.push_str("\"");
                    }
                    self.push_str(": ");
                    self.render_expression(value, 0);
                }
                self.push_str("}");
            }
            Expression::Index { target, index, .. } => {
                self.render_expression(target, prec);
                self.push_str("[");
                self.render_expression(index, 0);
                self.push_str("]");
            }
            Expression::DomQuery {
                query_type,
                selector,
                ..
            } => {
                self.push_str("dom_query(");
                self.push_str("\"");
                self.push_str(&format!("{:?}", query_type));
                self.push_str("\", ");
                self.render_expression(selector, 0);
                self.push_str(")");
            }
            Expression::Match {
                expression, arms, ..
            } => {
                let simple = arms.iter().all(|arm| {
                    arm.body.len() == 1 && matches!(arm.body[0], Statement::ExprStmt { .. })
                });
                self.push_str("match ");
                self.render_expression(expression, 0);
                if simple {
                    self.push_str(" { ");
                    for (i, arm) in arms.iter().enumerate() {
                        if i > 0 {
                            self.push_str(", ");
                        }
                        self.render_pattern(&arm.pattern);
                        self.push_str(" -> ");
                        if let Statement::ExprStmt { expr, .. } = &arm.body[0] {
                            self.render_expression(expr, 0);
                        }
                    }
                    self.push_str(" }");
                } else {
                    self.push_str(" {\n");
                    self.indent += 1;
                    for arm in arms {
                        self.write_indent();
                        self.render_pattern(&arm.pattern);
                        self.push_str(" -> ");
                        if arm.body.len() == 1 {
                            if let Statement::ExprStmt { expr, .. } = &arm.body[0] {
                                self.render_expression(expr, 0);
                                self.push_str(",\n");
                            } else {
                                self.push_str("{\n");
                                self.indent += 1;
                                for s in &arm.body {
                                    self.render_statement(s);
                                }
                                self.indent -= 1;
                                self.write_indent();
                                self.push_str("},\n");
                            }
                        } else {
                            self.push_str("{\n");
                            self.indent += 1;
                            for s in &arm.body {
                                self.render_statement(s);
                            }
                            self.indent -= 1;
                            self.write_indent();
                            self.push_str("},\n");
                        }
                    }
                    self.indent -= 1;
                    self.write_indent();
                    self.push_str("}");
                }
            }
            Expression::AI(ai_expr) => {
                self.push_str("# AI-NATIVE: read-only\n");
                self.write_indent();
                self.render_ai_expression(ai_expr);
            }
        }

        if needs_parens {
            self.push_str(")");
        }
    }

    fn render_pattern(&mut self, pattern: &Pattern) {
        match pattern {
            Pattern::Literal { value } => {
                self.render_expression(value, 0);
            }
            Pattern::Identifier { name } => {
                self.push_str(name);
            }
            Pattern::Struct { name, fields } => {
                self.push_str(name);
                self.push_str(" { ");
                for (i, (f_name, f_pat)) in fields.iter().enumerate() {
                    if i > 0 {
                        self.push_str(", ");
                    }
                    self.push_str(f_name);
                    self.push_str(": ");
                    self.render_pattern(f_pat);
                }
                self.push_str(" }");
            }
            Pattern::Wildcard => {
                self.push_str("_");
            }
        }
    }

    fn render_ai_statement(&mut self, stmt: &crush_cast::ai::AIStatement) {
        use crush_cast::ai::AIStatement;
        match stmt {
            AIStatement::GoalDeclaration {
                goal_id,
                description,
                success_criteria,
                priority,
                deadline,
            } => {
                self.write_indent();
                self.push_str("goal ");
                self.push_str(goal_id);
                self.push_str(" \"");
                self.push_str(&escape_string(description));
                self.push_str("\"");
                self.push_str(" [");
                self.push_str(&success_criteria.join(", "));
                self.push_str("]");
                self.push_str(" priority=");
                self.push_str(&format!("{:?}", priority));
                if let Some(d) = deadline {
                    self.push_str(" deadline=");
                    self.push_str(d);
                }
                self.newline();
            }
            AIStatement::ProgressUpdate {
                goal_id,
                progress,
                status_message,
                metrics,
            } => {
                self.write_indent();
                self.push_str("progress ");
                self.push_str(goal_id);
                self.push_str(" ");
                self.push_str(&progress.to_string());
                self.push_str(" \"");
                self.push_str(&escape_string(status_message));
                self.push_str("\"");
                if !metrics.is_empty() {
                    self.push_str(" { ");
                    for (i, (k, v)) in metrics.iter().enumerate() {
                        if i > 0 {
                            self.push_str(", ");
                        }
                        self.push_str(k);
                        self.push_str(": ");
                        self.push_str(&v.to_string());
                    }
                    self.push_str(" }");
                }
                self.newline();
            }
            AIStatement::KnowledgeSharing {
                knowledge_type,
                content,
                recipients,
                retention_policy,
            } => {
                self.write_indent();
                self.push_str("share ");
                self.push_str(&format!("{:?}", knowledge_type));
                self.push_str(" ");
                self.push_str(&content.to_string());
                self.push_str(" with ");
                self.push_str(&recipients.join(", "));
                self.push_str(" retain=");
                self.push_str(&format!("{:?}", retention_policy));
                self.newline();
            }
            AIStatement::CapabilityDiscovery {
                domain,
                requirements,
                discovery_strategy,
            } => {
                self.write_indent();
                self.push_str("discover ");
                self.push_str(domain);
                self.push_str(" [");
                self.push_str(&requirements.join(", "));
                self.push_str("] strategy=");
                self.push_str(&format!("{:?}", discovery_strategy));
                self.newline();
            }
            AIStatement::AdaptationRequest {
                adaptation_type,
                reason,
                parameters,
            } => {
                self.write_indent();
                self.push_str("adapt ");
                self.push_str(&format!("{:?}", adaptation_type));
                self.push_str(" \"");
                self.push_str(&escape_string(reason));
                self.push_str("\"");
                if !parameters.is_empty() {
                    self.push_str(" { ");
                    for (i, (k, v)) in parameters.iter().enumerate() {
                        if i > 0 {
                            self.push_str(", ");
                        }
                        self.push_str(k);
                        self.push_str(": ");
                        self.push_str(&v.to_string());
                    }
                    self.push_str(" }");
                }
                self.newline();
            }
            AIStatement::SemanticSwitch {
                target,
                cases,
                fallback,
            } => {
                self.write_indent();
                self.push_str("semantic_switch ");
                self.render_expression(target, 0);
                self.push_str(" {\n");
                self.indent += 1;
                for (concept, block) in cases {
                    self.write_indent();
                    self.push_str("case \"");
                    self.push_str(&escape_string(concept));
                    self.push_str("\":\n");
                    self.indent += 1;
                    for s in block {
                        self.render_statement(s);
                    }
                    self.indent -= 1;
                }
                if let Some(fb) = fallback {
                    self.write_indent();
                    self.push_str("fallback:\n");
                    self.indent += 1;
                    for s in fb {
                        self.render_statement(s);
                    }
                    self.indent -= 1;
                }
                self.indent -= 1;
                self.write_indent();
                self.push_str("}\n");
            }
        }
    }

    fn render_ai_expression(&mut self, expr: &crush_cast::ai::AIExpression) {
        use crush_cast::ai::AIExpression;
        match expr {
            AIExpression::Query {
                query,
                result_type,
                context,
            } => {
                self.push_str("ai_query(\"");
                self.push_str(&escape_string(query));
                self.push_str("\"");
                if let Some(rt) = result_type {
                    self.push_str(", type=\"");
                    self.push_str(rt);
                    self.push_str("\"");
                }
                if !context.is_empty() {
                    self.push_str(", context=");
                    self.push_str(&serde_json::to_string(context).unwrap_or_default());
                }
                self.push_str(")");
            }
            AIExpression::ToolChain {
                tools,
                strategy,
                error_handling,
            } => {
                self.push_str("ai_toolchain([\n");
                self.indent += 1;
                for tool in tools {
                    self.write_indent();
                    self.push_str("{ name: \"");
                    self.push_str(&tool.tool_name);
                    self.push_str("\", params: ");
                    self.push_str(&serde_json::to_string(&tool.parameters).unwrap_or_default());
                    self.push_str(" },\n");
                }
                self.indent -= 1;
                self.write_indent();
                self.push_str("], strategy=");
                self.push_str(&format!("{:?}", strategy));
                self.push_str(", error=");
                self.push_str(&format!("{:?}", error_handling));
                self.push_str(")");
            }
            AIExpression::AgentDelegation {
                task,
                agents,
                delegation_strategy,
                expected_format,
            } => {
                self.push_str("ai_delegate(\"");
                self.push_str(&escape_string(task));
                self.push_str("\", agents=[");
                self.push_str(&agents.join(", "));
                self.push_str("], strategy=");
                self.push_str(&format!("{:?}", delegation_strategy));
                if let Some(ef) = expected_format {
                    self.push_str(", format=\"");
                    self.push_str(ef);
                    self.push_str("\"");
                }
                self.push_str(")");
            }
            AIExpression::LearningLoop {
                learning_target,
                strategy,
                adaptations,
            } => {
                self.push_str("ai_learn(");
                self.push_str(&format!("{:?}", learning_target));
                self.push_str(", ");
                self.push_str(&format!("{:?}", strategy));
                self.push_str(", [");
                self.push_str(
                    &adaptations
                        .iter()
                        .map(|a| format!("{:?}", a))
                        .collect::<Vec<_>>()
                        .join(", "),
                );
                self.push_str("])");
            }
            AIExpression::ContextAware {
                expression,
                requires_context,
                provides_context,
            } => {
                self.push_str("ai_context(");
                self.render_expression(expression, 0);
                self.push_str(", requires=[");
                self.push_str(&requires_context.join(", "));
                self.push_str("], provides=[");
                self.push_str(&provides_context.join(", "));
                self.push_str("])");
            }
            AIExpression::SemanticMatch {
                target,
                concept,
                confidence_threshold,
            } => {
                self.push_str("ai_semantic_match(");
                self.render_expression(target, 0);
                self.push_str(&format!(", \"{}\", {})", escape_string(concept), confidence_threshold));
            }
            AIExpression::Synthesize {
                output_type,
                constraints,
                context_refs,
                examples: _,
            } => {
                self.push_str(&format!("ai_synthesize({:?}, constraints=[", output_type));
                self.push_str(&constraints.iter().map(|c| format!("\"{}\"", escape_string(c))).collect::<Vec<_>>().join(", "));
                self.push_str("], ctx=[");
                for (i, expr) in context_refs.iter().enumerate() {
                    if i > 0 { self.push_str(", "); }
                    self.render_expression(expr, 0);
                }
                self.push_str("])");
            }
        }
    }

    // ── annotation rendering ─────────────────────────────────────────────────

    fn render_fn_annotations(&mut self, ann: &FunctionAnnotations) {
        if !ann.errors_weighted.is_empty() {
            self.push_str("@errors {\n");
            self.indent += 1;
            for we in &ann.errors_weighted {
                self.write_indent();
                self.push_str(&we.variant);
                self.push_str(": ");
                self.push_str(&we.likelihood.to_string());
                self.push_str("\n");
            }
            self.indent -= 1;
            self.write_indent();
            self.push_str("}\n");
        } else {
            self.render_at_list("errors", &ann.errors);
        }
        self.render_at_list("reads", &ann.reads);
        self.render_at_list("writes", &ann.writes);
        self.render_at_list("does-not-write", &ann.does_not_write);
        self.render_at_list("covers", &ann.covers);
        self.render_at_list("relies-on", &ann.relies_on);
        if let Some(c) = ann.complexity {
            self.push_str(&format!("@complexity {}\n", c));
        }
    }

    fn render_at_list(&mut self, name: &str, items: &[String]) {
        if items.is_empty() {
            return;
        }
        self.push_str("@");
        self.push_str(name);
        self.push_str(" [");
        for (i, item) in items.iter().enumerate() {
            if i > 0 {
                self.push_str(", ");
            }
            self.push_str(item);
        }
        self.push_str("]\n");
    }

    fn render_manifest(&mut self, manifest: &ModuleManifest) {
        self.push_str("@module {\n");
        self.indent += 1;

        self.write_indent();
        self.push_str("purpose: \"");
        self.push_str(&escape_string(&manifest.purpose));
        self.push_str("\"\n");

        if !manifest.exports.is_empty() {
            self.write_indent();
            self.push_str("exports: [");
            self.push_str(&manifest.exports.join(", "));
            self.push_str("]\n");
        }
        if !manifest.related.is_empty() {
            self.write_indent();
            self.push_str("related: [");
            self.push_str(&manifest.related.join(", "));
            self.push_str("]\n");
        }
        if !manifest.exhaustive_types.is_empty() {
            self.write_indent();
            self.push_str("exhaustive_types: [");
            self.push_str(&manifest.exhaustive_types.join(", "));
            self.push_str("]\n");
        }

        self.indent -= 1;
        self.push_str("}\n");

        // Render each named invariant as a separate top-level @invariant block
        for inv in &manifest.invariants {
            if !inv.description.is_empty() {
                self.newline();
                self.render_single_invariant(inv);
            }
        }
    }

    fn render_single_invariant(&mut self, inv: &Invariant) {
        self.push_str("@invariant \"");
        self.push_str(&escape_string(&inv.name));
        self.push_str("\" {\n");
        self.indent += 1;

        if !inv.description.is_empty() {
            self.write_indent();
            self.push_str("description: \"");
            self.push_str(&escape_string(&inv.description));
            self.push_str("\"\n");
        }
        if !inv.applies_to.is_empty() {
            self.write_indent();
            self.push_str("applies_to: [");
            self.push_str(&inv.applies_to.join(", "));
            self.push_str("]\n");
        }
        if let Some(c) = &inv.consequence {
            self.write_indent();
            self.push_str("consequence: \"");
            self.push_str(&escape_string(c));
            self.push_str("\"\n");
        }

        self.indent -= 1;
        self.push_str("}\n");
    }
}

fn expr_precedence(expr: &Expression) -> u8 {
    match expr {
        Expression::IntLiteral { .. }
        | Expression::FloatLiteral { .. }
        | Expression::StringLiteral { .. }
        | Expression::BoolLiteral { .. }
        | Expression::NullLiteral { .. }
        | Expression::Var { .. }
        | Expression::ArrayLiteral { .. }
        | Expression::TupleLiteral { .. }
        | Expression::ListLiteral { .. }
        | Expression::VectorLiteral { .. }
        | Expression::SetLiteral { .. }
        | Expression::ObjectLiteral { .. }
        | Expression::NewStruct { .. }
        | Expression::Yield { .. } => 100,

        Expression::GetField { .. }
        | Expression::Index { .. }
        | Expression::Call { .. }
        | Expression::CapabilityCall { .. }
        | Expression::DomQuery { .. } => 90,

        Expression::UnaryOp { .. } | Expression::Await { .. } | Expression::Spawn { .. } => 80,

        Expression::Range { .. } => 75,

        Expression::BinaryOp { operator, .. } => match operator.as_str() {
            "*" | "/" | "%" => 70,
            "+" | "-" => 60,
            "<" | ">" | "<=" | ">=" => 50,
            "==" | "!=" => 40,
            "&&" => 30,
            "||" => 20,
            "|>" => 10,
            _ => 60,
        },

        Expression::Lambda { .. }
        | Expression::Match { .. }
        | Expression::Pipeline { .. }
        | Expression::AI { .. } => 5,
    }
}

fn escape_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

fn is_valid_ident(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let mut chars = s.chars();
    let first = chars.next().unwrap();
    if !first.is_ascii_alphabetic() && first != '_' {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::Parser;

    #[test]
    fn render_simple_let_statement() {
        let source = "let x = 42\n";
        let program = Parser::parse(source).expect("parse");
        let output = render_program(&program);
        assert_eq!(output, source);
    }

    #[test]
    fn render_expression_statement() {
        let source = "@io.print(42)\n";
        let program = Parser::parse("io.print(42)").expect("parse");
        let output = render_program(&program);
        assert_eq!(output, source);
    }

    #[test]
    fn render_if_else() {
        let source = "\
if true {
    @io.print(1)
} else {
    @io.print(2)
}
";
        let program = Parser::parse("if true { io.print(1) } else { io.print(2) }").expect("parse");
        let output = render_program(&program);
        assert_eq!(output, source);
    }

    #[test]
    fn render_function() {
        let source = "\
fn add(a: Int, b: Int) {
    return a + b
}
";
        let program = Parser::parse("fn add(a: int, b: int) { return a + b }").expect("parse");
        let output = render_program(&program);
        assert_eq!(output, source);
    }

    #[test]
    fn render_while_loop() {
        let source = "\
while true {
    @io.print(1)
}
";
        let program = Parser::parse("while true { io.print(1) }").expect("parse");
        let output = render_program(&program);
        assert_eq!(output, source);
    }

    #[test]
    fn render_binary_op_parentheses() {
        let source = "1 + 2 * 3\n";
        let program = Parser::parse(source).expect("parse");
        let output = render_program(&program);
        assert_eq!(output, source);
    }

    #[test]
    fn render_binary_op_extra_parens() {
        let source = "(1 + 2) * 3\n";
        let program = Parser::parse(source).expect("parse");
        let output = render_program(&program);
        assert_eq!(output, source);
    }

    #[test]
    fn render_nested_if_else() {
        let source = "\
if x > 0 {
    @io.print(1)
} else if x < 0 {
    @io.print(-1)
} else {
    @io.print(0)
}
";
        let program = Parser::parse(
            "if x > 0 { io.print(1) } else if x < 0 { io.print(-1) } else { io.print(0) }",
        )
        .expect("parse");
        let output = render_program(&program);
        assert_eq!(output, source);
    }

    #[test]
    fn render_string_with_escapes() {
        let source = "let msg = \"hello \\\"world\\\"\"\n";
        let program = Parser::parse(source).expect("parse");
        let output = render_program(&program);
        assert_eq!(output, source);
    }

    #[test]
    fn render_comparison_chain() {
        let source = "1 < 2 && 3 > 1\n";
        let program = Parser::parse(source).expect("parse");
        let output = render_program(&program);
        assert_eq!(output, source);
    }

    #[test]
    fn render_round_trip_let_expression() {
        let source = "\
let x = 42
let y = x + 1
let z = y
";
        let program = Parser::parse(source).expect("parse");
        let rendered = render_program(&program);
        let reparsed = Parser::parse(&rendered).expect("reparse");
        assert_eq!(program.functions.len(), reparsed.functions.len());
        assert_eq!(render_program(&reparsed), rendered);
    }

    #[test]
    fn render_round_trip_function() {
        let _source = "\
fn helper(x: Int) {
    return x * 2
}
let y = helper(21)
";
        let program =
            Parser::parse("fn helper(x: int) { return x * 2 }\nlet y = helper(21)").expect("parse");
        let rendered = render_program(&program);
        let reparsed = Parser::parse(&rendered).expect("reparse");
        assert_eq!(
            render_program(&reparsed),
            rendered,
            "round-trip should be idempotent"
        );
    }

    #[test]
    fn render_array_literal() {
        let source = "[1, 2, 3]\n";
        let program = Parser::parse(source).expect("parse");
        let output = render_program(&program);
        assert_eq!(output, source);
    }

    #[test]
    fn render_capability_call() {
        let source = "let result = @str.concat(\"hello\", \" world\")\n";
        let program =
            Parser::parse("let result = str.concat(\"hello\", \" world\")").expect("parse");
        let output = render_program(&program);
        assert_eq!(output, source);
    }

    #[test]
    fn render_not_and_negation_exprs() {
        let source = "!false && -1\n";
        let program = Parser::parse(source).expect("parse");
        let output = render_program(&program);
        assert_eq!(output, source);
    }

    #[test]
    fn render_return_statement() {
        let source = "return 42\n";
        let program = Parser::parse(source).expect("parse");
        let output = render_program(&program);
        assert_eq!(output, source);
    }

    #[test]
    fn render_break_continue() {
        let source = "\
while true {
    break
    continue
}
";
        let program = Parser::parse(source).expect("parse");
        let output = render_program(&program);
        assert_eq!(output, source);
    }
}
