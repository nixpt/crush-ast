//! Hand-built CASM kernels for the crush→PTX (Way-3) spike.
//!
//! The frontend has no `ptx_*` intrinsics yet, so a real GPU kernel is assembled as a
//! CASM [`Program`] directly (the tracing-builder model from the design doc). The one
//! kernel here — a Q6_K dequant→GEMV — is the load-bearing quant-decode spike: it
//! exercises Tiers 0-4 (SIMT ids, typed sub-word loads, 6-bit unpack, fma accumulate,
//! warp-shuffle reduction) and has a byte-exact oracle (zorro `dequantize_q6_k`).
//!
//! ## Algorithm (matches zorro's ironsand `gemv_q6k_warp`, the Way-1 twin)
//! One warp per output row; the 32 lanes stride the row's columns, each lane dequantises
//! its Q6_K elements, multiplies by the (f32) activation and accumulates in f32, then a
//! butterfly shuffle sums the warp and lane 0 writes `y[row]`. Pure W6A32 dequant·dot —
//! no activation quantisation — so the oracle is simply `y[r] = Σ_c dequant(W)[r,c]·x[c]`.
//!
//! ## Q6_K super-block (210 bytes / 256 elements), per element `e` in `[0,256)`
//! ```text
//!   i    = e & 31          half = e >> 7        gg = (e >> 5) & 3
//!   ql byte  = ql[ half*64 + (gg&1)*32 + i ]        (ql region: bytes [0,128))
//!   nibble   = ((e>>6)&1) ? (ql>>4) : (ql&15)
//!   qh byte  = qh[ 128 + half*32 + i ]              (qh region: bytes [128,192))
//!   qh_val   = (qh >> (gg<<1)) & 3
//!   q6       = (nibble | (qh_val<<4)) - 32          (signed, [-32,31])
//!   scale    = (i8) scales[ 192 + (e>>4) ]          (16 int8 scales at [192,208))
//!   d        = (f16)  bytes [208,210)
//!   value    = d · scale · q6
//! ```
//!
//! ABI (all params passed as `.u64`; scalars are u64-valued):
//! `(p_w, p_x, p_y, p_cols, p_rows)` — device pointers + column/row counts.
//! Launch: `grid = ceil(rows*32 / 256)`, `block = 256` (8 warps/block, warp = row).

use casm::{Function, Instruction, Program};
use serde_json::json;

/// A tiny append-only body builder that tracks instruction indices so forward branch
/// targets can be patched once their landing site is known.
struct Body {
    instrs: Vec<Instruction>,
}

impl Body {
    fn new() -> Self {
        Body { instrs: Vec::new() }
    }
    /// Push an instruction, returning its index (for label/branch patching).
    fn op(&mut self, op: &str, args: serde_json::Value) -> usize {
        let i = self.instrs.len();
        self.instrs.push(Instruction { op: op.into(), lang: None, meta: None, args });
        i
    }
    fn plain(&mut self, op: &str) {
        self.op(op, json!({}));
    }
    fn load(&mut self, name: &str) {
        self.op("load", json!({ "name": name }));
    }
    fn store(&mut self, name: &str) {
        self.op("store", json!({ "name": name }));
    }
    fn pi(&mut self, v: i64) {
        self.op("push_int", json!({ "value": v }));
    }
    fn cvt(&mut self, ty: &str) {
        self.op("cvt", json!({ "type": ty }));
    }
    fn ldg(&mut self, ty: &str) {
        self.op("ptx_ld_global", json!({ "type": ty }));
    }
    fn here(&self) -> usize {
        self.instrs.len()
    }
    /// Overwrite a previously-emitted branch's `target` (forward-patch).
    fn patch_target(&mut self, idx: usize, target: usize) {
        self.instrs[idx].args = json!({ "target": target });
    }
}

