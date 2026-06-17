//! # CASM - Crush Assembly
//!
//! Low-level bytecode format for the Crush execution environment.
//!
//! ## Overview
//!
//! CASM (Crush Assembly) is the intermediate representation used by the Crush VM.
//! It provides a stack-based instruction set with support for:
//!
//! - **Stack operations**: Push, pop, dup, swap
//! - **Arithmetic**: Add, sub, mul, div, mod
//! - **Comparison**: Eq, ne, lt, gt, le, ge
//! - **Control flow**: Jump, conditional jumps, call, return
//! - **Capability calls**: Invoke external capabilities
//!
//! ## Architecture
//!
//! ```text
//! Source Code (Crush/Python/etc.)
//!        ↓
//!     CAST (AST)
//!        ↓
//!     CASM (Bytecode)
//!        ↓
//!     CrushVM
//! ```
//!
//! ## Program Structure
//!
//! A CASM program consists of:
//! - **Version**: CASM format version
//! - **Functions**: Named functions with parameters and instructions
//! - **Manifest**: Capability permissions
//!
//! ## Example
//!
//! ```json
//! {
//!   "version": "1.0",
//!   "functions": {
//!     "main": {
//!       "params": [],
//!       "locals": [],
//!       "body": [
//!         {"op": "push_int", "value": 42},
//!         {"op": "cap_call", "name": "io.print", "argc": 1}
//!       ]
//!     }
//!   }
//! }
//! ```

use crush_errors::CrushResult;
pub use crush_errors::convert::casm::CasmError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

pub mod debug_info;
pub mod ecasm;

pub use debug_info::{DebugInfo, SourceLocation};

