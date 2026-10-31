//! CVM1 binary bytecode format.
//!
//! Wire layout: MAGIC(4) | version(1) | manifest_len(2 BE) | manifest_json |
//!              n_consts(2 BE) | consts[each: len(2 BE) + utf-8] |
//!              code_len(4 BE) | code
//!              [ version >= 3: kind_table ]
//!
//! The kind table (COSM K1.3, CRUSH-zorro-42) is the program's COSM module —
//! its `KindDecl`s and `InterfaceDecl`s:
//!
//!   kind_table = kt_format(1) | kt_len(4 BE) | kt_bytes
//!   kt_format 1 = the module's canonical COSM JSON (`CosmModule::to_json`)
//!
//! Versioning: the container version says whether the section is present.
//! [`Program::to_blob`] writes version [`VERSION_KIND_TABLE`] (3) only when
//! the program has a non-empty kind table; every other program is written
//! exactly as before, as version [`VERSION_NO_KIND_TABLE`] (2), byte for
//! byte. Versions 1 and 2 have no kind table and load with `cosm = None`. A
//! runtime that predates the section rejects a version-3 blob as an
//! unsupported version, so a kind table is never silently dropped. The
//! section's own format byte lets its encoding change without another
//! container bump; an unknown one is an error. Nothing may follow the
//! section.
//!
//! Operand encoding per opcode:
//!   PUSH      i64  8B signed BE
//!   PUSH_F64  f64  8B IEEE-754 BE
//!   PUSH_STR  u16  const-pool index
//!   LOAD/STORE u16 slot index (frame-local since v2)
//!   JMP/JZ/JNZ u32 byte offset
//!   CAP_CALL  u16 const-pool idx + u8 argc
//!   CALL      u16 const-pool idx (function name)
//!   MAKE_CLOSURE u16 const-pool idx (function) + u8 arity + u8 captured count
//!   CALL_VALUE   u16 argc
//!   NEW_ARRAY u16 element count
//!   GET_MEMBER/SET_MEMBER/HAS_MEMBER/DEL_MEMBER/IS_KIND/NEW_RECORD u16 const-pool idx
//!   CALL_MEMBER/CONSTRUCT u16 const-pool idx (member / Kind name) + u8 argc
//!   GET_SLOT/SET_SLOT u16 slot index
//!   others    no operand
//!
//! The COSM ops (0xC0–0xCA, K1.4, CRUSH-zorro-43) run on the scheduler VM and
//! the portable VM (see `cosm_rt`). A program that uses them carries a kind
//! table, and its function table records each function's arity (`arity` in
//! the manifest), which function values made from members need.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const MAGIC: &[u8; 4] = b"CVM1";
/// The newest container version this runtime reads (and writes, for a
/// program with a kind table). [`Program::to_blob`] writes
/// [`VERSION_NO_KIND_TABLE`] (2) for a program without one — most programs —
/// so a fresh compile's version byte is usually 2; readers accept
/// [`MIN_VERSION`]..=[`VERSION`] (1..=3).
pub const VERSION: u8 = VERSION_KIND_TABLE;
pub const MIN_VERSION: u8 = 1;
/// The version written for a program without a kind table (unchanged since
/// frame-local slots).
pub const VERSION_NO_KIND_TABLE: u8 = 2;
/// The first version with a kind-table section (COSM K1.3, CRUSH-zorro-42).
pub const VERSION_KIND_TABLE: u8 = 3;
/// Kind-table encoding: the module's canonical COSM JSON.
pub const KIND_TABLE_FORMAT_JSON: u8 = 1;

