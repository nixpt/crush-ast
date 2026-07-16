//! CASM text assembler and disassembler.
//!
//! Text format:
//!   ; or # start line comments
//!   label:              jump target label
//!   .func NAME          function entry point directive
//!   PUSH 42             integer operand
//!   PUSH_STR "hello"    string operand (double-quoted, \n \t \\ \" escapes)
//!   JMP loop            jump to a label
//!   CAP_CALL "io.print" 1    capability call
//!   HALT
//!
//! Two-pass: pass 1 builds the label/function offset table; pass 2 emits code.

use std::collections::HashMap;

use crate::bytecode::{Manifest, OperandKind, Program, instruction_size, operand_kind};

#[derive(Debug, thiserror::Error)]
#[error("line {line}: {msg}")]
pub struct AssemblyError {
    pub line: usize,
    pub msg: String,
}

impl AssemblyError {
    fn new(line: usize, msg: impl Into<String>) -> Self {
        Self {
            line,
            msg: msg.into(),
        }
    }
}

struct Parsed {
    op: String,
    opcode: u8,
    args: Vec<String>,
    line: usize,
}

pub fn assemble(
    source: &str,
    permissions: Option<&[&str]>,
    name: Option<&str>,
) -> Result<Program, AssemblyError> {
    let mut parsed: Vec<Parsed> = Vec::new();
    let mut labels: HashMap<String, usize> = HashMap::new();
    let mut functions: HashMap<String, usize> = HashMap::new();
    let mut entry_name: Option<String> = None;
    let mut consts: Vec<String> = Vec::new();
    let mut const_index: HashMap<String, usize> = HashMap::new();
    let mut offset: usize = 0;

    let intern =
        |s: String, consts: &mut Vec<String>, const_index: &mut HashMap<String, usize>| -> usize {
            if let Some(&i) = const_index.get(&s) {
                return i;
            }
            let i = consts.len();
            const_index.insert(s.clone(), i);
            consts.push(s);
            i
        };

    // Pass 1: compute label byte offsets.
    for (raw_line, raw) in source.lines().enumerate() {
        let lineno = raw_line + 1;
        let mut line = strip_comment(raw).trim().to_string();
        if line.is_empty() {
            continue;
        }

        // Strip label prefixes.
        loop {
            if let Some(rest) = strip_label(&line) {
                let colon = line.find(':').unwrap();
                let label = line[..colon].to_string();
                if labels.contains_key(&label) {
                    return Err(AssemblyError::new(
                        lineno,
                        format!("duplicate label {label:?}"),
                    ));
                }
                labels.insert(label, offset);
                line = rest.trim().to_string();
            } else {
                break;
            }
        }
        if line.is_empty() {
            continue;
        }

        if line.starts_with('.') {
            let toks = split_tokens(&line);
            match toks[0].as_str() {
                ".func" => {
                    if toks.len() != 2 {
                        return Err(AssemblyError::new(lineno, ".func needs exactly one name"));
                    }
                    let fname = toks[1].clone();
                    if !is_valid_ident(&fname) {
                        return Err(AssemblyError::new(
                            lineno,
                            format!("invalid function name {fname:?}"),
                        ));
                    }
                    if functions.contains_key(&fname) {
                        return Err(AssemblyError::new(
                            lineno,
                            format!("duplicate function {fname:?}"),
                        ));
                    }
                    functions.insert(fname.clone(), offset);
                    // Prefer 'main' as entry point, otherwise use first function
                    if entry_name.is_none() || fname == "main" {
                        entry_name = Some(fname);
                    }
                }
                d => {
                    return Err(AssemblyError::new(
                        lineno,
                        format!("unknown directive {d:?}"),
                    ));
                }
            }
            continue;
        }

        let toks = split_tokens(&line);
        let op = toks[0].to_uppercase();
        let opcode = opcode_for(&op)
            .ok_or_else(|| AssemblyError::new(lineno, format!("unknown opcode {:?}", toks[0])))?;
        let isize = instruction_size(opcode).unwrap();
        parsed.push(Parsed {
            op,
            opcode,
            args: toks[1..].to_vec(),
            line: lineno,
        });
        offset += isize;
    }

    // Pass 2: emit code.
    let mut code: Vec<u8> = Vec::new();
    let mut source_map: Vec<(usize, usize)> = Vec::new();
    for p in &parsed {
        source_map.push((p.line, code.len()));
        code.push(p.opcode);
        let kind = operand_kind(p.opcode).unwrap();
        match kind {
            OperandKind::None => {
                if !p.args.is_empty() {
                    return Err(AssemblyError::new(
                        p.line,
                        format!("{} takes no operand", p.op),
                    ));
                }
            }
            OperandKind::I64 => {
                let (v,) = require_args(&p.args, 1, &p.op, p.line)?;
                let i = parse_int(v, p.line)?;
                code.extend_from_slice(&i.to_be_bytes());
            }
            OperandKind::F64 => {
                let (v,) = require_args(&p.args, 1, &p.op, p.line)?;
                let f: f64 = v
                    .parse()
                    .map_err(|_| AssemblyError::new(p.line, format!("invalid float {v:?}")))?;
                code.extend_from_slice(&f.to_be_bytes());
            }
            OperandKind::Str => {
                let (v,) = require_args(&p.args, 1, &p.op, p.line)?;
                let s = parse_string(v, p.line)?;
                let idx = intern(s, &mut consts, &mut const_index);
                code.extend_from_slice(&(idx as u16).to_be_bytes());
            }
            OperandKind::Slot | OperandKind::Count => {
                let (v,) = require_args(&p.args, 1, &p.op, p.line)?;
                let val = parse_int(v, p.line)?;
                if !(0..=0xFFFF).contains(&val) {
                    return Err(AssemblyError::new(
                        p.line,
                        format!("operand out of range: {val}"),
                    ));
                }
                code.extend_from_slice(&(val as u16).to_be_bytes());
            }
            OperandKind::Addr => {
                let (v,) = require_args(&p.args, 1, &p.op, p.line)?;
                let target = labels
                    .get(v)
                    .copied()
                    .ok_or_else(|| AssemblyError::new(p.line, format!("unknown label {v:?}")))?;
                code.extend_from_slice(&(target as u32).to_be_bytes());
            }
            OperandKind::Cap => {
                if p.args.len() != 2 {
                    return Err(AssemblyError::new(
                        p.line,
                        "CAP_CALL needs <\"cap\"> <argc>",
                    ));
                }
                let cap = parse_string(&p.args[0], p.line)?;
                let argc_val = parse_int(&p.args[1], p.line)?;
                if !(0..=255).contains(&argc_val) {
                    return Err(AssemblyError::new(
                        p.line,
                        format!("argc out of range: {argc_val}"),
                    ));
                }
                let idx = intern(cap, &mut consts, &mut const_index);
                code.extend_from_slice(&(idx as u16).to_be_bytes());
                code.push(argc_val as u8);
            }
            OperandKind::Func => {
                let (v,) = require_args(&p.args, 1, &p.op, p.line)?;
                if !functions.contains_key(v) {
                    return Err(AssemblyError::new(
                        p.line,
                        format!("call to unknown function {v:?}"),
                    ));
                }
                let idx = intern(v.to_string(), &mut consts, &mut const_index);
                code.extend_from_slice(&(idx as u16).to_be_bytes());
            }
        }
    }

    let mut manifest = Manifest {
        runtime: "casm-v0".to_string(),
        permissions: permissions
            .map(|p| p.iter().map(|s| s.to_string()).collect())
            .unwrap_or_default(),
        name: name.map(str::to_string),
        ..Default::default()
    };
    if !functions.is_empty() {
        manifest.runtime = "casm-v1".to_string();
        manifest.functions = functions
            .iter()
            .map(|(k, &v)| (k.clone(), crate::bytecode::FunctionEntry { entry: v }))
            .collect();
        manifest.entry = entry_name;
    }

    Ok(Program::with_source_map(code, consts, manifest, source_map))
}

