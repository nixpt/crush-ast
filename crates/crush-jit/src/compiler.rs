//! Cranelift-based JIT compiler for Crush bytecode.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use cranelift_codegen::ir::{
    self, types, AbiParam, BlockArg, InstBuilder, MemFlagsData, Signature,
};
use cranelift_codegen::isa::{self, CallConv};
use cranelift_codegen::settings;
use cranelift_codegen::Context;
use cranelift_codegen::ir::condcodes::{IntCC, FloatCC};
use cranelift_codegen::ir::UserFuncName;
use cranelift_control::ControlPlane;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_native;

use crush_vm::fastvm::{FastInstr, FastOp, LoweredProgram};

use crate::runtime::JitContext;

const OFF_STACK: i64 = 0;
const OFF_STACK_TOP: i64 = 8192;
const OFF_LOCALS: i64 = 8200;
const OFF_RESULT: i64 = 8720;
const OFF_BUDGET: i64 = 8728;
const OFF_ERROR: i64 = 8736;

const TAG_NULL: i64 = 0x7FFC_0000_0000_0000i64;
const TAG_TRUE: i64 = 0x7FFC_0000_0000_0001i64;
const TAG_FALSE: i64 = 0x7FFC_0000_0000_0002i64;
const TAG_INT: i64 = 0x7FFD_0000_0000_0000i64;
const TAG_REF: i64 = 0x7FFE_0000_0000_0000i64;
const MASK_U64: u64 = 0xFFFF_0000_0000_0000;

pub struct JitProgram {
    _mem: region::Allocation,
    func: unsafe extern "C" fn(*mut JitContext),
}
impl JitProgram {
    #[inline]
    pub fn execute(&self, ctx: &mut JitContext) { unsafe { (self.func)(ctx as *mut JitContext) } }
}

pub struct JitCompiler { isa: Arc<dyn isa::TargetIsa> }

impl JitCompiler {
    pub fn new() -> anyhow::Result<Self> {
        let flags = settings::Flags::new(settings::builder());
        let b = cranelift_native::builder().map_err(|e| anyhow::anyhow!("{}", e))?;
        let isa = b.finish(flags).map_err(|e| anyhow::anyhow!("{:?}", e))?;
        Ok(Self { isa })
    }
    pub fn compile(&self, program: &LoweredProgram) -> anyhow::Result<JitProgram> {
        let ptr_ty = self.isa.pointer_type();
        let blocks = analyze_blocks(program);
        let mut func = ir::Function::with_name_signature(UserFuncName::user(0,0), sig(ptr_ty));
        let mut fctx = FunctionBuilderContext::new();
        let mut bld = FunctionBuilder::new(&mut func, &mut fctx);
        build_fn(&mut bld, &blocks);
        bld.finalize();

        let mut ctx = Context::for_function(func);
        let mut cp = ControlPlane::default();
        let compiled = ctx.compile(&*self.isa, &mut cp)
            .map_err(|e| anyhow::anyhow!("Cranelift error: {e:?}"))?;
        let code = compiled.code_buffer();
        if code.is_empty() { anyhow::bail!("empty code"); }
        let mut mem = region::alloc(code.len(), region::Protection::READ_WRITE)
            .map_err(|e| anyhow::anyhow!("mmap: {e}"))?;
        unsafe {
            std::ptr::copy_nonoverlapping(code.as_ptr(), mem.as_mut_ptr::<u8>(), code.len());
            region::protect(mem.as_ptr::<u8>(), mem.len(), region::Protection::READ_EXECUTE)
                .map_err(|e| anyhow::anyhow!("mprotect: {e}"))?;
        }
        let ptr = mem.as_mut_ptr::<u8>();
        let func = unsafe { std::mem::transmute::<*mut u8, unsafe extern "C" fn(*mut JitContext)>(ptr) };
        Ok(JitProgram { _mem: mem, func })
    }
}
impl Default for JitCompiler { fn default() -> Self { Self::new().unwrap() } }

fn sig(ptr: types::Type) -> Signature {
    let mut s = Signature::new(CallConv::SystemV); s.params.push(AbiParam::new(ptr)); s
}

