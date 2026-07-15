//! Fast instruction representation and CASM → FastInstr lowering pass.
//!
//! This module converts high-level CASM (string-based, flexible) into
//! a compact, index-based representation optimized for execution speed.

use casm::{Instruction, Program};
use hex;
use std::collections::HashMap;

/// Compact opcode enum - fits in a single byte
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FastOp {
    // Stack operations
    PushInt,   // arg = i64 value
    PushFloat, // arg = f64 bits
    PushBool,  // arg = 0 or 1
    PushNull,  // arg = unused
    PushStr,   // arg = string table index
    Pop,       // arg = unused
    Dup,       // arg = unused

    // Local variable access (index-based)
    LoadLocal,  // arg = local slot index
    StoreLocal, // arg = local slot index

    // Control flow
    Jump,      // arg = target PC
    JumpIf,    // arg = target PC
    JumpIfNot, // arg = target PC
    Call,      // arg = function table index
    Return,    // arg = unused

    // Capability calls (pre-resolved)
    CapCall, // arg = capability table index, arg2 = argc

    // Arithmetic (operate on stack top)
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Neg,

    // Comparison
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,

    // Logical
    And,
    Or,
    Not,

    // Structured data
    MakeList, // arg = element count
    MakeMap,  // arg = pair count
    Index,    // arg = unused (key on stack)

    // VM control
    Yield,
    Halt,
    Nop,
    Gc,
    Spawn,
    Restart,
    Watchdog,

    // Variables
    ExportVar,
    ImportVar,

    // Exception handling
    EnterTry,
    ExitTry,
    Throw,

    // Host interaction
    CallHost,
    CallInterface,
    ExecLang,
    CrossLangCall,
    Await,

    // Object/Struct
    NewObj,
    NewStruct,
    GetField,
    SetField,
    NewArray,
    ArrayPush,
    ArrayPop,
    NewTuple,
    TuplePush,
    NewList,
    ListPush,
    NewVector,
    VectorPush,
    NewSet,
    SetPush,
    Len,
    MakeRange,

    // Bitwise
    BitAnd,
    BitOr,
    BitXor,
    BitNot,
    Shl,
    Shr,

    // Stack manipulation
    Swap,
    Rot,
    Pick, // arg = n (copy nth item from stack top)
    Roll, // arg = n (move nth item to stack top)

    // Loop control
    Break,    // arg = target PC (loop exit)
    Continue, // arg = target PC (loop start)

    // Type operations
    TypeOf,
    Cast, // arg = string table index (target type name)

    // String
    StrContains,
    StrSplit,
    StrReplace,
    StrJoin,
    StrSim, // arg = unused (two strings on stack), results in similarity float on stack
}

/// Compact instruction - 16 bytes total
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct FastInstr {
    pub op: FastOp,
    pub arg: u64,  // Primary argument (index, immediate, or target)
    pub arg2: u32, // Secondary argument (e.g., argc for calls)
}

impl FastInstr {
    #[inline]
    pub fn new(op: FastOp, arg: u64, arg2: u32) -> Self {
        Self { op, arg, arg2 }
    }

