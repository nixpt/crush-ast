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

use crate::runtime::{JitContext, jit_runtime_helper, JIT_MAX_CALL_DEPTH, OP_PUSH_STR, OP_MAKE_LIST, OP_MAKE_MAP, OP_INDEX, OP_LEN, OP_TYPEOF, OP_NEW_ARRAY, OP_ARRAY_PUSH, OP_ARRAY_POP, OP_ARR_SET, OP_STR_CONTAINS, OP_STR_STARTS_WITH, OP_STR_ENDS_WITH, OP_STR_TO_UPPER, OP_STR_TO_LOWER, OP_STR_TRIM, OP_STR_SPLIT, OP_STR_REPLACE, OP_STR_JOIN, OP_CAST, OP_NEW_TUPLE, OP_NEW_LIST, OP_NEW_VECTOR, OP_NEW_SET, OP_MAKE_RANGE, OP_CAP_CALL, OP_TUPLE_PUSH, OP_LIST_PUSH, OP_VECTOR_PUSH, OP_SET_PUSH, OP_GET_FIELD, OP_SET_FIELD, OP_NEW_OBJ, OP_NEW_STRUCT, OP_STR_SIM, OP_ENTER_TRY, OP_EXIT_TRY, OP_THROW};

const OFF_STACK: i64 = 0;
const OFF_STACK_TOP: i64 = 8192;
const OFF_LOCALS: i64 = 8200;
const OFF_RESULT: i64 = 8720;
const OFF_BUDGET: i64 = 8728;
const OFF_ERROR: i64 = 8736;
const OFF_CALL_STACK: i64 = 8768;
const OFF_CALL_STACK_TOP: i64 = 9792;
const OFF_HELPER_FN: i64 = 9800;
const OFF_HANDLER_PC: i64 = 9816;
const OFF_SAVED_PC: i64 = 10088;
const OFF_HOST_REQUEST_TAG: i64 = 10096;

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
        let helper_sig = import_helper_sig(&mut bld, ptr_ty);
        build_fn(&mut bld, &blocks, program, ptr_ty, helper_sig);
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

/// Signature for the JIT runtime helper function: `extern "C" fn(*mut JitContext, i64, i64)`.
fn helper_sig(ptr: types::Type) -> Signature {
    let mut s = Signature::new(CallConv::SystemV);
    s.params.push(AbiParam::new(ptr));       // ctx pointer
    s.params.push(AbiParam::new(types::I64)); // opcode
    s.params.push(AbiParam::new(types::I64)); // arg
    s
}

/// Import the helper signature into the builder, returning a `SigRef`.
fn import_helper_sig(bld: &mut FunctionBuilder, ptr_ty: types::Type) -> ir::SigRef {
    bld.import_signature(helper_sig(ptr_ty))
}

