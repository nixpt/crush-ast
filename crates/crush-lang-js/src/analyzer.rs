use swc_ecma_ast::*;
use walker_core::FeatureReport;

fn wtf8_str(a: &swc_atoms::Wtf8Atom) -> &str {
    a.as_str().unwrap_or_default()
}

pub fn analyze_item(item: &ModuleItem, r: &mut FeatureReport) {
    match item {
        ModuleItem::ModuleDecl(decl) => analyze_module_decl(decl, r),
        ModuleItem::Stmt(stmt) => analyze_stmt(stmt, r),
    }
}

fn analyze_module_decl(decl: &ModuleDecl, r: &mut FeatureReport) {
    match decl {
        ModuleDecl::Import(import) => {
            let src = wtf8_str(&import.src.value);
            r.uses_imports.push(src.to_string());
            if is_dangerous_import(src) {
                r.dangerous_imports.push(src.to_string());
            }
        }
        ModuleDecl::ExportDecl(_)
        | ModuleDecl::ExportDefaultDecl(_)
        | ModuleDecl::ExportDefaultExpr(_)
        | ModuleDecl::ExportAll(_)
        | ModuleDecl::ExportNamed(_) => {
            r.uses_imports.push("export".to_string());
        }
        ModuleDecl::TsImportEquals(_)
        | ModuleDecl::TsExportAssignment(_)
        | ModuleDecl::TsNamespaceExport(_) => {}
    }
    r.estimated_complexity += 1;
}

fn analyze_stmt(stmt: &Stmt, r: &mut FeatureReport) {
    match stmt {
        Stmt::Decl(decl) => match decl {
            Decl::Fn(f) => {
                r.uses_functions = true;
                if f.function.is_async {
                    r.uses_async = true;
                }
                if f.function.is_generator {
                    r.uses_generators = true;
                }
            }
            Decl::Class(_) => r.uses_classes = true,
            Decl::Var(_) => {}
            Decl::Using(_)
            | Decl::TsInterface(_)
            | Decl::TsTypeAlias(_)
            | Decl::TsEnum(_)
            | Decl::TsModule(_) => {}
        },
        Stmt::Expr(ExprStmt { expr, .. }) => {
            if let Expr::Call(CallExpr { callee, .. }) = expr.as_ref() {
                if let Callee::Expr(callee_expr) = callee {
                    if let Expr::Ident(ident) = callee_expr.as_ref() {
                        if ident.sym.as_ref() == "eval" || ident.sym.as_ref() == "Function" {
                            r.dangerous_imports
                                .push(format!("eval-like: {}", ident.sym));
                        }
                    }
                }
            }
        }
        Stmt::ForOf(ForOfStmt { is_await: true, .. }) => {
            r.uses_async = true;
        }
        Stmt::ForOf(_) | Stmt::ForIn(_) | Stmt::For(_) | Stmt::While(_) | Stmt::DoWhile(_) => {}
        Stmt::If(_) | Stmt::Switch(_) => {}
        Stmt::Try(_) => r.uses_exceptions = true,
        Stmt::Return(_) | Stmt::Throw(_) | Stmt::Break(_) | Stmt::Continue(_) => {}
        Stmt::With(_) => {
            r.dangerous_imports.push("with-statement".to_string());
        }
        Stmt::Block(b) => {
            for s in &b.stmts {
                analyze_stmt(s, r);
            }
        }
        Stmt::Debugger(_) | Stmt::Empty(_) | Stmt::Labeled(_) => {}
    }
    r.estimated_complexity += 1;
}

fn is_dangerous_import(module: &str) -> bool {
    let dangerous = [
        "child_process",
        "fs",
        "net",
        "dgram",
        "cluster",
        "vm",
        "worker_threads",
        "os",
        "process",
        "module",
        "electron",
    ];
    let base = module.split('/').next().unwrap_or(module);
    dangerous.contains(&base)
}
