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

pub mod assigned;
pub mod debug_info;

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
    /// Call a function value with arguments supplied as an array (CRUSH-293 D3).
    CallValueApply,
    Ret,
    Break,    // Exit innermost loop
    Continue, // Jump to loop start
    Spawn,    // Create new task
    Yield,    // Yield execution
    Await {
        handle: String,
    }, // Await async operation (event_id from handle)
    EnterTry,
    ExitTry,
    Throw,

    // Array operations
    NewArray(usize), // Create array with n elements from stack
    ArrGet,          // array, index -> value
    ArrSet,          // array, index, value -> array
    ArrLen,          // array -> length
    ArrPush,         // array, value -> array
    ArrPop,          // array -> array, value
    /// Frontend alias for indexed collection access.
    Index,
    /// Frontend alias for collection length.
    Len,
    /// Frontend array append operation.
    ArrayPush,
    /// Frontend array pop operation.
    ArrayPop,
    /// Construct a range from its operands.
    MakeRange,

    // Collection operations
    NewTuple(usize),
    TuplePush,
    NewList(usize),
    ListPush,
    NewVector(usize),
    VectorPush,
    NewSet(usize),
    SetPush,

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

    // AI opcodes
    AiQuery(serde_json::Value),
    AiAdaptationRequest(serde_json::Value),
    AiCapabilityDiscovery(serde_json::Value),
    AiSemanticSwitch(serde_json::Value),
    #[serde(rename = "ai_tool_chain")]
    AiToolchain(serde_json::Value),
    AiAgentDelegation(serde_json::Value),
    AiLearningLoop(serde_json::Value),
    AiContextAware(serde_json::Value),
    AiSemanticMatch(serde_json::Value),
    AiSynthesize(serde_json::Value),
    #[serde(rename = "ai_goal_decl")]
    AiGoalDeclaration(serde_json::Value),
    AiProgressUpdate(serde_json::Value),
    #[serde(rename = "ai_knowledge_share")]
    AiKnowledgeSharing(serde_json::Value),
    MathPow,
    MathSqrt,
    MathAbs,
    MathRound,
    MathFloor,
    MathCeil,
    StrStartsWith,
    StrEndsWith,
    StrToUpper,
    StrToLower,
    StrTrim,

    // DOM operations
    DomQuery(serde_json::Value),
    DomMutate(serde_json::Value),
    DomEventListener(serde_json::Value),

    // Program control
    Halt, // Halt execution

    // String intrinsics
    StrContains, // str, pattern -> bool
    StrSplit,    // str, delimiter -> array[str]
    StrReplace,  // str, old, new -> str
    StrJoin,     // array[str], delimiter -> str

    // COSM K1.4 (CRUSH-zorro-43): the generic member ops. They run on the
    // scheduler VM and the portable VM; every other engine refuses them.
    /// `obj -> value`: a field, a method (bound per the Kind's receiver) or a
    /// static member, looked up along the Kind's resolution order.
    GetMember(String),
    /// `obj, value -> obj`.
    SetMember(String),
    /// `obj -> bool`.
    HasMember(String),
    /// `obj -> obj`.
    DelMember(String),
    /// `argN … arg1, obj -> result`: a method call through the receiver's
    /// Kind (arguments pushed last-first, as for `call`; the receiver on top).
    CallMember {
        name: String,
        argc: usize,
    },
    /// `argN … arg1 -> instance`: create an instance of a Kind by its
    /// `construct` strategy.
    Construct {
        kind: String,
        argc: usize,
    },
    /// `record -> value`: the declared field at slot `i` (static fast path).
    GetSlot(usize),
    /// `record, value -> record`.
    SetSlot(usize),
    /// `value -> bool`: is it an instance of the Kind (or of a Kind that has
    /// it in its resolution order)? What a struct pattern tests.
    IsKind(String),
    /// `value -> value`: a copy of an instance of a `value(…)` Kind (value
    /// semantics on assignment, argument passing and return); any other value
    /// unchanged.
    Copy,
    /// `-> kind`: the Kind itself as a value (CRUSH-277 D8) — what a class
    /// name evaluates to in JS (`const K = Counter`), the receiver of a
    /// static member access. Runs where member ops run.
    LoadKind(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Function {
    #[serde(default)]
    pub params: Vec<String>,
    #[serde(default)]
    pub locals: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_hints: Option<HashMap<String, String>>,
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

    /// Materialize a typed opcode as the legacy JSON-backed instruction view.
    ///
    /// The compatibility view is intentionally retained while compiler emission
    /// migrates incrementally to typed opcodes.
    pub fn from_opcode(
        opcode: OpCode,
        lang: Option<String>,
        meta: Option<serde_json::Value>,
    ) -> Result<Self> {
        let (op, args) = match opcode {
            OpCode::PushInt(value) => ("push_int", serde_json::json!({"value": value})),
            OpCode::PushFloat(value) => ("push_float", serde_json::json!({"value": value})),
            OpCode::PushStr(value) => ("push_str", serde_json::json!({"value": value})),
            OpCode::PushBool(value) => ("push_bool", serde_json::json!({"value": value})),
            OpCode::PushNull => ("push_null", serde_json::json!({})),
            OpCode::Load(name) => ("load", serde_json::json!({"name": name})),
            OpCode::CallValueApply => ("call_value_apply", serde_json::json!({})),
            OpCode::Store(name) => ("store", serde_json::json!({"name": name})),
            OpCode::Add => ("add", serde_json::json!({})),
            OpCode::Sub => ("sub", serde_json::json!({})),
            OpCode::Mul => ("mul", serde_json::json!({})),
            OpCode::Div => ("div", serde_json::json!({})),
            OpCode::Mod => ("mod", serde_json::json!({})),
            OpCode::Neg => ("neg", serde_json::json!({})),
            OpCode::Eq => ("eq", serde_json::json!({})),
            OpCode::Ne => ("ne", serde_json::json!({})),
            OpCode::Lt => ("lt", serde_json::json!({})),
            OpCode::Gt => ("gt", serde_json::json!({})),
            OpCode::Le => ("le", serde_json::json!({})),
            OpCode::Ge => ("ge", serde_json::json!({})),
            OpCode::Pop => ("pop", serde_json::json!({})),
            OpCode::Dup => ("dup", serde_json::json!({})),
            other => {
                return Err(CasmError::UnknownOpcode(format!(
                    "typed opcode emission unsupported: {other:?}"
                ))
                .into());
            }
        };
        Ok(Self {
            op: op.to_string(),
            lang,
            meta,
            args,
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
            "call_value_apply" => Ok(OpCode::CallValueApply),
            "ret" => Ok(OpCode::Ret),
            "break" => Ok(OpCode::Break),
            "continue" => Ok(OpCode::Continue),
            "spawn" => Ok(OpCode::Spawn),
            "yield" => Ok(OpCode::Yield),
            "enter_try" => Ok(OpCode::EnterTry),
            "exit_try" => Ok(OpCode::ExitTry),
            "throw" => Ok(OpCode::Throw),
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
            "index" => Ok(OpCode::Index),
            "len" => Ok(OpCode::Len),
            "array_push" => Ok(OpCode::ArrayPush),
            "array_pop" => Ok(OpCode::ArrayPop),
            "make_range" => Ok(OpCode::MakeRange),
            "new_tuple" => Ok(OpCode::NewTuple(
                self.args.get("size").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
            )),
            "tuple_push" => Ok(OpCode::TuplePush),
            "new_list" => Ok(OpCode::NewList(
                self.args.get("size").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
            )),
            "list_push" => Ok(OpCode::ListPush),
            "new_vector" => Ok(OpCode::NewVector(
                self.args.get("size").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
            )),
            "vector_push" => Ok(OpCode::VectorPush),
            "new_set" => Ok(OpCode::NewSet(
                self.args.get("size").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
            )),
            "set_push" => Ok(OpCode::SetPush),
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
                let var_count = self
                    .args
                    .get("var_count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as usize;
                Ok(OpCode::ExecLang {
                    lang,
                    code,
                    var_count,
                })
            }
            "call_interface" => Ok(OpCode::CallInterface {
                handle: self.require_field("handle", |v| v.as_str().map(String::from))?,
                method: self.require_field("method", |v| v.as_str().map(String::from))?,
                argc: self.require_field("argc", |v| v.as_u64().map(|n| n as usize))?,
            }),
            "ai_query" => Ok(OpCode::AiQuery(self.args.clone())),
            "ai_adaptation_request" => Ok(OpCode::AiAdaptationRequest(self.args.clone())),
            "ai_capability_discovery" => Ok(OpCode::AiCapabilityDiscovery(self.args.clone())),
            "ai_semantic_switch" => Ok(OpCode::AiSemanticSwitch(self.args.clone())),
            "ai_toolchain" | "ai_tool_chain" => Ok(OpCode::AiToolchain(self.args.clone())),
            "ai_agent_delegation" => Ok(OpCode::AiAgentDelegation(self.args.clone())),
            "ai_learning_loop" => Ok(OpCode::AiLearningLoop(self.args.clone())),
            "ai_context_aware" => Ok(OpCode::AiContextAware(self.args.clone())),
            "ai_semantic_match" => Ok(OpCode::AiSemanticMatch(self.args.clone())),
            "ai_synthesize" => Ok(OpCode::AiSynthesize(self.args.clone())),
            "ai_goal_declaration" | "ai_goal_decl" => {
                Ok(OpCode::AiGoalDeclaration(self.args.clone()))
            }
            "ai_progress_update" => Ok(OpCode::AiProgressUpdate(self.args.clone())),
            "ai_knowledge_sharing" | "ai_knowledge_share" => {
                Ok(OpCode::AiKnowledgeSharing(self.args.clone()))
            }
            "dom_query" => Ok(OpCode::DomQuery(self.args.clone())),
            "dom_mutate" => Ok(OpCode::DomMutate(self.args.clone())),
            "dom_event_listener" => Ok(OpCode::DomEventListener(self.args.clone())),
            "math_pow" => Ok(OpCode::MathPow),
            "math_sqrt" => Ok(OpCode::MathSqrt),
            "math_abs" => Ok(OpCode::MathAbs),
            "math_round" => Ok(OpCode::MathRound),
            "math_floor" => Ok(OpCode::MathFloor),
            "math_ceil" => Ok(OpCode::MathCeil),
            "str_starts_with" => Ok(OpCode::StrStartsWith),
            "str_ends_with" => Ok(OpCode::StrEndsWith),
            "str_to_upper" => Ok(OpCode::StrToUpper),
            "str_to_lower" => Ok(OpCode::StrToLower),
            "str_trim" => Ok(OpCode::StrTrim),
            "get_member" => Ok(OpCode::GetMember(
                self.require_field("name", |v| v.as_str().map(String::from))?,
            )),
            "set_member" => Ok(OpCode::SetMember(
                self.require_field("name", |v| v.as_str().map(String::from))?,
            )),
            "has_member" => Ok(OpCode::HasMember(
                self.require_field("name", |v| v.as_str().map(String::from))?,
            )),
            "del_member" => Ok(OpCode::DelMember(
                self.require_field("name", |v| v.as_str().map(String::from))?,
            )),
            "call_member" => Ok(OpCode::CallMember {
                name: self.require_field("name", |v| v.as_str().map(String::from))?,
                argc: self.args.get("argc").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
            }),
            "construct" => Ok(OpCode::Construct {
                kind: self.require_field("kind", |v| v.as_str().map(String::from))?,
                argc: self.args.get("argc").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
            }),
            "get_slot" => Ok(OpCode::GetSlot(
                self.require_field("slot", |v| v.as_u64().map(|n| n as usize))?,
            )),
            "set_slot" => Ok(OpCode::SetSlot(
                self.require_field("slot", |v| v.as_u64().map(|n| n as usize))?,
            )),
            "is_kind" => Ok(OpCode::IsKind(
                self.require_field("kind", |v| v.as_str().map(String::from))?,
            )),
            "copy" => Ok(OpCode::Copy),
            "load_kind" => Ok(OpCode::LoadKind(
                self.require_field("kind", |v| v.as_str().map(String::from))?,
            )),
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
    /// COSM K1.3 (CRUSH-zorro-42): the program's kind table — every object
    /// declaration (`KindDecl` / `InterfaceDecl`) its source lowered to COSM.
    /// `None` for a program that declares none, and for CASM written before
    /// the field existed (it is omitted when `None`, so such a program
    /// serializes exactly as before).
    ///
    /// Keep this the **last** field: the binary format encodes a struct as a
    /// positional array, and an omitted trailing field is what lets old
    /// binary CASM (four fields) still load. In the binary format the module
    /// is embedded as its canonical COSM JSON (see [`cosm_serde`]).
    #[serde(default, skip_serializing_if = "Option::is_none", with = "cosm_serde")]
    pub cosm: Option<crush_cosm::CosmModule>,
}

impl Program {
    /// A program with no functions, the current [`CASM_VERSION`] and every
    /// other field empty. Prefer this (or [`Default`]) plus field assignment
    /// to a struct literal, so a new field doesn't break every construction
    /// site.
    pub fn new() -> Self {
        Self::default()
    }

    /// The kind table, if the program has a non-empty one (at least one Kind
    /// or interface).
    pub fn kind_table(&self) -> Option<&crush_cosm::CosmModule> {
        self.cosm.as_ref().filter(|m| !cosm_is_empty(m))
    }

    /// `Err(message)` when the kind table declares a Kind that can't run as a
    /// plain map ([`cosm_unsupported_reason`]): the refusal for an engine
    /// whose struct instances are maps (FastVM, the JIT, AOT). `engine` names
    /// the refusing engine.
    pub fn refuse_cosm(&self, engine: &str) -> std::result::Result<(), String> {
        self.refuse_cosm_on(engine, CosmSupport::Maps)
    }

    /// [`Program::refuse_cosm`] for an engine that implements `support`
    /// ([`cosm_unsupported_reason_on`]).
    pub fn refuse_cosm_on(&self, engine: &str, support: CosmSupport) -> std::result::Result<(), String> {
        match self.cosm.as_ref().and_then(|m| cosm_unsupported_reason_on(m, engine, support)) {
            Some(reason) => Err(reason),
            None => Ok(()),
        }
    }

    /// `Some("spawn")` when this program spawns a green task, else `None`
    /// (issue #161). Only `spawn` *starts* a task, so it is the one construct
    /// refused by name on an engine with no green-task scheduler. A `yield` or
    /// `await` without a `spawn` is not a green-task construct: with no
    /// spawned task `yield` is a no-op and `await` has no handle (the scheduler
    /// and the portable VM both push null), so each backend lowers those to a
    /// no-op instead of refusing.
    ///
    /// A `make_closure` by itself (a function value used without `spawn`) is a
    /// distinct, already-refused feature (CRUSH-251), so it is deliberately
    /// not reported here.
    pub fn green_task_op(&self) -> Option<&'static str> {
        self.functions
            .values()
            .flat_map(|f| &f.body)
            .any(|i| i.op == "spawn")
            .then_some("spawn")
    }

    /// `Err(message)` when this program `spawn`s a green task, naming `spawn`
    /// and `engine` (issue #161) — the refusal for an engine with no green-task
    /// scheduler. Mirrors [`Program::refuse_cosm`]: refuse by name rather than
    /// fail on the `make_closure` a `spawn` compiles through. A `yield`/`await`
    /// without `spawn` is not refused here; each backend lowers it to a no-op.
    pub fn refuse_green_task(&self, engine: &str) -> std::result::Result<(), String> {
        match self.green_task_op() {
            Some(op) => Err(format!("`{op}` (green tasks) isn't supported on {engine} yet (#161)")),
            None => Ok(()),
        }
    }

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
            Format::Json => serde_json::from_slice(data).map_err(|e| {
                crush_errors::CrushError::from(CasmError::DeserializationError(e.to_string()))
            })?,
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
                rmp_serde::from_slice(data).map_err(|e| {
                    crush_errors::CrushError::from(CasmError::DeserializationError(e.to_string()))
                })?
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
            cosm: None,
        }
    }
}

/// Whether a COSM module declares nothing (no Kind, no interface).
pub fn cosm_is_empty(module: &crush_cosm::CosmModule) -> bool {
    module.kinds.is_empty() && module.interfaces.is_empty()
}

/// How much of COSM an engine implements: the level the one shared kind-table
/// rule ([`cosm_unsupported_reason_on`]) checks a program against. Defined
/// here, once, so every engine refuses exactly what it can't run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CosmSupport {
    /// Struct instances are plain maps and methods are ordinary functions
    /// called by their qualified name: FastVM, the JIT, AOT Rust and C,
    /// crush-aotc.
    Maps,
    /// Records and Kinds (COSM K1.4, CRUSH-zorro-43): the scheduler VM and the
    /// portable VM.
    Kinds,
}

