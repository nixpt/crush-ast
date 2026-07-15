use anyhow::Result;
use crush_cast::{Expression, Function, Program, Statement};
use std::collections::HashMap;
use crush_walker_core::WalkerError;
use wasmparser::{Parser as WasmParser, Payload, TypeRef};

pub fn walk_wasm(wasm_bytes: &[u8], filename: &str) -> Result<Program, WalkerError> {
    let mut functions = HashMap::new();
    let mut imported_funcs = Vec::new();
    let mut exported_funcs = HashMap::new();
    let mut func_types = Vec::new();
    let mut func_bodies = Vec::new();
    let mut type_definitions = Vec::new();

    let parser = WasmParser::new(0);
    for payload in parser.parse_all(wasm_bytes) {
        match payload.map_err(|e| WalkerError::ParseError(e.to_string()))? {
            Payload::TypeSection(reader) => {
                for type_def in reader {
                    type_definitions
                        .push(type_def.map_err(|e| WalkerError::ParseError(e.to_string()))?);
                }
            }
            Payload::ImportSection(reader) => {
                for import in reader {
                    let import = import.map_err(|e| WalkerError::ParseError(e.to_string()))?;
                    if let TypeRef::Func(type_idx) = import.ty {
                        imported_funcs.push((import.module, import.name, type_idx));
                    }
                }
            }
            Payload::FunctionSection(reader) => {
                for type_idx in reader {
                    func_types.push(type_idx.map_err(|e| WalkerError::ParseError(e.to_string()))?);
                }
            }
            Payload::ExportSection(reader) => {
                for export in reader {
                    let export = export.map_err(|e| WalkerError::ParseError(e.to_string()))?;
                    if let wasmparser::ExternalKind::Func = export.kind {
                        exported_funcs.insert(export.index, export.name);
                    }
                }
            }
            Payload::CodeSectionEntry(body) => {
                func_bodies.push(body);
            }
            _ => {}
        }
    }

    let import_count = imported_funcs.len();

    for (i, body) in func_bodies.iter().enumerate() {
        let func_idx = (import_count + i) as u32;
        let mut func_name = format!("func_{}", func_idx);

        if let Some(export_name) = exported_funcs.get(&func_idx) {
            func_name = (*export_name).to_string();
        }

        let mut crush_body = Vec::new();
        let mut operators = body
            .get_operators_reader()
            .map_err(|e: wasmparser::BinaryReaderError| WalkerError::ParseError(e.to_string()))?;

        while !operators.eof() {
            let op = operators
                .read()
                .map_err(|e: wasmparser::BinaryReaderError| {
                    WalkerError::ParseError(e.to_string())
                })?;
            match op {
                wasmparser::Operator::Call { function_index } => {
                    if (function_index as usize) < import_count {
                        let (module, name, _) = imported_funcs[function_index as usize];
                        if module == "wasi_snapshot_preview1" && name == "fd_write" {
                            crush_body.push(Statement::ExprStmt {
                                expr: Expression::CapabilityCall {
                                    name: "io.print".to_string(),
                                    args: vec![],
                                    meta: HashMap::new(),
                                },
                                meta: HashMap::new(),
                            });
                        } else {
                            crush_body.push(Statement::ExprStmt {
                                expr: Expression::Call {
                                    function: format!("{}.{}", module, name),
                                    args: vec![],
                                    meta: HashMap::new(),
                                },
                                meta: HashMap::new(),
                            });
                        }
                    } else {
                        crush_body.push(Statement::ExprStmt {
                            expr: Expression::Call {
                                function: format!("func_{}", function_index),
                                args: vec![],
                                meta: HashMap::new(),
                            },
                            meta: HashMap::new(),
                        });
                    }
                }
                wasmparser::Operator::End => break,
                _ => {}
            }
        }

        functions.insert(
            func_name,
            Function {
                params: vec![],
                body: crush_body,
                meta: HashMap::from([
                    ("file".to_string(), serde_json::json!(filename)),
                    ("lang".to_string(), serde_json::json!("wasm")),
                ]),
                ..Default::default()
            },
        );
    }

    if functions.contains_key("_start") {
        functions.insert(
            "main".to_string(),
            Function {
                params: vec![],
                body: vec![Statement::ExprStmt {
                    expr: Expression::Call {
                        function: "_start".to_string(),
                        args: vec![],
                        meta: HashMap::new(),
                    },
                    meta: HashMap::new(),
                }],
                meta: HashMap::new(),
                ..Default::default()
            },
        );
    }

    Ok(Program {
        cast_version: "0.2".to_string(),
        entry: "main".to_string(),
        lang: Some("wasm".to_string()),
        functions,
        ai_meta: None,
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_wasm() {
        // Minimum valid Wasm module (magic + version)
        let wasm_bytes = b"\x00asm\x01\x00\x00\x00";
        let program = walk_wasm(wasm_bytes, "test.wasm").unwrap();
        assert_eq!(program.lang.unwrap(), "wasm");
        assert!(program.functions.is_empty());
    }

    #[test]
    fn test_wasi_print_wasm() {
        let wat_src = r#"
(module
  (import "wasi_snapshot_preview1" "fd_write" (func $fd_write (param i32 i32 i32 i32) (result i32)))
  (func (export "_start")
    (call $fd_write (i32.const 1) (i32.const 2) (i32.const 3) (i32.const 4))
    drop
  )
)
        "#;
        let wasm_bytes = wat::parse_str(wat_src).unwrap();
        let program = walk_wasm(&wasm_bytes, "test.wasm").unwrap();
        
        assert_eq!(program.lang.unwrap(), "wasm");
        assert!(program.functions.contains_key("_start"));
        assert!(program.functions.contains_key("main"));
        
        let start_func = &program.functions["_start"];
        assert_eq!(start_func.body.len(), 1);
        if let Statement::ExprStmt { expr, .. } = &start_func.body[0] {
            if let Expression::CapabilityCall { name, .. } = expr {
                assert_eq!(name, "io.print");
            } else {
                panic!("Expected Expression::CapabilityCall");
            }
        } else {
            panic!("Expected Statement::ExprStmt");
        }
    }
}

// ── Adapter ──────────────────────────────────────────────────────────────────

use crush_walker_core::LanguageAdapter;

pub struct WasmAdapter;
impl LanguageAdapter for WasmAdapter {
    fn language_name(&self) -> &'static str { "wasm" }
    fn file_extensions(&self) -> &[&'static str] { &["wasm"] }
    fn walk(&self, source: &str, _filename: &str) -> anyhow::Result<(crush_walker_core::FeatureReport, crush_cast::Program)> {
        let program = crate::walk_wasm(source.as_bytes(), "input.wasm")
            .map_err(|e| anyhow::anyhow!("wasm@walk: {e:?}"))?;
        Ok((crush_walker_core::FeatureReport { lang: "wasm".to_string(), ..Default::default() }, program))
    }
}