pub fn disassemble(program: &Program) -> String {
    use crate::bytecode::*;

    let code = &program.code;
    let mut lines: Vec<String> = Vec::new();

    // Collect jump targets and function offsets for label placement.
    let mut jump_targets: std::collections::BTreeSet<usize> = Default::default();
    let func_at: HashMap<usize, &str> = program
        .manifest
        .functions
        .iter()
        .map(|(k, v)| (v.entry, k.as_str()))
        .collect();
    let mut scan = 0usize;
    while scan < code.len() {
        let op = code[scan];
        if let Some(kind) = operand_kind(op) {
            if kind == OperandKind::Addr {
                let t = u32::from_be_bytes(code[scan + 1..scan + 5].try_into().unwrap()) as usize;
                jump_targets.insert(t);
            }
            scan += 1 + kind.byte_width();
        } else {
            break;
        }
    }

    let name_of: HashMap<u8, &str> = [
        (NOP, "NOP"),
        (PUSH, "PUSH"),
        (PUSH_STR, "PUSH_STR"),
        (POP, "POP"),
        (DUP, "DUP"),
        (SWAP, "SWAP"),
        (ROT, "ROT"),
        (PICK, "PICK"),
        (ROLL, "ROLL"),
        (PUSH_F64, "PUSH_F64"),
        (PUSH_NULL, "PUSH_NULL"),
        (PUSH_BOOL, "PUSH_BOOL"),
        (ADD, "ADD"),
        (SUB, "SUB"),
        (MUL, "MUL"),
        (DIV, "DIV"),
        (MOD, "MOD"),
        (NEG, "NEG"),
        (TYPEOF, "TYPEOF"),
        (CAST, "CAST"),
        (EQ, "EQ"),
        (LT, "LT"),
        (GT, "GT"),
        (NOT, "NOT"),
        (NE, "NE"),
        (LE, "LE"),
        (GE, "GE"),
        (AND, "AND"),
        (OR, "OR"),
        (BITAND, "BITAND"),
        (BITOR, "BITOR"),
        (BITXOR, "BITXOR"),
        (BITNOT, "BITNOT"),
        (SHL, "SHL"),
        (SHR, "SHR"),
        (LOAD, "LOAD"),
        (STORE, "STORE"),
        (JMP, "JMP"),
        (JZ, "JZ"),
        (JNZ, "JNZ"),
        (PRINT, "PRINT"),
        (CAP_CALL, "CAP_CALL"),
        (CALL, "CALL"),
        (RET, "RET"),
        (ENTER_TRY, "ENTER_TRY"),
        (EXIT_TRY, "EXIT_TRY"),
        (THROW, "THROW"),
        (SPAWN, "SPAWN"), (YIELD, "YIELD"), (AWAIT, "AWAIT"),
        (STR_CONTAINS, "STR_CONTAINS"),
        (STR_SPLIT, "STR_SPLIT"),
        (STR_REPLACE, "STR_REPLACE"),
        (STR_JOIN, "STR_JOIN"),
        (MAKE_RANGE, "MAKE_RANGE"),
        (EXEC_LANG, "EXEC_LANG"),
        (AI_QUERY, "AI_QUERY"),
        (AI_SYNTHESIZE, "AI_SYNTHESIZE"),
        (AI_AGENT_DELEGATION, "AI_AGENT_DELEGATION"),
        (AI_SEMANTIC_MATCH, "AI_SEMANTIC_MATCH"),
        (AI_LEARNING_LOOP, "AI_LEARNING_LOOP"),
        (AI_CONTEXT_AWARE, "AI_CONTEXT_AWARE"),
        (AI_TOOLCHAIN, "AI_TOOLCHAIN"),
        (MATH_POW, "MATH_POW"),
        (MATH_SQRT, "MATH_SQRT"),
        (MATH_ABS, "MATH_ABS"),
        (MATH_ROUND, "MATH_ROUND"),
        (MATH_FLOOR, "MATH_FLOOR"),
        (MATH_CEIL, "MATH_CEIL"),
        (VEC_ADD, "VEC_ADD"),
        (VEC_DOT, "VEC_DOT"),
        (MAT_MUL, "MAT_MUL"),
        (STR_STARTS_WITH, "STR_STARTS_WITH"),
        (STR_ENDS_WITH, "STR_ENDS_WITH"),
        (STR_TO_UPPER, "STR_TO_UPPER"),
        (STR_TO_LOWER, "STR_TO_LOWER"),
        (STR_TRIM, "STR_TRIM"),
        (NEW_OBJ, "NEW_OBJ"),
        (SET_FIELD, "SET_FIELD"),
        (GET_FIELD, "GET_FIELD"),
        (NEW_ARRAY, "NEW_ARRAY"),
        (ARR_GET, "ARR_GET"),
        (ARR_SET, "ARR_SET"),
        (ARR_LEN, "ARR_LEN"),
        (ARR_PUSH, "ARR_PUSH"),
        (ARR_POP, "ARR_POP"),
        (NEW_TUPLE, "NEW_TUPLE"),
        (TUPLE_PUSH, "TUPLE_PUSH"),
        (NEW_LIST, "NEW_LIST"),
        (LIST_PUSH, "LIST_PUSH"),
        (NEW_VECTOR, "NEW_VECTOR"),
        (VECTOR_PUSH, "VECTOR_PUSH"),
        (NEW_SET, "NEW_SET"),
        (SET_PUSH, "SET_PUSH"),
        (HALT, "HALT"),
    ]
    .iter()
    .copied()
    .collect();

    let mut ip = 0usize;
    while ip < code.len() {
        if let Some(fname) = func_at.get(&ip) {
            lines.push(format!(".func {fname}"));
        }
        if jump_targets.contains(&ip) {
            lines.push(format!("L{ip}:"));
        }
        let opcode = code[ip];
        let op_name = name_of.get(&opcode).copied().unwrap_or("<unknown>");
        let kind = match operand_kind(opcode) {
            Some(k) => k,
            None => {
                lines.push(format!("    {op_name}"));
                ip += 1;
                continue;
            }
        };
        let line = match kind {
            OperandKind::None => format!("    {op_name}"),
            OperandKind::I64 => {
                let v = i64::from_be_bytes(code[ip + 1..ip + 9].try_into().unwrap());
                format!("    {op_name} {v}")
            }
            OperandKind::F64 => {
                let v = f64::from_be_bytes(code[ip + 1..ip + 9].try_into().unwrap());
                format!("    {op_name} {v:?}")
            }
            OperandKind::Str => {
                let idx = u16::from_be_bytes(code[ip + 1..ip + 3].try_into().unwrap()) as usize;
                let s = program.consts.get(idx).map(|s| s.as_str()).unwrap_or("?");
                format!("    {op_name} {s:?}")
            }
            OperandKind::Slot | OperandKind::Count => {
                let v = u16::from_be_bytes(code[ip + 1..ip + 3].try_into().unwrap());
                format!("    {op_name} {v}")
            }
            OperandKind::Addr => {
                let t = u32::from_be_bytes(code[ip + 1..ip + 5].try_into().unwrap());
                format!("    {op_name} L{t}")
            }
            OperandKind::Cap => {
                let idx = u16::from_be_bytes(code[ip + 1..ip + 3].try_into().unwrap()) as usize;
                let argc = code[ip + 3];
                let cap = program.consts.get(idx).map(|s| s.as_str()).unwrap_or("?");
                format!("    {op_name} {cap:?} {argc}")
            }
            OperandKind::Func => {
                let idx = u16::from_be_bytes(code[ip + 1..ip + 3].try_into().unwrap()) as usize;
                let fname = program.consts.get(idx).map(|s| s.as_str()).unwrap_or("?");
                format!("    {op_name} {fname}")
            }
        };
        lines.push(line);
        ip += 1 + kind.byte_width();
    }
    lines.join("\n")
}

