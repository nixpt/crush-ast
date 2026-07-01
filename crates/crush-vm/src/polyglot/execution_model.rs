//! Execution models and conversion strategies

use serde::{Deserialize, Serialize};


/// Execution models for different languages
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ExecutionModel {
    /// Interpreted execution (Python, Ruby, etc.)
    Interpreted,
    /// JIT compilation (JavaScript, Lua, etc.)
    JIT,
    /// Ahead-of-time compilation (Rust, Go, C++, etc.)
    AOT,
    /// WASM-based execution
    WASM,
    /// Mixed execution (Java, C# with JIT)
    Mixed,
    /// Native execution with OS integration
    Native,
}

/// Cross-language argument passing
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CrossLangArg {
    /// Pass by value with type conversion
    Value {
        value: serde_json::Value,
        target_type: Option<String>,
    },
    /// Pass by reference (language-specific object)
    Reference { ref_id: String, source_lang: String },
    /// Pass callback function
    Callback {
        function_ref: String,
        signature: String,
    },
}

/// Bridge types for language interop
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BridgeType {
    /// Direct function calls
    FunctionBridge,
    /// Object property access
    PropertyBridge,
    /// Exception propagation
    ExceptionBridge,
    /// Memory buffer sharing
    MemoryBridge,
    /// Event system integration
    EventBridge,
}

/// Data conversion strategies
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ConversionStrategy {
    /// Automatic type conversion
    Automatic,
    /// Strict type checking
    Strict,
    /// Custom conversion function
    Custom { converter_ref: String },
    /// Raw memory transfer
    RawMemory,
    /// Language-specific serialization
    Serialized { format: String },
}