/// Why a Kind can't run as a plain map: each strategy it declares that a map
/// would silently not honour (empty when it can run as one). The
/// [`CosmSupport::Maps`] level of [`kind_unsupported_strategies_on`].
///
/// On an engine without Kinds a struct instance is a map and a method is an
/// ordinary function called by its qualified name. Crush's defaults —
/// `reference` semantics, `structural` equality, `literal` construction — are
/// exactly how maps behave, and the strategies that only govern member lookup
/// and receiver binding (storage, resolution, parents, receiver,
/// conformances) are inert there: what uses them — `p.m()`, a struct
/// pattern, `type_of` — compiles to member ops (`call_member`, `is_kind`)
/// these engines refuse, and FastVM refuses `type_of` in a program with
/// Kinds. What a map can't honour:
/// - `value(…)` semantics (instances would be shared, not copied or moved);
/// - `identity` equality (maps compare structurally);
/// - a non-`literal` construction (no constructor would run);
/// - a protocol hook (`eq`, `str`, operators, …);
/// - an accessor member (a field read would bypass the getter);
/// - a field default (a literal that omits the field would leave it unset).
pub fn kind_unsupported_strategies(k: &crush_cosm::KindDecl) -> Vec<String> {
    use crush_cosm::{Construct, Equality, Member, Semantics, Strategy};
    let mut why = Vec::new();
    if k.semantics != Semantics::Reference {
        why.push(format!("semantics: {}", k.semantics.as_str()));
    }
    if k.equality != Equality::Structural {
        why.push(format!("equality: {}", k.equality.as_str()));
    }
    if k.construct != Construct::Literal {
        why.push(format!("construct: {}", k.construct.strategy_name()));
    }
    if !k.hooks.is_empty() {
        why.push("protocol hooks".to_string());
    }
    for m in &k.members {
        if matches!(m.member, Member::Accessor { .. }) {
            why.push(format!("accessor `{}`", m.name));
        }
    }
    for f in k.declared_fields() {
        if f.default.is_some() {
            why.push(format!("a default for field `{}`", f.name));
        }
    }
    why
}