// ── helpers ─────────────────────────────────────────────────────────────────

fn opcode_for(name: &str) -> Option<u8> {
    use crate::bytecode::*;
    match name {
        "NOP" => Some(NOP),
        "PUSH" => Some(PUSH),
        "PUSH_STR" => Some(PUSH_STR),
        "POP" => Some(POP),
        "DUP" => Some(DUP),
        "SWAP" => Some(SWAP),
        "ROT" => Some(ROT),
        "PICK" => Some(PICK),
        "ROLL" => Some(ROLL),
        "PUSH_F64" => Some(PUSH_F64),
        "PUSH_NULL" => Some(PUSH_NULL),
        "PUSH_BOOL" => Some(PUSH_BOOL),
        "ADD" => Some(ADD),
        "SUB" => Some(SUB),
        "MUL" => Some(MUL),
        "DIV" => Some(DIV),
        "MOD" => Some(MOD),
        "NEG" => Some(NEG),
        "TYPEOF" => Some(TYPEOF),
        "CAST" => Some(CAST),
        "EQ" => Some(EQ),
        "LT" => Some(LT),
        "GT" => Some(GT),
        "NOT" => Some(NOT),
        "NE" => Some(NE),
        "LE" => Some(LE),
        "GE" => Some(GE),
        "AND" => Some(AND),
        "OR" => Some(OR),
        "BITAND" => Some(BITAND),
        "BITOR" => Some(BITOR),
        "BITXOR" => Some(BITXOR),
        "BITNOT" => Some(BITNOT),
        "SHL" => Some(SHL),
        "SHR" => Some(SHR),
        "LOAD" => Some(LOAD),
        "STORE" => Some(STORE),
        "JMP" => Some(JMP),
        "JZ" => Some(JZ),
        "JNZ" => Some(JNZ),
        "PRINT" => Some(PRINT),
        "CAP_CALL" => Some(CAP_CALL),
        "CALL" => Some(CALL),
        "RET" => Some(RET),
        "EXEC_LANG" => Some(EXEC_LANG),
        "ENTER_TRY" => Some(ENTER_TRY),
        "EXIT_TRY" => Some(EXIT_TRY),
        "THROW" => Some(THROW),
        "STR_CONTAINS" => Some(STR_CONTAINS),
        "STR_SPLIT" => Some(STR_SPLIT),
        "STR_REPLACE" => Some(STR_REPLACE),
        "STR_JOIN" => Some(STR_JOIN),
        "MAKE_RANGE" => Some(MAKE_RANGE),
        "NEW_OBJ" => Some(NEW_OBJ),
        "SET_FIELD" => Some(SET_FIELD),
        "GET_FIELD" => Some(GET_FIELD),
        "NEW_ARRAY" => Some(NEW_ARRAY),
        "ARR_GET" => Some(ARR_GET),
        "ARR_SET" => Some(ARR_SET),
        "ARR_LEN" => Some(ARR_LEN),
        "ARR_PUSH" => Some(ARR_PUSH),
        "ARR_POP" => Some(ARR_POP),
        "NEW_TUPLE" => Some(NEW_TUPLE),
        "TUPLE_PUSH" => Some(TUPLE_PUSH),
        "NEW_LIST" => Some(NEW_LIST),
        "LIST_PUSH" => Some(LIST_PUSH),
        "NEW_VECTOR" => Some(NEW_VECTOR),
        "VECTOR_PUSH" => Some(VECTOR_PUSH),
        "NEW_SET" => Some(NEW_SET),
        "SET_PUSH" => Some(SET_PUSH),
        "SPAWN" => Some(SPAWN),
        "YIELD" => Some(YIELD),
        "AWAIT" => Some(AWAIT),
        "AI_QUERY" => Some(AI_QUERY),
        "AI_SYNTHESIZE" => Some(AI_SYNTHESIZE),
        "AI_AGENT_DELEGATION" => Some(AI_AGENT_DELEGATION),
        "AI_SEMANTIC_MATCH" => Some(AI_SEMANTIC_MATCH),
        "AI_LEARNING_LOOP" => Some(AI_LEARNING_LOOP),
        "AI_CONTEXT_AWARE" => Some(AI_CONTEXT_AWARE),
        "AI_TOOLCHAIN" => Some(AI_TOOLCHAIN),
        "MATH_POW" => Some(MATH_POW),
        "MATH_SQRT" => Some(MATH_SQRT),
        "MATH_ABS" => Some(MATH_ABS),
        "MATH_ROUND" => Some(MATH_ROUND),
        "MATH_FLOOR" => Some(MATH_FLOOR),
        "MATH_CEIL" => Some(MATH_CEIL),
        "vec_add" | "VEC_ADD" => Some(VEC_ADD),
        "vec_dot" | "VEC_DOT" => Some(VEC_DOT),
        "mat_mul" | "MAT_MUL" => Some(MAT_MUL),
        "STR_STARTS_WITH" => Some(STR_STARTS_WITH),
        "STR_ENDS_WITH" => Some(STR_ENDS_WITH),
        "STR_TO_UPPER" => Some(STR_TO_UPPER),
        "STR_TO_LOWER" => Some(STR_TO_LOWER),
        "STR_TRIM" => Some(STR_TRIM),
        "HALT" => Some(HALT),
        _ => None,
    }
}