fn analyze_blocks(program: &LoweredProgram) -> Vec<(usize, Vec<FastInstr>)> {
    let mut starts = BTreeSet::new();
    starts.insert(0);
    let insns = &program.instructions;
    for (i, instr) in insns.iter().enumerate() {
        match instr.op {
            FastOp::Jump | FastOp::JumpIf | FastOp::JumpIfNot | FastOp::Return | FastOp::Halt | FastOp::Call => {
                starts.insert(i + 1);
                if matches!(instr.op, FastOp::Jump | FastOp::JumpIf | FastOp::JumpIfNot) {
                    starts.insert(instr.arg as usize);
                }
            }
            FastOp::EnterTry => {
                starts.insert(i + 1);
                starts.insert(instr.arg as usize);
            }
            FastOp::Throw => {
                starts.insert(i + 1);
            }
            // M2 Phase 5 Tier 2: yield ops create block boundaries so the next
            // instruction is a valid resume point.
            FastOp::CallHost | FastOp::ExecLang | FastOp::Spawn
            | FastOp::Gc | FastOp::ImportVar | FastOp::Await
            | FastOp::CrossLangCall => {
                starts.insert(i + 1);
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

fn build_fn(bld: &mut FunctionBuilder, blocks: &[(usize, Vec<FastInstr>)], program: &LoweredProgram, ptr_ty: types::Type, helper_sig: ir::SigRef) {
    let mut map: HashMap<usize, ir::Block> = HashMap::new();
    for &(off, _) in blocks { map.insert(off, bld.create_block()); }

    // Collect handler entries (unique handler PCs → CLIF blocks) for Throw dispatch.
    // The analyzer already creates blocks at handler PCs via EnterTry, so each
    // handler PC maps to an existing block in `map`.
    let mut handler_map: HashMap<usize, ir::Block> = HashMap::new();
    for instr in program.instructions.iter() {
        if instr.op == FastOp::EnterTry {
            if let Some(&blk) = map.get(&(instr.arg as usize)) {
                handler_map.entry(instr.arg as usize).or_insert(blk);
            }
        }
    }
    let mut handler_entries: Vec<(usize, ir::Block)> = handler_map.into_iter().collect();
    handler_entries.sort_by_key(|e| e.0);

    // Entry block — separate from instruction blocks.  Filled and sealed
    // FIRST so we can safely switch away from it later.
    let entry = bld.create_block();
    bld.append_block_params_for_function_params(entry);
    bld.switch_to_block(entry);
    let ctx = bld.block_params(entry)[0];

    // M2 Phase 5 Tier 2: trampoline entry with resume support.
    // On initial entry: saved_pc == 0 → jump to program.entry_point.
    // On resume: saved_pc != 0 → jump to the saved block, then clear saved_pc.
    let saved_pc_addr = iadd_imm(bld, ctx, OFF_SAVED_PC);
    let saved_pc = load(bld, saved_pc_addr);
    let zero = iconst(bld, 0);
    let is_resume = icmp_ne(bld, saved_pc, zero);

    let resume_bb = bld.create_block();
    let init_bb = bld.create_block();
    let merge_bb = bld.create_block();
    bld.append_block_param(merge_bb, ir::types::I64); // target block offset
    bld.ins().brif(is_resume, resume_bb, &[] as &[BlockArg], init_bb, &[] as &[BlockArg]);

    // Resume path: jump to saved_pc, then clear it.
    bld.switch_to_block(resume_bb);
    // Clear saved_pc so subsequent resumes don't loop.
    let zero2 = iconst(bld, 0);
    store(bld, saved_pc_addr, zero2);
    bld.ins().jump(merge_bb, &[BlockArg::Value(saved_pc)]);

    // Initial path: jump to entry_point.
    bld.switch_to_block(init_bb);
    let entry_pc = iconst(bld, program.entry_point as i64);
    bld.ins().jump(merge_bb, &[BlockArg::Value(entry_pc)]);

    // Merge: dispatch to the target block.
    bld.switch_to_block(merge_bb);
    let target_pc_val = bld.block_params(merge_bb)[0];
    // Build a dispatch cascade from PC offsets → CLIF blocks.
    let dispatch_targets: Vec<(i64, ir::Block)> = blocks.iter()
        .map(|&(off, _)| (off as i64, map[&off]))
        .collect();
    // If the target is in our map, jump there. Otherwise return null (safety net).
    if !dispatch_targets.is_empty() {
        dec_budget(bld, ctx);
        dispatch_by_eq(bld, target_pc_val, &dispatch_targets);
    } else {
        let n = iconst(bld, TAG_NULL);
        sres(bld, ctx, n);
        bld.ins().return_(&[] as &[ir::Value]);
    }
    bld.seal_block(entry);
    bld.seal_block(resume_bb);
    bld.seal_block(init_bb);
    bld.seal_block(merge_bb);

    // Create return blocks (one per CALL site).
    let mut return_blocks: Vec<ir::Block> = Vec::new();
    let mut call_idx_to_ret: HashMap<usize, usize> = HashMap::new();
    for (global_idx, instr) in program.instructions.iter().enumerate() {
        if instr.op == FastOp::Call {
            let ret_idx = return_blocks.len();
            return_blocks.push(bld.create_block());
            call_idx_to_ret.insert(global_idx, ret_idx);
        }
    }
    let ret_blocks: Vec<ir::Block> = return_blocks.clone();

    // Build return block bodies (need ctx for the body).
    // DON'T seal yet — Return instructions in main blocks also jump here.
    for (global_idx, _) in program.instructions.iter().enumerate() {
        if let Some(&ret_idx) = call_idx_to_ret.get(&global_idx) {
            let rb = ret_blocks[ret_idx];
            let next_pc = global_idx + 1;
            bld.switch_to_block(rb);
            if let Some(&next_block) = map.get(&next_pc) {
                dec_budget(bld, ctx);
                bld.ins().jump(next_block, &[] as &[BlockArg]);
            } else {
                let n = iconst(bld, TAG_NULL);
                sres(bld, ctx, n);
                bld.ins().return_(&[] as &[ir::Value]);
            }
        }
    }

    // Build a map from block offset to the next sequential block (if any)
    // so non-terminator blocks can fall through to the next block naturally.
    let mut next_off_map: HashMap<usize, ir::Block> = HashMap::new();
    for i in 0..blocks.len() {
        let (off, _) = blocks[i];
        if let Some(&(next_off, _)) = blocks.get(i + 1) {
            if let Some(&next_block) = map.get(&next_off) {
                next_off_map.insert(off, next_block);
            }
        }
    }

    // Collect handler block offsets for deferred sealing.
    let handler_offs: BTreeSet<usize> = handler_entries.iter().map(|(pc, _)| *pc).collect();

    // Process main blocks — fill with instructions but do NOT seal yet.
    // Sealing is deferred to a separate pass. Cranelift's seal_block can
    // recursively seal successor blocks through the SSA state machine
    // when all their predecessors are sealed. To prevent double-seals:
    // 1. Non-handler blocks are sealed in REVERSE order — successors get
    //    sealed first, so later predecessor seals won't cascade.
    // 2. Handler blocks are sealed last, after all Throw dispatch
    //    predecessors have been established.
    let mut non_handler_offs: Vec<usize> = Vec::new();
    for &(off, ref instrs) in blocks {
        let block = map[&off];
        bld.switch_to_block(block);
        let mut term = false;
        for (i, instr) in instrs.iter().enumerate() {
            term = emit_one(bld, ctx, off + i, instr, &map, program, &ret_blocks, &call_idx_to_ret, &handler_entries, ptr_ty, helper_sig);
        }
        if !term {
            if let Some(&next_block) = next_off_map.get(&off) {
                dec_budget(bld, ctx);
                bld.ins().jump(next_block, &[] as &[BlockArg]);
            } else {
                let n = iconst(bld, TAG_NULL);
                sres(bld, ctx, n);
                bld.ins().return_(&[] as &[ir::Value]);
            }
        }
        if !handler_offs.contains(&off) {
            non_handler_offs.push(off);
        }
    }

    // Seal non-handler blocks in REVERSE order — successors before
    // predecessors — to prevent SSA cascade double-seals.
    for off in non_handler_offs.iter().rev() {
        if let Some(&block) = map.get(off) {
            bld.seal_block(block);
        }
    }

    // Seal handler blocks after all Throw dispatch sites have been filled.
    // CONTRACT: handler blocks must contain a terminator (Throw, Return,
    // or Halt). The reverse-order non-handler sealing pass above may
    // cascade-seal a handler block if it is reachable via normal
    // fallthrough from a non-handler block. The terminator prevents this.
    for &off in &handler_offs {
        if let Some(&block) = map.get(&off) {
            bld.seal_block(block);
        }
    }

    // Seal return blocks.
    for &rb in &ret_blocks {
        bld.seal_block(rb);
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
fn ishl(b: &mut FunctionBuilder, v: ir::Value, a: ir::Value) -> ir::Value { b.ins().ishl(v, a) }
fn sshr(b: &mut FunctionBuilder, v: ir::Value, a: ir::Value) -> ir::Value { b.ins().sshr(v, a) }
fn bxor(b: &mut FunctionBuilder, a: ir::Value, b2: ir::Value) -> ir::Value { b.ins().bxor(a, b2) }
fn select(b: &mut FunctionBuilder, c: ir::Value, t: ir::Value, f: ir::Value) -> ir::Value { b.ins().select(c, t, f) }
fn bf64(b: &mut FunctionBuilder, v: ir::Value) -> ir::Value { b.ins().bitcast(types::F64, MemFlagsData::new(), v) }
fn bi64(b: &mut FunctionBuilder, v: ir::Value) -> ir::Value { b.ins().bitcast(types::I64, MemFlagsData::new(), v) }
fn iadd2(b: &mut FunctionBuilder, x: ir::Value, y: ir::Value) -> ir::Value { b.ins().iadd(x, y) }
fn isub(b: &mut FunctionBuilder, x: ir::Value, y: ir::Value) -> ir::Value { b.ins().isub(x, y) }
fn imul(b: &mut FunctionBuilder, x: ir::Value, y: ir::Value) -> ir::Value { b.ins().imul(x, y) }
fn sdiv(b: &mut FunctionBuilder, x: ir::Value, y: ir::Value) -> ir::Value { b.ins().sdiv(x, y) }
fn srem(b: &mut FunctionBuilder, x: ir::Value, y: ir::Value) -> ir::Value { b.ins().srem(x, y) }
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
    // Decrement, but saturate at 0 (don't wrap).
    // When budget reaches 0, all subsequent dec_budget calls keep it at 0.
    // The host detects exhaustion after execution via ctx.budget == 0.
    // This is ExoLight Tier 1 fuel metering.
    //
    // Use icmp_eq(cur, 0) rather than icmp_slt on the decremented value
    // because u64::MAX ("no limit") is -1 as I64, which icmp_slt would
    // incorrectly treat as wrapped.
    let zero = iconst(b, 0);
    let is_zero = icmp_eq(b, cur, zero);
    let dec = iadd_imm(b, cur, -1);
    let nv = select(b, is_zero, zero, dec);
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

// ── Call-stack helpers ───────────────────────────────────────────────────────

/// Push a return frame onto the JIT call stack.
fn push_frame(b: &mut FunctionBuilder, ctx: ir::Value, return_block: i64) {
    let cst_addr = iadd_imm(b, ctx, OFF_CALL_STACK_TOP);
    let top = load(b, cst_addr);
    let frame_addr = iadd_imm(b, ctx, OFF_CALL_STACK);
    let entry_off = imul_imm(b, top, 16);
    let entry = iadd(b, frame_addr, entry_off);
    let rb_val = iconst(b, return_block);
    store(b, entry, rb_val);
    let new_top = iadd_imm(b, top, 1);
    store(b, cst_addr, new_top);
}

/// Pop a return frame from the JIT call stack, returning the saved `return_block` index.
fn pop_frame(b: &mut FunctionBuilder, ctx: ir::Value) -> ir::Value {
    let cst_addr = iadd_imm(b, ctx, OFF_CALL_STACK_TOP);
    let top = load(b, cst_addr);
    let new_top = iadd_imm(b, top, -1);
    store(b, cst_addr, new_top);
    let frame_addr = iadd_imm(b, ctx, OFF_CALL_STACK);
    let entry_off = imul_imm(b, new_top, 16);
    let entry = iadd(b, frame_addr, entry_off);
    load(b, entry)
}

/// Dispatch to one of `targets` by comparing `val` to each target's key
/// using a brif cascade (avoids complex JumpTableData API).
///
/// For targets [(k0, b0), ..., (kn, bn)], builds:
///   brif(val == k0, b0, chain_0)
///   chain_0: brif(val == k1, b1, chain_1)
///   ...
///   chain_{n-2}: jump bn
///
/// All intermediate chain blocks are created and sealed here.
/// If `targets` is empty, emits an immediate return (safety net —
/// should not occur in practice, but retained for defense-in-depth).
fn dispatch_by_eq(b: &mut FunctionBuilder, val: ir::Value, targets: &[(i64, ir::Block)]) {
    if targets.is_empty() {
        b.ins().return_(&[] as &[ir::Value]);
        return;
    }
    if targets.len() == 1 {
        b.ins().jump(targets[0].1, &[] as &[BlockArg]);
        return;
    }
    // Pre-create all intermediate chain blocks so they are available as brif targets.
    let chain: Vec<ir::Block> = (0..targets.len() - 1).map(|_| b.create_block()).collect();
    for (i, (key, blk)) in targets[..targets.len() - 1].iter().enumerate() {
        if i > 0 {
            b.switch_to_block(chain[i - 1]);
        }
        let next_bb = chain[i];
        let const_key = iconst(b, *key);
        let cmp = icmp_eq(b, val, const_key);
        b.ins().brif(cmp, *blk, &[] as &[BlockArg], next_bb, &[] as &[BlockArg]);
    }
    // Final chain block: unconditional jump to last target
    b.switch_to_block(*chain.last().unwrap());
    b.ins().jump(targets[targets.len() - 1].1, &[] as &[BlockArg]);
    // Seal all chain blocks now that they have terminators
    for &cb in &chain {
        b.seal_block(cb);
    }
}

/// Thin wrapper: converts handler entries (usize PC → Block) to the
/// unified dispatch format.
fn emit_handler_dispatch(b: &mut FunctionBuilder, hp_val: ir::Value, entries: &[(usize, ir::Block)]) {
    let targets: Vec<(i64, ir::Block)> = entries.iter().map(|(pc, blk)| (*pc as i64, *blk)).collect();
    dispatch_by_eq(b, hp_val, &targets);
}

/// Thin wrapper: converts sequential return blocks (0..N → Block) to the
/// unified dispatch format.
fn emit_return_dispatch(b: &mut FunctionBuilder, idx: ir::Value, targets: &[ir::Block]) {
    let targets: Vec<(i64, ir::Block)> = targets.iter().enumerate().map(|(i, &blk)| (i as i64, blk)).collect();
    dispatch_by_eq(b, idx, &targets);
}

// ── M2 Phase 5 Tier 2: Host-request yield (trampoline escape) ──────────────

/// Tag values for `host_request_tag` in JitContext.
/// Must match the variant ordering in `HostRequest` enum.
const HOST_REQ_CALL_HOST: i64 = 0;
const HOST_REQ_EXEC_LANG: i64 = 1;
const HOST_REQ_SPAWN: i64 = 2;
const HOST_REQ_GC: i64 = 3;
const HOST_REQ_IMPORT_VAR: i64 = 4;
const HOST_REQ_AWAIT: i64 = 5;

/// Emit a host-request yield: save `next_pc` so the trampoline can resume
/// after the host processes the request, set `host_request_tag`, store null
/// result, and `return_()` to the host.
fn emit_host_yield(b: &mut FunctionBuilder, ctx: ir::Value, next_pc: usize, request_tag: i64) {
    // Save next_pc (the block offset to resume at)
    let spc_addr = iadd_imm(b, ctx, OFF_SAVED_PC);
    let spc_val = iconst(b, next_pc as i64);
    store(b, spc_addr, spc_val);

    // Set host_request_tag so the host knows which request variant to handle
    let req_addr = iadd_imm(b, ctx, OFF_HOST_REQUEST_TAG);
    let req_val = b.ins().iconst(ir::types::I32, request_tag);
    b.ins().store(MemFlagsData::trusted(), req_val, req_addr, 0);

    // Store null result (the host will populate result before resume)
    let null_val = iconst(b, TAG_NULL);
    sres(b, ctx, null_val);

    b.ins().return_(&[] as &[ir::Value]);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Compile one instruction
// ═══════════════════════════════════════════════════════════════════════════════

/// Emit a call to the runtime helper function via `call_indirect`.
/// Does NOT check for errors — use `emit_helper_call_checked` for
/// helpers that can set `OFF_ERROR`.
fn emit_helper_call(b: &mut FunctionBuilder, ctx: ir::Value, opcode: i64, arg: i64, ptr_ty: types::Type, helper_sig: ir::SigRef) {
    let off_addr = iadd_imm(b, ctx, OFF_HELPER_FN);
    let helper_addr = load(b, off_addr);
    let callee = b.ins().bitcast(ptr_ty, MemFlagsData::new(), helper_addr);
    let op_val = iconst(b, opcode);
    let arg_val = iconst(b, arg);
    b.ins().call_indirect(helper_sig, callee, &[ctx, op_val, arg_val]);
}

/// Emit a checked call to the runtime helper. After `call_indirect`,
/// loads `OFF_ERROR` and branches to a trap path if the helper set
/// the error flag (CRUSH-17 #2). Switches to the success block so
/// the caller continues naturally. The trap path stores null result
/// and returns.
fn emit_helper_call_checked(
    b: &mut FunctionBuilder, ctx: ir::Value, opcode: i64, arg: i64,
    ptr_ty: types::Type, helper_sig: ir::SigRef,
) {
    // Emit the call_indirect (same as emit_helper_call).
    emit_helper_call(b, ctx, opcode, arg, ptr_ty, helper_sig);

    // Load error flag and branch.
    // NOTE: error is i32, but we load I64 (8 bytes). Mask to low 32 bits
    // since the adjacent throw_consumed_handler field may be non-zero.
    let err_addr = iadd_imm(b, ctx, OFF_ERROR);
    let err_val_raw = b.ins().load(types::I64, MemFlagsData::new(), err_addr, 0);
    let err_mask = iconst(b, 0xFFFF_FFFFi64);
    let err_val = band(b, err_val_raw, err_mask);
    let zero = iconst(b, 0);
    let has_err = icmp_ne(b, err_val, zero);
    let ok_bb = b.create_block();
    let err_bb = b.create_block();
    b.ins().brif(has_err, err_bb, &[] as &[BlockArg], ok_bb, &[] as &[BlockArg]);

    // Error path: null result, halt.
    b.switch_to_block(err_bb);
    let null_val = iconst(b, TAG_NULL);
    sres(b, ctx, null_val);
    b.ins().return_(&[] as &[ir::Value]);
    b.seal_block(err_bb);

    // Success path: seal ok_bb before returning (matches do_cmp/math_unary
    // pattern — cranelift allows adding terminators after sealing).
    b.switch_to_block(ok_bb);
    b.seal_block(ok_bb);
}

fn emit_one(
    b: &mut FunctionBuilder, ctx: ir::Value, global_idx: usize,
    instr: &FastInstr, clif: &HashMap<usize, ir::Block>,
    program: &LoweredProgram, return_blocks: &[ir::Block],
    call_idx_to_ret: &HashMap<usize, usize>,
    handler_entries: &[(usize, ir::Block)],
    _ptr_ty: types::Type,
    helper_sig: ir::SigRef,
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
        Mod => {
            let bv = pop(b, ctx);
            let a = pop(b, ctx);
            let ia = is_int(b, a);
            let ib = is_int(b, bv);
            let bi = band(b, ia, ib);
            let ibb = b.create_block();
            let fbb = b.create_block();
            let mbb = b.create_block();
            b.append_block_param(mbb, types::I64);
            b.ins().brif(bi, ibb, &[] as &[BlockArg], fbb, &[] as &[BlockArg]);

            // Int mod: srem(a, b)
            b.switch_to_block(ibb);
            let av = eint(b, a);
            let bvv = eint(b, bv);
            let ri = srem(b, av, bvv);
            let rti = tint(b, ri);
            b.ins().jump(mbb, &[BlockArg::Value(rti)]);

            // Float mod: not yet implemented (needs fmod which Cranelift lacks)
            b.switch_to_block(fbb);
            // Check if both are actually float
            let fa = is_float(b, a);
            let fb = is_float(b, bv);
            let bf = band(b, fa, fb);
            let ok = b.create_block();
            let err = b.create_block();
            b.ins().brif(bf, ok, &[] as &[BlockArg], err, &[] as &[BlockArg]);

            b.switch_to_block(ok);
            // fmod(a, b) = a - trunc(a / b) * b
            let af = bf64(b, a);
            let bf2 = bf64(b, bv);
            let div = fdiv(b, af, bf2);
            // Compute trunc(div) via floor/ceil.
            // Band with sign_mask gives 0 or 0x8000...; select tests the LOW bit,
            // so we must icmp_ne to produce a proper bool before selecting.
            let div_bits = bi64(b, div);
            let sign_mask = iconst(b, (1u64 << 63) as i64);
            let is_neg_raw = band(b, div_bits, sign_mask);
            let zero = iconst(b, 0);
            let is_neg = icmp_ne(b, is_neg_raw, zero);
            let floor_v = b.ins().floor(div);
            let ceil_v = b.ins().ceil(div);
            let trunc_v = select(b, is_neg, ceil_v, floor_v);
            let prod = fmul(b, trunc_v, bf2);
            let rem = fsub(b, af, prod);
            let rfb = bi64(b, rem);
            b.ins().jump(mbb, &[BlockArg::Value(rfb)]);

            b.switch_to_block(err);
            serr(b, ctx);
            let nv = iconst(b, TAG_NULL);
            b.ins().jump(mbb, &[BlockArg::Value(nv)]);

            b.switch_to_block(mbb);
            push(b, ctx, b.block_params(mbb)[0]);

            b.seal_block(ibb);
            b.seal_block(fbb);
            b.seal_block(ok);
            b.seal_block(err);
            b.seal_block(mbb);
        }

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

        // ── Function calls ────────────────────────────────────────────────────

        Call => {
            let func_name = &program.symbols.strings[instr.arg as usize];
            if let Some(&(target_pc, _, _arity)) = program.symbols.functions.get(func_name) {
                let argc = instr.arg2 as usize;

                // Reverse args on stack so first arg is on top (callee pops first).
                // Same semantics as FastVM's Call handler.
                if argc > 1 {
                    let mut args: Vec<ir::Value> = Vec::with_capacity(argc);
                    for _ in 0..argc { args.push(pop(b, ctx)); }
                    for &arg in &args { push(b, ctx, arg); }
                }

                // Guard: check call-stack depth (max 64 frames) before pushing.
                // Overflow overwrites adjacent context (OFF_HELPER_FN, OFF_HANDLER_PC).
                let cst_addr = iadd_imm(b, ctx, OFF_CALL_STACK_TOP);
                let cst = load(b, cst_addr);
                let limit = iconst(b, JIT_MAX_CALL_DEPTH as i64);
                let overflow = icmp(b, IntCC::SignedGreaterThanOrEqual, cst, limit);
                let ok_bb = b.create_block();
                let overflow_bb = b.create_block();
                b.ins().brif(overflow, overflow_bb, &[] as &[BlockArg], ok_bb, &[] as &[BlockArg]);

                // Overflow path: set error, store null result, halt
                b.switch_to_block(overflow_bb);
                serr(b, ctx);
                let null_val = iconst(b, TAG_NULL);
                sres(b, ctx, null_val);
                b.ins().return_(&[] as &[ir::Value]);
                b.seal_block(overflow_bb);

                // Normal path: push frame and jump to callee
                b.switch_to_block(ok_bb);
                b.seal_block(ok_bb);
                let ret_idx = call_idx_to_ret.get(&global_idx).copied().unwrap_or(0);
                push_frame(b, ctx, ret_idx as i64);

                dec_budget(b, ctx);
                if let Some(&target_block) = clif.get(&target_pc) {
                    b.ins().jump(target_block, &[] as &[BlockArg]);
                    return true;
                }
            }
            // Fallback: function not resolved
            let cv = iconst(b, TAG_NULL);
            push(b, ctx, cv);
        }

        Return => {
            let val = pop(b, ctx);
            // Check if call_stack is empty (top-level return or handler after unwind).
            // When Throw unwinds the call stack via OP_THROW, call_stack_top can
            // become 0, so Return must behave like Halt: store result and exit.
            let cst_addr = iadd_imm(b, ctx, OFF_CALL_STACK_TOP);
            let cst = load(b, cst_addr);
            let zero = iconst(b, 0);
            let is_top = icmp_eq(b, cst, zero);

            let top_bb = b.create_block();
            let ret_bb = b.create_block();
            b.ins().brif(is_top, top_bb, &[] as &[BlockArg], ret_bb, &[] as &[BlockArg]);

            // Top-level return (no caller) — behave like Halt
            b.switch_to_block(top_bb);
            sres(b, ctx, val);
            b.ins().return_(&[] as &[ir::Value]);

            // Normal return — pop frame and dispatch to caller
            b.switch_to_block(ret_bb);
            let ret_idx = pop_frame(b, ctx);
            push(b, ctx, val);
            emit_return_dispatch(b, ret_idx, return_blocks);

            b.seal_block(top_bb);
            b.seal_block(ret_bb);
            return true;
        }

        Halt => { let val = pop(b, ctx); sres(b, ctx, val); b.ins().return_(&[] as &[ir::Value]); return true; }
        Nop => {}

        LoadLocal => { let v = lload(b, ctx, instr.arg as usize); push(b, ctx, v); }
        StoreLocal => { let v = pop(b, ctx); lstore(b, ctx, instr.arg as usize, v); }

        // ── Stack manipulation ─────────────────────────────────────────────
        Rot => {
            // [a, b, c] -> [b, c, a]
            let a = peek(b, ctx, 2);
            let bv = peek(b, ctx, 1);
            let c = peek(b, ctx, 0);
            poke(b, ctx, 2, bv);
            poke(b, ctx, 1, c);
            poke(b, ctx, 0, a);
        }
        Pick => {
            let n = instr.arg as u32;
            let v = peek(b, ctx, n);
            push(b, ctx, v);
        }

        // ── Bitwise ────────────────────────────────────────────────────────
        BitAnd => { bitwise_bin(b, ctx, |bb, x, y| band(bb, x, y)); }
        BitOr  => { bitwise_bin(b, ctx, |bb, x, y| bor(bb, x, y)); }
        BitXor => { bitwise_bin(b, ctx, |bb, x, y| bxor(bb, x, y)); }
        BitNot => { let v = pop(b, ctx); let ev = eint(b, v); let r = bnot(b, ev); let tr = tint(b, r); push(b, ctx, tr); }
        Shl    => { bitwise_bin(b, ctx, |bb, x, y| ishl(bb, x, y)); }
        Shr    => { bitwise_bin(b, ctx, |bb, x, y| sshr(bb, x, y)); }

        // ── Loop control ───────────────────────────────────────────────────
        Break | Continue => {
            if let Some(&t) = clif.get(&(instr.arg as usize)) {
                dec_budget(b, ctx);
                b.ins().jump(t, &[] as &[BlockArg]);
                return true;
            }
        }

        // ── Math ───────────────────────────────────────────────────────────
        MathSqrt  => math_unary(b, ctx, MathOp::Sqrt),
        MathAbs   => math_unary(b, ctx, MathOp::Abs),
        MathRound => math_unary(b, ctx, MathOp::Round),
        MathFloor => math_unary(b, ctx, MathOp::Floor),
        MathCeil  => math_unary(b, ctx, MathOp::Ceil),
        MathPow   => { let cv = iconst(b, TAG_NULL); push(b, ctx, cv); }, // TODO: runtime pow(f64, f64) helper

        // ── Arena-dependent ops (Phase 3b: runtime helpers) ─────────────────
        PushStr => emit_helper_call_checked(b, ctx, OP_PUSH_STR, instr.arg as i64, _ptr_ty, helper_sig),

        Roll => {
            let n = instr.arg as u32;
            let len_a = iadd_imm(b, ctx, OFF_STACK_TOP);
            let top = load(b, len_a);
            let idx = iadd_imm(b, top, -(n as i64 + 1));
            let base = iadd_imm(b, ctx, OFF_STACK);
            let off = imul_imm(b, idx, 8);
            let addr = iadd(b, base, off);
            let v = load(b, addr);
            push(b, ctx, v);
        }

        MakeList => emit_helper_call_checked(b, ctx, OP_MAKE_LIST, instr.arg as i64, _ptr_ty, helper_sig),
        MakeMap  => emit_helper_call_checked(b, ctx, OP_MAKE_MAP, instr.arg as i64, _ptr_ty, helper_sig),
        Index    => emit_helper_call_checked(b, ctx, OP_INDEX, 0, _ptr_ty, helper_sig),
        Len      => emit_helper_call_checked(b, ctx, OP_LEN, 0, _ptr_ty, helper_sig),
        TypeOf   => emit_helper_call_checked(b, ctx, OP_TYPEOF, 0, _ptr_ty, helper_sig),
        NewArray => emit_helper_call_checked(b, ctx, OP_NEW_ARRAY, instr.arg as i64, _ptr_ty, helper_sig),
        ArrayPush => emit_helper_call_checked(b, ctx, OP_ARRAY_PUSH, 0, _ptr_ty, helper_sig),
        ArrayPop  => emit_helper_call_checked(b, ctx, OP_ARRAY_POP, 0, _ptr_ty, helper_sig),
        ArrSet    => emit_helper_call_checked(b, ctx, OP_ARR_SET, 0, _ptr_ty, helper_sig),

        StrContains   => emit_helper_call_checked(b, ctx, OP_STR_CONTAINS, 0, _ptr_ty, helper_sig),
        StrStartsWith => emit_helper_call_checked(b, ctx, OP_STR_STARTS_WITH, 0, _ptr_ty, helper_sig),
        StrEndsWith   => emit_helper_call_checked(b, ctx, OP_STR_ENDS_WITH, 0, _ptr_ty, helper_sig),
        StrToUpper    => emit_helper_call_checked(b, ctx, OP_STR_TO_UPPER, 0, _ptr_ty, helper_sig),
        StrToLower    => emit_helper_call_checked(b, ctx, OP_STR_TO_LOWER, 0, _ptr_ty, helper_sig),
        StrTrim       => emit_helper_call_checked(b, ctx, OP_STR_TRIM, 0, _ptr_ty, helper_sig),
        StrSplit      => emit_helper_call_checked(b, ctx, OP_STR_SPLIT, 0, _ptr_ty, helper_sig),
        StrReplace    => emit_helper_call_checked(b, ctx, OP_STR_REPLACE, 0, _ptr_ty, helper_sig),
        StrJoin       => emit_helper_call_checked(b, ctx, OP_STR_JOIN, 0, _ptr_ty, helper_sig),

        Cast    => emit_helper_call_checked(b, ctx, OP_CAST, instr.arg as i64, _ptr_ty, helper_sig),
        NewTuple  => emit_helper_call_checked(b, ctx, OP_NEW_TUPLE, instr.arg as i64, _ptr_ty, helper_sig),
        NewList   => emit_helper_call_checked(b, ctx, OP_NEW_LIST, instr.arg as i64, _ptr_ty, helper_sig),
        NewVector => emit_helper_call_checked(b, ctx, OP_NEW_VECTOR, instr.arg as i64, _ptr_ty, helper_sig),
        NewSet    => emit_helper_call_checked(b, ctx, OP_NEW_SET, instr.arg as i64, _ptr_ty, helper_sig),
        MakeRange => emit_helper_call_checked(b, ctx, OP_MAKE_RANGE, 0, _ptr_ty, helper_sig),

        TuplePush  => emit_helper_call_checked(b, ctx, OP_TUPLE_PUSH, 0, _ptr_ty, helper_sig),
        ListPush   => emit_helper_call_checked(b, ctx, OP_LIST_PUSH, 0, _ptr_ty, helper_sig),
        VectorPush => emit_helper_call_checked(b, ctx, OP_VECTOR_PUSH, 0, _ptr_ty, helper_sig),
        SetPush    => emit_helper_call_checked(b, ctx, OP_SET_PUSH, 0, _ptr_ty, helper_sig),
        GetField   => emit_helper_call_checked(b, ctx, OP_GET_FIELD, instr.arg as i64, _ptr_ty, helper_sig),
        SetField   => emit_helper_call_checked(b, ctx, OP_SET_FIELD, instr.arg as i64, _ptr_ty, helper_sig),
        NewObj     => emit_helper_call_checked(b, ctx, OP_NEW_OBJ, 0, _ptr_ty, helper_sig),
        NewStruct  => emit_helper_call_checked(b, ctx, OP_NEW_STRUCT, instr.arg as i64, _ptr_ty, helper_sig),
        StrSim     => emit_helper_call_checked(b, ctx, OP_STR_SIM, 0, _ptr_ty, helper_sig),

        EnterTry  => emit_helper_call(b, ctx, OP_ENTER_TRY, instr.arg as i64, _ptr_ty, helper_sig),
        ExitTry   => emit_helper_call(b, ctx, OP_EXIT_TRY, 0, _ptr_ty, helper_sig),

        Throw => {
            // 1. Call runtime helper — pops error, walks handler stack, sets ctx.error
            //    and ctx.handler_pc (if handler found).
            emit_helper_call(b, ctx, OP_THROW, 0, _ptr_ty, helper_sig);

            // 2. Load ctx.error WITHOUT trusted flag to prevent Cranelift from
            //    reordering this load before the call_indirect.
            //    (trusted() loads can be hoisted past calls; we MUST see the
            //     value written by the helper.)
            //
            //    NOTE: error is i32 at OFF_ERROR, but we load I64 (8 bytes).
            //    The adjacent 4 bytes at OFF_ERROR+4 is throw_consumed_handler.
            //    Mask the low 32 bits to isolate the error value (CRUSH-17 #6).
            let err_addr = iadd_imm(b, ctx, OFF_ERROR);
            let err_val_raw = b.ins().load(types::I64, MemFlagsData::new(), err_addr, 0);
            let err_mask = iconst(b, 0xFFFF_FFFFi64);
            let err_val = band(b, err_val_raw, err_mask);
            let found = iconst(b, 2);
            let handler_found = icmp_eq(b, err_val, found);

            let dispatch_bb = b.create_block();
            let return_bb = b.create_block();
            b.ins().brif(handler_found, dispatch_bb, &[] as &[BlockArg], return_bb, &[] as &[BlockArg]);

            // 3. Handler found — clear ONLY error flag (i32, 4 bytes),
            //    preserving throw_consumed_handler (offset OFF_ERROR+4) for ExitTry.
            b.switch_to_block(dispatch_bb);
            let err_addr2 = iadd_imm(b, ctx, OFF_ERROR);
            let zero_i32 = b.ins().iconst(types::I32, 0);
            b.ins().store(MemFlagsData::trusted(), zero_i32, err_addr2, 0);
            let hp_addr = iadd_imm(b, ctx, OFF_HANDLER_PC);
            let hp_val = b.ins().load(types::I64, MemFlagsData::new(), hp_addr, 0);
            emit_handler_dispatch(b, hp_val, handler_entries);

            // 4. No handler — return null
            b.switch_to_block(return_bb);
            let null_val = iconst(b, TAG_NULL);
            sres(b, ctx, null_val);
            b.ins().return_(&[] as &[ir::Value]);

            // Seal the throw blocks
            b.seal_block(dispatch_bb);
            b.seal_block(return_bb);

            return true;
        }

        CapCall => {
            // Push argc onto JIT stack as a properly tagged int, then call helper with cap_idx
            let argc_val = (instr.arg2 as u64) & 0xFFFF;
            let cv = iconst(b, (TAG_INT as u64 | argc_val) as i64);
            push(b, ctx, cv);
            emit_helper_call_checked(b, ctx, OP_CAP_CALL, instr.arg as i64, _ptr_ty, helper_sig);
        }

        // ── M2 Phase 5 Tier 2: Host-request yield ops (trampoline escape) ──
        // These opcodes save the resume PC, signal the host via host_request_tag,
        // and return. The host processes the request and calls resume() to continue.
        CallHost => {
            emit_host_yield(b, ctx, global_idx + 1, HOST_REQ_CALL_HOST);
            return true;
        }
        ExecLang => {
            emit_host_yield(b, ctx, global_idx + 1, HOST_REQ_EXEC_LANG);
            return true;
        }
        Spawn => {
            emit_host_yield(b, ctx, global_idx + 1, HOST_REQ_SPAWN);
            return true;
        }
        Gc => {
            emit_host_yield(b, ctx, global_idx + 1, HOST_REQ_GC);
            return true;
        }
        ImportVar => {
            emit_host_yield(b, ctx, global_idx + 1, HOST_REQ_IMPORT_VAR);
            return true;
        }
        Await => {
            emit_host_yield(b, ctx, global_idx + 1, HOST_REQ_AWAIT);
            return true;
        }

        // Remaining unimplemented ops fall through to push null.
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

// ── Bitwise helpers ────────────────────────────────────────────────────

/// Pop two values, extract ints, apply binary op, re-tag, push.
fn bitwise_bin(b: &mut FunctionBuilder, ctx: ir::Value, op: impl Fn(&mut FunctionBuilder, ir::Value, ir::Value) -> ir::Value) {
    let bv = pop(b, ctx); let bv = eint(b, bv);
    let av = pop(b, ctx); let av = eint(b, av);
    let r = op(b, av, bv);
    let tagged = tint(b, r);
    push(b, ctx, tagged);
}

// ── Math helpers ───────────────────────────────────────────────────────────

enum MathOp { Sqrt, Abs, Round, Floor, Ceil }

/// Apply a unary math function: pop val, dispatch int/float, push float result.
fn math_unary(b: &mut FunctionBuilder, ctx: ir::Value, op: MathOp) {
    let val = pop(b, ctx);
    let is_i = is_int(b, val);
    let ibb = b.create_block();
    let fbb = b.create_block();
    let mbb = b.create_block();
    b.append_block_param(mbb, types::I64);
    b.ins().brif(is_i, ibb, &[] as &[BlockArg], fbb, &[] as &[BlockArg]);

    b.switch_to_block(ibb);
    let ev = eint(b, val);
    let as_f64 = b.ins().fcvt_from_sint(types::F64, ev);
    let rf = math_apply_f64(b, as_f64, &op);
    let rfb = bi64(b, rf);
    b.ins().jump(mbb, &[BlockArg::Value(rfb)]);

    b.switch_to_block(fbb);
    let fv = bf64(b, val);
    let rf2 = math_apply_f64(b, fv, &op);
    let rfb2 = bi64(b, rf2);
    b.ins().jump(mbb, &[BlockArg::Value(rfb2)]);

    b.switch_to_block(mbb);
    push(b, ctx, b.block_params(mbb)[0]);

    b.seal_block(ibb);
    b.seal_block(fbb);
    b.seal_block(mbb);
}

/// Apply a binary math function: pop two, dispatch int/float, push float result.
fn math_binary(b: &mut FunctionBuilder, ctx: ir::Value, op: MathOp) {
    let exp = pop(b, ctx);
    let base = pop(b, ctx);
    let ia = is_int(b, base);
    let ib = is_int(b, exp);
    let bi = band(b, ia, ib);
    let ibb = b.create_block();
    let fbb = b.create_block();
    let mbb = b.create_block();
    b.append_block_param(mbb, types::I64);
    b.ins().brif(bi, ibb, &[] as &[BlockArg], fbb, &[] as &[BlockArg]);

    b.switch_to_block(ibb);
    let be = eint(b, base);
    let ee = eint(b, exp);
    let bf = b.ins().fcvt_from_sint(types::F64, be);
    let ef2 = b.ins().fcvt_from_sint(types::F64, ee);
    let rf = math_apply_f64_bin(b, bf, ef2, &op);
    let rfb = bi64(b, rf);
    b.ins().jump(mbb, &[BlockArg::Value(rfb)]);

    b.switch_to_block(fbb);
    // Promote mixed int<->float to float
    let raw_b = bf64(b, base);
    let test_b = is_float(b, base);
    let iv_b = eint(b, base);
    let conv_b = b.ins().fcvt_from_sint(types::F64, iv_b);
    let bf2 = select(b, test_b, raw_b, conv_b);

    let raw_e = bf64(b, exp);
    let test_e = is_float(b, exp);
    let iv_e = eint(b, exp);
    let conv_e = b.ins().fcvt_from_sint(types::F64, iv_e);
    let ef2 = select(b, test_e, raw_e, conv_e);

    let rf2 = math_apply_f64_bin(b, bf2, ef2, &op);
    let rfb2 = bi64(b, rf2);
    b.ins().jump(mbb, &[BlockArg::Value(rfb2)]);

    b.switch_to_block(mbb);
    push(b, ctx, b.block_params(mbb)[0]);

    b.seal_block(ibb);
    b.seal_block(fbb);
    b.seal_block(mbb);
}

/// Apply a f64 unary math operation via CLIF instructions.
fn math_apply_f64(b: &mut FunctionBuilder, v: ir::Value, op: &MathOp) -> ir::Value {
    match op {
        MathOp::Sqrt  => b.ins().sqrt(v),
        MathOp::Abs   => b.ins().fabs(v),
        MathOp::Round => b.ins().nearest(v),
        MathOp::Floor => b.ins().floor(v),
        MathOp::Ceil  => b.ins().ceil(v),
    }
}

/// Apply a f64 binary math operation (placeholder — only Pow uses this, but it's
/// handled by pushing null in emit_one for now; kept for future runtime helper use).
fn math_apply_f64_bin(b: &mut FunctionBuilder, a: ir::Value, e: ir::Value, _op: &MathOp) -> ir::Value {
    // Only called from math_binary for Pow, which currently pushes null instead.
    // If a second binary math op is added, implement it here.
    b.ins().fadd(a, e)
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