/// What a Kind declares that an engine implementing `support` can't run
/// (empty when it runs).
///
/// - `CosmSupport::Maps`: see [`kind_unsupported_strategies`].
/// - [`CosmSupport::Kinds`] (the scheduler VM and the portable VM, K1.4) run
///   every storage, `reference` and `value(…)` semantics (`move` behaves as
///   `copy` at run time: use-after-move is a compile-time check, and the VMs
///   don't need it to be exact), resolution `none`, `single`, `c3` and
///   `prototype_chain` (the JS walker's slice, CRUSH-277 S2), every
///   receiver strategy, every construction except a Python `__new__`, field
///   defaults, both equalities, accessor members (CRUSH-277 S4: getters and
///   setters run where the member op stands), and the `str`, `repr` and
///   `clone` hooks. Still refused, until the phase that brings them:
///   resolution `embedding_promotion` (the Go walker, K3), a `py_new_init`
///   with a `new_fn`, and every other hook (operators and the
///   eq/hash/cmp/iter/call/missing/set protocols, K2).
pub fn kind_unsupported_strategies_on(k: &crush_cosm::KindDecl, support: CosmSupport) -> Vec<String> {
    use crush_cosm::{Construct, Resolution, Strategy};
    if support == CosmSupport::Maps {
        return kind_unsupported_strategies(k);
    }
    let mut why = Vec::new();
    if matches!(k.resolution, Resolution::EmbeddingPromotion) {
        why.push(format!("resolution: {}", k.resolution.as_str()));
    }
    if let Construct::PyNewInit { new_fn: Some(f), .. } = &k.construct {
        why.push(format!("construct: py_new_init with a `__new__` (`{f}`)"));
    }
    let h = &k.hooks;
    let mut hooks: Vec<String> = [
        ("missing", h.missing.is_some()),
        ("set", h.set.is_some()),
        ("call", h.call.is_some()),
        ("iter", h.iter.is_some()),
        ("eq", h.eq.is_some()),
        ("hash", h.hash.is_some()),
        ("cmp", h.cmp.is_some()),
    ]
    .into_iter()
    .filter(|(_, declared)| *declared)
    .map(|(name, _)| name.to_string())
    .collect();
    hooks.extend(h.op.keys().map(|op| format!("op:{}", op_symbol(*op))));
    hooks.extend(h.op_reflected.keys().map(|op| format!("op_reflected:{}", op_symbol(*op))));
    if !hooks.is_empty() {
        why.push(format!("hooks {}", hooks.join(", ")));
    }
    why
}