fn strip_comment(line: &str) -> &str {
    let chars: Vec<char> = line.chars().collect();
    let mut in_str = false;
    let mut esc = false;
    for (i, &ch) in chars.iter().enumerate() {
        if in_str {
            if esc {
                esc = false;
            } else if ch == '\\' {
                esc = true;
            } else if ch == '"' {
                in_str = false;
            }
            continue;
        }
        if ch == '"' {
            in_str = true;
            continue;
        }
        if ch == ';' || ch == '#' {
            return &line[..line
                .char_indices()
                .nth(i)
                .map(|(b, _)| b)
                .unwrap_or(line.len())];
        }
    }
    line
}

fn strip_label(line: &str) -> Option<String> {
    // Match ^([A-Za-z_][A-Za-z0-9_]*):(.*)$
    let pos = line.find(':')?;
    let candidate = &line[..pos];
    if candidate.is_empty() {
        return None;
    }
    if !candidate
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
    {
        return None;
    }
    if !candidate
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return None;
    }
    Some(line[pos + 1..].to_string())
}

fn is_valid_ident(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
}

fn split_tokens(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut tokens: Vec<String> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_whitespace() {
            i += 1;
            continue;
        }
        if chars[i] == '"' {
            let start = i;
            i += 1;
            let mut esc = false;
            while i < chars.len() {
                if esc {
                    esc = false;
                } else if chars[i] == '\\' {
                    esc = true;
                } else if chars[i] == '"' {
                    i += 1;
                    break;
                }
                i += 1;
            }
            tokens.push(chars[start..i].iter().collect());
        } else {
            let start = i;
            while i < chars.len() && !chars[i].is_whitespace() {
                i += 1;
            }
            tokens.push(chars[start..i].iter().collect());
        }
    }
    tokens
}