pub const NOP: u8 = 0x00;
pub const PUSH: u8 = 0x01;
pub const PUSH_STR: u8 = 0x02;
pub const POP: u8 = 0x03;
pub const DUP: u8 = 0x04;
pub const SWAP: u8 = 0x05;
pub const PUSH_F64: u8 = 0x06;
pub const PUSH_NULL: u8 = 0x07;
pub const PUSH_BOOL: u8 = 0x08;
pub const ROT: u8 = 0x09;
pub const PICK: u8 = 0x0A;
pub const ROLL: u8 = 0x0B;
pub const ADD: u8 = 0x10;
pub const SUB: u8 = 0x11;
pub const MUL: u8 = 0x12;
pub const DIV: u8 = 0x13;
pub const MOD: u8 = 0x14;
pub const NEG: u8 = 0x15;
pub const TYPEOF: u8 = 0x16;
pub const CAST: u8 = 0x17;
pub const EQ: u8 = 0x20;
pub const LT: u8 = 0x21;
pub const GT: u8 = 0x22;
pub const NOT: u8 = 0x23;
pub const NE: u8 = 0x24;
pub const LE: u8 = 0x25;
pub const GE: u8 = 0x26;
pub const AND: u8 = 0x27;
pub const OR: u8 = 0x28;
pub const BITAND: u8 = 0x29;
pub const BITOR: u8 = 0x2A;
pub const BITXOR: u8 = 0x2B;
pub const BITNOT: u8 = 0x2C;
pub const SHL: u8 = 0x2D;
pub const SHR: u8 = 0x2E;
pub const LOAD: u8 = 0x30;
pub const STORE: u8 = 0x31;
pub const JMP: u8 = 0x40;
pub const JZ: u8 = 0x41;
pub const JNZ: u8 = 0x42;
pub const PRINT: u8 = 0x50;
pub const CAP_CALL: u8 = 0x51;
pub const CALL: u8 = 0x52;
pub const RET: u8 = 0x53;
pub const ENTER_TRY: u8 = 0x54;
pub const EXIT_TRY: u8 = 0x55;
pub const THROW: u8 = 0x56;
pub const STR_CONTAINS: u8 = 0x57;
pub const STR_SPLIT: u8 = 0x58;
pub const STR_REPLACE: u8 = 0x59;
pub const STR_JOIN: u8 = 0x5A;
pub const MAKE_RANGE: u8 = 0x5B;
pub const NEW_ARRAY: u8 = 0x60;
pub const ARR_GET: u8 = 0x61;
pub const ARR_SET: u8 = 0x62;
pub const ARR_LEN: u8 = 0x63;
pub const ARR_PUSH: u8 = 0x64;
pub const ARR_POP: u8 = 0x65;
pub const NEW_TUPLE: u8 = 0x66;
pub const TUPLE_PUSH: u8 = 0x67;
pub const NEW_LIST: u8 = 0x68;
pub const LIST_PUSH: u8 = 0x69;
pub const NEW_VECTOR: u8 = 0x6A;
pub const VECTOR_PUSH: u8 = 0x6B;
pub const NEW_SET: u8 = 0x6C;
pub const SET_PUSH: u8 = 0x6D;
pub const MAP_KEYS: u8 = 0x6E;
pub const EXEC_LANG: u8 = 0x70;
pub const NEW_OBJ: u8 = 0x71;
pub const SET_FIELD: u8 = 0x72;
pub const GET_FIELD: u8 = 0x73;
pub const SPAWN: u8 = 0x80;
pub const YIELD: u8 = 0x81;
pub const AWAIT: u8 = 0x82;
/// Push a function value (CRUSH-251): pops the captured values, pushes
/// `Func { function, arity, captured }`.
pub const MAKE_CLOSURE: u8 = 0x83;
/// Call a function value: pops the callee (above the args), checks it is a
/// function of arity `argc`, pushes its captured values, calls it.
pub const CALL_VALUE: u8 = 0x84;
/// Box the top of the stack in a fresh cell (a variable closures capture by
/// reference, CRUSH-251 step 3).
pub const CELL_NEW: u8 = 0x85;
/// Pop a cell, push its value.
pub const CELL_GET: u8 = 0x86;
/// Pop a cell, then a value; store the value in the cell.
pub const CELL_SET: u8 = 0x87;
/// Call a function value with arguments stored in an array (CRUSH-293 D3).
pub const CALL_VALUE_APPLY: u8 = 0x88;
pub const AI_QUERY: u8 = 0x90;
pub const AI_SYNTHESIZE: u8 = 0x91;
pub const AI_AGENT_DELEGATION: u8 = 0x92;
pub const AI_SEMANTIC_MATCH: u8 = 0x93;
pub const AI_LEARNING_LOOP: u8 = 0x94;
pub const AI_CONTEXT_AWARE: u8 = 0x95;
pub const AI_TOOLCHAIN: u8 = 0x96;
pub const AI_GOAL_DECLARATION: u8 = 0x97;
pub const AI_PROGRESS_UPDATE: u8 = 0x98;
pub const AI_KNOWLEDGE_SHARING: u8 = 0x99;

