import os
import re

def replace_in_file(path, old, new):
    if not os.path.exists(path):
        return
    with open(path, "r") as f:
        content = f.read()
    content = content.replace(old, new)
    with open(path, "w") as f:
        f.write(content)

# 1. crush-cast/src/lib.rs
cast_path = "crates/crush-cast/src/lib.rs"
replace_in_file(cast_path, """
    VarDecl {
        name: String,
        value: Expression,
        #[serde(default)]
        type_hint: CastType,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
""", """
    VarDecl {
        name: String,
        value: Expression,
        #[serde(default)]
        type_hint: CastType,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    Assign {
        target: String,
        value: Expression,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
""")

# 2. crush-frontend/src/parser/mod.rs
parser_path = "crates/crush-frontend/src/parser/mod.rs"
replace_in_file(parser_path, """
    fn parse_expression_statement(&mut self) -> Result<Statement, ()> {
        let expr = self.parse_expression()?;

        self.maybe_semicolon();

        Ok(Statement::ExprStmt {
            expr,
            meta: HashMap::new(),
        })
    }
""", """
    fn parse_expression_statement(&mut self) -> Result<Statement, ()> {
        let expr = self.parse_expression()?;

        if matches!(self.peek(), Token::Assign(_)) {
            if let Expression::Var { name, .. } = expr {
                self.advance();
                let value = self.parse_expression()?;
                self.maybe_semicolon();
                return Ok(Statement::Assign {
                    target: name,
                    value,
                    meta: HashMap::new(),
                });
            }
        }

        self.maybe_semicolon();

        Ok(Statement::ExprStmt {
            expr,
            meta: HashMap::new(),
        })
    }
""")

# 3. compiler.rs
compiler_path = "crates/crush-frontend/src/compiler.rs"
replace_in_file(compiler_path, """
            Statement::VarDecl {
                name, value, meta, ..
            } => {
                self.compile_expr_with_name_hint(value, instrs, Some(name))?;
                instrs.push(self.create_instr("store", serde_json::json!({"name": name}), meta));
            }
""", """
            Statement::VarDecl {
                name, value, meta, ..
            } => {
                self.compile_expr_with_name_hint(value, instrs, Some(name))?;
                instrs.push(self.create_instr("store", serde_json::json!({"name": name}), meta));
            }
            Statement::Assign {
                target, value, meta
            } => {
                self.compile_expr_with_name_hint(value, instrs, Some(target))?;
                instrs.push(self.create_instr("store", serde_json::json!({"name": target}), meta));
            }
""")

replace_in_file(compiler_path, "Statement::VarDecl { name, .. } => {", """Statement::VarDecl { name, .. } => {
                        local_vars.insert(name.clone());
                    }
                    Statement::Assign { target, .. } => {""")
replace_in_file(compiler_path, """
                    Statement::VarDecl { name, .. } => {
                        local_vars.insert(name.clone());
                    }
                    Statement::Assign { target, .. } => {
                        local_vars.insert(name.clone());
                    }
""", """
                    Statement::VarDecl { name, .. } | Statement::Assign { target: name, .. } => {
                        local_vars.insert(name.clone());
                    }
""")

# 4. semantics.rs
sem_path = "crates/crush-frontend/src/semantics.rs"
replace_in_file(sem_path, """
                Statement::VarDecl { name, value, .. } => {
                    let ty = self.check_expr(value)?;
                    self.define_var(name, ty);
                }
""", """
                Statement::VarDecl { name, value, .. } => {
                    let ty = self.check_expr(value)?;
                    self.define_var(name, ty);
                }
                Statement::Assign { target, value, .. } => {
                    let ty = self.check_expr(value)?;
                    // In a stricter lang we'd check if target is defined and matches ty.
                }
""")
replace_in_file(sem_path, """
            Statement::VarDecl {
                name,
                value,
                type_hint,
                ..
            } => {
""", """
            Statement::VarDecl {
                name,
                value,
                type_hint,
                ..
            } => {
""")
replace_in_file(sem_path, """
                if let Statement::VarDecl { name, value, .. } = stmt {
""", """
                if let Statement::VarDecl { name, value, .. } = stmt {
""")

# 5. optimizer.rs
opt_path = "crates/crush-frontend/src/optimizer.rs"
replace_in_file(opt_path, """
            Statement::VarDecl {
                name,
                value,
                type_hint,
                meta,
            } => {
                let opt_val = self.optimize_expression(value)?;
                vec![Statement::VarDecl {
                    name: name.clone(),
                    value: opt_val,
                    type_hint: type_hint.clone(),
                    meta: meta.clone(),
                }]
            }
""", """
            Statement::VarDecl {
                name,
                value,
                type_hint,
                meta,
            } => {
                let opt_val = self.optimize_expression(value)?;
                vec![Statement::VarDecl {
                    name: name.clone(),
                    value: opt_val,
                    type_hint: type_hint.clone(),
                    meta: meta.clone(),
                }]
            }
            Statement::Assign { target, value, meta } => {
                let opt_val = self.optimize_expression(value)?;
                vec![Statement::Assign {
                    target: target.clone(),
                    value: opt_val,
                    meta: meta.clone(),
                }]
            }
""")
replace_in_file(opt_path, """
        Statement::VarDecl { value, .. } | Statement::Export { value, .. } => {
""", """
        Statement::VarDecl { value, .. } | Statement::Assign { value, .. } | Statement::Export { value, .. } => {
""")

# 6. mutation_check.rs
mut_path = "crates/crush-frontend/src/mutation_check.rs"
replace_in_file(mut_path, """
        crush_cast::Statement::VarDecl { value, .. } => call_name_in_expr(value),
""", """
        crush_cast::Statement::VarDecl { value, .. } | crush_cast::Statement::Assign { value, .. } => call_name_in_expr(value),
""")

# 7. cast_enrich.rs
enrich_path = "crates/crush-frontend/src/cast_enrich.rs"
replace_in_file(enrich_path, """
        Statement::VarDecl { value, .. } | Statement::Export { value, .. } => {
""", """
        Statement::VarDecl { value, .. } | Statement::Assign { value, .. } | Statement::Export { value, .. } => {
""")

# 8. render.rs
render_path = "crates/crush-frontend/src/render.rs"
replace_in_file(render_path, """
            Statement::VarDecl { name, value, .. } => {
                self.push_str("let ");
                self.push_str(name);
                self.push_str(" = ");
                self.render_expr(value);
            }
""", """
            Statement::VarDecl { name, value, .. } => {
                self.push_str("let ");
                self.push_str(name);
                self.push_str(" = ");
                self.render_expr(value);
            }
            Statement::Assign { target, value, .. } => {
                self.push_str(target);
                self.push_str(" = ");
                self.render_expr(value);
            }
""")

print("Patched.")