fn parse_string(token: &str, lineno: usize) -> Result<String, AssemblyError> {
    if token.len() < 2 || !token.starts_with('"') || !token.ends_with('"') {
        return Err(AssemblyError::new(
            lineno,
            format!("expected quoted string, got {token:?}"),
        ));
    }
    let body = &token[1..token.len() - 1];
    let mut out = String::new();
    let mut esc = false;
    for ch in body.chars() {
        if esc {
            out.push(match ch {
                'n' => '\n',
                't' => '\t',
                '"' => '"',
                '\\' => '\\',
                c => c,
            });
            esc = false;
        } else if ch == '\\' {
            esc = true;
        } else {
            out.push(ch);
        }
    }
    Ok(out)
}

fn parse_int(token: &str, lineno: usize) -> Result<i64, AssemblyError> {
    if let Some(hex) = token
        .strip_prefix("0x")
        .or_else(|| token.strip_prefix("0X"))
    {
        i64::from_str_radix(hex, 16)
    } else {
        token.parse::<i64>()
    }
    .map_err(|_| AssemblyError::new(lineno, format!("invalid integer {token:?}")))
}

fn require_args<'a>(
    args: &'a [String],
    n: usize,
    op: &str,
    lineno: usize,
) -> Result<(&'a str,), AssemblyError> {
    if args.len() != n {
        return Err(AssemblyError::new(
            lineno,
            format!("{op} takes {n} operand(s), got {}", args.len()),
        ));
    }
    Ok((&args[0],))
}