// CRUSH-33: DOM opcodes — split into two non-colliding byte ranges to
// avoid the existing MATH/VEC block at 0xA0-0xA8 (MATH_POW..MAT_MUL).
//   0x9A-0x9F: query/get/set/create/remove/child (CRUD + tree-nav back-half of depth)
//   0xB5-0xB8: parent/attr/text/event (CRUD back-half + DOM-tree text/event surface)
// 0x9A lands directly after AI_KNOWLEDGE_SHARING 0x99 (single-byte
// adjacency). 0xB5 lands directly after STR_TRIM 0xB4 (also single-byte
// adjacency on the back-half).
pub const DOM_QUERY: u8 = 0x9A;
pub const DOM_GET: u8 = 0x9B;
pub const DOM_SET: u8 = 0x9C;
pub const DOM_CREATE: u8 = 0x9D;
pub const DOM_REMOVE: u8 = 0x9E;
pub const DOM_CHILD: u8 = 0x9F;
pub const DOM_PARENT: u8 = 0xB5;
pub const DOM_ATTR: u8 = 0xB6;
pub const DOM_TEXT: u8 = 0xB7;
pub const DOM_EVENT: u8 = 0xB8;

pub const MATH_POW: u8 = 0xA0;
pub const MATH_SQRT: u8 = 0xA1;
pub const MATH_ABS: u8 = 0xA2;
pub const MATH_ROUND: u8 = 0xA3;
pub const MATH_FLOOR: u8 = 0xA4;
pub const MATH_CEIL: u8 = 0xA5;

pub const VEC_ADD: u8 = 0xA6;
pub const VEC_DOT: u8 = 0xA7;
pub const MAT_MUL: u8 = 0xA8;

pub const STR_STARTS_WITH: u8 = 0xB0;
pub const STR_ENDS_WITH: u8 = 0xB1;
pub const STR_TO_UPPER: u8 = 0xB2;
pub const STR_TO_LOWER: u8 = 0xB3;
pub const STR_TRIM: u8 = 0xB4;

// COSM K1.4 (CRUSH-zorro-43): the generic member ops (see `cosm_rt`).
/// Pop an object, push its member (`GET_MEMBER "name"`).
pub const GET_MEMBER: u8 = 0xC0;
/// Pop a value, then an object; write the member; push the object back.
pub const SET_MEMBER: u8 = 0xC1;
/// Pop an object, push whether it has the member.
pub const HAS_MEMBER: u8 = 0xC2;
/// Pop an object, delete the member, push the object back.
pub const DEL_MEMBER: u8 = 0xC3;
/// `CALL_MEMBER "name" argc`: the receiver on top of `argc` arguments
/// (pushed last-first, as for `CALL`); call the method its Kind resolves.
pub const CALL_MEMBER: u8 = 0xC4;
/// `CONSTRUCT "Kind" argc`: create an instance from `argc` arguments by the
/// Kind's `construct` strategy. For a Kind built by a constructor function
/// the next instruction is a `POP` of that function's return value.
pub const CONSTRUCT: u8 = 0xC5;
/// Pop a Record, push the declared field at slot `i`.
pub const GET_SLOT: u8 = 0xC6;
/// Pop a value, then a Record; write slot `i`; push the Record back.
pub const SET_SLOT: u8 = 0xC7;
/// Pop a value, push whether it is an instance of the Kind (or of one that
/// has it in its resolution order).
pub const IS_KIND: u8 = 0xC8;
/// Pop a value, push a copy if it is an instance of a `value(…)` Kind.
pub const COPY: u8 = 0xC9;
/// Push a fresh instance of the Kind (field defaults applied), without
/// running any constructor: what a struct literal builds before it writes
/// its fields (`SET_FIELD`s follow).
pub const NEW_RECORD: u8 = 0xCA;
/// Push the Kind itself as a value (CRUSH-277 D8): what a class name
/// evaluates to, the receiver of a static member access.
pub const LOAD_KIND: u8 = 0xCB;

pub const HALT: u8 = 0xFF;