    #[inline]
    pub fn simple(op: FastOp) -> Self {
        Self {
            op,
            arg: 0,
            arg2: 0,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HostCallSite {
    pub capsule_idx: u32,
    pub method_idx: u32,
    pub ic_id: [u8; 32],
    pub argc: u32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InterfaceCallSite {
    pub handle_var_idx: u32,
    pub method_idx: u32,
    pub argc: u32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecLangSite {
    pub lang_idx: u32,
    pub code_idx: u32,
    pub var_count: u32,
    pub var_names: Vec<u32>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CrossLangCallSite {
    pub target_lang_idx: u32,
    pub function_name_idx: u32,
    pub argc: u32,
}

/// Symbol tables built during lowering
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct SymbolTables {
    /// Variable name → local slot index
    pub locals: HashMap<String, u32>,
    /// Function name → (start_pc, end_pc, arity)
    pub functions: HashMap<String, (usize, usize, u32)>,
    /// Capability name → cap table index
    pub capabilities: HashMap<String, u32>,
    /// String literals pool
    pub strings: Vec<String>,
    /// Reverse lookup for strings
    string_index: HashMap<String, u32>,

    // Call sites for complex instructions
    pub host_calls: Vec<HostCallSite>,
    pub interface_calls: Vec<InterfaceCallSite>,
    pub exec_lang_calls: Vec<ExecLangSite>,
    pub cross_lang_calls: Vec<CrossLangCallSite>,
}

impl SymbolTables {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn new_with_config<T>(_config: T) -> Self {
        Self::default()
    }

    /// Intern a string, returning its index
    pub fn intern_string(&mut self, s: &str) -> u32 {
        if let Some(&idx) = self.string_index.get(s) {
            return idx;
        }
        let idx = self.strings.len() as u32;
        self.strings.push(s.to_string());
        self.string_index.insert(s.to_string(), idx);
        idx
    }

    pub fn add_string(&mut self, s: String) -> u32 {
        self.intern_string(&s)
    }

    pub fn get_string(&self, idx: u64) -> Option<&str> {
        let idx = idx as usize;
        if idx < self.strings.len() {
            Some(&self.strings[idx])
        } else {
            None
        }
    }

    /// Get or create local slot for variable
    pub fn get_or_create_local(&mut self, name: &str) -> u32 {
        if let Some(&idx) = self.locals.get(name) {
            return idx;
        }
        let idx = self.locals.len() as u32;
        self.locals.insert(name.to_string(), idx);
        idx
    }

    pub fn register_capability(&mut self, name: &str) -> u32 {
        if let Some(&idx) = self.capabilities.get(name) {
            return idx;
        }
        let idx = self.capabilities.len() as u32;
        self.capabilities.insert(name.to_string(), idx);
        idx
    }

    pub fn register_host_call(&mut self, site: HostCallSite) -> u32 {
        let idx = self.host_calls.len() as u32;
        self.host_calls.push(site);
        idx
    }

    pub fn register_interface_call(&mut self, site: InterfaceCallSite) -> u32 {
        let idx = self.interface_calls.len() as u32;
        self.interface_calls.push(site);
        idx
    }

    pub fn register_exec_lang(&mut self, site: ExecLangSite) -> u32 {
        let idx = self.exec_lang_calls.len() as u32;
        self.exec_lang_calls.push(site);
        idx
    }

    pub fn register_cross_lang_call(&mut self, site: CrossLangCallSite) -> u32 {
        let idx = self.cross_lang_calls.len() as u32;
        self.cross_lang_calls.push(site);
        idx
    }
}

/// Result of lowering a CASM program
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LoweredProgram {
    pub instructions: Vec<FastInstr>,
    pub symbols: SymbolTables,
    /// Entry point PC for "main" function
    pub entry_point: usize,
}

impl LoweredProgram {
    pub fn empty() -> Self {
        Self {
            instructions: Vec::new(),
            symbols: SymbolTables::default(),
            entry_point: 0,
        }
    }
}

/// Errors during lowering
#[derive(Debug, Clone)]
pub enum LowerError {
    UnknownOpcode(String),
    MissingArgument(String),
    InvalidJumpTarget(String),
    FunctionNotFound(String),
}

impl std::fmt::Display for LowerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LowerError::UnknownOpcode(op) => write!(f, "Unknown opcode: {}", op),
            LowerError::MissingArgument(msg) => write!(f, "Missing argument: {}", msg),
            LowerError::InvalidJumpTarget(t) => write!(f, "Invalid jump target: {}", t),
            LowerError::FunctionNotFound(name) => write!(f, "Function not found: {}", name),
        }
    }
}

impl std::error::Error for LowerError {}

/// Lower a CASM program to FastInstr representation
pub fn lower_program(program: &Program) -> Result<LoweredProgram, LowerError> {
    let mut symbols = SymbolTables::new();
    let mut instructions = Vec::new();
    let mut label_positions: HashMap<String, usize> = HashMap::new();
    let mut pending_jumps: Vec<(usize, String)> = Vec::new();

    // First pass: collect function entry points and labels
    // Use sorted function names for deterministic ordering across runs
    let mut func_names: Vec<&String> = program.functions.keys().collect();
    func_names.sort();

    let mut pc = 0;
    for func_name in &func_names {
        let func = &program.functions[*func_name];
        let start_pc = pc;
        symbols
            .functions
            .insert((*func_name).clone(), (start_pc, 0, func.params.len() as u32));

        for instr in &func.body {
            if instr.op == "label" {
                if let Some(name) = instr.args.get("name").and_then(|v| v.as_str()) {
                    label_positions.insert(format!("{}:{}", func_name, name), pc);
                }
            }
            pc += 1;
        }

        // Update end_pc
        if let Some(entry) = symbols.functions.get_mut(*func_name) {
            entry.1 = pc;
        }
    }

    // Second pass: lower instructions (same sorted order)
    pc = 0;
    for func_name in &func_names {
        let func = &program.functions[*func_name];
        // Reset locals per-function so parameters always start at slot 0.
        // This matches Call's expectation that args are placed at
        // locals_base + 0..argc-1 inside the callee frame.
        symbols.locals.clear();

        // Create local slots for parameters (slot 0..N-1)
        for param in &func.params {
            symbols.get_or_create_local(param);
        }

        for instr in &func.body {
            let fast = lower_instruction(
                instr,
                func_name,
                &mut symbols,
                &label_positions,
                &mut pending_jumps,
                pc,
            )?;
            instructions.push(fast);
            pc += 1;
        }
    }

    // Third pass: resolve pending jumps
    for (instr_pc, label) in pending_jumps {
        if let Some(&target_pc) = label_positions.get(&label) {
            instructions[instr_pc].arg = target_pc as u64;
        } else {
            return Err(LowerError::InvalidJumpTarget(label));
        }
    }

    // Find entry point
    let entry_point = symbols
        .functions
        .get("main")
        .map(|(pc, _, _)| *pc)
        .unwrap_or(0);

    Ok(LoweredProgram {
        instructions,
        symbols,
        entry_point,
    })
}

fn lower_instruction(
    instr: &Instruction,
    current_func: &str,
    symbols: &mut SymbolTables,
    labels: &HashMap<String, usize>,
    pending_jumps: &mut Vec<(usize, String)>,
    pc: usize,
) -> Result<FastInstr, LowerError> {
    match instr.op.as_str() {
        "push_int" => {
            let val = instr
                .args
                .get("value")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| LowerError::MissingArgument("push_int value".into()))?;
            Ok(FastInstr::new(FastOp::PushInt, val as u64, 0))
        }

        "push_float" => {
            let val = instr
                .args
                .get("value")
                .and_then(|v| v.as_f64())
                .ok_or_else(|| LowerError::MissingArgument("push_float value".into()))?;
            Ok(FastInstr::new(FastOp::PushFloat, val.to_bits(), 0))
        }

        "push_bool" => {
            let val = instr
                .args
                .get("value")
                .and_then(|v| v.as_bool())
                .ok_or_else(|| LowerError::MissingArgument("push_bool value".into()))?;
            Ok(FastInstr::new(FastOp::PushBool, val as u64, 0))
        }

        "push_null" | "push_nil" => Ok(FastInstr::simple(FastOp::PushNull)),

        "push_str" | "push_string" => {
            let val = instr
                .args
                .get("value")
                .and_then(|v| v.as_str())
                .ok_or_else(|| LowerError::MissingArgument("push_str value".into()))?;
            let idx = symbols.intern_string(val);
            Ok(FastInstr::new(FastOp::PushStr, idx as u64, 0))
        }

        "pop" => Ok(FastInstr::simple(FastOp::Pop)),
        "dup" => Ok(FastInstr::simple(FastOp::Dup)),

        "load" | "load_local" | "get_local" => {
            let name = instr
                .args
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| LowerError::MissingArgument("load name".into()))?;
            let idx = symbols.get_or_create_local(name);
            Ok(FastInstr::new(FastOp::LoadLocal, idx as u64, 0))
        }

        "store" | "store_local" | "set_local" => {
            let name = instr
                .args
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| LowerError::MissingArgument("store name".into()))?;
            let idx = symbols.get_or_create_local(name);
            Ok(FastInstr::new(FastOp::StoreLocal, idx as u64, 0))
        }

        "export_var" => {
            let name = instr
                .args
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| LowerError::MissingArgument("export_var name".into()))?;
            let idx = symbols.intern_string(name);
            Ok(FastInstr::new(FastOp::ExportVar, idx as u64, 0))
        }

