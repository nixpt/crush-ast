//! Feature detection and safety analysis for Python code.

use rustpython_ast as py_ast;
use rustpython_parser::Parse;

/// Result of analyzing Python code for Crush compatibility.
pub struct Analysis {
    pub dangerous_imports: Vec<String>,
    pub unsupported_features: Vec<String>,
    pub can_lower: bool,
}

/// Check if Python code can be lowered to CAST by the Crush Python compiler.
pub fn analyze(source: &str) -> Analysis {
    let mut dangerous_imports = Vec::new();
    let mut unsupported_features = Vec::new();

    let ast = match py_ast::Suite::parse(source, "<crush>") {
        Ok(stmts) => stmts,
        Err(e) => {
            unsupported_features.push(format!("parse error: {}", e));
            return Analysis {
                dangerous_imports,
                unsupported_features,
                can_lower: false,
            };
        }
    };

    for stmt in &ast {
        analyze_stmt(stmt, &mut dangerous_imports, &mut unsupported_features);
    }

    let can_lower = unsupported_features.is_empty();
    Analysis {
        dangerous_imports,
        unsupported_features,
        can_lower,
    }
}

fn analyze_stmt(stmt: &py_ast::Stmt, dangerous: &mut Vec<String>, unsupported: &mut Vec<String>) {
    match stmt {
        py_ast::Stmt::Import(py_ast::StmtImport { names, .. }) => {
            for alias in names {
                if is_dangerous_import(&alias.name.to_string()) {
                    dangerous.push(alias.name.to_string());
                }
            }
        }
        py_ast::Stmt::ImportFrom(py_ast::StmtImportFrom { module, .. }) => {
            if let Some(module) = module {
                let mod_name = module.to_string();
                if is_dangerous_import(&mod_name) {
                    dangerous.push(mod_name);
                }
            }
        }
        py_ast::Stmt::ClassDef { .. } => unsupported.push("class definitions".to_string()),
        py_ast::Stmt::With { .. } => unsupported.push("with statements".to_string()),
        py_ast::Stmt::Try { .. } | py_ast::Stmt::Raise { .. } => {
            unsupported.push("exception handling".to_string())
        }
        py_ast::Stmt::Match { .. } => unsupported.push("match statements".to_string()),
        py_ast::Stmt::Delete { .. } => unsupported.push("del statements".to_string()),
        py_ast::Stmt::Assert { .. } => unsupported.push("assert statements".to_string()),
        py_ast::Stmt::Global { .. } => unsupported.push("global keyword".to_string()),
        py_ast::Stmt::Nonlocal { .. } => unsupported.push("nonlocal keyword".to_string()),
        _ => {}
    }
}

pub fn is_dangerous_import(module: &str) -> bool {
    let dangerous = [
        "os",
        "sys",
        "subprocess",
        "socket",
        "ctypes",
        "signal",
        "multiprocessing",
        "threading",
        "fcntl",
        "termios",
        "pty",
        "tty",
        "resource",
        "mmap",
        "cffi",
        "importlib",
    ];
    let base = module.split('.').next().unwrap_or(module);
    dangerous.contains(&base)
}