/// CRUSH-32: map an AI-opcode byte (0x90-0x99) to its `ai_native.<kind>`
/// gate suffix. Returns `None` for any non-AI opcode (defensive — callers
/// should only invoke this on bytes they already matched into the combined
/// AI match arm). Drives the host_caps.cap-call dispatch from
/// `crush-vm::scheduler` and `crush-vm::portable_vm`.
pub fn ai_native_kind_for_opcode(opcode: u8) -> Option<&'static str> {
    match opcode {
        AI_QUERY => Some("query"),
        AI_SYNTHESIZE => Some("synthesize"),
        AI_AGENT_DELEGATION => Some("agent_delegation"),
        AI_SEMANTIC_MATCH => Some("semantic_match"),
        AI_LEARNING_LOOP => Some("learning_loop"),
        AI_CONTEXT_AWARE => Some("context_aware"),
        AI_TOOLCHAIN => Some("toolchain"),
        AI_GOAL_DECLARATION => Some("goal_declaration"),
        AI_PROGRESS_UPDATE => Some("progress_update"),
        AI_KNOWLEDGE_SHARING => Some("knowledge_sharing"),
        _ => None,
    }
}

/// CRUSH-33: map a DOM-opcode byte (0x9A-0x9F or 0xB5-0xB8) to its
/// `dom_native.<kind>` gate suffix. Returns `None` for any non-DOM
/// opcode (defensive -- callers should only invoke this on bytes they
/// already matched into the combined DOM match arm wired in Commit 2).
/// Drives the host_caps cap-call dispatch from `crush-vm::scheduler`
/// and `crush-vm::portable_vm` (Commit 2).
///
/// Complexity: O(1) per slot byte — a match on a constant byte with at
/// most 10 arms, same micro-cost as the AI bridge; no perf concern.
pub fn dom_native_kind_for_opcode(opcode: u8) -> Option<&'static str> {
    match opcode {
        DOM_QUERY => Some("query"),
        DOM_GET => Some("get"),
        DOM_SET => Some("set"),
        DOM_CREATE => Some("create"),
        DOM_REMOVE => Some("remove"),
        DOM_CHILD => Some("child"),
        DOM_PARENT => Some("parent"),
        DOM_ATTR => Some("attr"),
        DOM_TEXT => Some("text"),
        DOM_EVENT => Some("event"),
        _ => None,
    }
}

/// CRUSH-34: map a concurrency-opcode byte (SPAWN=0x80, YIELD=0x81,
/// AWAIT=0x82; PRE-EXISTING slots in `bytecode.rs:95-97` -- NOT new
/// per CRUSH-34) to its `concurrency_native.<kind>` gate suffix.
/// Returns `None` for any non-concurrency opcode (defensive --
/// callers should only invoke this on bytes they already matched
/// into the combined concurrency match arm wired in Commit 2).
/// Drives the host_caps cap-call dispatch from
/// `crush-vm::scheduler` and `crush-vm::portable_vm` (Commit 2).
///
/// Complexity: O(1) per slot byte -- a match on a constant byte with
/// at most 3 arms (smaller than AI's 10 / DOM's 10), same micro-cost
/// as the AI bridge; no perf concern.
///
/// CRITICAL DIFFERENCE FROM ai/dom: concurrency ops carry ASYNC
/// semantics (cooperative scheduling, event-loop yield,
/// await-on-handle). The current stubs in `concurrency_native`
/// ignore per-opcode operands (task_id for SPAWN, event_id for
/// AWAIT, yield-count/duration for YIELD) -- revisit when the real
/// async backend lands.
pub fn concurrency_native_kind_for_opcode(opcode: u8) -> Option<&'static str> {
    match opcode {
        SPAWN => Some("spawn"),
        YIELD => Some("yield"),
        AWAIT => Some("await"),
        _ => None,
    }
}

/// How an opcode's operand bytes are interpreted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperandKind {
    None,
    I64,   // 8B signed big-endian
    F64,   // 8B IEEE-754 big-endian
    Str,   // 2B const-pool index
    Slot,  // 2B memory slot
    Addr,  // 4B byte offset
    Cap,   // 2B const-pool idx + 1B argc
    Func,  // 2B const-pool index
    Count, // 2B element count
    Closure, // 2B const-pool idx (function) + 1B arity + 1B captured count
    /// A member or Kind name + argc (`CALL_MEMBER`, `CONSTRUCT`): encoded like
    /// [`OperandKind::Cap`] — 2B const-pool idx + 1B argc — but the name is not
    /// a capability (COSM K1.4).
    Member,
}

