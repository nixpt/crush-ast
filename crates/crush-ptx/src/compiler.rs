use casm::{OpCode, Program, Function};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PtxType {
    B64, B32, B16, B8,
    U64, U32, U16, U8,
    S64, S32, S16, S8,
    F64, F32, F16,
    Pred,
}

impl PtxType {
    pub fn to_str(&self) -> &'static str {
        match self {
            PtxType::B64 => ".b64", PtxType::B32 => ".b32", PtxType::B16 => ".b16", PtxType::B8 => ".b8",
            PtxType::U64 => ".u64", PtxType::U32 => ".u32", PtxType::U16 => ".u16", PtxType::U8 => ".u8",
            PtxType::S64 => ".s64", PtxType::S32 => ".s32", PtxType::S16 => ".s16", PtxType::S8 => ".s8",
            PtxType::F64 => ".f64", PtxType::F32 => ".f32", PtxType::F16 => ".f16",
            PtxType::Pred => ".pred",
        }
    }
}

/// PTX bitwise/shl ops are typed by operand *width* only (`.b16/.b32/.b64`), not by
/// signedness — `and.s64` is not a legal instruction, `and.b64` is.
fn bitwise_ty(t: &PtxType) -> Result<&'static str, String> {
    Ok(match t {
        PtxType::B64 | PtxType::U64 | PtxType::S64 | PtxType::F64 => ".b64",
        PtxType::B32 | PtxType::U32 | PtxType::S32 | PtxType::F32 => ".b32",
        PtxType::B16 | PtxType::U16 | PtxType::S16 | PtxType::F16 => ".b16",
        PtxType::B8 | PtxType::U8 | PtxType::S8 => ".b8",
        PtxType::Pred => return Err("bitwise op not valid on a predicate".into()),
    })
}

#[derive(Debug, Clone)]
pub struct Reg {
    pub id: usize,
    pub ty: PtxType,
}

impl Reg {
    pub fn name(&self) -> String {
        match self.ty {
            PtxType::Pred => format!("%p{}", self.id),
            PtxType::F64 | PtxType::F32 | PtxType::F16 => format!("%f{}", self.id),
            _ => format!("%r{}", self.id),
        }
    }
}

pub struct PtxCompiler {
    reg_count: usize,
    stack: Vec<Reg>,
    locals: HashMap<String, Reg>,
    regs: Vec<Reg>, // to declare them later
}

impl PtxCompiler {
    pub fn new() -> Self {
        Self {
            reg_count: 0,
            stack: Vec::new(),
            locals: HashMap::new(),
            regs: Vec::new(),
        }
    }

    fn next_reg(&mut self, ty: PtxType) -> Reg {
        let r = Reg { id: self.reg_count, ty: ty.clone() };
        self.reg_count += 1;
        self.regs.push(r.clone());
        r
    }