fn analyze_blocks(program: &LoweredProgram) -> Vec<(usize, Vec<FastInstr>)> {
    let mut starts = BTreeSet::new();
    starts.insert(0);
    let insns = &program.instructions;
    for (i, instr) in insns.iter().enumerate() {
        match instr.op {
            FastOp::Jump | FastOp::JumpIf | FastOp::JumpIfNot | FastOp::Return | FastOp::Halt => {
                starts.insert(i + 1);
                if matches!(instr.op, FastOp::Jump | FastOp::JumpIf | FastOp::JumpIfNot) {
                    starts.insert(instr.arg as usize);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    let offs: Vec<usize> = starts.into_iter().collect();
    for i in 0..offs.len() {
        let s = offs[i];
        if s >= insns.len() { continue; }
        let e = offs.get(i+1).copied().unwrap_or(insns.len());
        if e > s { out.push((s, insns[s..e].to_vec())); }
    }
    out
}

// ── Builder ────────────────────────────────────────────────────────────────

fn build_fn(bld: &mut FunctionBuilder, blocks: &[(usize, Vec<FastInstr>)]) {
    let mut map: HashMap<usize, ir::Block> = HashMap::new();
    for &(off, _) in blocks { map.insert(off, bld.create_block()); }
    let entry = bld.create_block();
    bld.append_block_params_for_function_params(entry);
    bld.switch_to_block(entry);
    let ctx = bld.block_params(entry)[0];

    if let Some(&first) = map.get(&0) {
        dec_budget(bld, ctx);
        bld.ins().jump(first, &[] as &[BlockArg]);
    } else {
        let n = iconst(bld, TAG_NULL);
        sres(bld, ctx, n);
        bld.ins().return_(&[] as &[ir::Value]);
    }
    bld.seal_block(entry);

    for &(off, ref instrs) in blocks {
        let block = map[&off];
        bld.switch_to_block(block);
        let mut term = false;
        for (i, instr) in instrs.iter().enumerate() {
            term = emit_one(bld, ctx, off + i, instr, &map);
        }
        if !term {
            let n = iconst(bld, TAG_NULL);
            sres(bld, ctx, n);
            bld.ins().return_(&[] as &[ir::Value]);
        }
        bld.seal_block(block);
    }
}

// ── Primitives (single builder.ins() call each) ────────────────────────────

fn iconst(b: &mut FunctionBuilder, v: i64) -> ir::Value { b.ins().iconst(types::I64, v) }
fn load(b: &mut FunctionBuilder, a: ir::Value) -> ir::Value { b.ins().load(types::I64, MemFlagsData::trusted(), a, 0) }
fn store(b: &mut FunctionBuilder, a: ir::Value, v: ir::Value) { b.ins().store(MemFlagsData::trusted(), v, a, 0); }
fn iadd_imm(b: &mut FunctionBuilder, v: ir::Value, i: i64) -> ir::Value { b.ins().iadd_imm(v, i) }
fn imul_imm(b: &mut FunctionBuilder, v: ir::Value, i: i64) -> ir::Value { b.ins().imul_imm(v, i) }
fn iadd(b: &mut FunctionBuilder, a: ir::Value, b2: ir::Value) -> ir::Value { b.ins().iadd(a, b2) }
fn band(b: &mut FunctionBuilder, a: ir::Value, b2: ir::Value) -> ir::Value { b.ins().band(a, b2) }
fn bor(b: &mut FunctionBuilder, a: ir::Value, b2: ir::Value) -> ir::Value { b.ins().bor(a, b2) }
fn bnot(b: &mut FunctionBuilder, v: ir::Value) -> ir::Value { b.ins().bnot(v) }
fn band_imm(b: &mut FunctionBuilder, v: ir::Value, i: i64) -> ir::Value { b.ins().band_imm(v, i) }
fn icmp_eq(b: &mut FunctionBuilder, a: ir::Value, b2: ir::Value) -> ir::Value { b.ins().icmp(IntCC::Equal, a, b2) }
fn icmp_ne(b: &mut FunctionBuilder, a: ir::Value, b2: ir::Value) -> ir::Value { b.ins().icmp(IntCC::NotEqual, a, b2) }
fn ishl_imm(b: &mut FunctionBuilder, v: ir::Value, i: i64) -> ir::Value { b.ins().ishl_imm(v, i) }
fn sshr_imm(b: &mut FunctionBuilder, v: ir::Value, i: i64) -> ir::Value { b.ins().sshr_imm(v, i) }
fn select(b: &mut FunctionBuilder, c: ir::Value, t: ir::Value, f: ir::Value) -> ir::Value { b.ins().select(c, t, f) }
fn bf64(b: &mut FunctionBuilder, v: ir::Value) -> ir::Value { b.ins().bitcast(types::F64, MemFlagsData::new(), v) }
fn bi64(b: &mut FunctionBuilder, v: ir::Value) -> ir::Value { b.ins().bitcast(types::I64, MemFlagsData::new(), v) }
fn iadd2(b: &mut FunctionBuilder, x: ir::Value, y: ir::Value) -> ir::Value { b.ins().iadd(x, y) }
fn isub(b: &mut FunctionBuilder, x: ir::Value, y: ir::Value) -> ir::Value { b.ins().isub(x, y) }
fn imul(b: &mut FunctionBuilder, x: ir::Value, y: ir::Value) -> ir::Value { b.ins().imul(x, y) }
fn sdiv(b: &mut FunctionBuilder, x: ir::Value, y: ir::Value) -> ir::Value { b.ins().sdiv(x, y) }
fn ineg(b: &mut FunctionBuilder, v: ir::Value) -> ir::Value { b.ins().ineg(v) }
fn fadd(b: &mut FunctionBuilder, x: ir::Value, y: ir::Value) -> ir::Value { b.ins().fadd(x, y) }
fn fsub(b: &mut FunctionBuilder, x: ir::Value, y: ir::Value) -> ir::Value { b.ins().fsub(x, y) }
fn fmul(b: &mut FunctionBuilder, x: ir::Value, y: ir::Value) -> ir::Value { b.ins().fmul(x, y) }
fn fdiv(b: &mut FunctionBuilder, x: ir::Value, y: ir::Value) -> ir::Value { b.ins().fdiv(x, y) }
fn fneg(b: &mut FunctionBuilder, v: ir::Value) -> ir::Value { b.ins().fneg(v) }
fn icmp(b: &mut FunctionBuilder, cc: IntCC, a: ir::Value, b2: ir::Value) -> ir::Value { b.ins().icmp(cc, a, b2) }
fn fcmp(b: &mut FunctionBuilder, cc: FloatCC, a: ir::Value, b2: ir::Value) -> ir::Value { b.ins().fcmp(cc, a, b2) }

// ── Compound helpers (each builder.ins() call on its own line) ──────────────

fn dec_budget(b: &mut FunctionBuilder, ctx: ir::Value) {
    let a = iadd_imm(b, ctx, OFF_BUDGET);
    let cur = load(b, a);
    let nv = iadd_imm(b, cur, -1);
    store(b, a, nv);
}
fn sres(b: &mut FunctionBuilder, ctx: ir::Value, val: ir::Value) {
    let a = iadd_imm(b, ctx, OFF_RESULT);
    store(b, a, val);
}
fn serr(b: &mut FunctionBuilder, ctx: ir::Value) {
    let a = iadd_imm(b, ctx, OFF_ERROR);
    let one = iconst(b, 1);
    store(b, a, one);
}

fn push(b: &mut FunctionBuilder, ctx: ir::Value, val: ir::Value) {
    let ta = iadd_imm(b, ctx, OFF_STACK_TOP);
    let top = load(b, ta);
    let base = iadd_imm(b, ctx, OFF_STACK);
    let off = imul_imm(b, top, 8);
    let addr = iadd(b, base, off);
    store(b, addr, val);
    let nt = iadd_imm(b, top, 1);
    store(b, ta, nt);
}
fn pop(b: &mut FunctionBuilder, ctx: ir::Value) -> ir::Value {
    let ta = iadd_imm(b, ctx, OFF_STACK_TOP);
    let top = load(b, ta);
    let nt = iadd_imm(b, top, -1);
    store(b, ta, nt);
    let base = iadd_imm(b, ctx, OFF_STACK);
    let off = imul_imm(b, nt, 8);
    let addr = iadd(b, base, off);
    load(b, addr)
}
fn peek(b: &mut FunctionBuilder, ctx: ir::Value, n: u32) -> ir::Value {
    let ta = iadd_imm(b, ctx, OFF_STACK_TOP);
    let top = load(b, ta);
    let idx = iadd_imm(b, top, -(n as i64 + 1));
    let base = iadd_imm(b, ctx, OFF_STACK);
    let off = imul_imm(b, idx, 8);
    let addr = iadd(b, base, off);
    load(b, addr)
}
fn poke(b: &mut FunctionBuilder, ctx: ir::Value, n: u32, val: ir::Value) {
    let ta = iadd_imm(b, ctx, OFF_STACK_TOP);
    let top = load(b, ta);
    let idx = iadd_imm(b, top, -(n as i64 + 1));
    let base = iadd_imm(b, ctx, OFF_STACK);
    let off = imul_imm(b, idx, 8);
    let addr = iadd(b, base, off);
    store(b, addr, val);
}

fn lload(b: &mut FunctionBuilder, ctx: ir::Value, idx: usize) -> ir::Value {
    let addr = iadd_imm(b, ctx, OFF_LOCALS + (idx as i64) * 8);
    load(b, addr)
}
fn lstore(b: &mut FunctionBuilder, ctx: ir::Value, idx: usize, val: ir::Value) {
    let addr = iadd_imm(b, ctx, OFF_LOCALS + (idx as i64) * 8);
    store(b, addr, val);
}

fn is_int(b: &mut FunctionBuilder, val: ir::Value) -> ir::Value {
    let mask = iconst(b, MASK_U64 as i64);
    let int_pat = iconst(b, TAG_INT);
    let masked = band(b, val, mask);
    icmp_eq(b, masked, int_pat)
}
fn is_float(b: &mut FunctionBuilder, val: ir::Value) -> ir::Value {
    let mask = iconst(b, MASK_U64 as i64);
    let masked = band(b, val, mask);
    let s = iconst(b, 0x7FFC_0000_0000_0000i64);
    let i_tag = iconst(b, TAG_INT);
    let r_tag = iconst(b, TAG_REF);
    let eq_s = icmp_eq(b, masked, s);
    let eq_i = icmp_eq(b, masked, i_tag);
    let eq_r = icmp_eq(b, masked, r_tag);
    let or1 = bor(b, eq_s, eq_i);
    let or2 = bor(b, or1, eq_r);
    bnot(b, or2)
}
fn truthy(b: &mut FunctionBuilder, val: ir::Value) -> ir::Value {
    let ft = iconst(b, TAG_FALSE);
    let nt = iconst(b, TAG_NULL);
    let nf = icmp_ne(b, val, ft);
    let nn = icmp_ne(b, val, nt);
    band(b, nf, nn)
}
fn eint(b: &mut FunctionBuilder, val: ir::Value) -> ir::Value {
    let shl = ishl_imm(b, val, 48);
    sshr_imm(b, shl, 48)
}
fn tint(b: &mut FunctionBuilder, val: ir::Value) -> ir::Value {
    let base = iconst(b, TAG_INT);
    let low = band_imm(b, val, 0xFFFF);
    iadd(b, base, low)
}
fn tbool(b: &mut FunctionBuilder, cond: ir::Value) -> ir::Value {
    let t = iconst(b, TAG_TRUE);
    let f = iconst(b, TAG_FALSE);
    select(b, cond, t, f)
}

// ═══════════════════════════════════════════════════════════════════════════════
// Compile one instruction
// ═══════════════════════════════════════════════════════════════════════════════

fn emit_one(
    b: &mut FunctionBuilder, ctx: ir::Value, global_idx: usize,
    instr: &FastInstr, clif: &HashMap<usize, ir::Block>,
) -> bool {
    use FastOp::*;
    match instr.op {
        PushInt => {
            let v = (instr.arg as u64) & 0xFFFF;
            let cv = iconst(b, (TAG_INT as u64 | v) as i64);
            push(b, ctx, cv);
        }
        PushFloat => { let cv = iconst(b, instr.arg as i64); push(b, ctx, cv); }
        PushBool => {
            let v = if instr.arg != 0 { TAG_TRUE } else { TAG_FALSE };
            let cv = iconst(b, v);
            push(b, ctx, cv);
        }
        PushNull => { let cv = iconst(b, TAG_NULL); push(b, ctx, cv); }
        Dup => { let v = peek(b, ctx, 0); push(b, ctx, v); }
        Pop => { pop(b, ctx); }
        Swap => {
            let a = peek(b, ctx, 0);
            let bv = peek(b, ctx, 1);
            poke(b, ctx, 1, a);
            poke(b, ctx, 0, bv);
        }

        Add => arith(b, ctx, true, false, false),
        Sub => arith(b, ctx, false, true, false),
        Mul => arith(b, ctx, false, false, true),
        Div => arith(b, ctx, false, false, false),
        Mod => { let cv = iconst(b, TAG_NULL); push(b, ctx, cv); }

        Neg => {
            let val = pop(b, ctx);
            let ii = is_int(b, val);
            let int_bb = b.create_block();
            let float_bb = b.create_block();
            let merge_bb = b.create_block();
            b.append_block_param(merge_bb, types::I64);
            b.ins().brif(ii, int_bb, &[] as &[BlockArg], float_bb, &[] as &[BlockArg]);

            b.switch_to_block(int_bb);
            let ev = eint(b, val);
            let ne = ineg(b, ev);
            let r1 = tint(b, ne);
            b.ins().jump(merge_bb, &[BlockArg::Value(r1)]);

            b.switch_to_block(float_bb);
            let fv = bf64(b, val);
            let nf = fneg(b, fv);
            let r2 = bi64(b, nf);
            b.ins().jump(merge_bb, &[BlockArg::Value(r2)]);

            b.switch_to_block(merge_bb);
            push(b, ctx, b.block_params(merge_bb)[0]);

            b.seal_block(int_bb);
            b.seal_block(float_bb);
            b.seal_block(merge_bb);
        }

        Eq => do_cmp(b, ctx, IntCC::Equal, FloatCC::Equal),
        Ne => do_cmp(b, ctx, IntCC::NotEqual, FloatCC::NotEqual),
        Lt => do_cmp(b, ctx, IntCC::SignedLessThan, FloatCC::LessThan),
        Le => do_cmp(b, ctx, IntCC::SignedLessThanOrEqual, FloatCC::LessThanOrEqual),
        Gt => do_cmp(b, ctx, IntCC::SignedGreaterThan, FloatCC::GreaterThan),
        Ge => do_cmp(b, ctx, IntCC::SignedGreaterThanOrEqual, FloatCC::GreaterThanOrEqual),

        And => {
            let bv = pop(b, ctx);
            let a = pop(b, ctx);
            let ta = truthy(b, a);
            let tb = truthy(b, bv);
            let r = band(b, ta, tb);
            let rv = tbool(b, r);
            push(b, ctx, rv);
        }
        Or => {
            let bv = pop(b, ctx);
            let a = pop(b, ctx);
            let ta = truthy(b, a);
            let tb = truthy(b, bv);
            let r = bor(b, ta, tb);
            let rv = tbool(b, r);
            push(b, ctx, rv);
        }
        Not => {
            let a = pop(b, ctx);
            let ta = truthy(b, a);
            let nb = bnot(b, ta);
            let rv = tbool(b, nb);
            push(b, ctx, rv);
        }

        Jump => {
            if let Some(&t) = clif.get(&(instr.arg as usize)) {
                dec_budget(b, ctx);
                b.ins().jump(t, &[] as &[BlockArg]);
                return true;
            }
        }
        JumpIf => {
            let cond = pop(b, ctx);
            let c = truthy(b, cond);
            let ft = global_idx + 1;
            if let (Some(&tb), Some(&eb)) = (clif.get(&(instr.arg as usize)), clif.get(&ft)) {
                dec_budget(b, ctx);
                b.ins().brif(c, tb, &[] as &[BlockArg], eb, &[] as &[BlockArg]);
                return true;
            }
        }
        JumpIfNot => {
            let cond = pop(b, ctx);
            let t = truthy(b, cond);
            let nb = bnot(b, t);
            let ft = global_idx + 1;
            if let (Some(&tb), Some(&eb)) = (clif.get(&(instr.arg as usize)), clif.get(&ft)) {
                dec_budget(b, ctx);
                b.ins().brif(nb, tb, &[] as &[BlockArg], eb, &[] as &[BlockArg]);
                return true;
            }
        }
        Halt | Return => { let val = pop(b, ctx); sres(b, ctx, val); b.ins().return_(&[] as &[ir::Value]); return true; }
        Nop => {}

        LoadLocal => { let v = lload(b, ctx, instr.arg as usize); push(b, ctx, v); }
        StoreLocal => { let v = peek(b, ctx, 0); lstore(b, ctx, instr.arg as usize, v); }

        _ => { let cv = iconst(b, TAG_NULL); push(b, ctx, cv); }
    }
    false
}

// ── Arithmetic dispatch ─────────────────────────────────────────────────────

fn arith(b: &mut FunctionBuilder, ctx: ir::Value, add: bool, sub: bool, mul: bool) {
    let bv = pop(b, ctx);
    let a = pop(b, ctx);
    let ia = is_int(b, a);
    let ib = is_int(b, bv);
    let bi = band(b, ia, ib);
    let ibb = b.create_block();
    let fbb = b.create_block();
    let mb = b.create_block();
    b.append_block_param(mb, types::I64);
    b.ins().brif(bi, ibb, &[] as &[BlockArg], fbb, &[] as &[BlockArg]);

    b.switch_to_block(ibb);
    let av = eint(b, a);
    let bv2 = eint(b, bv);
    let ri = if add { iadd2(b, av, bv2) } else if sub { isub(b, av, bv2) } else if mul { imul(b, av, bv2) } else { sdiv(b, av, bv2) };
    let rti = tint(b, ri);
    b.ins().jump(mb, &[BlockArg::Value(rti)]);

    b.switch_to_block(fbb);
    let fa = is_float(b, a);
    let fb = is_float(b, bv);
    let bf = band(b, fa, fb);
    let ok = b.create_block();
    let err = b.create_block();
    b.ins().brif(bf, ok, &[] as &[BlockArg], err, &[] as &[BlockArg]);

    b.switch_to_block(ok);
    let af = bf64(b, a);
    let bf2 = bf64(b, bv);
    let rf = if add { fadd(b, af, bf2) } else if sub { fsub(b, af, bf2) } else if mul { fmul(b, af, bf2) } else { fdiv(b, af, bf2) };
    let rfb = bi64(b, rf);
    b.ins().jump(mb, &[BlockArg::Value(rfb)]);

    b.switch_to_block(err);
    serr(b, ctx);
    let nv = iconst(b, TAG_NULL);
    b.ins().jump(mb, &[BlockArg::Value(nv)]);

    b.switch_to_block(mb);
    push(b, ctx, b.block_params(mb)[0]);

    b.seal_block(ibb);
    b.seal_block(fbb);
    b.seal_block(ok);
    b.seal_block(err);
    b.seal_block(mb);
}

// ── Comparison dispatch ────────────────────────────────────────────────────

fn do_cmp(b: &mut FunctionBuilder, ctx: ir::Value, icc: IntCC, fcc: FloatCC) {
    let bv = pop(b, ctx);
    let a = pop(b, ctx);
    let ia = is_int(b, a);
    let ib = is_int(b, bv);
    let bi = band(b, ia, ib);
    let ibb = b.create_block();
    let fbb = b.create_block();
    let mb2 = b.create_block();
    b.append_block_param(mb2, types::I64);
    b.ins().brif(bi, ibb, &[] as &[BlockArg], fbb, &[] as &[BlockArg]);

    b.switch_to_block(ibb);
    let av = eint(b, a);
    let bv2 = eint(b, bv);
    let cmp = icmp(b, icc, av, bv2);
    let r1 = tbool(b, cmp);
    b.ins().jump(mb2, &[BlockArg::Value(r1)]);

    b.switch_to_block(fbb);
    let af = bf64(b, a);
    let bf2 = bf64(b, bv);
    let cmp = fcmp(b, fcc, af, bf2);
    let r2 = tbool(b, cmp);
    b.ins().jump(mb2, &[BlockArg::Value(r2)]);

    b.switch_to_block(mb2);
    push(b, ctx, b.block_params(mb2)[0]);

    b.seal_block(ibb);
    b.seal_block(fbb);
    b.seal_block(mb2);
}
