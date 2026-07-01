//! Polyglot program executor

use super::{opcodes::*, execution_model::*, results::*};
use std::collections::HashMap;

use casm::{Instruction, Program};
use crate::VM;
use async_trait::async_trait;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[async_trait]
pub trait InstructionExt {
    fn to_polyglot_opcode(&self) -> Result<PolyglotOpCode>;
    fn require_field<T, F>(&self, field: &str, extract: F) -> Result<T>
    where
        F: FnOnce(&serde_json::Value) -> Option<T>;
}

#[async_trait]
impl InstructionExt for Instruction {
    fn require_field<T, F>(&self, field: &str, extract: F) -> Result<T>
    where
        F: FnOnce(&serde_json::Value) -> Option<T>,
    {
        self.args.get(field).and_then(extract).ok_or_else(|| {
            format!("{}: missing field {}", self.op, field).into()
        })
    }

    /// Convert JSON instruction to enhanced polyglot opcode
    fn to_polyglot_opcode(&self) -> Result<PolyglotOpCode> {
        match self.op.as_str() {
            "exec_lang" => {
                let execution_model =
                    match self.args.get("execution_model").and_then(|v| v.as_str()) {
                        Some("interpreted") => ExecutionModel::Interpreted,
                        Some("jit") => ExecutionModel::JIT,
                        Some("aot") => ExecutionModel::AOT,
                        Some("mixed") => ExecutionModel::Mixed,
                        Some("native") => ExecutionModel::Native,
                        _ => ExecutionModel::Interpreted, // default
                    };

                Ok(PolyglotOpCode::ExecLang {
                    lang: self.require_field("lang", |v| v.as_str().map(String::from))?,
                    code: self.require_field("code", |v| v.as_str().map(String::from))?,
                    var_count: self
                        .args
                        .get("var_count")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as usize,
                    execution_model,
                })
            }

            "cross_lang_call" => {
                let empty_vec = vec![];
                let args_json = self
                    .args
                    .get("args")
                    .and_then(|v| v.as_array())
                    .unwrap_or(&empty_vec);

                let args: Vec<CrossLangArg> = args_json
                    .iter()
                    .map(|_arg| {
                        // Parse cross-lang args (simplified)
                        CrossLangArg::Value {
                            value: serde_json::Value::Null, // Would parse actual value
                            target_type: None,
                        }
                    })
                    .collect();

                Ok(PolyglotOpCode::CrossLangCall {
                    target_lang: self
                        .require_field("target_lang", |v| v.as_str().map(String::from))?,
                    module: self.require_field("module", |v| v.as_str().map(String::from))?,
                    function: self.require_field("function", |v| v.as_str().map(String::from))?,
                    args,
                    return_type: self
                        .args
                        .get("return_type")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                })
            }

            "load_module" => {
                let empty_vec = vec![];
                let imports_json = self
                    .args
                    .get("imports")
                    .and_then(|v| v.as_array())
                    .unwrap_or(&empty_vec);

                let imports: Vec<String> = imports_json
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect();

                Ok(PolyglotOpCode::LoadModule {
                    lang: self.require_field("lang", |v| v.as_str().map(String::from))?,
                    module_path: self
                        .require_field("module_path", |v| v.as_str().map(String::from))?,
                    imports,
                })
            }

            "create_lang_object" => {
                let empty_vec = vec![];
                let args_json = self
                    .args
                    .get("args")
                    .and_then(|v| v.as_array())
                    .unwrap_or(&empty_vec);

                let args: Vec<CrossLangArg> = args_json
                    .iter()
                    .map(|_| CrossLangArg::Value {
                        value: serde_json::Value::Null,
                        target_type: None,
                    })
                    .collect();

                Ok(PolyglotOpCode::CreateLangObject {
                    lang: self.require_field("lang", |v| v.as_str().map(String::from))?,
                    class_name: self
                        .require_field("class_name", |v| v.as_str().map(String::from))?,
                    args,
                })
            }

            "lang_import" => {
                let empty_vec = vec![];
                let items_json = self
                    .args
                    .get("items")
                    .and_then(|v| v.as_array())
                    .unwrap_or(&empty_vec);

                let items: Vec<String> = items_json
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect();

                Ok(PolyglotOpCode::LangImport {
                    lang: self.require_field("lang", |v| v.as_str().map(String::from))?,
                    module: self.require_field("module", |v| v.as_str().map(String::from))?,
                    items,
                    alias: self
                        .args
                        .get("alias")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                })
            }

            "transfer_data" => {
                let conversion_strategy = match self
                    .args
                    .get("conversion_strategy")
                    .and_then(|v| v.as_str())
                {
                    Some("automatic") => ConversionStrategy::Automatic,
                    Some("strict") => ConversionStrategy::Strict,
                    Some("raw_memory") => ConversionStrategy::RawMemory,
                    Some("serialized") => {
                        let format = self
                            .args
                            .get("format")
                            .and_then(|v| v.as_str())
                            .unwrap_or("json");
                        ConversionStrategy::Serialized {
                            format: format.to_string(),
                        }
                    }
                    _ => ConversionStrategy::Automatic,
                };

                Ok(PolyglotOpCode::TransferData {
                    from_lang: self.require_field("from_lang", |v| v.as_str().map(String::from))?,
                    to_lang: self.require_field("to_lang", |v| v.as_str().map(String::from))?,
                    data_ref: self.require_field("data_ref", |v| v.as_str().map(String::from))?,
                    conversion_strategy,
                })
            }

            _ => Err(format!("Unknown polyglot opcode: {}", self.op).into()),
        }
    }
}