/// Build the Q6_K dequant→GEMV kernel as a CASM [`Program`] (entry `gemv_q6k_crush`).
pub fn q6k_gemv_program() -> Program {
    let mut b = Body::new();

    // ── SIMT ids → s64 ───────────────────────────────────────────────────────────
    b.plain("ptx_block_dim_x"); b.cvt("s64"); b.store("ntid");
    b.plain("ptx_block_idx_x"); b.cvt("s64"); b.store("ctaid");
    b.plain("ptx_thread_idx_x"); b.cvt("s64"); b.store("tid");
    // gtid = ctaid*ntid + tid ; row = gtid>>5 ; lane = gtid&31
    b.load("ctaid"); b.load("ntid"); b.plain("mul"); b.load("tid"); b.plain("add"); b.store("gtid");
    b.load("gtid"); b.pi(5); b.plain("shr"); b.store("row");
    b.load("gtid"); b.pi(31); b.plain("and"); b.store("lane");

    // ── params → s64 ─────────────────────────────────────────────────────────────
    b.load("p_w"); b.cvt("s64"); b.store("wbase");
    b.load("p_x"); b.cvt("s64"); b.store("xbase");
    b.load("p_y"); b.cvt("s64"); b.store("ybase");
    b.load("p_cols"); b.cvt("s64"); b.store("cols");
    b.load("p_rows"); b.cvt("s64"); b.store("rows");
    b.load("cols"); b.pi(8); b.plain("shr"); b.store("bpr");   // super-blocks per row = cols/256
    b.load("row"); b.load("bpr"); b.plain("mul"); b.store("rb"); // super-blocks before this row

    // ── bounds: if row >= rows -> ret ────────────────────────────────────────────
    b.load("row"); b.load("rows"); b.plain("ge");
    let j_bounds = b.op("jmp_if", json!({ "target": 0 })); // patched -> RET

    // ── acc = 0.0f (f32) ; c = lane ──────────────────────────────────────────────
    b.op("push_float", json!({ "value": 0.0 })); b.cvt("f32"); b.store("acc");
    b.load("lane"); b.store("c");

    // ── loop over the lane's strided columns ─────────────────────────────────────
    let loop_top = b.here();
    b.load("c"); b.load("cols"); b.plain("ge");
    let j_endloop = b.op("jmp_if", json!({ "target": 0 })); // patched -> ENDLOOP

    // e = c&255 ; sb = c>>8 ; wsb = wbase + (rb+sb)*210
    b.load("c"); b.pi(255); b.plain("and"); b.store("e");
    b.load("c"); b.pi(8); b.plain("shr"); b.store("sb");
    b.load("rb"); b.load("sb"); b.plain("add"); b.pi(210); b.plain("mul"); b.load("wbase"); b.plain("add"); b.store("wsb");

    // i = e&31 ; half = e>>7 ; gg = (e>>5)&3
    b.load("e"); b.pi(31); b.plain("and"); b.store("i");
    b.load("e"); b.pi(7); b.plain("shr"); b.store("half");
    b.load("e"); b.pi(5); b.plain("shr"); b.pi(3); b.plain("and"); b.store("gg");

    // ql byte: addr = wsb + half*64 + (gg&1)*32 + i
    b.load("half"); b.pi(64); b.plain("mul");
    b.load("gg"); b.pi(1); b.plain("and"); b.pi(32); b.plain("mul");
    b.plain("add"); b.store("qlbase");
    b.load("wsb"); b.load("qlbase"); b.plain("add"); b.load("i"); b.plain("add"); b.store("qladdr");
    b.load("qladdr"); b.ldg("u8"); b.cvt("s64"); b.store("qlbyte");

    // nibble = ((e>>6)&1) ? (qlbyte>>4) : (qlbyte&15)  — branchless via shift = hi*4
    b.load("e"); b.pi(6); b.plain("shr"); b.pi(1); b.plain("and"); b.store("hi");
    b.load("hi"); b.pi(2); b.plain("shl"); b.store("nibsh");
    b.load("qlbyte"); b.load("nibsh"); b.plain("shr"); b.pi(15); b.plain("and"); b.store("qlval");

    // qh byte: addr = wsb + 128 + i + half*32 ; qh_val = (qh >> (gg<<1)) & 3
    b.load("wsb"); b.pi(128); b.plain("add"); b.load("i"); b.plain("add");
    b.load("half"); b.pi(32); b.plain("mul"); b.plain("add"); b.store("qhaddr");
    b.load("qhaddr"); b.ldg("u8"); b.cvt("s64"); b.store("qhbyte");
    b.load("gg"); b.pi(1); b.plain("shl"); b.store("qhsh");
    b.load("qhbyte"); b.load("qhsh"); b.plain("shr"); b.pi(3); b.plain("and"); b.store("qhval");

    // q6 = (qlval | (qhval<<4)) - 32
    b.load("qlval"); b.load("qhval"); b.pi(4); b.plain("shl"); b.plain("or"); b.pi(32); b.plain("sub"); b.store("q6");

    // scale: sc = sign-extend-s8( u8[ wsb + 192 + (e>>4) ] )  via (b<<56)>>56
    b.load("wsb"); b.pi(192); b.plain("add"); b.load("e"); b.pi(4); b.plain("shr"); b.plain("add"); b.store("scaddr");
    b.load("scaddr"); b.ldg("u8"); b.cvt("s64"); b.store("scbyte");
    b.load("scbyte"); b.pi(56); b.plain("shl"); b.pi(56); b.plain("shr"); b.store("sc");

    // d = f16[ wsb + 208 ] (widened to f32 by the loader)
    b.load("wsb"); b.pi(208); b.plain("add"); b.store("daddr");
    b.load("daddr"); b.ldg("f16"); b.store("dval");

    // dq = dval * (f32)sc * (f32)q6
    b.load("sc"); b.cvt("f32"); b.store("scf");
    b.load("q6"); b.cvt("f32"); b.store("q6f");
    b.load("dval"); b.load("scf"); b.plain("mul"); b.load("q6f"); b.plain("mul"); b.store("dq");

    // x[c] = f32[ xbase + c*4 ] ; acc = fma(dq, x, acc)
    b.load("xbase"); b.load("c"); b.pi(2); b.plain("shl"); b.plain("add"); b.store("xaddr");
    b.load("xaddr"); b.ldg("f32"); b.store("xval");
    b.load("dq"); b.load("xval"); b.load("acc"); b.plain("fma"); b.store("acc");

    // c += 32 ; back-edge
    b.load("c"); b.pi(32); b.plain("add"); b.store("c");
    b.op("jmp", json!({ "target": loop_top }));

    // ── ENDLOOP: warp butterfly reduction over 32 lanes ──────────────────────────
    let endloop = b.here();
    b.patch_target(j_endloop, endloop);
    for off in [16i64, 8, 4, 2, 1] {
        b.load("acc");                                   // shfl var
        b.pi(off); b.cvt("u32");                          // idx (butterfly lane mask)
        b.pi(31); b.cvt("u32");                           // c-operand 0x1f (full-warp width)
        b.op("ptx_shfl_sync_bfly", json!({ "membermask": 0xffffffffu64 }));
        b.load("acc"); b.plain("add"); b.store("acc");    // acc += shuffled
    }

    // ── lane 0 writes y[row] ─────────────────────────────────────────────────────
    b.load("lane"); b.pi(0); b.plain("ne");
    let j_skip = b.op("jmp_if", json!({ "target": 0 })); // patched -> RET (skip store if lane!=0)
    b.load("ybase"); b.load("row"); b.pi(2); b.plain("shl"); b.plain("add"); b.store("yaddr");
    b.load("yaddr"); b.load("acc"); b.plain("ptx_st_global");

    // ── RET ──────────────────────────────────────────────────────────────────────
    let ret_idx = b.here();
    b.patch_target(j_bounds, ret_idx);
    b.patch_target(j_skip, ret_idx);
    b.plain("ret");

    let func = Function {
        params: vec![
            "p_w".into(),
            "p_x".into(),
            "p_y".into(),
            "p_cols".into(),
            "p_rows".into(),
        ],
        locals: vec![],
        type_hints: None,
        body: b.instrs,
    };
    let mut program = Program::default();
    program.functions.insert("gemv_q6k_crush".to_string(), func);
    program
}
