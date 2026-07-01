//! # CRUSH VM Polyglot Execution System
//!
//! This module implements a sophisticated polyglot execution system that enables
//! seamless cross-language execution and interoperability within the CRUSH VM.
//!
//! ## Overview
//!
//! The polyglot execution system provides:
//!
//! - **Cross-Language Execution**: Execute code across multiple programming languages
//! - **Language Interoperability**: Seamless data exchange between different languages
//! - **Enhanced Opcodes**: Specialized opcodes for polyglot operations
//! - **Execution Models**: Flexible execution models for different language combinations
//! - **Type Conversion**: Automatic type conversion and marshaling
//! - **Performance Optimization**: Optimized execution for cross-language calls
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    Polyglot Execution                       │
//! │  ┌─────────────────────────────────────────────────────────┐ │
//! │  │                    Execution Model                      │ │
//! │  │  ┌────────────┐  ┌────────────┐  ┌───────────────────┐  │ │
//! │  │  │ Language   │  │ Bridge     │  │ Conversion        │  │ │
//! │  │  │ Runtimes   │  │ Types      │  │ Strategies        │  │ │
//! │  │  │ • Rust     │  │ • Direct   │  │ • Automatic     │  │ │
//! │  │  │ • Python   │  │ • Proxy    │  │ • Manual        │  │ │
//! │  │  │ • JS       │  │ • Adapter  │  │ • Custom        │  │ │
//! │  │  └────────────┘  └────────────┘  └───────────────────┘  │ │
//! │  │         │              │                    │           │ │
//! │  │         └──────────────┼────────────────────┘           │ │
//! │  │                        │                                │ │
//! │  │  ┌─────────────────────▼────────────────────────────────┐ │ │
//! │  │  │                   Opcodes                           │ │ │
//! │  │  │  ┌────────────┐  ┌────────────┐  ┌───────────────┐  │ │ │
//! │  │  │  │ Language   │  │ Cross-Lang │  │ Data Transfer │  │ │ │
//! │  │  │  │ Switch     │  │ Call       │  │ Operations    │  │ │ │
//! │  │  │  │ • ExecLang │  │ • CallLang │  │ • Marshal     │  │ │ │
//! │  │  │  │ • LangInfo │  │ • Return   │  │ • Unmarshal   │  │ │ │
//! │  │  │  │ • LangState│  │ • Exception│  │ • Validate    │  │ │ │
//! │  │  │  └────────────┘  └────────────┘  └───────────────┘  │ │ │
//! │  │  └─────────────────────────────────────────────────────┘ │ │
//! │  │                        │                                │ │
//! │  │                        ▼                                │ │
//! │  │  ┌─────────────────────▼────────────────────────────────┐ │ │
//! │  │  │                    Executor                         │ │ │
//! │  │  │  ┌────────────┐  └────────────┐  ┌───────────────┐  │ │ │
//! │  │  │  │ Instruction│  │ Program    │  │ Execution     │  │ │ │
//! │  │  │  │ Extensions │  │ Extensions │  │ Control       │  │ │ │
//! │  │  │  │ • LangExec │  │ • LangExec │  │ • Scheduling  │  │ │ │
//! │  │  │  │ • LangCall │  │ • LangCall │  │ • Coordination│  │ │ │
//! │  │  │  │ • LangData │  │ • LangData │  │ • Monitoring  │  │ │ │
//! │  │  │  └────────────┘  └────────────┘  └───────────────┘  │ │ │
//! │  │  └─────────────────────────────────────────────────────┘ │ │
//! │  │                        │                                │ │
//! │  │                        ▼                                │ │
//! │  │  ┌─────────────────────▼────────────────────────────────┐ │ │
//! │  │  │                     Results                         │ │ │
//! │  │  │  ┌────────────┐  └────────────┐  ┌───────────────┐  │ │ │
//! │  │  │  │ Execution  │  │ Language   │  │ Cross-Language│  │ │ │
//! │  │  │  │ Results    │  │ Results    │  │ Calls         │  │ │ │
//! │  │  │  │ • Success  │  │ • Output   │  │ • Data Flow   │  │ │ │
//! │  │  │  │ • Error    │  │ • Errors   │  │ • Performance │  │ │ │
//! │  │  │  │ • Metrics  │  │ • Metrics  │  │ • Monitoring  │  │ │ │
//! │  │  │  └────────────┘  └────────────┘  └───────────────┘  │ │ │
//! │  │  └─────────────────────────────────────────────────────┘ │ │
//! │  └─────────────────────────────────────────────────────────┘ │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Key Components
//!
//! ### Execution Model
//! Flexible execution models for different language combinations:
//! - **Language Runtimes**: Support for multiple language runtimes
//! - **Bridge Types**: Different bridging strategies for language interaction
//! - **Conversion Strategies**: Automatic and manual type conversion approaches
//! - **Execution Control**: Fine-grained control over cross-language execution
//!
//! ### Opcodes
//! Enhanced opcodes specifically designed for polyglot operations:
//! - **Language Switch**: Switch execution context between languages
//! - **Cross-Language Call**: Call functions across language boundaries
//! - **Data Transfer**: Efficient data marshaling and unmarshaling
//! - **Exception Handling**: Cross-language exception propagation
//!
//! ### Executor
//! Core execution engine for polyglot operations:
//! - **Instruction Extensions**: Extended instruction set for polyglot features
//! - **Program Extensions**: Enhanced program structure for cross-language code
//! - **Execution Control**: Scheduling and coordination of language execution
//! - **Performance Monitoring**: Real-time performance tracking
//!
//! ### Results
//! Comprehensive result handling for polyglot execution:
//! - **Execution Results**: Success, error, and performance metrics
//! - **Language Results**: Language-specific output and error handling
//! - **Cross-Language Calls**: Data flow and performance monitoring
//! - **Result Aggregation**: Combined results from multiple languages
//!
//! ## Supported Languages
//!
//! ### Rust
//! Primary language with native performance:
//! - **Zero-Cost Abstractions**: High-level features with no runtime overhead
//! - **Memory Safety**: Compile-time memory safety guarantees
//! - **Performance**: Native compilation with optimization
//! - **Integration**: Direct integration with other language runtimes
//!
//! ### Python
//! High-level scripting with extensive ecosystem:
//! - **Dynamic Typing**: Flexible dynamic typing system
//! - **Extensive Libraries**: Access to Python's vast library ecosystem
//! - **Easy Integration**: Simple integration with other languages
//! - **Rapid Development**: Fast development and prototyping
//!
//! ### JavaScript
//! Web-standard scripting language:
//! - **Event-Driven**: Asynchronous event-driven programming model
//! - **Web Standards**: Access to web-standard APIs
//! - **Performance**: High-performance JavaScript engines
//! - **Portability**: Runs in any JavaScript environment
//!
//! ### WebAssembly
//! Portable binary format:
//! - **Cross-Platform**: Runs on any platform with WASM support
//! - **Performance**: Near-native performance with JIT compilation
//! - **Security**: Sandboxed execution environment
//! - **Portability**: Language-agnostic binary format
//!
//! ## Usage Examples
//!
//! ### Basic Cross-Language Execution
//!
//! ```text
//! use nanovm::polyglot::{PolyglotOpCode, ExecutionModel, BridgeType};
//!
//! // Create execution model
//! let model = ExecutionModel {
//!     bridge_type: BridgeType::Direct,
//!     conversion_strategy: ConversionStrategy::Automatic,
//!     languages: vec!["rust", "python", "javascript"],
//! };
//!
//! // Execute cross-language code
//! let result = vm.execute_polyglot(
//!     &model,
//!     r#"
//!     // Rust code
//!     let rust_result = add(2, 3);
//!
//!     // Python code
//!     python_result = multiply(rust_result, 2)
//!
//!     // JavaScript code
//!     js_result = divide(python_result, 4)
//!     "#,
//! )?;
//!
//! println!("Final result: {:?}", result);
//! ```
//!
//! ### Cross-Language Function Calls
//!
//! ```text
//! use nanovm::polyglot::{CrossLangCallResult, PolyglotOpCode};
//!
//! // Call Python function from Rust
//! let python_result = vm.call_language_function(
//!     "python",
//!     "process_data",
//!     vec![RuntimeValue::Int(42)],
//! )?;
//!
//! // Call JavaScript function from Python
//! let js_result = vm.call_language_function(
//!     "javascript",
//!     "analyze_result",
//!     vec![python_result],
//! )?;
//!
//! // Process final result in Rust
//! let final_result = vm.process_result(js_result)?;
//! ```
//!
//! ### Data Transfer and Marshaling
//!
//! ```text
//! use nanovm::polyglot::{DataTransferResult, PolyglotOpCode};
//!
//! // Transfer data between languages
//! let data = vec![
//!     ("name", RuntimeValue::Str("test".to_string())),
//!     ("value", RuntimeValue::Int(42)),
//!     ("active", RuntimeValue::Bool(true)),
//! ];
//!
//! // Marshal data for Python
//! let python_data = vm.marshal_data("python", &data)?;
//!
//! // Transfer to JavaScript
//! let js_data = vm.transfer_data("python", "javascript", python_data)?;
//!
//! // Unmarshal in JavaScript
//! let final_data = vm.unmarshal_data("javascript", js_data)?;
//! ```
//!
//! ### Exception Handling
//!
//! ```text
//! use nanovm::polyglot::{PolyglotExecutionResult, CrossLangCallResult};
//!
//! match vm.execute_polyglot_with_error_handling(
//!     &model,
//!     r#"
//!     try {
//!         // Rust code that might fail
//!         let result = risky_operation();
//!
//!         // Python code that might fail
//!         python_result = process_rust_result(result)
//!
//!         // JavaScript code that might fail
//!         js_result = finalize_processing(python_result)
//!     } catch (error) {
//!         // Handle cross-language exceptions
//!         handle_error(error)
//!     }
//!     "#,
//! ) {
//!     Ok(result) => println!("Success: {:?}", result),
//!     Err(PolyglotExecutionResult::LanguageError { language, error }) => {
//!         println!("{} error: {}", language, error);
//!     }
//!     Err(PolyglotExecutionResult::ConversionError { from, to, error }) => {
//!         println!("Conversion error from {} to {}: {}", from, to, error);
//!     }
//!     Err(PolyglotExecutionResult::BridgeError { bridge_type, error }) => {
//!         println!("Bridge error ({}): {}", bridge_type, error);
//!     }
//! }
//! ```
//!
//! ## Performance Characteristics
//!
//! ### Cross-Language Performance
//! - **Minimal Overhead**: Optimized bridges minimize cross-language overhead
//! - **Data Marshaling**: Efficient data conversion and marshaling
//! - **Memory Sharing**: Zero-copy data sharing where possible
//! - **Caching**: Intelligent caching of frequently used data and code
//!
//! ### Language-Specific Optimization
//! - **Rust**: Native compilation with full optimization
//! - **Python**: PyPy integration for improved performance
//! - **JavaScript**: V8 optimization with JIT compilation
//! - **WASM**: Native compilation with sandboxing
//!
//! ### Resource Efficiency
//! - **Memory Efficiency**: Shared memory pools where safe
//! - **CPU Efficiency**: Intelligent CPU time allocation
//! - **I/O Efficiency**: Buffered I/O with caching
//! - **Network Efficiency**: Connection pooling and reuse
//!
//! ## Security Features
//!
//! ### Language Isolation
//! - **Sandboxing**: Each language runs in isolated environment
//! - **Resource Limits**: Enforced per-language resource limits
//! - **Permission Control**: Fine-grained permission management
//! - **Security Monitoring**: Continuous security monitoring
//!
//! ### Data Security
//! - **Type Safety**: Type-safe data exchange between languages
//! - **Validation**: Input validation and sanitization
//! - **Encryption**: Secure data transmission where needed
//! - **Audit Logging**: Complete audit trail of cross-language operations
//!
//! ## Integration with VM
//!
//! The polyglot system integrates seamlessly with VM execution:
//!
//! ### Language Execution
//! ```text
//! // Called during polyglot execution
//! let result = polyglot_executor.execute_cross_language(code)?;
//! vm.store_result(result);
//! ```
//!
//! ### Cross-Language Calls
//! ```text
//! // Called for cross-language function calls
//! let result = polyglot_executor.call_language_function(target_language, function_name, args)?;
//! vm.return_result(result);
//! ```
//!
//! ### Data Transfer
//! ```text
//! // Called for data marshaling
//! let marshaled = polyglot_executor.marshal_data(target_language, data)?;
//! vm.transfer_data(marshaled);
//! ```
//!
//! ### Exception Handling
//! ```text
//! // Called for cross-language exception handling
//! let handled = polyglot_executor.handle_exception(exception)?;
//! vm.report_error(handled);
//! ```
//!
//! ## Testing and Validation
//!
//! The polyglot system includes comprehensive tests for:
//! - Cross-language interoperability and data exchange
//! - Performance optimization and resource management
//! - Security isolation and permission control
//! - Error handling and recovery mechanisms
//! - Integration with VM execution
//! - Language-specific feature testing
//!
//! This polyglot execution system provides a robust, secure, and efficient
//! foundation for cross-language execution in the CRUSH VM with excellent
//! performance and interoperability guarantees.

pub mod opcodes;
pub mod execution_model;
pub mod executor;
pub mod results;
pub mod exec;
pub mod executor_registry;
pub mod builtin_executors;

// Re-export main types
pub use opcodes::PolyglotOpCode;
pub use execution_model::{ExecutionModel, BridgeType, ConversionStrategy};
pub use executor::{InstructionExt, ProgramExt};
pub use results::{PolyglotExecutionResult, LanguageExecutionResult, CrossLangCallResult, DataTransferResult};
pub use exec::{LanguageExecutor, NativeExecutor, PolyglotExecutionResult as ExecResult, runtime_value_to_json, json_to_runtime_value};
pub use executor_registry::{
    RuntimeExecutor, ExecutorRegistry, ExecutorResult, 
    FunctionSignature, SessionState,
    register_executor, execute_global, call_function_global,
    supports_language_global, global_registry,
    get_persistent_global, set_persistent_global, clear_persistent_global,
    get_exported_functions_global
};
pub use builtin_executors::register_builtin_executors;