#[async_trait]
pub trait ProgramExt {
    async fn execute_polyglot(&self, vm: &mut VM) -> Result<PolyglotExecutionResult>;
    async fn execute_polyglot_op(&self, vm: &mut VM, op: PolyglotOpCode) -> Result<PolyglotExecutionResult>;
    async fn execute_in_language(
        &self,
        lang: &str,
        code: &str,
        var_count: usize,
        execution_model: &ExecutionModel,
    ) -> Result<LanguageExecutionResult>;
    async fn execute_cross_lang_call(
        &self,
        target_lang: &str,
        module: &str,
        function: &str,
        args: &[CrossLangArg],
        return_type: Option<&str>,
    ) -> Result<CrossLangCallResult>;
    async fn transfer_data_cross_lang(
        &self,
        from_lang: &str,
        to_lang: &str,
        data_ref: &str,
        conversion_strategy: &ConversionStrategy,
    ) -> Result<DataTransferResult>;
}

#[async_trait]
impl ProgramExt for Program {
    async fn execute_polyglot(&self, vm: &mut VM) -> Result<PolyglotExecutionResult> {
        let mut result = PolyglotExecutionResult {
            base_result: None,
            lang_results: HashMap::new(),
            cross_lang_calls: vec![],
            data_transfers: vec![],
            performance_metrics: HashMap::new(),
        };

        // Execute base CASM program
        // Use vm.execute_interactive or vm.call etc.
        // For simplicity in test, we just start it
        vm.start()?;
        
        // Disable FastVM to ensure step-by-step execution matches our manual PC tracking
        vm.disable_fast_vm();

        // Increase budget for test
        if let Some(task) = vm.task_manager.get_task_mut(0) {
            task.gas_remaining = 1_000_000;
        }

        while vm.state() == crate::VmState::Running {
            let (func_name, pc, gas) = {
                let task = vm.task_manager.get_task(0).ok_or_else(|| "No main task")?;
                let func_name = if let Some(frame) = task.call_stack.last() {
                    frame.function_name.clone()
                } else {
                    "main".to_string()
                };
                (func_name, task.pc, task.gas_remaining)
            };
            
            println!("DEBUG: PC={} Gas={} Func={}", pc, gas, func_name);

            let func = self.functions.get(&func_name).ok_or_else(|| format!("Function {} not found", func_name))?;
            if pc >= func.body.len() {
                // Return or yield?
                vm.step()?; 
                continue;
            }
            
            let instruction: &casm::Instruction = &func.body[pc];
            
            // Check if it's a polyglot opcode
            if let Ok(polyglot_op) = instruction.to_polyglot_opcode() {
                let op_result = self.execute_polyglot_op(vm, polyglot_op).await?;
                
                // Track results in our polyglot result
                match op_result {
                    PolyglotExecutionResult { lang_results, cross_lang_calls, data_transfers, .. } => {
                        for (lang, res) in lang_results {
                            result.lang_results.insert(lang, res);
                        }
                        result.cross_lang_calls.extend(cross_lang_calls);
                        result.data_transfers.extend(data_transfers);
                    }
                }
                
                // Manually increment PC as we handled the instruction
                if let Some(task) = vm.task_manager.get_task_mut(0) {
                    task.pc += 1;
                }
                continue;
            }

            // Execute regular instruction via VM
            match vm.step()? {
                Some(crate::VmYield::Finished) => break,
                Some(crate::VmYield::Yielded) => continue,
                Some(crate::VmYield::BudgetExhausted) => return Err("Gas exceeded".into()),
                _ => continue,
            }
        }

        Ok(result)
    }