/// The declared fields of the Kind `kind` in `module` when its storage is
/// `sealed` (`None` for open storage or an unknown Kind). Every engine
/// refuses a write of any other field to an instance of such a Kind, with
/// [`sealed_field_error`]'s message: the VMs on their Records, FastVM, the
/// JIT and AOT on the struct maps they build (COSM K1.4).
pub fn sealed_fields<'m>(module: &'m crush_cosm::CosmModule, kind: &str) -> Option<Vec<&'m str>> {
    let k = module.kind(kind)?;
    match &k.storage {
        crush_cosm::Storage::Sealed { fields } => Some(fields.iter().map(|f| f.name.as_str()).collect()),
        _ => None,
    }
}

/// The one error text for a write of `field` to an instance of the sealed
/// Kind `kind` declaring `fields`.
pub fn sealed_field_error(kind: &str, field: &str, fields: &[&str]) -> String {
    let list = if fields.is_empty() { "none".to_string() } else { fields.join(", ") };
    format!("`{kind}` is sealed: it has no field `{field}` (fields: {list})")
}

/// How `CONSTRUCT` uses a Kind's constructor function ([`kind_constructor`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstructorUse {
    /// The function initialises a new, empty instance: it gets the instance
    /// as its first parameter (`self`), then the arguments, and its own
    /// return value is dropped — Dart/Java constructors, JS `new`, Python
    /// `__init__`.
    Initialiser,
    /// The function is a static member of the Kind: it gets the arguments
    /// and returns the instance (a Crush `fn new(x, y) { return P { … } }`).
    Factory,
}