pub type Result<T> = CrushResult<T>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpCode {
    // Stack operations
    PushInt(i64),
    PushFloat(f64),
    PushStr(String),
    PushBool(bool),
    PushNull,
    Pop,
    Dup,

    // Memory operations
    Store(String),
    Load(String),
    ExportVar(String),
    ImportVar(String),

    // Arithmetic operations
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Neg,

    // Comparison operations
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,

    // Logical operations
    And,
    Or,
    Not,

    // Bitwise operations
    BitAnd,
    BitOr,
    BitXor,
    BitNot,
    Shl,
    Shr,

    // Stack manipulation
    Swap,
    Rot,
    Pick(usize), // Copy nth item from stack top
    Roll(usize), // Move nth item to stack top

    // Control flow
    Jmp(usize),
    JmpIf(usize),
    JmpIfNot(usize),
    Call(String),
    Ret,
    Break,    // Exit innermost loop
    Continue, // Jump to loop start
    Spawn,    // Create new task
    Yield,    // Yield execution
    Await {
        handle: String,
    }, // Await async operation (event_id from handle)

    // Array operations
    NewArray(usize), // Create array with n elements from stack
    ArrGet,          // array, index -> value
    ArrSet,          // array, index, value -> array
    ArrLen,          // array -> length
    ArrPush,         // array, value -> array
    ArrPop,          // array -> array, value

    // Object operations
    NewObj,            // Create empty object
    NewStruct(String), // Create named struct
    GetField(String),  // object -> value
    SetField(String),  // object, value -> object

    // Type operations
    TypeOf,       // value -> type string
    Cast(String), // value -> casted value

    // Capability calls
    CapCall {
        name: String,
        argc: usize,
    },
    /// dedicated instruction for calling host capabilities
    CallHost {
        capsule: String,
        ic_id: [u8; 32],
        method: String,
        argc: usize,
    },
    /// structured interface call per CSCS v1
    CallInterface {
        handle: String, // Variable name holding the ObjectHandle/Token
        method: String,
        argc: usize,
    },

    // Polyglot execution (WASI-based)
    /// Execute code in a language sandbox
    /// lang: language name (python, javascript, rust, etc.)
    /// code: source code to execute
    /// var_count: number of variables to inject from stack
    ExecLang {
        lang: String,
        code: String,
        var_count: usize,
    },

    // Program control
    Halt, // Halt execution

    // String intrinsics
    StrContains, // str, pattern -> bool
    StrSplit,    // str, delimiter -> array[str]
    StrReplace,  // str, old, new -> str
    StrJoin,     // array[str], delimiter -> str
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Function {
    #[serde(default)]
    pub params: Vec<String>,
    #[serde(default)]
    pub locals: Vec<String>,
    pub body: Vec<Instruction>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Instruction {
    pub op: String,
    #[serde(default, rename = "instr_lang")]
    pub lang: Option<String>,
    #[serde(default)]
    pub meta: Option<serde_json::Value>,
    #[serde(flatten)]
    pub args: serde_json::Value,
}

/// Cached instruction with pre-parsed opcode and Arc<str> caching for maximum performance
///
/// This structure eliminates string parsing and allocations during execution by caching
/// the parsed OpCode and using Arc<str> for string data.
#[derive(Debug, Clone)]
pub struct CachedInstruction {
    /// Original instruction for debugging and serialization
    pub instruction: Instruction,
    /// Pre-parsed opcode for fast dispatch
    pub opcode: OpCode,
    /// Cached operation name as Arc<str> to avoid allocations
    pub op_cached: Arc<str>,
}

impl CachedInstruction {
    /// Create a new cached instruction with parsed opcode and Arc<str> caching
    pub fn new(instruction: Instruction) -> Result<Self> {
        let opcode = instruction.to_opcode()?;
        let op_cached = Arc::from(instruction.op.as_str());
        Ok(Self {
            instruction,
            opcode,
            op_cached,
        })
    }

    /// Get the opcode without any parsing overhead
    #[inline]
    pub fn opcode(&self) -> &OpCode {
        &self.opcode
    }

    /// Get the cached operation name as Arc<str> (zero-copy access)
    #[inline]
    pub fn op_cached(&self) -> &Arc<str> {
        &self.op_cached
    }

    /// Get access to the original instruction
    #[inline]
    pub fn instruction(&self) -> &Instruction {
        &self.instruction
    }
}

impl Instruction {
    fn require_field<T, F>(&self, field: &str, extract: F) -> Result<T>
    where
        F: FnOnce(&serde_json::Value) -> Option<T>,
    {
        self.args.get(field).and_then(extract).ok_or_else(|| {
            CasmError::MissingField {
                op: self.op.clone(),
                field: field.to_string(),
            }
            .into()
        })
    }

    /// Convert JSON instruction to typed OpCode
    pub fn to_opcode(&self) -> Result<OpCode> {
        match self.op.as_str() {
            "push_int" => Ok(OpCode::PushInt(
                self.require_field("value", |v| v.as_i64())?,
            )),
            "push_float" => Ok(OpCode::PushFloat(
                self.require_field("value", |v| v.as_f64())?,
            )),
            "push_str" => {
                Ok(OpCode::PushStr(self.require_field("value", |v| {
                    v.as_str().map(String::from)
                })?))
            }
            "push_bool" => Ok(OpCode::PushBool(
                self.require_field("value", |v| v.as_bool())?,
            )),
            "push_null" => Ok(OpCode::PushNull),
            "pop" => Ok(OpCode::Pop),
            "dup" => Ok(OpCode::Dup),
            "store" => Ok(OpCode::Store(
                self.require_field("name", |v| v.as_str().map(String::from))?,
            )),
            "load" => Ok(OpCode::Load(
                self.require_field("name", |v| v.as_str().map(String::from))?,
            )),
            "export_var" => Ok(OpCode::ExportVar(
                self.require_field("name", |v| v.as_str().map(String::from))?,
            )),
            "import_var" => Ok(OpCode::ImportVar(
                self.require_field("name", |v| v.as_str().map(String::from))?,
            )),
            "add" => Ok(OpCode::Add),
            "sub" => Ok(OpCode::Sub),
            "mul" => Ok(OpCode::Mul),
            "div" => Ok(OpCode::Div),
            "mod" => Ok(OpCode::Mod),
            "neg" => Ok(OpCode::Neg),
            "eq" => Ok(OpCode::Eq),
            "ne" => Ok(OpCode::Ne),
            "lt" => Ok(OpCode::Lt),
            "gt" => Ok(OpCode::Gt),
            "le" => Ok(OpCode::Le),
            "ge" => Ok(OpCode::Ge),
            "and" => Ok(OpCode::And),
            "or" => Ok(OpCode::Or),
            "not" => Ok(OpCode::Not),
            "bit_and" => Ok(OpCode::BitAnd),
            "bit_or" => Ok(OpCode::BitOr),
            "bit_xor" => Ok(OpCode::BitXor),
            "bit_not" => Ok(OpCode::BitNot),
            "shl" => Ok(OpCode::Shl),
            "shr" => Ok(OpCode::Shr),
            "swap" => Ok(OpCode::Swap),
            "rot" => Ok(OpCode::Rot),
            "pick" => Ok(OpCode::Pick(
                self.require_field("n", |v| v.as_u64().map(|n| n as usize))?,
            )),
            "roll" => Ok(OpCode::Roll(
                self.require_field("n", |v| v.as_u64().map(|n| n as usize))?,
            )),
            "jmp" => {
                Ok(OpCode::Jmp(self.require_field("target", |v| {
                    v.as_u64().map(|n| n as usize)
                })?))
            }
            "jmp_if" => {
                Ok(OpCode::JmpIf(self.require_field("target", |v| {
                    v.as_u64().map(|n| n as usize)
                })?))
            }
            "jmp_if_not" => {
                Ok(OpCode::JmpIfNot(self.require_field("target", |v| {
                    v.as_u64().map(|n| n as usize)
                })?))
            }
            "call" => {
                Ok(OpCode::Call(self.require_field("function", |v| {
                    v.as_str().map(String::from)
                })?))
            }
            "ret" => Ok(OpCode::Ret),
            "break" => Ok(OpCode::Break),
            "continue" => Ok(OpCode::Continue),
            "spawn" => Ok(OpCode::Spawn),
            "yield" => Ok(OpCode::Yield),
            "await" => Ok(OpCode::Await {
                handle: self
                    .args
                    .get("handle")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            }),
            "new_array" => Ok(OpCode::NewArray(
                self.args.get("size").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
            )),
            "arr_get" => Ok(OpCode::ArrGet),
            "arr_set" => Ok(OpCode::ArrSet),
            "arr_len" => Ok(OpCode::ArrLen),
            "arr_push" => Ok(OpCode::ArrPush),
            "arr_pop" => Ok(OpCode::ArrPop),
            "new_obj" => Ok(OpCode::NewObj),
            "new_struct" => Ok(OpCode::NewStruct(
                self.args
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Anonymous")
                    .to_string(),
            )),
            "get_field" => Ok(OpCode::GetField(
                self.require_field("name", |v| v.as_str().map(String::from))?,
            )),
            "set_field" => Ok(OpCode::SetField(
                self.require_field("name", |v| v.as_str().map(String::from))?,
            )),
            "type_of" => Ok(OpCode::TypeOf),
            "cast" => Ok(OpCode::Cast(
                self.require_field("type", |v| v.as_str().map(String::from))?,
            )),
            "cap_call" => Ok(OpCode::CapCall {
                name: self.require_field("name", |v| v.as_str().map(String::from))?,
                argc: self.args.get("argc").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
            }),
            "call_host" => {
                let ic_id_hex = self.require_field("ic_id", |v| v.as_str().map(String::from))?;
                let mut ic_id = [0u8; 32];
                hex::decode_to_slice(&ic_id_hex, &mut ic_id)
                    .map_err(|e| CasmError::InvalidHex(e.to_string()))?;

                Ok(OpCode::CallHost {
                    capsule: self.require_field("capsule", |v| v.as_str().map(String::from))?,
                    ic_id,
                    method: self.require_field("method", |v| v.as_str().map(String::from))?,
                    argc: self.args.get("argc").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
                })
            }
            "str_contains" => Ok(OpCode::StrContains),
            "str_split" => Ok(OpCode::StrSplit),
            "str_replace" => Ok(OpCode::StrReplace),
            "halt" => Ok(OpCode::Halt),
            "str_join" => Ok(OpCode::StrJoin),
            "exec_lang" => {
                let lang = self.require_field("lang", |v| v.as_str().map(String::from))?;
                let code = self.require_field("code", |v| v.as_str().map(String::from))?;
                let var_count = self.args.get("var_count").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                Ok(OpCode::ExecLang { lang, code, var_count })
            }
            "call_interface" => Ok(OpCode::CallInterface {
                handle: self.require_field("handle", |v| v.as_str().map(String::from))?,
                method: self.require_field("method", |v| v.as_str().map(String::from))?,
                argc: self.args.get("argc").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
            }),
            _ => Err(CasmError::UnknownOpcode(self.op.clone()).into()),
        }
    }
}

// Manifest for capability permissions
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Manifest {
    pub permissions: Vec<String>,
}

/// Serialization format for CASM programs
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Format {
    Json,   // Human-readable (.casm)
    Binary, // Compact binary (.casmb)
}

impl Format {
    /// Detect format from file extension
    pub fn from_path(path: &std::path::Path) -> Self {
        match path.extension().and_then(|e| e.to_str()) {
            Some("casmb") => Format::Binary,
            _ => Format::Json,
        }
    }
}

/// The CASM bytecode format version this runtime supports.
///
/// Compatibility is gated on the **major** component: a program whose major
/// version differs (or whose `version` field is unparseable) is rejected at
/// load time with `CasmError::Version` (the unified `crush_errors::VersionMismatch`,
/// boundary = casm). Minor bumps stay compatible.
/// Mirrors the IPC/ABI `Envelope` version gate (`exo-kernel::abi`, VER-02 of the
/// v1.7 version-boundary work) — the second of four load-time boundaries.
pub const CASM_VERSION: &str = "1.0";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Program {
    pub version: String,
    pub functions: HashMap<String, Function>,
    #[serde(default)]
    pub manifest: Manifest,
    /// Optional language hint for the program (e.g., "crush", "python")
    #[serde(default)]
    pub lang: Option<String>,
}

/// Cached program with pre-parsed opcodes for maximum performance
///
/// This structure optimizes execution by pre-parsing all instructions
/// into opcodes, eliminating the need for string-based dispatch during runtime.
#[derive(Debug, Clone)]
pub struct CachedProgram {
    /// Original program for reference
    pub program: Program,
    /// Pre-parsed functions with cached instructions
    pub cached_functions: HashMap<String, CachedFunction>,
}

/// Cached function with pre-parsed instructions and Arc<str> caching
#[derive(Debug, Clone)]
pub struct CachedFunction {
    /// Original function reference
    pub function: Function,
    /// Pre-parsed instructions for fast execution
    pub instructions: Vec<CachedInstruction>,
    /// Cached function name as Arc<str> to avoid allocations
    pub name_cached: Arc<str>,
    /// Fast call target lookup - pre-resolved function indices for common calls
    pub call_targets: HashMap<Arc<str>, usize>,
}

impl Program {
    /// Convert this program to a cached version for maximum performance
    ///
    /// This method pre-parses all instructions into opcodes, eliminating
    /// the need for string-based dispatch during execution.
    ///
    /// Performance impact:
    /// - Before: Every instruction requires string hashing and matching
    /// - After: Direct enum dispatch, 10-100x faster execution
    pub fn to_cached(&self) -> Result<CachedProgram> {
        let mut cached_functions = HashMap::new();

        for (name, function) in &self.functions {
            let mut instructions = Vec::with_capacity(function.body.len());

            for instruction in &function.body {
                let cached = CachedInstruction::new(instruction.clone())?;
                instructions.push(cached);
            }

            // Pre-build call target lookup for fast dispatch
            let mut call_targets = HashMap::new();
            for (i, other_func_name) in self.functions.keys().enumerate() {
                call_targets.insert(Arc::from(other_func_name.as_str()), i);
            }

            cached_functions.insert(
                name.clone(),
                CachedFunction {
                    function: function.clone(),
                    instructions,
                    name_cached: Arc::from(name.as_str()),
                    call_targets,
                },
            );
        }

        Ok(CachedProgram {
            program: self.clone(),
            cached_functions,
        })
    }
}

impl Program {
    /// Serialize program to bytes
    pub fn serialize(&self, format: Format) -> Result<Vec<u8>> {
        match format {
            Format::Json => serde_json::to_vec_pretty(self)
                .map_err(|e| CasmError::SerializationError(e.to_string()).into()),
            Format::Binary => {
                let mut data = Vec::new();
                // Add shebang
                data.extend_from_slice(b"#!/usr/bin/env crush run\n");
                rmp_serde::encode::write(&mut data, self).map_err(|e| {
                    crush_errors::CrushError::from(CasmError::SerializationError(e.to_string()))
                })?;
                Ok(data)
            }
        }
    }

    /// Major version component, if `version` parses as `MAJOR[.MINOR...]`.
    fn major_of(version: &str) -> Option<u32> {
        version.split('.').next()?.parse().ok()
    }

    /// Load-time CASM version gate (VER-02).
    ///
    /// Accepts programs whose major version matches [`CASM_VERSION`]'s major;
    /// rejects a differing major or an unparseable `version` with a typed
    /// `CasmError::Version` carrying the unified `crush_errors::VersionMismatch`
    /// (boundary = casm). Modeled on the `Envelope` ABI gate.
    pub fn check_version(&self) -> Result<()> {
        let supported_major = Self::major_of(CASM_VERSION);
        match Self::major_of(&self.version) {
            Some(found) if Some(found) == supported_major => Ok(()),
            _ => Err(CasmError::Version(crush_errors::VersionMismatch::casm(
                CASM_VERSION,
                self.version.clone(),
            ))
            .into()),
        }
    }

    /// Deserialize program from bytes.
    ///
    /// Enforces the [`check_version`](Self::check_version) gate after parsing so
    /// every execution load path (JSON or binary) fails closed on an
    /// incompatible bytecode version.
    pub fn deserialize(data: &[u8], format: Format) -> Result<Self> {
        let program: Self = match format {
            Format::Json => serde_json::from_slice(data)
                .map_err(|e| crush_errors::CrushError::from(CasmError::DeserializationError(e.to_string())))?,
            Format::Binary => {
                // Check for shebang and skip it
                let data = if data.starts_with(b"#!") {
                    if let Some(pos) = data.iter().position(|&b| b == b'\n') {
                        &data[pos + 1..]
                    } else {
                        data
                    }
                } else {
                    data
                };
                rmp_serde::from_slice(data)
                    .map_err(|e| crush_errors::CrushError::from(CasmError::DeserializationError(e.to_string())))?
            }
        };
        program.check_version()?;
        Ok(program)
    }

    /// Save program to file (format detected from extension)
    pub fn save(&self, path: &std::path::Path) -> Result<()> {
        let format = Format::from_path(path);
        let data = self.serialize(format)?;
        std::fs::write(path, data).map_err(|e| CasmError::IoError(e.to_string()))?;

        // Make executable if binary
        #[cfg(unix)]
        if format == Format::Binary {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(metadata) = std::fs::metadata(path) {
                let mut perms = metadata.permissions();
                perms.set_mode(perms.mode() | 0o111);
                let _ = std::fs::set_permissions(path, perms);
            }
        }

        Ok(())
    }

    /// Load program from file (format detected from extension)
    pub fn load(path: &std::path::Path) -> Result<Self> {
        let data = std::fs::read(path).map_err(|e| CasmError::IoError(e.to_string()))?;
        let format = Format::from_path(path);
        Self::deserialize(&data, format)
    }
}

impl Default for Program {
    fn default() -> Self {
        Self {
            version: "1.0".to_string(),
            functions: HashMap::new(),
            manifest: Manifest::default(),
            lang: None,
        }
    }
}

/// Format runtime errors with source location when available.
pub fn format_runtime_error_with_location(
    message: &str,
    debug_info: Option<&DebugInfo>,
    pc: usize,
) -> String {
    if let Some(loc) = debug_info.and_then(|d| d.source_location_for_pc(pc)) {
        return format!("Error at line {}, col {}: {}", loc.line, loc.col, message);
    }
    message.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_error_location_formats_with_source_location() {
        let mut dbg = DebugInfo::new();
        dbg.push_source_location(SourceLocation::new(42, 10, Some("main.crush".to_string())));

        let msg = format_runtime_error_with_location("division by zero", Some(&dbg), 0);
        assert_eq!(msg, "Error at line 42, col 10: division by zero");
    }

    #[test]
    fn runtime_error_location_falls_back_without_source() {
        let dbg = DebugInfo::new();
        let msg = format_runtime_error_with_location("division by zero", Some(&dbg), 0);
        assert_eq!(msg, "division by zero");
    }

    // VER-02 CASM version-gate tests live in `tests/ver02_version_gate.rs`
    // (an integration test) so they compile against the public API and dodge
    // the pre-existing uncompilable `ecasm.rs` inline tests (see EXO-151).
}