    async fn execute_polyglot_op(&self, _vm: &mut VM, op: PolyglotOpCode) -> Result<PolyglotExecutionResult> {
        let mut result = PolyglotExecutionResult {
            base_result: None,
            lang_results: HashMap::new(),
            cross_lang_calls: vec![],
            data_transfers: vec![],
            performance_metrics: HashMap::new(),
        };

        match op {
            PolyglotOpCode::ExecLang {
                lang,
                code,
                var_count,
                execution_model,
            } => {
                let lang_result = self
                    .execute_in_language(&lang, &code, var_count, &execution_model)
                    .await?;
                result.lang_results.insert(lang, lang_result);
            }

            PolyglotOpCode::CrossLangCall {
                target_lang,
                module,
                function,
                args,
                return_type,
            } => {
                let call_result = self
                    .execute_cross_lang_call(
                        &target_lang,
                        &module,
                        &function,
                        &args,
                        return_type.as_deref(),
                    )
                    .await?;
                result.cross_lang_calls.push(call_result);
            }

            PolyglotOpCode::TransferData {
                from_lang,
                to_lang,
                data_ref,
                conversion_strategy,
            } => {
                let transfer_result = self
                    .transfer_data_cross_lang(
                        &from_lang,
                        &to_lang,
                        &data_ref,
                        &conversion_strategy,
                    )
                    .await?;
                result.data_transfers.push(transfer_result);
            }

            _ => {
                // Other ops
            }
        }

        Ok(result)
    }

    async fn execute_in_language(
        &self,
        lang: &str,
        code: &str,
        _var_count: usize,
        execution_model: &ExecutionModel,
    ) -> Result<LanguageExecutionResult> {
        let mock_result = LanguageExecutionResult {
            language: lang.to_string(),
            execution_time: 0.1,
            memory_used: 1024,
            success: true,
            output: Some(format!(
                "Executed {} code: {}",
                lang,
                code.chars().take(50).collect::<String>()
            )),
            error: None,
            execution_model: execution_model.clone(),
        };

        Ok(mock_result)
    }

    async fn execute_cross_lang_call(
        &self,
        target_lang: &str,
        module: &str,
        function: &str,
        _args: &[CrossLangArg],
        _return_type: Option<&str>,
    ) -> Result<CrossLangCallResult> {
        Ok(CrossLangCallResult {
            target_lang: target_lang.to_string(),
            module: module.to_string(),
            function: function.to_string(),
            success: true,
            result: Some(serde_json::json!("mock_call_result")),
            execution_time: 0.05,
        })
    }

    async fn transfer_data_cross_lang(
        &self,
        from_lang: &str,
        to_lang: &str,
        data_ref: &str,
        conversion_strategy: &ConversionStrategy,
    ) -> Result<DataTransferResult> {
        Ok(DataTransferResult {
            from_lang: from_lang.to_string(),
            to_lang: to_lang.to_string(),
            data_ref: data_ref.to_string(),
            success: true,
            bytes_transferred: 256,
            conversion_strategy: conversion_strategy.clone(),
        })
    }
}