/// The function `CONSTRUCT` calls to build an instance of a Kind whose
/// resolution order is `mro` (the Kind first), and how: `ctor(f)` /
/// `js_new(f)` → `f`; `py_new_init` → its `init_fn`, else an `__init__`
/// method inherited through the MRO; `None` for a `literal` Kind (or a
/// Python class with no `__init__` anywhere). `f` is a [`ConstructorUse::Factory`]
/// when it is one of the Kind's static members, an initialiser otherwise.
/// Shared by the compiler (`C(…)` takes the function's parameters, after
/// `self` for an initialiser) and the VMs.
pub fn kind_constructor<'d>(
    mro: &[&'d crush_cosm::KindDecl],
) -> Option<(&'d crush_cosm::FnRef, ConstructorUse)> {
    use crush_cosm::{Construct, Member, StaticValue};
    let decl = mro.first()?;
    let func = match &decl.construct {
        Construct::Literal => None,
        Construct::Ctor { func } => Some(func),
        Construct::JsNew { ctor_fn } => Some(ctor_fn),
        Construct::PyNewInit { init_fn: Some(f), .. } => Some(f),
        Construct::PyNewInit { init_fn: None, .. } => mro.iter().find_map(|k| match k.member("__init__") {
            Some(crush_cosm::MemberDecl { member: Member::Method { func, .. }, .. }) => Some(func),
            _ => None,
        }),
    }?;
    let factory = decl
        .members
        .iter()
        .any(|m| matches!(&m.member, Member::Static { value: StaticValue::Fn(f) } if f == func));
    Some((func, if factory { ConstructorUse::Factory } else { ConstructorUse::Initialiser }))
}

