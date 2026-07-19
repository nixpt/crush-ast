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

use crate::runtime::{JitContext, jit_runtime_helper, OP_PUSH_STR, OP_MAKE_LIST, OP_MAKE_MAP, OP_INDEX, OP_LEN, OP_TYPEOF, OP_NEW_ARRAY, OP_ARRAY_PUSH, OP_ARRAY_POP, OP_ARR_SET, OP_STR_CONTAINS, OP_STR_STARTS_WITH, OP_STR_ENDS_WITH, OP_STR_TO_UPPER, OP_STR_TO_LOWER, OP_STR_TRIM, OP_STR_SPLIT, OP_STR_REPLACE, OP_STR_JOIN, OP_CAST, OP_NEW_TUPLE, OP_NEW_LIST, OP_NEW_VECTOR, OP_NEW_SET, OP_MAKE_RANGE, OP_CAP_CALL, OP_TUPLE_PUSH, OP_LIST_PUSH, OP_VECTOR_PUSH, OP_SET_PUSH, OP_GET_FIELD, OP_SET_FIELD, OP_NEW_OBJ, OP_NEW_STRUCT, OP_STR_SIM, OP_ENTER_TRY, OP_EXIT_TRY, OP_THROW};

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

    if let Some(&first) = map.get(&0) {
        dec_budget(bld, ctx);
        bld.ins().jump(first, &[] as &[BlockArg]);
    } else {
        let n = iconst(bld, TAG_NULL);
        sres(bld, ctx, n);
        bld.ins().return_(&[] as &[ir::Value]);
    }
    bld.seal_block(entry);

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

    // Collect handler block offsets so we can defer sealing. Handler blocks
    // may receive additional predecessors from Throw dispatch (which happens
    // in a later block), so they must be sealed AFTER all main blocks are
    // processed.
    let handler_offs: BTreeSet<usize> = handler_entries.iter().map(|(pc, _)| *pc).collect();

    // Process main blocks.
    for &(off, ref instrs) in blocks {
        let block = map[&off];
        bld.switch_to_block(block);
        let mut term = false;
        for (i, instr) in instrs.iter().enumerate() {
            term = emit_one(bld, ctx, off + i, instr, &map, program, &ret_blocks, &call_idx_to_ret, &handler_entries, ptr_ty, helper_sig);
        }
        if !term {
            // Non-terminator block — jump to the next sequential block if there is one
            if let Some(&next_block) = next_off_map.get(&off) {
                dec_budget(bld, ctx);
                bld.ins().jump(next_block, &[] as &[BlockArg]);
            } else {
                // Last block — return null
                let n = iconst(bld, TAG_NULL);
                sres(bld, ctx, n);
                bld.ins().return_(&[] as &[ir::Value]);
            }
        }
        // Defer sealing for handler blocks — they may get extra predecessors
        // from Throw dispatch in later blocks.
        if !handler_offs.contains(&off) {
            bld.seal_block(block);
        }
    }

    // Seal deferred handler blocks now that all blocks have been processed
    // and any Throw-dispatch predecessors have been registered.
    for &off in &handler_offs {
        if let Some(&block) = map.get(&off) {
            bld.seal_block(block);
        }
    }

    // Seal return blocks now that all predecessors are established.
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

/// Dispatch to the handler block by comparing runtime `hp_val` to each known
/// handler PC (brif cascade — avoids complex JumpTableData API).
///
/// Intermediate blocks are NOT sealed inside this function — the caller
/// must seal `dispatch_bb` afterwards (which is already done by emit_one).
fn emit_handler_dispatch(b: &mut FunctionBuilder, hp_val: ir::Value, entries: &[(usize, ir::Block)]) {
    if entries.is_empty() {
        // Safety net — should not happen (handler_found implies >=1 entry).
        // The function returns `()`, so return with an empty argument list.
        b.ins().return_(&[] as &[ir::Value]);
        return;
    }
    if entries.len() == 1 {
        b.ins().jump(entries[0].1, &[] as &[BlockArg]);
        return;
    }
    // Pre-create all intermediate blocks so they are available as brif targets.
    let chain: Vec<ir::Block> = (0..entries.len() - 1).map(|_| b.create_block()).collect();

    // Build chained brif cascade. Each chain[i] contains:
    //   brif(hp == entries[i].0, entries[i].1, chain[i+1])
    // The last chain block jumps unconditionally to entries[N-1].
    for (i, (pc, blk)) in entries[..entries.len() - 1].iter().enumerate() {
        if i > 0 {
            b.switch_to_block(chain[i - 1]);
        }
        let next_bb = chain[i];
        let const_pc = iconst(b, *pc as i64);
        let cmp = icmp_eq(b, hp_val, const_pc);
        b.ins().brif(cmp, *blk, &[] as &[BlockArg], next_bb, &[] as &[BlockArg]);
    }

    // Final chain block: unconditional jump to last handler entry
    b.switch_to_block(*chain.last().unwrap());
    b.ins().jump(entries[entries.len() - 1].1, &[] as &[BlockArg]);

    // Seal all chain blocks now that they have terminators
    for &cb in &chain {
        b.seal_block(cb);
    }
}

