//! CVM1 binary bytecode format.
//!
//! Wire layout: MAGIC(4) | version(1) | manifest_len(2 BE) | manifest_json |
//!              n_consts(2 BE) | consts[each: len(2 BE) + utf-8] |
//!              code_len(4 BE) | code
//!
//! Operand encoding per opcode:
//!   PUSH      i64  8B signed BE
//!   PUSH_F64  f64  8B IEEE-754 BE
//!   PUSH_STR  u16  const-pool index
//!   LOAD/STORE u16 slot index (frame-local since v2)
//!   JMP/JZ/JNZ u32 byte offset
//!   CAP_CALL  u16 const-pool idx + u8 argc
//!   CALL      u16 const-pool idx (function name)
//!   NEW_ARRAY u16 element count
//!   others    no operand

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const MAGIC: &[u8; 4] = b"CVM1";
pub const VERSION: u8 = 2;
pub const MIN_VERSION: u8 = 1;

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
pub const EXEC_LANG: u8 = 0x70;
pub const NEW_OBJ: u8 = 0x71;
pub const SET_FIELD: u8 = 0x72;
pub const GET_FIELD: u8 = 0x73;
pub const SPAWN: u8 = 0x80;
pub const YIELD: u8 = 0x81;
pub const AWAIT: u8 = 0x82;
pub const HALT: u8 = 0xFF;

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
}

impl OperandKind {
    #[inline]
    pub fn byte_width(self) -> usize {
        match self {
            OperandKind::None => 0,
            OperandKind::I64 | OperandKind::F64 => 8,
            OperandKind::Str | OperandKind::Slot | OperandKind::Func | OperandKind::Count => 2,
            OperandKind::Addr => 4,
            OperandKind::Cap => 3,
        }
    }
}

pub fn operand_kind(opcode: u8) -> Option<OperandKind> {
    match opcode {
        NOP | POP | DUP | SWAP | ROT | PUSH_NULL | PRINT | RET | EXIT_TRY | THROW | STR_CONTAINS
        | STR_SPLIT | STR_REPLACE | STR_JOIN | MAKE_RANGE
        | SPAWN | YIELD | AWAIT | HALT
        | TYPEOF
        | ADD | SUB | MUL | DIV | MOD
        | NEG | EQ | LT | GT | NOT | NE | LE | GE | AND | OR | BITAND | BITOR | BITXOR | BITNOT
        | SHL | SHR | ARR_GET | ARR_SET | ARR_LEN | ARR_PUSH | ARR_POP => Some(OperandKind::None),
        PUSH | PUSH_BOOL => Some(OperandKind::I64),
        PUSH_F64 => Some(OperandKind::F64),
        PUSH_STR => Some(OperandKind::Str),
        LOAD | STORE => Some(OperandKind::Slot),
        JMP | JZ | JNZ | ENTER_TRY => Some(OperandKind::Addr),
        CAP_CALL => Some(OperandKind::Cap),
        CALL => Some(OperandKind::Func),
        EXEC_LANG => Some(OperandKind::Str),
        SET_FIELD | GET_FIELD | CAST => Some(OperandKind::Str),
        NEW_OBJ => Some(OperandKind::None),
        PICK | ROLL => Some(OperandKind::Count),
        NEW_ARRAY => Some(OperandKind::Count),
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
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Manifest {
    #[serde(default)]
    pub runtime: String,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub functions: HashMap<String, FunctionEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<String>,
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
}

impl Program {
    pub fn new(code: Vec<u8>, consts: Vec<String>, manifest: Manifest) -> Self {
        Self {
            code,
            consts,
            manifest,
            source_map: Vec::new(),
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
        }
    }

    pub fn to_blob(&self) -> Vec<u8> {
        let manifest_json = serde_json::to_string(&self.manifest).expect("manifest serialization");
        let mb = manifest_json.as_bytes();
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.push(VERSION);
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
        Ok(Self {
            code,
            consts,
            manifest,
            source_map: Vec::new(),
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