    pub fn compile_function(&mut self, name: &str, func: &Function) -> Result<String, String> {
        let mut ptx = String::new();
        ptx.push_str(".version 7.5\n");
        ptx.push_str(".target sm_80\n");
        ptx.push_str(".address_size 64\n\n");
        
        ptx.push_str(&format!(".visible .entry {} (\n", name));
        
        for (i, param) in func.params.iter().enumerate() {
            let comma = if i < func.params.len() - 1 { "," } else { "" };
            ptx.push_str(&format!("\t.param .u64 {param}{comma}\n"));
        }
        ptx.push_str(") {\n");

        let mut instrs = Vec::new();
        let mut labels = HashSet::new();

        // Push params onto the stack in reverse order so the first param is at the top
        for param in func.params.iter().rev() {
            let r = self.next_reg(PtxType::S64);
            instrs.push(format!("\tld.param.u64 {}, [{}];", r.name(), param));
            self.stack.push(r);
        }

        // Pass 1: Translate
        for (i, instr) in func.body.iter().enumerate() {
            labels.insert(i); // We might jump to any instruction, but realistically only jump targets.
            
            // To handle labels nicely, we just insert a label before each instruction if it's targeted.
            // For now, let's just prefix every instruction with a label just in case, or collect targets.
        }

        let mut jump_targets = HashSet::new();
        for instr in &func.body {
            if let Ok(OpCode::Jmp(target)) = instr.to_opcode() { jump_targets.insert(target); }
            if let Ok(OpCode::JmpIf(target)) = instr.to_opcode() { jump_targets.insert(target); }
            if let Ok(OpCode::JmpIfNot(target)) = instr.to_opcode() { jump_targets.insert(target); }
        }

        for (i, instr) in func.body.iter().enumerate() {
            if jump_targets.contains(&i) {
                instrs.push(format!("L_{}:", i));
            }

            match instr.op.as_str() {
                "push_int" => {
                    let val = instr.args.get("value").and_then(|v| v.as_i64()).unwrap_or(0);
                    let r = self.next_reg(PtxType::S64);
                    instrs.push(format!("\tmov.u64 {}, {};", r.name(), val));
                    self.stack.push(r);
                }
                "push_float" => {
                    let val = instr.args.get("value").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let r = self.next_reg(PtxType::F64);
                    instrs.push(format!("\tmov.f64 {}, 0d{:016x}; // {}", r.name(), val.to_bits(), val));
                    self.stack.push(r);
                }
                "load" => {
                    let name = instr.args.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    if let Some(r) = self.locals.get(name) {
                        self.stack.push(r.clone());
                    } else if func.params.contains(&name.to_string()) {
                        let r = self.next_reg(PtxType::U64);
                        instrs.push(format!("\tld.param.u64 {}, [{}];", r.name(), name));
                        self.locals.insert(name.to_string(), r.clone());
                        self.stack.push(r);
                    } else {
                        return Err(format!("Unknown local: {}", name));
                    }
                }
                "store" => {
                    let name = instr.args.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    let val = self.stack.pop().ok_or("Stack underflow on store")?;
                    let r = self.next_reg(val.ty.clone());
                    instrs.push(format!("\tmov{} {}, {};", r.ty.to_str(), r.name(), val.name()));
                    self.locals.insert(name.to_string(), r);
                }
                "ptx_block_idx_x" => {
                    let r = self.next_reg(PtxType::U32);
                    instrs.push(format!("\tmov.u32 {}, %ctaid.x;", r.name()));
                    self.stack.push(r);
                }
                "ptx_block_dim_x" => {
                    let r = self.next_reg(PtxType::U32);
                    instrs.push(format!("\tmov.u32 {}, %ntid.x;", r.name()));
                    self.stack.push(r);
                }
                "ptx_lane_id" => {
                    let r = self.next_reg(PtxType::U32);
                    instrs.push(format!("\tmov.u32 {}, %laneid;", r.name()));
                    self.stack.push(r);
                }
                "add" | "sub" | "div" => {
                    let b = self.stack.pop().ok_or("Stack underflow on arith")?;
                    let a = self.stack.pop().ok_or("Stack underflow on arith")?;
                    if a.ty != b.ty {
                        return Err(format!("Type mismatch: {:?} != {:?}", a.ty, b.ty));
                    }
                    let r = self.next_reg(a.ty.clone());
                    let op_str = instr.op.as_str();
                    // PTX ISA requires a rounding modifier on floating-point `div` (ptxas:
                    // "Rounding modifier required for instruction 'div'") — integer div and
                    // add/sub don't take one.
                    let rnd = if op_str == "div" && matches!(a.ty, PtxType::F32 | PtxType::F64) {
                        ".rn"
                    } else {
                        ""
                    };
                    instrs.push(format!("\t{}{}{} {}, {}, {};", op_str, rnd, r.ty.to_str(), r.name(), a.name(), b.name()));
                    self.stack.push(r);
                }
                "mul" => {
                    let b = self.stack.pop().ok_or("Stack underflow on mul")?;
                    let a = self.stack.pop().ok_or("Stack underflow on mul")?;
                    if a.ty != b.ty {
                        return Err(format!("Type mismatch: {:?} != {:?}", a.ty, b.ty));
                    }
                    let r = self.next_reg(a.ty.clone());
                    // Integer `mul` needs a half-selector (.lo keeps the low N bits) — ptxas rejects
                    // a bare `mul.s64`. Float `mul` needs a rounding modifier (.rn).
                    let modf = if matches!(a.ty, PtxType::F32 | PtxType::F64 | PtxType::F16) {
                        ".rn"
                    } else {
                        ".lo"
                    };
                    instrs.push(format!("\tmul{}{} {}, {}, {};", modf, r.ty.to_str(), r.name(), a.name(), b.name()));
                    self.stack.push(r);
                }
                "and" | "or" | "xor" => {
                    let b = self.stack.pop().ok_or("Stack underflow on bitwise")?;
                    let a = self.stack.pop().ok_or("Stack underflow on bitwise")?;
                    if a.ty != b.ty {
                        return Err(format!("Type mismatch: {:?} != {:?}", a.ty, b.ty));
                    }
                    let r = self.next_reg(a.ty.clone());
                    // Bitwise ops are typed by *width* only in PTX (.b16/.b32/.b64), never .s*/.u*.
                    let bt = bitwise_ty(&a.ty)?;
                    instrs.push(format!("\t{}{} {}, {}, {};", instr.op.as_str(), bt, r.name(), a.name(), b.name()));
                    self.stack.push(r);
                }
                "shl" | "shr" => {
                    let b = self.stack.pop().ok_or("Stack underflow on shift")?;
                    let a = self.stack.pop().ok_or("Stack underflow on shift")?;
                    // The PTX shift amount must be a 32-bit value; a 64-bit shift-count register is
                    // rejected ("Arguments mismatch for instruction 'shr'"). Narrow it if needed.
                    let cnt = if matches!(b.ty, PtxType::U32 | PtxType::S32 | PtxType::B32) {
                        b.name()
                    } else {
                        let t = self.next_reg(PtxType::U32);
                        instrs.push(format!("\tcvt.u32{} {}, {};", b.ty.to_str(), t.name(), b.name()));
                        t.name()
                    };
                    let r = self.next_reg(a.ty.clone());
                    // shl is width-typed (.b*); shr keeps signedness (.s* = arithmetic).
                    let sty = if instr.op == "shl" { bitwise_ty(&a.ty)?.to_string() } else { a.ty.to_str().to_string() };
                    instrs.push(format!("\t{}{} {}, {}, {};", instr.op.as_str(), sty, r.name(), a.name(), cnt));
                    self.stack.push(r);
                }
                "fma" => {
                    let c = self.stack.pop().ok_or("Stack underflow on fma c")?;
                    let b = self.stack.pop().ok_or("Stack underflow on fma b")?;
                    let a = self.stack.pop().ok_or("Stack underflow on fma a")?;
                    if a.ty != b.ty || b.ty != c.ty {
                        return Err(format!("Type mismatch in fma: {:?}, {:?}, {:?}", a.ty, b.ty, c.ty));
                    }
                    let r = self.next_reg(a.ty.clone());
                    // default to round-to-nearest-even (.rn) if it's float
                    let rnd = if matches!(a.ty, PtxType::F32 | PtxType::F64) { ".rn" } else { "" };
                    instrs.push(format!("\tfma{}{} {}, {}, {}, {};", rnd, r.ty.to_str(), r.name(), a.name(), b.name(), c.name()));
                    self.stack.push(r);
                }
                "cvt" => {
                    let a = self.stack.pop().ok_or("Stack underflow on cvt")?;
                    let to_ty_str = instr.args.get("type").and_then(|v| v.as_str()).unwrap_or("f32");
                    let to_ty = match to_ty_str {
                        "f32" => PtxType::F32,
                        "f64" => PtxType::F64,
                        "u32" => PtxType::U32,
                        "s64" => PtxType::S64,
                        "s32" => PtxType::S32,
                        "s8" => PtxType::S8,
                        "u8" => PtxType::U8,
                        "f16" => PtxType::F16,
                        _ => return Err(format!("Unsupported cvt dest type: {}", to_ty_str)),
                    };
                    let r = self.next_reg(to_ty.clone());
                    // PTX ISA requires a rounding modifier on `cvt` whenever the destination
                    // is an integer converted from a float (`.rni`), or a float narrowed from
                    // a wider float (`.rn`) — ptxas rejects the instruction otherwise ("Rounding
                    // modifier required for instruction 'cvt'"). Widening (e.g. f32->f64) and
                    // plain integer<->integer conversions need no modifier.
                    let is_float = |t: &PtxType| matches!(t, PtxType::F64 | PtxType::F32 | PtxType::F16);
                    let float_width = |t: &PtxType| match t {
                        PtxType::F64 => 64, PtxType::F32 => 32, PtxType::F16 => 16, _ => 0,
                    };
                    let rnd = if is_float(&a.ty) && !is_float(&r.ty) {
                        ".rni"
                    } else if !is_float(&a.ty) && is_float(&r.ty) {
                        // int -> float: ptxas *requires* a rounding modifier here too, e.g.
                        // `cvt.f32.s64` is rejected ("Rounding modifier required for instruction
                        // 'cvt'"; verified on ptxas 13.3). Round-to-nearest-even.
                        ".rn"
                    } else if is_float(&a.ty) && is_float(&r.ty) && float_width(&r.ty) < float_width(&a.ty) {
                        ".rn"
                    } else {
                        ""
                    };
                    instrs.push(format!("\tcvt{}{}{} {}, {};", rnd, r.ty.to_str(), a.ty.to_str(), r.name(), a.name()));
                    self.stack.push(r);
                }
                "ptx_shfl_sync_bfly" => {
                    let mask = self.stack.pop().ok_or("Stack underflow on shfl mask")?;
                    let idx = self.stack.pop().ok_or("Stack underflow on shfl idx")?;
                    let var = self.stack.pop().ok_or("Stack underflow on shfl var")?;
                    let membermask = instr.args.get("membermask").and_then(|v| v.as_u64()).unwrap_or(0xffffffff);
                    let r = self.next_reg(var.ty.clone());
                    // shfl.sync.bfly.b32 d, a, b, c
                    // var is 'a', idx is 'b', mask is 'c' (usually 0x1f for warp). Wait, membermask is the first arg in asm.
                    // shfl.sync.bfly.b32 %r, %var, %idx, %mask, membermask
                    // We'll map to b32 for f32/u32
                    let b_ty = match var.ty {
                        PtxType::F32 | PtxType::U32 | PtxType::S32 => ".b32",
                        PtxType::F64 | PtxType::U64 | PtxType::S64 => return Err("shfl on 64-bit not supported yet".into()),
                        _ => ".b32"
                    };
                    instrs.push(format!("\tshfl.sync.bfly{} {}, {}, {}, {}, {:#x};", b_ty, r.name(), var.name(), idx.name(), mask.name(), membermask));
                    self.stack.push(r);
                }
                "lt" | "le" | "gt" | "ge" | "eq" | "ne" => {
                    let b = self.stack.pop().ok_or("Stack underflow on cmp")?;
                    let a = self.stack.pop().ok_or("Stack underflow on cmp")?;
                    if a.ty != b.ty {
                        return Err(format!("Type mismatch on cmp: {:?} != {:?}", a.ty, b.ty));
                    }
                    let p = self.next_reg(PtxType::Pred);
                    let op_str = instr.op.as_str();
                    instrs.push(format!("\tsetp.{}{} {}, {}, {};", op_str, a.ty.to_str(), p.name(), a.name(), b.name()));
                    self.stack.push(p);
                }
                "jmp" => {
                    let target = instr.args.get("target").and_then(|v| v.as_u64()).unwrap_or(0);
                    instrs.push(format!("\tbra L_{};", target));
                }
                "jmp_if" => {
                    let target = instr.args.get("target").and_then(|v| v.as_u64()).unwrap_or(0);
                    let p = self.stack.pop().ok_or("Stack underflow on jmp_if")?;
                    instrs.push(format!("\t@{} bra L_{};", p.name(), target));
                }
                "jmp_if_not" => {
                    let target = instr.args.get("target").and_then(|v| v.as_u64()).unwrap_or(0);
                    let p = self.stack.pop().ok_or("Stack underflow on jmp_if_not")?;
                    instrs.push(format!("\t@!{} bra L_{};", p.name(), target));
                }
                "ret" => {
                    instrs.push("\tret;".to_string());
                }
                "ptx_ld_global" => {
                    // Custom intrinsic: pop ptr, push loaded value.
                    let ty_str = instr.args.get("type").and_then(|v| v.as_str()).unwrap_or("f32");
                    let ptr = self.stack.pop().ok_or("Stack underflow on ld.global")?;
                    match ty_str {
                        // f16: ptxas rejects `ld.global.f16` into an .f16 register ("Unexpected
                        // instruction types specified for 'ld'"; verified on ptxas 13.3). Load the
                        // raw 16 bits into a .b16 register then widen to f32 — the only way Q6_K's
                        // super-block scale (a single f16) is consumed. Result on the stack is f32.
                        "f16" => {
                            let raw = self.next_reg(PtxType::B16);
                            instrs.push(format!("\tld.global.b16 {}, [{}];", raw.name(), ptr.name()));
                            let f = self.next_reg(PtxType::F32);
                            instrs.push(format!("\tcvt.f32.f16 {}, {};", f.name(), raw.name()));
                            self.stack.push(f);
                        }
                        // u8: PTX has no <=16-bit general-purpose register usable in the ALU, so a
                        // sub-word load zero-extends straight into a 32-bit register. (Q6_K packs its
                        // quants + int8 scales as bytes; the caller sign-extends the scale itself.)
                        "u8" => {
                            let r = self.next_reg(PtxType::U32);
                            instrs.push(format!("\tld.global.u8 {}, [{}];", r.name(), ptr.name()));
                            self.stack.push(r);
                        }
                        _ => {
                            let ty = match ty_str {
                                "f32" => PtxType::F32,
                                "f64" => PtxType::F64,
                                "u32" => PtxType::U32,
                                _ => return Err(format!("Unsupported ld.global type: {}", ty_str)),
                            };
                            let r = self.next_reg(ty.clone());
                            instrs.push(format!("\tld.global{} {}, [{}];", ty.to_str(), r.name(), ptr.name()));
                            self.stack.push(r);
                        }
                    }
                }
                "ptx_st_global" => {
                    // Custom intrinsic: pop value, pop ptr, store
                    let val = self.stack.pop().ok_or("Stack underflow on st.global val")?;
                    let ptr = self.stack.pop().ok_or("Stack underflow on st.global ptr")?;
                    instrs.push(format!("\tst.global{} [{}], {};", val.ty.to_str(), ptr.name(), val.name()));
                }
                "ptx_thread_idx_x" => {
                    let r = self.next_reg(PtxType::U32);
                    instrs.push(format!("\tmov.u32 {}, %tid.x;", r.name()));
                    self.stack.push(r);
                }

                // Capability call (stubbed)
                "cap_call" => {
                    let name = instr.args.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    if name.starts_with("gpu.") {
                        instrs.push(format!("\t// STUB: cap_call {}", name));
                    } else {
                        return Err(format!("Unimplemented capability call: {}", name));
                    }
                }
                op => {
                    // LOUD ERROR on unimplemented
                    return Err(format!("HARD ERROR: Unimplemented opcode in crush-ptx backend: {}", op));
                }
            }
        }

        // Output declarations
        let mut decls: HashMap<PtxType, Vec<String>> = HashMap::new();
        for r in &self.regs {
            decls.entry(r.ty.clone()).or_default().push(r.name());
        }

        for (ty, names) in decls {
            ptx.push_str(&format!("\t.reg {} {};\n", ty.to_str(), names.join(", ")));
        }

        ptx.push_str("\n");
        for instr in instrs {
            ptx.push_str(&format!("{}\n", instr));
        }

        ptx.push_str("}\n");
        Ok(ptx)
    }
}

pub fn compile_program(program: &Program) -> Result<String, String> {
    let mut compiler = PtxCompiler::new();
    let mut out = String::new();
    for (name, func) in &program.functions {
        let ptx = compiler.compile_function(name, func)?;
        out.push_str(&ptx);
    }
    Ok(out)
}