impl OperandKind {
    #[inline]
    pub fn byte_width(self) -> usize {
        match self {
            OperandKind::None => 0,
            OperandKind::I64 | OperandKind::F64 => 8,
            OperandKind::Str | OperandKind::Slot | OperandKind::Func | OperandKind::Count => 2,
            OperandKind::Addr => 4,
            OperandKind::Cap | OperandKind::Member => 3,
            OperandKind::Closure => 4,
        }
    }
}

pub fn operand_kind(opcode: u8) -> Option<OperandKind> {
    match opcode {
        NOP | POP | DUP | SWAP | ROT | PUSH_NULL | PRINT | RET | EXIT_TRY | THROW | STR_CONTAINS
        | STR_SPLIT | STR_REPLACE | STR_JOIN | MAKE_RANGE
        | YIELD | AWAIT | HALT
        | TYPEOF
        | ADD | SUB | MUL | DIV | MOD
        | NEG | EQ | LT | GT | NOT | NE | LE | GE | AND | OR | BITAND | BITOR | BITXOR | BITNOT
        | SHL | SHR | ARR_GET | ARR_SET | ARR_LEN | ARR_PUSH | ARR_POP
        | TUPLE_PUSH | LIST_PUSH | VECTOR_PUSH | SET_PUSH | MAP_KEYS => Some(OperandKind::None),
        SPAWN => Some(OperandKind::Count),
        PUSH | PUSH_BOOL => Some(OperandKind::I64),
        PUSH_F64 => Some(OperandKind::F64),
        PUSH_STR => Some(OperandKind::Str),
        LOAD | STORE => Some(OperandKind::Slot),
        JMP | JZ | JNZ | ENTER_TRY => Some(OperandKind::Addr),
        CAP_CALL => Some(OperandKind::Cap),
        CALL => Some(OperandKind::Func),
        MAKE_CLOSURE => Some(OperandKind::Closure),
        CALL_VALUE => Some(OperandKind::Count),
        CELL_NEW | CELL_GET | CELL_SET | CALL_VALUE_APPLY => Some(OperandKind::None),
        GET_MEMBER | SET_MEMBER | HAS_MEMBER | DEL_MEMBER | IS_KIND | NEW_RECORD | LOAD_KIND => Some(OperandKind::Str),
        CALL_MEMBER | CONSTRUCT => Some(OperandKind::Member),
        GET_SLOT | SET_SLOT => Some(OperandKind::Count),
        COPY => Some(OperandKind::None),
        EXEC_LANG | GET_FIELD | SET_FIELD | CAST | AI_QUERY | AI_SYNTHESIZE | AI_AGENT_DELEGATION | AI_SEMANTIC_MATCH | AI_LEARNING_LOOP | AI_CONTEXT_AWARE | AI_TOOLCHAIN | AI_GOAL_DECLARATION | AI_PROGRESS_UPDATE | AI_KNOWLEDGE_SHARING | DOM_QUERY | DOM_GET | DOM_SET | DOM_CREATE | DOM_REMOVE | DOM_CHILD | DOM_PARENT | DOM_ATTR | DOM_TEXT | DOM_EVENT => Some(OperandKind::Str),
        NEW_OBJ | MATH_POW | MATH_SQRT | MATH_ABS | MATH_ROUND | MATH_FLOOR | MATH_CEIL | VEC_ADD | VEC_DOT | MAT_MUL | STR_STARTS_WITH | STR_ENDS_WITH | STR_TO_UPPER | STR_TO_LOWER | STR_TRIM => Some(OperandKind::None),
        PICK | ROLL => Some(OperandKind::Count),
        NEW_ARRAY | NEW_TUPLE | NEW_LIST | NEW_VECTOR | NEW_SET => Some(OperandKind::Count),
        _ => None,
    }
}

#[inline]
pub fn instruction_size(opcode: u8) -> Option<usize> {
    operand_kind(opcode).map(|k| 1 + k.byte_width())
}

