//! Enhanced opcodes for polyglot execution

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum PolyglotOpCode {
    /// Execute code in a language runtime (enhanced version)
    ExecLang {
        lang: String,
        code: String,
        var_count: usize,
        execution_model: ExecutionModel,
    },

    /// Call function across language boundaries
    CrossLangCall {
        target_lang: String,
        module: String,
        function: String,
        args: Vec<CrossLangArg>,
        return_type: Option<String>,
    },

    /// Load and execute a language module
    LoadModule {
        lang: String,
        module_path: String,
        imports: Vec<String>,
    },

    /// Create language-specific object
    CreateLangObject {
        lang: String,
        class_name: String,
        args: Vec<CrossLangArg>,
    },

    /// Access property on cross-language object
    AccessLangProperty {
        object_ref: String,
        property: String,
        lang: String,
    },

    /// Set property on cross-language object
    SetLangProperty {
        object_ref: String,
        property: String,
        value: CrossLangArg,
        lang: String,
    },

    /// Execute language-specific expression
    EvalLangExpression {
        lang: String,
        expression: String,
        context_vars: Vec<String>,
    },

    /// Import from language ecosystem
    LangImport {
        lang: String,
        module: String,
        items: Vec<String>,
        alias: Option<String>,
    },

    /// Export to language ecosystem
    LangExport {
        lang: String,
        name: String,
        value_ref: String,
    },

    /// Handle language-specific exceptions
    CatchLangException {
        lang: String,
        exception_type: String,
        handler_block: Vec<Instruction>,
    },

    /// Yield execution to language runtime
    YieldToLang {
        lang: String,
        continuation: usize, // PC to resume at
    },

    /// Resume from language runtime
    ResumeFromLang { result_ref: String },

    /// Create language bridge for interop
    CreateLangBridge {
        source_lang: String,
        target_lang: String,
        bridge_type: BridgeType,
    },

    /// Transfer data across language boundary
    TransferData {
        from_lang: String,
        to_lang: String,
        data_ref: String,
        conversion_strategy: ConversionStrategy,
    },
}

// Re-export types from execution_model that are needed here
pub use super::execution_model::{ExecutionModel, BridgeType, ConversionStrategy, CrossLangArg};
use casm::Instruction;