        "import_var" => {
            let name = instr
                .args
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| LowerError::MissingArgument("import_var name".into()))?;
            let idx = symbols.intern_string(name);
            Ok(FastInstr::new(FastOp::ImportVar, idx as u64, 0))
        }

        "jump" | "jmp" => {
            return lower_jump(instr, current_func, labels, pending_jumps, pc, FastOp::Jump);
        }

        "jump_if" | "jmp_if" | "branch_if" => {
            return lower_jump(
                instr,
                current_func,
                labels,
                pending_jumps,
                pc,
                FastOp::JumpIf,
            );
        }

        "jump_if_not" | "jmp_if_not" | "branch_if_not" => {
            return lower_jump(
                instr,
                current_func,
                labels,
                pending_jumps,
                pc,
                FastOp::JumpIfNot,
            );
        }

        "call" => {
            let name = instr
                .args
                .get("name")
                .and_then(|v| v.as_str())
                .or_else(|| instr.args.get("function").and_then(|v| v.as_str()))
                .ok_or_else(|| LowerError::MissingArgument("call name/function".into()))?;
            let argc = instr.args.get("argc").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let idx = symbols.intern_string(name);
            Ok(FastInstr::new(FastOp::Call, idx as u64, argc))
        }

        "return" | "ret" => Ok(FastInstr::simple(FastOp::Return)),

        "cap_call" => {
            let name = instr
                .args
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| LowerError::MissingArgument("cap_call name".into()))?;
            let argc = instr.args.get("argc").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let idx = symbols.register_capability(name);
            Ok(FastInstr::new(FastOp::CapCall, idx as u64, argc))
        }

        "call_host" => {
            let capsule = instr
                .args
                .get("capsule")
                .and_then(|v| v.as_str())
                .ok_or_else(|| LowerError::MissingArgument("call_host capsule".into()))?;
            let method = instr
                .args
                .get("method")
                .and_then(|v| v.as_str())
                .ok_or_else(|| LowerError::MissingArgument("call_host method".into()))?;
            let ic_id_hex = instr
                .args
                .get("ic_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| LowerError::MissingArgument("call_host ic_id".into()))?;
            let argc = instr.args.get("argc").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

            let mut ic_id = [0u8; 32];
            hex::decode_to_slice(ic_id_hex, &mut ic_id)
                .map_err(|_| LowerError::MissingArgument("Invalid hex in ic_id".into()))?;

            let site = HostCallSite {
                capsule_idx: symbols.intern_string(capsule),
                method_idx: symbols.intern_string(method),
                ic_id,
                argc,
            };
            let idx = symbols.register_host_call(site);
            Ok(FastInstr::new(FastOp::CallHost, idx as u64, 0))
        }

        "call_interface" => {
            let handle = instr
                .args
                .get("handle")
                .and_then(|v| v.as_str())
                .ok_or_else(|| LowerError::MissingArgument("call_interface handle".into()))?;
            let method = instr
                .args
                .get("method")
                .and_then(|v| v.as_str())
                .ok_or_else(|| LowerError::MissingArgument("call_interface method".into()))?;
            let argc = instr.args.get("argc").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

            let site = InterfaceCallSite {
                handle_var_idx: symbols.get_or_create_local(handle),
                method_idx: symbols.intern_string(method),
                argc,
            };
            let idx = symbols.register_interface_call(site);
            Ok(FastInstr::new(FastOp::CallInterface, idx as u64, 0))
        }

        "exec_lang" => {
            let lang = instr
                .args
                .get("lang")
                .and_then(|v| v.as_str())
                .ok_or_else(|| LowerError::MissingArgument("exec_lang lang".into()))?;
            let code = instr
                .args
                .get("code")
                .and_then(|v| v.as_str())
                .ok_or_else(|| LowerError::MissingArgument("exec_lang code".into()))?;
            let var_count = instr
                .args
                .get("var_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;

            let var_names_indices =
                if let Some(arr) = instr.args.get("var_names").and_then(|v| v.as_array()) {
                    arr.iter()
                        .filter_map(|v| v.as_str())
                        .map(|s| symbols.intern_string(s))
                        .collect()
                } else {
                    Vec::new()
                };

            let site = ExecLangSite {
                lang_idx: symbols.intern_string(lang),
                code_idx: symbols.intern_string(code),
                var_count,
                var_names: var_names_indices,
            };
            let idx = symbols.register_exec_lang(site);
            Ok(FastInstr::new(FastOp::ExecLang, idx as u64, 0))
        }

        "cross_lang_call" => {
            let target_lang = instr
                .args
                .get("target_lang")
                .and_then(|v| v.as_str())
                .ok_or_else(|| LowerError::MissingArgument("cross_lang_call target_lang".into()))?;
            let function_name = instr
                .args
                .get("function")
                .and_then(|v| v.as_str())
                .ok_or_else(|| LowerError::MissingArgument("cross_lang_call function".into()))?;
            let argc = instr.args.get("argc").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

            let site = CrossLangCallSite {
                target_lang_idx: symbols.intern_string(target_lang),
                function_name_idx: symbols.intern_string(function_name),
                argc,
            };
            let idx = symbols.register_cross_lang_call(site);
            Ok(FastInstr::new(FastOp::CrossLangCall, idx as u64, 0))
        }

        // Bitwise
        "bit_and" => Ok(FastInstr::simple(FastOp::BitAnd)),
        "bit_or" => Ok(FastInstr::simple(FastOp::BitOr)),
        "bit_xor" => Ok(FastInstr::simple(FastOp::BitXor)),
        "bit_not" => Ok(FastInstr::simple(FastOp::BitNot)),
        "shl" => Ok(FastInstr::simple(FastOp::Shl)),
        "shr" => Ok(FastInstr::simple(FastOp::Shr)),

        // Stack manipulation
        "swap" => Ok(FastInstr::simple(FastOp::Swap)),
        "rot" => Ok(FastInstr::simple(FastOp::Rot)),
        "pick" => {
            let n = instr
                .args
                .get("n")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| LowerError::MissingArgument("pick n".into()))?;
            Ok(FastInstr::new(FastOp::Pick, n, 0))
        }
        "roll" => {
            let n = instr
                .args
                .get("n")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| LowerError::MissingArgument("roll n".into()))?;
            Ok(FastInstr::new(FastOp::Roll, n, 0))
        }

        // Loop control — these are typically patched by the compiler to jump targets
        "break" => Ok(FastInstr::simple(FastOp::Break)),
        "continue" => Ok(FastInstr::simple(FastOp::Continue)),

        // Type operations
        "type_of" => Ok(FastInstr::simple(FastOp::TypeOf)),
        "cast" => {
            let type_name = instr
                .args
                .get("type")
                .and_then(|v| v.as_str())
                .ok_or_else(|| LowerError::MissingArgument("cast type".into()))?;
            let idx = symbols.intern_string(type_name);
            Ok(FastInstr::new(FastOp::Cast, idx as u64, 0))
        }

        // Array aliases (map to existing ops)
        "arr_get" => Ok(FastInstr::simple(FastOp::Index)),
        "arr_set" => {
            // ArrSet: array, index, value -> array (handled as special in VM)
            // For now, we don't have a dedicated SetIndex; this needs VM support
            Ok(FastInstr::simple(FastOp::Nop)) // TODO: implement ArrSet in VM
        }
        "arr_len" => Ok(FastInstr::simple(FastOp::Len)),
        "arr_push" => Ok(FastInstr::simple(FastOp::ArrayPush)),
        "arr_pop" => Ok(FastInstr::simple(FastOp::ArrayPop)),

        // Arithmetic
        "add" => Ok(FastInstr::simple(FastOp::Add)),
        "sub" => Ok(FastInstr::simple(FastOp::Sub)),
        "mul" => Ok(FastInstr::simple(FastOp::Mul)),
        "div" => Ok(FastInstr::simple(FastOp::Div)),
        "mod" => Ok(FastInstr::simple(FastOp::Mod)),
        "neg" => Ok(FastInstr::simple(FastOp::Neg)),

        // Comparison
        "eq" => Ok(FastInstr::simple(FastOp::Eq)),
        "ne" => Ok(FastInstr::simple(FastOp::Ne)),
        "lt" => Ok(FastInstr::simple(FastOp::Lt)),
        "le" => Ok(FastInstr::simple(FastOp::Le)),
        "gt" => Ok(FastInstr::simple(FastOp::Gt)),
        "ge" => Ok(FastInstr::simple(FastOp::Ge)),

        // Logical
        "and" => Ok(FastInstr::simple(FastOp::And)),
        "or" => Ok(FastInstr::simple(FastOp::Or)),
        "not" => Ok(FastInstr::simple(FastOp::Not)),

        // Structured data
        "make_list" | "list" => {
            let count = instr
                .args
                .get("count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            Ok(FastInstr::new(FastOp::MakeList, count, 0))
        }

        "make_map" | "map" => {
            let count = instr
                .args
                .get("count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            Ok(FastInstr::new(FastOp::MakeMap, count, 0))
        }

        "new_array" => {
            let size = instr
                .args
                .get("size")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            Ok(FastInstr::new(FastOp::NewArray, size, 0))
        }
        "new_tuple" => {
            let size = instr.args.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
            Ok(FastInstr::new(FastOp::NewTuple, size, 0))
        }
        "new_list" => {
            let size = instr.args.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
            Ok(FastInstr::new(FastOp::NewList, size, 0))
        }
        "new_vector" => {
            let size = instr.args.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
            Ok(FastInstr::new(FastOp::NewVector, size, 0))
        }
        "new_set" => {
            let size = instr.args.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
            Ok(FastInstr::new(FastOp::NewSet, size, 0))
        }

        "index" | "get_index" => Ok(FastInstr::simple(FastOp::Index)),
        "len" => Ok(FastInstr::simple(FastOp::Len)),
        "make_range" => Ok(FastInstr::simple(FastOp::MakeRange)),
        "array_pop" => Ok(FastInstr::simple(FastOp::ArrayPop)),
        "array_push" => Ok(FastInstr::simple(FastOp::ArrayPush)),
        "tuple_push" => Ok(FastInstr::simple(FastOp::TuplePush)),
        "list_push" => Ok(FastInstr::simple(FastOp::ListPush)),
        "vector_push" => Ok(FastInstr::simple(FastOp::VectorPush)),
        "set_push" => Ok(FastInstr::simple(FastOp::SetPush)),

        "new_obj" => Ok(FastInstr::simple(FastOp::NewObj)),
        "new_struct" => {
            let name = instr
                .args
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("Anonymous");
            let idx = symbols.intern_string(name);
            Ok(FastInstr::new(FastOp::NewStruct, idx as u64, 0))
        }
        "get_field" => {
            let name = instr
                .args
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| LowerError::MissingArgument("get_field name".into()))?;
            let idx = symbols.intern_string(name);
            Ok(FastInstr::new(FastOp::GetField, idx as u64, 0))
        }
        "set_field" => {
            let name = instr
                .args
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| LowerError::MissingArgument("set_field name".into()))?;
            let idx = symbols.intern_string(name);
            Ok(FastInstr::new(FastOp::SetField, idx as u64, 0))
        }

        // VM control
        "yield" => Ok(FastInstr::simple(FastOp::Yield)),
        "halt" => Ok(FastInstr::simple(FastOp::Halt)),
        "nop" | "label" => Ok(FastInstr::simple(FastOp::Nop)),
        "gc" => Ok(FastInstr::simple(FastOp::Gc)),
        "spawn" => Ok(FastInstr::simple(FastOp::Spawn)),
        "restart" => Ok(FastInstr::simple(FastOp::Restart)),
        "watchdog" => Ok(FastInstr::simple(FastOp::Watchdog)),

        // Exception
        "throw" => Ok(FastInstr::simple(FastOp::Throw)),
        "exit_try" => Ok(FastInstr::simple(FastOp::ExitTry)),
        "enter_try" => {
            let target = instr
                .args
                .get("target")
                .and_then(|v| v.as_u64()) // In CASM enter_try might use label or absolute?
                // VM impl says: `target` as u64.
                // Wait, VM impl uses `instr.args.get("target")... as usize`.
                // Usually labels are resolved. But `enter_try` in VM looks like it expects an absolute PC or resolved label.
                // In CASM, if it's a label name, we need to resolve it.
                // If it's "enter_try target:label", we treat it like jump.
                .map(|v| v as usize);

            if let Some(t) = target {
                // Absolute? Unlikely in CASM.
                Ok(FastInstr::new(FastOp::EnterTry, t as u64, 0))
            } else {
                // Try as string label
                let target_str = instr
                    .args
                    .get("target")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| LowerError::MissingArgument("enter_try target".into()))?;
                let label = format!("{}:{}", current_func, target_str);
                if let Some(&target_pc) = labels.get(&label) {
                    Ok(FastInstr::new(FastOp::EnterTry, target_pc as u64, 0))
                } else {
                    // Pending jump resolution for try?
                    // Yes, logic is same as jump.
                    pending_jumps.push((pc, label));
                    Ok(FastInstr::new(FastOp::EnterTry, 0, 0))
                }
            }
        }

        "await" => Ok(FastInstr::simple(FastOp::Await)),

        // String
        "str_contains" => Ok(FastInstr::simple(FastOp::StrContains)),
        "str_split" => Ok(FastInstr::simple(FastOp::StrSplit)),
        "str_replace" => Ok(FastInstr::simple(FastOp::StrReplace)),
        "str_join" => Ok(FastInstr::simple(FastOp::StrJoin)),

        other => Err(LowerError::UnknownOpcode(other.to_string())),
    }
}