/// Dispatch to one of `targets` by comparing `idx` to each target index
/// (brif cascade — avoids complex JumpTableData API).
fn emit_return_dispatch(b: &mut FunctionBuilder, idx: ir::Value, targets: &[ir::Block]) {
    if targets.is_empty() {
        return;
    }
    if targets.len() == 1 {
        b.ins().jump(targets[0], &[] as &[BlockArg]);
        return;
    }
    // Chain: brif(idx == 0, targets[0], fallthrough) → brif(idx == 1, targets[1], fallthrough) → ... → jump(last)
    let mut prev_block: Option<ir::Block> = None;
    for (i, &tgt) in targets[..targets.len() - 1].iter().enumerate() {
        let next_bb = b.create_block();
        if let Some(pb) = prev_block {
            b.switch_to_block(pb);
        }
        let const_idx = iconst(b, i as i64);
        let cmp = icmp_eq(b, idx, const_idx);
        b.ins().brif(cmp, tgt, &[] as &[BlockArg], next_bb, &[] as &[BlockArg]);
        b.seal_block(next_bb);
        prev_block = Some(next_bb);
    }
    // Last target (default)
    if let Some(pb) = prev_block {
        b.switch_to_block(pb);
    }
    b.ins().jump(targets[targets.len() - 1], &[] as &[BlockArg]);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Compile one instruction
// ═══════════════════════════════════════════════════════════════════════════════

/// Emit a call to the runtime helper function via `call_indirect`.
fn emit_helper_call(b: &mut FunctionBuilder, ctx: ir::Value, opcode: i64, arg: i64, ptr_ty: types::Type, helper_sig: ir::SigRef) {
    let off_addr = iadd_imm(b, ctx, OFF_HELPER_FN);
    let helper_addr = load(b, off_addr);
    let callee = b.ins().bitcast(ptr_ty, MemFlagsData::new(), helper_addr);
    let op_val = iconst(b, opcode);
    let arg_val = iconst(b, arg);
    b.ins().call_indirect(helper_sig, callee, &[ctx, op_val, arg_val]);
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
            // Compute trunc(div) via floor/ceil
            let div_bits = bi64(b, div);
            let sign_mask = iconst(b, (1u64 << 63) as i64);
            let is_neg = band(b, div_bits, sign_mask);
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

                // Push return frame onto call stack
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
        PushStr => emit_helper_call(b, ctx, OP_PUSH_STR, instr.arg as i64, _ptr_ty, helper_sig),

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

        MakeList => emit_helper_call(b, ctx, OP_MAKE_LIST, instr.arg as i64, _ptr_ty, helper_sig),
        MakeMap  => emit_helper_call(b, ctx, OP_MAKE_MAP, instr.arg as i64, _ptr_ty, helper_sig),
        Index    => emit_helper_call(b, ctx, OP_INDEX, 0, _ptr_ty, helper_sig),
        Len      => emit_helper_call(b, ctx, OP_LEN, 0, _ptr_ty, helper_sig),
        TypeOf   => emit_helper_call(b, ctx, OP_TYPEOF, 0, _ptr_ty, helper_sig),
        NewArray => emit_helper_call(b, ctx, OP_NEW_ARRAY, instr.arg as i64, _ptr_ty, helper_sig),
        ArrayPush => emit_helper_call(b, ctx, OP_ARRAY_PUSH, 0, _ptr_ty, helper_sig),
        ArrayPop  => emit_helper_call(b, ctx, OP_ARRAY_POP, 0, _ptr_ty, helper_sig),
        ArrSet    => emit_helper_call(b, ctx, OP_ARR_SET, 0, _ptr_ty, helper_sig),

        StrContains   => emit_helper_call(b, ctx, OP_STR_CONTAINS, 0, _ptr_ty, helper_sig),
        StrStartsWith => emit_helper_call(b, ctx, OP_STR_STARTS_WITH, 0, _ptr_ty, helper_sig),
        StrEndsWith   => emit_helper_call(b, ctx, OP_STR_ENDS_WITH, 0, _ptr_ty, helper_sig),
        StrToUpper    => emit_helper_call(b, ctx, OP_STR_TO_UPPER, 0, _ptr_ty, helper_sig),
        StrToLower    => emit_helper_call(b, ctx, OP_STR_TO_LOWER, 0, _ptr_ty, helper_sig),
        StrTrim       => emit_helper_call(b, ctx, OP_STR_TRIM, 0, _ptr_ty, helper_sig),
        StrSplit      => emit_helper_call(b, ctx, OP_STR_SPLIT, 0, _ptr_ty, helper_sig),
        StrReplace    => emit_helper_call(b, ctx, OP_STR_REPLACE, 0, _ptr_ty, helper_sig),
        StrJoin       => emit_helper_call(b, ctx, OP_STR_JOIN, 0, _ptr_ty, helper_sig),

        Cast    => emit_helper_call(b, ctx, OP_CAST, instr.arg as i64, _ptr_ty, helper_sig),
        NewTuple  => emit_helper_call(b, ctx, OP_NEW_TUPLE, instr.arg as i64, _ptr_ty, helper_sig),
        NewList   => emit_helper_call(b, ctx, OP_NEW_LIST, instr.arg as i64, _ptr_ty, helper_sig),
        NewVector => emit_helper_call(b, ctx, OP_NEW_VECTOR, instr.arg as i64, _ptr_ty, helper_sig),
        NewSet    => emit_helper_call(b, ctx, OP_NEW_SET, instr.arg as i64, _ptr_ty, helper_sig),
        MakeRange => emit_helper_call(b, ctx, OP_MAKE_RANGE, 0, _ptr_ty, helper_sig),

        TuplePush  => emit_helper_call(b, ctx, OP_TUPLE_PUSH, 0, _ptr_ty, helper_sig),
        ListPush   => emit_helper_call(b, ctx, OP_LIST_PUSH, 0, _ptr_ty, helper_sig),
        VectorPush => emit_helper_call(b, ctx, OP_VECTOR_PUSH, 0, _ptr_ty, helper_sig),
        SetPush    => emit_helper_call(b, ctx, OP_SET_PUSH, 0, _ptr_ty, helper_sig),
        GetField   => emit_helper_call(b, ctx, OP_GET_FIELD, instr.arg as i64, _ptr_ty, helper_sig),
        SetField   => emit_helper_call(b, ctx, OP_SET_FIELD, instr.arg as i64, _ptr_ty, helper_sig),
        NewObj     => emit_helper_call(b, ctx, OP_NEW_OBJ, 0, _ptr_ty, helper_sig),
        NewStruct  => emit_helper_call(b, ctx, OP_NEW_STRUCT, instr.arg as i64, _ptr_ty, helper_sig),
        StrSim     => emit_helper_call(b, ctx, OP_STR_SIM, 0, _ptr_ty, helper_sig),

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
            let err_addr = iadd_imm(b, ctx, OFF_ERROR);
            let err_val = b.ins().load(types::I64, MemFlagsData::new(), err_addr, 0);
            let found = iconst(b, 2);
            let handler_found = icmp_eq(b, err_val, found);

            let dispatch_bb = b.create_block();
            let return_bb = b.create_block();
            b.ins().brif(handler_found, dispatch_bb, &[] as &[BlockArg], return_bb, &[] as &[BlockArg]);

            // 3. Handler found — clear error flag, read handler_pc, dispatch to handler block
            b.switch_to_block(dispatch_bb);
            let err_addr2 = iadd_imm(b, ctx, OFF_ERROR);
            let zero_val = iconst(b, 0);
            store(b, err_addr2, zero_val);
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
            emit_helper_call(b, ctx, OP_CAP_CALL, instr.arg as i64, _ptr_ty, helper_sig);
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

/// Comparison family (EQ/NE/LT/LE/GT/GE) share this lowering. Both operands
/// int-tagged -> integer compare. Otherwise -> float compare, with each
/// operand independently promoted: an int-tagged operand is converted via
/// `fcvt_from_sint` (matching `math_binary`'s mixed-type promotion), a
/// float-tagged operand is reinterpreted via `bf64` bitcast (it already IS
/// an f64 bit pattern), and any other tag (bool/null/ref) falls through to
/// the bitcast path unchanged -- preserving the pre-existing "never equal
/// to a real number" behavior for non-numeric operands (their NaN-boxed
/// bits bitcast to a NaN, and IEEE-754 NaN never compares equal). This is
/// what keeps `2 == 2.0` numerically true while `true == 1` and `2 == "2"`
/// stay unequal: only the (Int, Float) / (Float, Int) pairing is coerced,
/// not (Bool, Int) or anything else. Precision note: ints with |i| > 2^53
/// lose exactness under `i as f64`, same caveat as the interpreter tiers.
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
    // Promote per-operand: int-tagged -> real float conversion; anything else
    // (including a genuine float operand) -> bitcast reinterpretation.
    let raw_a = bf64(b, a);
    let conv_a = { let iv = eint(b, a); b.ins().fcvt_from_sint(types::F64, iv) };
    let af = select(b, ia, conv_a, raw_a);

    let raw_b = bf64(b, bv);
    let conv_b = { let iv = eint(b, bv); b.ins().fcvt_from_sint(types::F64, iv) };
    let bf2 = select(b, ib, conv_b, raw_b);

    let cmp = fcmp(b, fcc, af, bf2);
    let r2 = tbool(b, cmp);
    b.ins().jump(mb2, &[BlockArg::Value(r2)]);

    b.switch_to_block(mb2);
    push(b, ctx, b.block_params(mb2)[0]);

    b.seal_block(ibb);
    b.seal_block(fbb);
    b.seal_block(mb2);
}