/// [`kind_constructor`] for the Kind named `kind` in `module` (its
/// resolution order computed there); `None` when it has none, or its order
/// can't be computed.
pub fn kind_constructor_in<'m>(
    module: &'m crush_cosm::CosmModule,
    kind: &str,
) -> Option<(&'m crush_cosm::FnRef, ConstructorUse)> {
    let order = crush_cosm::resolution_order(&crush_cosm::KindRef::new(kind), module).ok()?;
    let mro: Vec<&crush_cosm::KindDecl> = order.iter().filter_map(|r| module.kind(r.as_str())).collect();
    kind_constructor(&mro)
}

/// An operator's COSM spelling (`+`, `[]`, `neg`).
fn op_symbol(op: crush_cosm::Op) -> String {
    serde_json::to_value(op).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_else(|| format!("{op:?}"))
}

/// The one COSM load/compile-time rule for an engine whose struct instances
/// are plain maps: [`cosm_unsupported_reason_on`] at [`CosmSupport::Maps`].
pub fn cosm_unsupported_reason(module: &crush_cosm::CosmModule, engine: &str) -> Option<String> {
    cosm_unsupported_reason_on(module, engine, CosmSupport::Maps)
}

/// The one COSM load/compile-time rule every engine applies: a kind table
/// runs on an engine implementing `support` iff every Kind's
/// [`kind_unsupported_strategies_on`] is empty. Returns the refusal — naming
/// `engine`, each refused Kind and what it declares — or `None` when the
/// program runs.
pub fn cosm_unsupported_reason_on(
    module: &crush_cosm::CosmModule,
    engine: &str,
    support: CosmSupport,
) -> Option<String> {
    let problems: Vec<String> = module
        .kinds
        .iter()
        .filter_map(|k| {
            let why = kind_unsupported_strategies_on(k, support);
            (!why.is_empty()).then(|| format!("`{}` declares {}", k.name, why.join(", ")))
        })
        .collect();
    if problems.is_empty() {
        return None;
    }
    Some(match support {
        CosmSupport::Maps => format!(
            "COSM objects aren't supported on {engine} yet (CRUSH-zorro-43): {}; \
             without Kinds its instances would silently run as plain maps",
            problems.join("; ")
        ),
        CosmSupport::Kinds => format!(
            "COSM strategies not supported on {engine} yet: {}; operator and protocol hooks \
             arrive with COSM K2, embedding with the Go walker (K3)",
            problems.join("; ")
        ),
    })
}