/// Helper for lowering jump instructions that accept either integer or string label targets
fn lower_jump(
    instr: &Instruction,
    current_func: &str,
    labels: &HashMap<String, usize>,
    pending_jumps: &mut Vec<(usize, String)>,
    pc: usize,
    op: FastOp,
) -> Result<FastInstr, LowerError> {
    let target_val = instr
        .args
        .get("target")
        .ok_or_else(|| LowerError::MissingArgument(format!("{:?} target", op)))?;

    // Integer target: absolute PC offset
    if let Some(n) = target_val.as_u64() {
        return Ok(FastInstr::new(op, n, 0));
    }

    // String target: label name to resolve
    let target_str = target_val.as_str().ok_or_else(|| {
        LowerError::MissingArgument(format!("{:?} target (not int or string)", op))
    })?;
    let label = format!("{}:{}", current_func, target_str);
    if let Some(&target_pc) = labels.get(&label) {
        Ok(FastInstr::new(op, target_pc as u64, 0))
    } else {
        pending_jumps.push((pc, label));
        Ok(FastInstr::new(op, 0, 0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_interning() {
        let mut symbols = SymbolTables::new();
        let idx1 = symbols.intern_string("hello");
        let idx2 = symbols.intern_string("world");
        let idx3 = symbols.intern_string("hello"); // duplicate

        assert_eq!(idx1, 0);
        assert_eq!(idx2, 1);
        assert_eq!(idx3, 0); // same as first
        assert_eq!(symbols.strings.len(), 2);
    }

    #[test]
    fn test_local_slots() {
        let mut symbols = SymbolTables::new();
        let idx1 = symbols.get_or_create_local("x");
        let idx2 = symbols.get_or_create_local("y");
        let idx3 = symbols.get_or_create_local("x"); // duplicate

        assert_eq!(idx1, 0);
        assert_eq!(idx2, 1);
        assert_eq!(idx3, 0);
    }
}
