use anyhow::{Context, Result};
use clap::Parser;
use crush_cast::{Expression, Function, Program, Statement};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use walker_core::WalkerError;
use wasmparser::{Parser as WasmParser, Payload, TypeRef};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Path to WASM file
    #[arg(value_name = "FILE")]
    file: PathBuf,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let wasm_bytes =
        fs::read(&cli.file).with_context(|| format!("Failed to read WASM file: {:?}", cli.file))?;

    let program = walk_wasm(&wasm_bytes, cli.file.to_str().unwrap())?;
    println!("{}", serde_json::to_string_pretty(&program)?);

    Ok(())
}

fn walk_wasm(wasm_bytes: &[u8], filename: &str) -> Result<Program, WalkerError> {
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

    // Process functions
    // Note: Function index space includes imports first, then internal functions
    let import_count = imported_funcs.len();

    for (i, body) in func_bodies.iter().enumerate() {
        let func_idx = (import_count + i) as u32;
        let _type_idx = func_types[i];

        // Basic name generation
        let mut func_name = format!("func_{}", func_idx);

        // Use export name if available
        if let Some(export_name) = exported_funcs.get(&func_idx) {
            func_name = (*export_name).to_string();
        }

        // Create function body
        // For now, we'll just parse a few basic instructions to demonstrate
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
                    // Check if it's an imported function (capability)
                    if (function_index as usize) < import_count {
                        let (module, name, _) = imported_funcs[function_index as usize];
                        // Map trivial WASI/env imports to capabilities
                        if module == "wasi_snapshot_preview1" && name == "fd_write" {
                            crush_body.push(Statement::ExprStmt {
                                expr: Expression::CapabilityCall {
                                    name: "io.print".to_string(), // Simplified mapping
                                    args: vec![], // Arguments would need stack analysis
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
                        // Internal call
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
                _ => {
                    // Ignore other ops for this proof-of-concept
                }
            }
        }

        functions.insert(
            func_name,
            Function {
                params: vec![], // Would need to parse type definition
                body: crush_body,
                meta: HashMap::from([
                    ("file".to_string(), serde_json::json!(filename)),
                    ("lang".to_string(), serde_json::json!("wasm")),
                ]),
            },
        );
    }

    // Add entry point wrapper if no main but _start exists (WASI standard)
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
            },
        );
    }

    Ok(Program {
        cast_version: "0.1".to_string(),
        entry: "main".to_string(),
        lang: Some("wasm".to_string()),
        functions,
        ai_meta: None,
    })
}