/// Serde for [`Program::cosm`]: the module as-is in human-readable formats
/// (CASM JSON), and as its canonical COSM JSON text in binary ones. The COSM
/// schema omits empty fields, which a positional binary encoding (MessagePack
/// structs-as-arrays, as [`Format::Binary`] uses) can't represent.
pub mod cosm_serde {
    use crush_cosm::CosmModule;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(v: &Option<CosmModule>, s: S) -> Result<S::Ok, S::Error> {
        if s.is_human_readable() {
            v.serialize(s)
        } else {
            v.as_ref().map(CosmModule::to_json).serialize(s)
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<CosmModule>, D::Error> {
        if d.is_human_readable() {
            Option::<CosmModule>::deserialize(d)
        } else {
            match Option::<String>::deserialize(d)? {
                Some(json) => CosmModule::from_json(&json)
                    .map(Some)
                    .map_err(|e| serde::de::Error::custom(format!("kind table: {e}"))),
                None => Ok(None),
            }
        }
    }
}

/// Format runtime errors with source location when available.
pub fn format_runtime_error_with_location(
    message: &str,
    debug_info: Option<&DebugInfo>,
    pc: usize,
    fn_name: Option<&str>,
) -> String {
    let loc = if let Some(name) = fn_name {
        debug_info.and_then(|d| d.source_location_for_function_pc(name, pc))
    } else {
        debug_info.and_then(|d| d.source_location_for_pc(pc))
    };
    if let Some(loc) = loc {
        return format!("Error at line {}, col {}: {}", loc.line, loc.col, message);
    }
    message.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prototype_chain_runs_at_kinds_level_embedding_does_not() {
        // CRUSH-277 S2: the JS slice's resolution strategy leaves the
        // refusal list; Go's embedding stays on it.
        let mut k = crush_cosm::KindDecl::crush("C");
        k.resolution = crush_cosm::Resolution::PrototypeChain;
        assert_eq!(kind_unsupported_strategies_on(&k, CosmSupport::Kinds), Vec::<String>::new());
        k.resolution = crush_cosm::Resolution::EmbeddingPromotion;
        assert_eq!(
            kind_unsupported_strategies_on(&k, CosmSupport::Kinds),
            vec!["resolution: embedding_promotion".to_string()]
        );
        // A maps-level engine still can't honour a JS construction.
        k.resolution = crush_cosm::Resolution::PrototypeChain;
        k.construct = crush_cosm::Construct::JsNew { ctor_fn: crush_cosm::FnRef::new("C::constructor") };
        assert_eq!(
            kind_unsupported_strategies_on(&k, CosmSupport::Maps),
            vec!["construct: js_new".to_string()]
        );
    }

    #[test]
    fn runtime_error_location_formats_with_source_location() {
        let mut dbg = DebugInfo::new();
        dbg.push_source_location(SourceLocation::new(42, 10, Some("main.crush".to_string())));

        let msg = format_runtime_error_with_location("division by zero", Some(&dbg), 0, None);
        assert_eq!(msg, "Error at line 42, col 10: division by zero");
    }

    #[test]
    fn runtime_error_location_falls_back_without_source() {
        let dbg = DebugInfo::new();
        let msg = format_runtime_error_with_location("division by zero", Some(&dbg), 0, None);
        assert_eq!(msg, "division by zero");
    }

    #[test]
    fn runtime_error_location_falls_back_without_debug_info() {
        let msg = format_runtime_error_with_location("division by zero", None, 0, None);
        assert_eq!(msg, "division by zero");
    }

    #[test]
    fn runtime_error_location_with_function_name_resolves_correct_function() {
        let mut dbg = DebugInfo::new();
        // Simulate a two-function program: "main" at pc 0-1, "helper" at pc 2-3.
        dbg.push_source_location(SourceLocation::new(1, 1, Some("main.crush".to_string())));
        dbg.push_source_location(SourceLocation::new(2, 1, Some("main.crush".to_string())));
        dbg.record_function_range("main", 0, 2);
        dbg.push_source_location(SourceLocation::new(10, 5, Some("main.crush".to_string())));
        dbg.push_source_location(SourceLocation::new(11, 5, Some("main.crush".to_string())));
        dbg.record_function_range("helper", 2, 4);

        // Without function name: pc=0 returns line 1 (first function).
        let msg = format_runtime_error_with_location("boom", Some(&dbg), 0, None);
        assert_eq!(msg, "Error at line 1, col 1: boom");

        // With "helper": pc=0 returns line 10 (helper's first instruction).
        let msg = format_runtime_error_with_location("boom", Some(&dbg), 0, Some("helper"));
        assert_eq!(msg, "Error at line 10, col 5: boom");

        // With "helper": pc=1 returns line 11.
        let msg = format_runtime_error_with_location("boom", Some(&dbg), 1, Some("helper"));
        assert_eq!(msg, "Error at line 11, col 5: boom");
    }

    #[test]
    fn green_task_op_names_the_starting_construct() {
        let instr = |op: &str| Instruction {
            op: op.to_string(),
            lang: None,
            meta: None,
            args: serde_json::json!({}),
        };
        let body_of = |ops: &[&str]| Function {
            params: vec![],
            locals: vec![],
            type_hints: None,
            body: ops.iter().map(|op| instr(op)).collect(),
        };
        let mut program = Program::new();

        // `spawn` is the only green-task construct refused by name — it is what
        // *starts* the task (issue #161), even though `yield` appears first.
        program.functions.insert("main".to_string(), body_of(&["make_closure", "yield", "spawn"]));
        assert_eq!(program.green_task_op(), Some("spawn"));

        // `yield` and `await` without `spawn` are no-ops, not green-task
        // constructs: they are not reported (each backend lowers them to
        // nothing / a null push).
        program.functions.insert("main".to_string(), body_of(&["yield"]));
        assert_eq!(program.green_task_op(), None);
        program.functions.insert("main".to_string(), body_of(&["await"]));
        assert_eq!(program.green_task_op(), None);

        // A `make_closure` by itself (a function value used without `spawn`) is
        // NOT a green-task construct.
        program.functions.insert("main".to_string(), body_of(&["make_closure"]));
        assert_eq!(program.green_task_op(), None);

        // `refuse_green_task` names the construct and the engine.
        program.functions.insert("main".to_string(), body_of(&["spawn"]));
        assert_eq!(
            program.refuse_green_task("the test backend"),
            Err("`spawn` (green tasks) isn't supported on the test backend yet (#161)".to_string())
        );
    }

    // VER-02 CASM version-gate tests live in `tests/ver02_version_gate.rs`
    // (an integration test) so they exercise only the public API.
}