/// Optional function entry in the manifest function table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionEntry {
    pub entry: usize,
    /// The function's parameter count. Recorded for programs with a kind
    /// table (COSM K1.4): a method used as a value (a bound method) needs it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arity: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Manifest {
    #[serde(default)]
    pub runtime: String,
    #[serde(default)]
    pub permissions: Vec<String>,
    /// The source's `@capabilities [...]` declaration (#258), kept so
    /// `crush-run caps` can report it for a compiled `.cvm1`. Informational:
    /// what the VM enforces is `permissions` plus the host's grants.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub declared_capabilities: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Serialized in name order (`sorted_functions`), so the same program
    /// writes the same `.cvm1` bytes every time (#365: `HashMap` order made
    /// two compiles of one file differ).
    #[serde(default, skip_serializing_if = "HashMap::is_empty", serialize_with = "sorted_functions")]
    pub functions: HashMap<String, FunctionEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<String>,
}

fn sorted_functions<S: serde::Serializer>(
    functions: &HashMap<String, FunctionEntry>,
    s: S,
) -> Result<S::Ok, S::Error> {
    let sorted: std::collections::BTreeMap<&String, &FunctionEntry> = functions.iter().collect();
    serde::Serialize::serialize(&sorted, s)
}

/// A compiled CVM1 program: manifest + const pool + flat code section.
#[derive(Debug, Clone, Default)]
pub struct Program {
    pub code: Vec<u8>,
    pub consts: Vec<String>,
    pub manifest: Manifest,
    /// Source line → bytecode offset mapping for debugger use.
    /// Each entry is `(line_number, bytecode_offset)`.
    /// Not serialized in the binary blob — only populated by
    /// `assemble()`.
    pub source_map: Vec<(usize, usize)>,
    /// Compile-time warnings collected while building this program
    /// (CRUSH-270's polyglot implicit-output/clobber warnings). Not
    /// serialized in the binary blob; the embedder (crush-run) surfaces them
    /// to stderr.
    pub warnings: Vec<String>,
    /// The kind table (COSM K1.3, CRUSH-zorro-42): the program's object
    /// declarations, or `None`. Serialized as the version-3 kind-table
    /// section when non-empty (see the module docs).
    pub cosm: Option<crush_cosm::CosmModule>,
}

impl Program {
    pub fn new(code: Vec<u8>, consts: Vec<String>, manifest: Manifest) -> Self {
        Self {
            code,
            consts,
            manifest,
            source_map: Vec::new(),
            warnings: Vec::new(),
            cosm: None,
        }
    }

    pub fn with_source_map(
        code: Vec<u8>,
        consts: Vec<String>,
        manifest: Manifest,
        source_map: Vec<(usize, usize)>,
    ) -> Self {
        Self {
            code,
            consts,
            manifest,
            source_map,
            warnings: Vec::new(),
            cosm: None,
        }
    }

    /// The kind table, if non-empty (at least one Kind or interface).
    pub fn kind_table(&self) -> Option<&crush_cosm::CosmModule> {
        self.cosm.as_ref().filter(|m| !casm::cosm_is_empty(m))
    }

    /// `Err(message)` when the kind table declares a Kind that can't run as
    /// a plain map yet — the rule every engine shares
    /// ([`casm::cosm_unsupported_reason`], see [`casm::Program::refuse_cosm`]).
    pub fn refuse_cosm(&self, engine: &str) -> Result<(), String> {
        match self.cosm.as_ref().and_then(|m| casm::cosm_unsupported_reason(m, engine)) {
            Some(reason) => Err(reason),
            None => Ok(()),
        }
    }

    pub fn to_blob(&self) -> Vec<u8> {
        let manifest_json = serde_json::to_string(&self.manifest).expect("manifest serialization");
        let mb = manifest_json.as_bytes();
        let kind_table = self.kind_table().map(crush_cosm::CosmModule::to_json);
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.push(if kind_table.is_some() { VERSION_KIND_TABLE } else { VERSION_NO_KIND_TABLE });
        out.extend_from_slice(&(mb.len() as u16).to_be_bytes());
        out.extend_from_slice(mb);
        out.extend_from_slice(&(self.consts.len() as u16).to_be_bytes());
        for s in &self.consts {
            let b = s.as_bytes();
            out.extend_from_slice(&(b.len() as u16).to_be_bytes());
            out.extend_from_slice(b);
        }
        out.extend_from_slice(&(self.code.len() as u32).to_be_bytes());
        out.extend_from_slice(&self.code);
        if let Some(json) = kind_table {
            out.push(KIND_TABLE_FORMAT_JSON);
            out.extend_from_slice(&(json.len() as u32).to_be_bytes());
            out.extend_from_slice(json.as_bytes());
        }
        out
    }

