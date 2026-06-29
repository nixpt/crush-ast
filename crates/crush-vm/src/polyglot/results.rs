//! Result types for polyglot execution

use std::collections::HashMap;
use super::execution_model::{ExecutionModel, ConversionStrategy};


/// Results from polyglot program execution
#[derive(Debug)]
pub struct PolyglotExecutionResult {
    pub base_result: Option<crate::vm::ExecutionResult>,
    pub lang_results: HashMap<String, LanguageExecutionResult>,
    pub cross_lang_calls: Vec<CrossLangCallResult>,
    pub data_transfers: Vec<DataTransferResult>,
    pub performance_metrics: HashMap<String, f64>,
}

/// Result from executing code in a specific language
#[derive(Debug)]
pub struct LanguageExecutionResult {
    pub language: String,
    pub execution_time: f64,
    pub memory_used: usize,
    pub success: bool,
    pub output: Option<String>,
    pub error: Option<String>,
    pub execution_model: ExecutionModel,
}

/// Result from cross-language function call
#[derive(Debug)]
pub struct CrossLangCallResult {
    pub target_lang: String,
    pub module: String,
    pub function: String,
    pub success: bool,
    pub result: Option<serde_json::Value>,
    pub execution_time: f64,
}

/// Result from data transfer between languages
#[derive(Debug)]
pub struct DataTransferResult {
    pub from_lang: String,
    pub to_lang: String,
    pub data_ref: String,
    pub success: bool,
    pub bytes_transferred: usize,
    pub conversion_strategy: ConversionStrategy,
}