    pub fn from_blob(blob: &[u8]) -> Result<Self, crate::Error> {
        if blob.len() < 7 || &blob[..4] != MAGIC.as_ref() {
            return Err(crate::Error::BadMagic);
        }
        let mut off = 4usize;
        let version = blob[off];
        off += 1;
        if !(MIN_VERSION..=VERSION).contains(&version) {
            return Err(crate::Error::UnsupportedVersion(version));
        }
        let man_len = rd_u16(blob, &mut off)? as usize;
        if off + man_len > blob.len() {
            return Err(crate::Error::Truncated);
        }
        let manifest: Manifest = serde_json::from_slice(&blob[off..off + man_len])
            .map_err(|e| crate::Error::BadManifest(e.to_string()))?;
        off += man_len;
        let n_consts = rd_u16(blob, &mut off)? as usize;
        let mut consts = Vec::with_capacity(n_consts);
        for _ in 0..n_consts {
            let slen = rd_u16(blob, &mut off)? as usize;
            if off + slen > blob.len() {
                return Err(crate::Error::Truncated);
            }
            let s = std::str::from_utf8(&blob[off..off + slen])
                .map_err(|e| crate::Error::BadManifest(e.to_string()))?
                .to_string();
            consts.push(s);
            off += slen;
        }
        let code_len = rd_u32(blob, &mut off)? as usize;
        if off + code_len > blob.len() {
            return Err(crate::Error::Truncated);
        }
        let code = blob[off..off + code_len].to_vec();
        off += code_len;
        // Validate the code section at load (CRUSH-285 review #8): every
        // byte must start an instruction of this format, so an unknown
        // opcode is refused here — never discovered mid-execution,
        // where it could desync instruction boundaries (or NOP on a
        // backend that skips unknown bytes).
        {
            let mut pc = 0usize;
            while pc < code.len() {
                let size = instruction_size(code[pc])
                    .ok_or(crate::Error::UnknownOpcode(code[pc], pc))?;
                pc += size;
            }
            if pc != code.len() {
                return Err(crate::Error::Truncated);
            }
        }
        let cosm = if version >= VERSION_KIND_TABLE {
            if off >= blob.len() {
                return Err(crate::Error::Truncated);
            }
            let format = blob[off];
            off += 1;
            if format != KIND_TABLE_FORMAT_JSON {
                return Err(crate::Error::BadKindTable(format!(
                    "unknown kind-table format {format}"
                )));
            }
            let len = rd_u32(blob, &mut off)? as usize;
            if off + len > blob.len() {
                return Err(crate::Error::Truncated);
            }
            let json = std::str::from_utf8(&blob[off..off + len])
                .map_err(|e| crate::Error::BadKindTable(e.to_string()))?;
            let module = crush_cosm::CosmModule::from_json(json)
                .map_err(|e| crate::Error::BadKindTable(e.to_string()))?;
            off += len;
            if off != blob.len() {
                return Err(crate::Error::BadKindTable(format!(
                    "{} trailing byte(s) after the kind table",
                    blob.len() - off
                )));
            }
            Some(module)
        } else {
            None
        };
        Ok(Self {
            code,
            consts,
            manifest,
            source_map: Vec::new(),
            warnings: Vec::new(),
            cosm,
        })
    }
}

fn rd_u16(blob: &[u8], off: &mut usize) -> Result<u16, crate::Error> {
    if *off + 2 > blob.len() {
        return Err(crate::Error::Truncated);
    }
    let v = u16::from_be_bytes(blob[*off..*off + 2].try_into().unwrap());
    *off += 2;
    Ok(v)
}

fn rd_u32(blob: &[u8], off: &mut usize) -> Result<u32, crate::Error> {
    if *off + 4 > blob.len() {
        return Err(crate::Error::Truncated);
    }
    let v = u32::from_be_bytes(blob[*off..*off + 4].try_into().unwrap());
    *off += 4;
    Ok(v)
}
