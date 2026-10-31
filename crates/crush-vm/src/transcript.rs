//! Tamper-evident CVM1 execution transcript (CRUSH-165, feature `transcript`).
//!
//! A run folds every executed instruction and every capability call into a
//! SHA-256 hash chain. Two runs of the same program that execute the same
//! instructions and get the same capability results end on the same digest;
//! any difference in what ran, or in what a capability answered, changes it.
//! The digest is something a host can sign (signing is the host's job, not
//! this module's) and a verifier can recompute from a log of the events.
//!
//! # Encoding, version 1
//!
//! All integers are big-endian. `str(s)` is `u32 len(s)` followed by the UTF-8
//! bytes. `H(x)` is SHA-256.
//!
//! ## Genesis
//!
//! ```text
//! program_hash = H( u32 len(code) || code
//!                 || u32 n_consts || str(const_0) || ... || str(const_{n-1}) )
//! h_0          = H( "crush-vm/cvm1-transcript/v1" || program_hash )
//! ```
//!
//! The program hash binds the chain to the constant pool, since instruction
//! operands that index it (`PUSH_STR`, `CAP_CALL`, `CALL`, ...) are recorded
//! as the raw index only. The manifest is not hashed: what it changes (entry
//! point, permissions) shows up in the instructions and cap outcomes recorded.
//!
//! ## Chain
//!
//! `h_n = H( h_{n-1} || event_n )`; the event count is `n`. Events:
//!
//! ```text
//! instruction: 0x01 || u64 ip || u32 len || code[ip .. ip+len]
//!              (the opcode byte plus its operand bytes, exactly as in CVM1 code)
//! cap call:    0x02 || str(name) || args_hash[32] || outcome
//!   args_hash = H( u32 argc || value(arg_0) || ... )
//!   outcome   = 0x00 || H(value(result))      a value was returned
//!             | 0x01                          no value returned
//!             | 0x02 || H(utf8(error text))   the call failed (VmError Display)
//!             | 0x03                          an index cap dispatched into an
//!                                             accessor function (its body is
//!                                             recorded as instructions)
//! ```
//!
//! An instruction is recorded when it is dispatched, before it executes; a cap
//! event is recorded while that instruction executes, so it follows its own
//! `CAP_CALL`/`EXEC_LANG`/AI/DOM instruction event in the chain.
//!
//! ## Values
//!
//! `value(v)` is a tagged, canonical encoding that never depends on map
//! iteration (insertion) order:
//!
//! | tag  | variant  | body |
//! |------|----------|------|
//! | 0x00 | Null     | — |
//! | 0x01 | Bool     | `u8` 0/1 |
//! | 0x02 | Int      | `i64` |
//! | 0x03 | Float    | `u64` IEEE-754 bits (`-0.0` ≠ `0.0`, NaN payload kept) |
//! | 0x04 | Str      | `str` |
//! | 0x05 | Array    | `u32 n` + `value` each, in order |
//! | 0x06 | Tuple    | as Array |
//! | 0x07 | List     | as Array |
//! | 0x08 | Vector   | as Array |
//! | 0x09 | Set      | `u32 n` + `value` each, **sorted bytewise by encoding** |
//! | 0x0A | Map      | `u32 n` + (`str(key)` `value`) each, **sorted by key bytes** |
//! | 0x0B | Error    | `str` |
//! | 0x0C | Bytes    | `u32 len` + bytes |
//! | 0x0D | Handle   | `u64` |
//! | 0x0E | Foreign  | `u64` |
//! | 0x0F | Func     | `str(name)` `u8 arity` + captured as Array body |
//! | 0x10 | Cell     | `value(inner)` (the contents, never the pointer) |
//! | 0x11 | Record   | `str(kind name)` + fields as Map body (sorted by name) |
//! | 0x12 | Kind     | `str(name)` |
//! | 0xFE | (cycle)  | a shared container met again inside itself |
//! | 0xFF | (depth)  | nesting deeper than [`MAX_VALUE_DEPTH`] |
//!
//! # What it covers
//!
//! The scheduler VM (`run_scheduled_with_transcript`) and [`crate::PortableVm`]
//! ([`crate::PortableVm::transcript_digest`]). In both:
//!
//! - every dispatched instruction, in every green task, in execution order —
//!   so `SPAWN`/`YIELD`/`AWAIT` and task interleaving are covered as the
//!   instruction stream they produce (they are not cap calls);
//! - `CAP_CALL` (built-in, host, index caps, `parallel_map`), with the args as
//!   the cap receives them;
//! - `EXEC_LANG` as cap `polyglot.<lang>`: args `[lang, code, (name, env)…]`,
//!   result `(visible output, result value)`. Gate refusals before the block
//!   runs are not cap events (the run just fails after the instruction event);
//! - AI opcodes as cap `ai_native.<kind>` (args `[payload, operands…]`), DOM
//!   opcodes as cap `dom_native.<kind>` (args `[]`, the pushed value);
//! - COSM `str`/`repr` print hooks, which run in a nested run of the program:
//!   their instructions are folded into the same chain.
//!
//! `parallel_map` workers are share-nothing VMs without host caps: their
//! instructions are NOT recorded, only the `parallel_map` cap event with the
//! results it returned.
//!
//! The two engines are NOT promised to produce the same digest for the same
//! program. They share the encoding, the cap-event shapes and the 50-instruction
//! task slice, and agree on the programs tested here (straight-line, cap calls,
//! `parallel_map`, one spawn/await), but their error texts can differ (a failed
//! cap hashes its `VmError` text) and no cross-engine equivalence over task
//! interleavings is claimed. Same-engine runs are deterministic.
//! FastVM, the JIT and the AOT backends are not covered.

use crate::bytecode::Program;
use crate::vm::{Value, VmError};
use sha2::{Digest, Sha256};

/// Encoding version. Bump on any change to the byte layout above.
pub const TRANSCRIPT_VERSION: u8 = 1;

/// Domain-separation prefix of `h_0`.
pub const DOMAIN: &[u8] = b"crush-vm/cvm1-transcript/v1";

/// Containers nested deeper than this encode as tag `0xFF`.
pub const MAX_VALUE_DEPTH: usize = 128;

const EV_INSTRUCTION: u8 = 0x01;
const EV_CAP: u8 = 0x02;

/// The end state of a transcript: what a host signs or compares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TranscriptDigest {
    /// [`TRANSCRIPT_VERSION`] of the encoding that produced `digest`.
    pub version: u8,
    /// `h_n`.
    pub digest: [u8; 32],
    /// `n`: instruction events plus cap events.
    pub events: u64,
}

impl TranscriptDigest {
    /// The digest as lowercase hex.
    pub fn hex(&self) -> String {
        hex::encode(self.digest)
    }
}

/// How a cap call ended (see the module doc's `outcome`).
pub enum CapOutcome<'a> {
    Value(&'a Value),
    NoValue,
    Error(&'a VmError),
    AccessorCall,
}

impl<'a> CapOutcome<'a> {
    /// The outcome of a cap returning `Result<Option<Value>, VmError>`.
    pub fn of(r: &'a Result<Option<Value>, VmError>) -> Self {
        match r {
            Ok(Some(v)) => CapOutcome::Value(v),
            Ok(None) => CapOutcome::NoValue,
            Err(e) => CapOutcome::Error(e),
        }
    }

    /// The outcome of a cap returning `Result<Value, VmError>`.
    pub fn of_value(r: &'a Result<Value, VmError>) -> Self {
        match r {
            Ok(v) => CapOutcome::Value(v),
            Err(e) => CapOutcome::Error(e),
        }
    }
}

/// A running hash chain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transcript {
    head: [u8; 32],
    events: u64,
}

impl Transcript {
    /// `h_0` for `program`.
    pub fn new(program: &Program) -> Self {
        let mut h = Sha256::new();
        h.update(DOMAIN);
        h.update(program_hash(program));
        Self { head: h.finalize().into(), events: 0 }
    }

    fn chain(&mut self, event: &[u8]) {
        let mut h = Sha256::new();
        h.update(self.head);
        h.update(event);
        self.head = h.finalize().into();
        self.events += 1;
    }

    /// Record the instruction at `ip` whose bytes (opcode + operands) are `bytes`.
    pub fn record_instruction(&mut self, ip: usize, bytes: &[u8]) {
        let mut ev = Vec::with_capacity(13 + bytes.len());
        ev.push(EV_INSTRUCTION);
        ev.extend_from_slice(&(ip as u64).to_be_bytes());
        ev.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
        ev.extend_from_slice(bytes);
        self.chain(&ev);
    }

    /// Record a call of cap `name` with `args` that ended in `outcome`.
    pub fn record_cap(&mut self, name: &str, args: &[Value], outcome: CapOutcome<'_>) {
        let mut ev = Vec::with_capacity(80 + name.len());
        ev.push(EV_CAP);
        put_str(&mut ev, name);
        ev.extend_from_slice(&hash_values(args));
        match outcome {
            CapOutcome::Value(v) => {
                ev.push(0x00);
                ev.extend_from_slice(&hash_value(v));
            }
            CapOutcome::NoValue => ev.push(0x01),
            CapOutcome::Error(e) => {
                ev.push(0x02);
                ev.extend_from_slice(&Sha256::digest(e.to_string().as_bytes()));
            }
            CapOutcome::AccessorCall => ev.push(0x03),
        }
        self.chain(&ev);
    }

    /// The current `h_n` and `n`.
    pub fn digest(&self) -> TranscriptDigest {
        TranscriptDigest { version: TRANSCRIPT_VERSION, digest: self.head, events: self.events }
    }
}

/// Record an `EXEC_LANG` block as cap `gate` (`polyglot.<lang>`) — the one
/// shape both engines use. Args: `[lang, code, (name, env value)…]` (the env
/// strings exactly as the executor receives them); result: `(visible output,
/// result value)`, or the error.
pub(crate) fn record_exec_lang(
    record: &mut dyn FnMut(&str, &[Value], CapOutcome<'_>),
    gate: &str,
    lang: &str,
    code: &str,
    env: &[(String, String)],
    outcome: Result<(&str, &Value), &VmError>,
) {
    let mut args = Vec::with_capacity(2 + env.len());
    args.push(Value::Str(lang.to_string()));
    args.push(Value::Str(code.to_string()));
    for (name, val) in env {
        args.push(Value::Tuple(vec![Value::Str(name.clone()), Value::Str(val.clone())]));
    }
    match outcome {
        Ok((visible, value)) => {
            let result = Value::Tuple(vec![Value::Str(visible.to_string()), value.clone()]);
            record(gate, &args, CapOutcome::Value(&result));
        }
        Err(e) => record(gate, &args, CapOutcome::Error(e)),
    }
}

/// `program_hash` from the module doc.
pub fn program_hash(program: &Program) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update((program.code.len() as u32).to_be_bytes());
    h.update(&program.code);
    h.update((program.consts.len() as u32).to_be_bytes());
    for c in &program.consts {
        h.update((c.len() as u32).to_be_bytes());
        h.update(c.as_bytes());
    }
    h.finalize().into()
}

/// `H(value(v))`.
pub fn hash_value(v: &Value) -> [u8; 32] {
    let mut buf = Vec::new();
    encode_value(v, &mut buf);
    Sha256::digest(&buf).into()
}

/// `H(u32 n || value(v_0) || ...)` — a cap's `args_hash`.
pub fn hash_values(vs: &[Value]) -> [u8; 32] {
    let mut buf = Vec::new();
    buf.extend_from_slice(&(vs.len() as u32).to_be_bytes());
    for v in vs {
        encode_value(v, &mut buf);
    }
    Sha256::digest(&buf).into()
}

/// Append `value(v)` (module doc) to `out`.
pub fn encode_value(v: &Value, out: &mut Vec<u8>) {
    Encoder { path: Vec::new() }.value(v, out, 0);
}

fn put_str(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u32).to_be_bytes());
    out.extend_from_slice(s.as_bytes());
}

/// Tracks the shared containers on the current path, for cycle detection.
struct Encoder {
    path: Vec<*const ()>,
}

impl Encoder {
    /// Run `body` for the shared container at `ptr`, or write the cycle marker
    /// when it is already being encoded further up.
    fn shared(&mut self, ptr: *const (), out: &mut Vec<u8>, body: impl FnOnce(&mut Self, &mut Vec<u8>)) {
        if self.path.contains(&ptr) {
            out.push(0xFE);
            return;
        }
        self.path.push(ptr);
        body(self, out);
        self.path.pop();
    }

    fn seq<'a>(&mut self, tag: u8, items: impl ExactSizeIterator<Item = &'a Value>, out: &mut Vec<u8>, depth: usize) {
        out.push(tag);
        out.extend_from_slice(&(items.len() as u32).to_be_bytes());
        for item in items {
            self.value(item, out, depth + 1);
        }
    }

    fn entries(&mut self, mut entries: Vec<(&str, &Value)>, out: &mut Vec<u8>, depth: usize) {
        entries.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
        out.extend_from_slice(&(entries.len() as u32).to_be_bytes());
        for (k, v) in entries {
            put_str(out, k);
            self.value(v, out, depth + 1);
        }
    }

    fn value(&mut self, v: &Value, out: &mut Vec<u8>, depth: usize) {
        if depth > MAX_VALUE_DEPTH {
            out.push(0xFF);
            return;
        }
        match v {
            Value::Null => out.push(0x00),
            Value::Bool(b) => {
                out.push(0x01);
                out.push(*b as u8);
            }
            Value::Int(i) => {
                out.push(0x02);
                out.extend_from_slice(&i.to_be_bytes());
            }
            Value::Float(f) => {
                out.push(0x03);
                out.extend_from_slice(&f.to_bits().to_be_bytes());
            }
            Value::Str(s) => {
                out.push(0x04);
                put_str(out, s);
            }
            Value::Array(a) => {
                self.shared(std::rc::Rc::as_ptr(a).cast(), out, |e, out| e.seq(0x05, a.borrow().iter(), out, depth))
            }
            Value::Tuple(t) => self.seq(0x06, t.iter(), out, depth),
            Value::List(l) => {
                self.shared(std::rc::Rc::as_ptr(l).cast(), out, |e, out| e.seq(0x07, l.borrow().iter(), out, depth))
            }
            Value::Vector(a) => {
                self.shared(std::rc::Rc::as_ptr(a).cast(), out, |e, out| e.seq(0x08, a.borrow().iter(), out, depth))
            }
            Value::Set(s) => self.shared(std::rc::Rc::as_ptr(s).cast(), out, |e, out| {
                let items = s.borrow();
                let mut encoded: Vec<Vec<u8>> = items
                    .iter()
                    .map(|item| {
                        let mut b = Vec::new();
                        e.value(item, &mut b, depth + 1);
                        b
                    })
                    .collect();
                encoded.sort();
                out.push(0x09);
                out.extend_from_slice(&(encoded.len() as u32).to_be_bytes());
                for b in encoded {
                    out.extend_from_slice(&b);
                }
            }),
            Value::Map(m) => self.shared(std::rc::Rc::as_ptr(m).cast(), out, |e, out| {
                out.push(0x0A);
                let m = m.borrow();
                e.entries(m.iter().map(|(k, v)| (k.as_str(), v)).collect(), out, depth);
            }),
            Value::Error(s) => {
                out.push(0x0B);
                put_str(out, s);
            }
            Value::Bytes(b) => {
                out.push(0x0C);
                out.extend_from_slice(&(b.len() as u32).to_be_bytes());
                out.extend_from_slice(b);
            }
            Value::Handle(h) => {
                out.push(0x0D);
                out.extend_from_slice(&h.to_be_bytes());
            }
            Value::Foreign(h) => {
                out.push(0x0E);
                out.extend_from_slice(&h.to_be_bytes());
            }
            Value::Func(f) => {
                out.push(0x0F);
                put_str(out, &f.name);
                out.push(f.arity);
                out.extend_from_slice(&(f.captured.len() as u32).to_be_bytes());
                for c in &f.captured {
                    self.value(c, out, depth + 1);
                }
            }
            Value::Cell(c) => self.shared(std::rc::Rc::as_ptr(c).cast(), out, |e, out| {
                out.push(0x10);
                e.value(&c.borrow(), out, depth + 1);
            }),
            Value::Record(r) => self.shared(std::rc::Rc::as_ptr(r).cast(), out, |e, out| {
                out.push(0x11);
                put_str(out, r.kind.name());
                let fields = r.fields();
                e.entries(fields.iter().map(|(k, v)| (k.as_str(), v)).collect(), out, depth);
            }),
            Value::Kind(k) => {
                out.push(0x12);
                put_str(out, k.name());
            }
        }
    }
}

/// The scheduler VM's recorder. The scheduler runs a program to completion on
/// one OS thread (nested `run_from`s for COSM print hooks included), so the
/// active transcript lives in a thread-local for the run's duration instead of
/// being threaded through every `execute_one` call.
pub(crate) mod active {
    use super::{CapOutcome, Transcript};
    use crate::vm::Value;
    use std::cell::RefCell;

    thread_local! {
        static ACTIVE: RefCell<Option<Transcript>> = const { RefCell::new(None) };
    }

    /// Restores the previous recorder when dropped (panic-safe).
    pub(crate) struct Scope {
        prev: Option<Option<Transcript>>,
    }

    impl Scope {
        /// Install `t` (or `None` to pause recording) until [`Scope::finish`] or drop.
        pub(crate) fn enter(t: Option<Transcript>) -> Self {
            let prev = ACTIVE.with(|a| a.replace(t));
            Self { prev: Some(prev) }
        }

        /// End the scope, returning the transcript recorded in it.
        pub(crate) fn finish(mut self) -> Option<Transcript> {
            let prev = self.prev.take().expect("scope finished once");
            ACTIVE.with(|a| a.replace(prev))
        }
    }

    impl Drop for Scope {
        fn drop(&mut self) {
            if let Some(prev) = self.prev.take() {
                ACTIVE.with(|a| *a.borrow_mut() = prev);
            }
        }
    }

    pub(crate) fn instruction(ip: usize, bytes: &[u8]) {
        ACTIVE.with(|a| {
            if let Some(t) = a.borrow_mut().as_mut() {
                t.record_instruction(ip, bytes);
            }
        });
    }

    pub(crate) fn cap(name: &str, args: &[Value], outcome: CapOutcome<'_>) {
        ACTIVE.with(|a| {
            if let Some(t) = a.borrow_mut().as_mut() {
                t.record_cap(name, args, outcome);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assembler::assemble;
    use crate::host::{HostCap, HostCapSpec, HostCaps};
    use crate::portable_vm::PortableVm;
    use crate::scheduler::run_scheduled_with_transcript;
    use crate::vm::{MapStore, Quotas};
    use std::cell::RefCell;
    use std::rc::Rc;

    /// `test.answer`: returns the configured value.
    struct Answer(i64);
    impl HostCap for Answer {
        fn spec(&self) -> HostCapSpec {
            HostCapSpec { name: "test.answer".to_string(), argc: Some(1), returns: true }
        }
        fn call(&self, _args: Vec<Value>) -> Result<Option<Value>, String> {
            Ok(Some(Value::Int(self.0)))
        }
    }

    fn caps(answer: i64) -> HostCaps {
        let mut h = HostCaps::new();
        h.register(Box::new(Answer(answer)));
        h
    }

    fn sched(src: &str, perms: &[&str], host: Option<&HostCaps>) -> TranscriptDigest {
        let p = assemble(src, Some(perms), Some("t")).unwrap();
        let (r, d) = run_scheduled_with_transcript(&p, &Quotas::default(), host, None);
        r.unwrap();
        d
    }

    fn portable(src: &str, perms: &[&str], host: Option<HostCaps>) -> TranscriptDigest {
        let p = assemble(src, Some(perms), Some("t")).unwrap();
        let mut vm = PortableVm::new(p);
        if let Some(h) = host {
            vm.set_host_caps(h);
        }
        vm.run().unwrap();
        vm.transcript_digest()
    }

    const ARITH: &str = "PUSH 2\nPUSH 3\nADD\nPUSH 4\nMUL\nHALT";
    const ARITH_SUB: &str = "PUSH 2\nPUSH 3\nSUB\nPUSH 4\nMUL\nHALT";
    const WITH_CAP: &str = "PUSH 7\nCAP_CALL \"test.answer\" 1\nPUSH 1\nADD\nHALT";
    const WITHOUT_CAP: &str = "PUSH 7\nPUSH 42\nPUSH 1\nADD\nHALT";
    const MAP_PROG: &str = "NEW_OBJ\nPUSH 1\nSET_FIELD \"a\"\nPUSH 2\nSET_FIELD \"b\"\nCAP_CALL \"io.print\" 1\nHALT";

    #[test]
    fn identical_runs_give_identical_digests_on_both_engines() {
        for src in [ARITH, MAP_PROG] {
            let a = sched(src, &["io.print"], None);
            let b = sched(src, &["io.print"], None);
            assert_eq!(a, b, "scheduler: {src}");
            assert!(a.events > 0);
            let c = portable(src, &["io.print"], None);
            let d = portable(src, &["io.print"], None);
            assert_eq!(c, d, "portable: {src}");
        }
        let a = sched(WITH_CAP, &["test.answer"], Some(&caps(42)));
        assert_eq!(a, sched(WITH_CAP, &["test.answer"], Some(&caps(42))));
        let c = portable(WITH_CAP, &["test.answer"], Some(caps(42)));
        assert_eq!(c, portable(WITH_CAP, &["test.answer"], Some(caps(42))));
    }

    #[test]
    fn one_instruction_difference_changes_the_digest() {
        assert_ne!(sched(ARITH, &[], None), sched(ARITH_SUB, &[], None));
        assert_ne!(portable(ARITH, &[], None), portable(ARITH_SUB, &[], None));
    }

    #[test]
    fn a_different_cap_result_changes_the_digest() {
        let a = sched(WITH_CAP, &["test.answer"], Some(&caps(42)));
        let b = sched(WITH_CAP, &["test.answer"], Some(&caps(43)));
        assert_ne!(a, b);
        assert_eq!(a.events, b.events, "same instructions, only the result differs");
        let c = portable(WITH_CAP, &["test.answer"], Some(caps(42)));
        let d = portable(WITH_CAP, &["test.answer"], Some(caps(43)));
        assert_ne!(c, d);
        assert_eq!(c.events, d.events);
    }

    #[test]
    fn a_cap_call_is_an_event_of_its_own() {
        let with = sched(WITH_CAP, &["test.answer"], Some(&caps(42)));
        let without = sched(WITHOUT_CAP, &["test.answer"], Some(&caps(42)));
        assert_ne!(with, without);
        // 5 instructions + 1 cap event vs 5 instructions.
        assert_eq!((with.events, without.events), (6, 5));
        let pwith = portable(WITH_CAP, &["test.answer"], Some(caps(42)));
        let pwithout = portable(WITHOUT_CAP, &["test.answer"], Some(caps(42)));
        assert_ne!(pwith, pwithout);
        assert_eq!((pwith.events, pwithout.events), (6, 5));
    }

    #[test]
    fn a_failed_cap_call_is_recorded_with_its_error() {
        // Declared but not provided by the host: the call fails.
        let p = assemble(WITH_CAP, Some(&["test.answer"]), Some("t")).unwrap();
        let (r, d) = run_scheduled_with_transcript(&p, &Quotas::default(), Some(&HostCaps::new()), None);
        assert!(r.is_err());
        assert_eq!(d.events, 3, "PUSH, CAP_CALL, the cap's error");
    }

    #[test]
    fn map_hashing_ignores_insertion_order() {
        let mk = |pairs: &[(&str, i64)]| {
            let mut m = MapStore::new();
            for (k, v) in pairs {
                m.insert(k.to_string(), Value::Int(*v));
            }
            Value::Map(Rc::new(RefCell::new(m)))
        };
        let ab = mk(&[("a", 1), ("b", 2)]);
        let ba = mk(&[("b", 2), ("a", 1)]);
        assert_eq!(hash_value(&ab), hash_value(&ba));
        assert_ne!(hash_value(&ab), hash_value(&mk(&[("a", 1), ("b", 3)])));
        // Nested inside an array too.
        let arr = |v: Value| Value::new_array(vec![v]);
        assert_eq!(hash_value(&arr(ab.clone())), hash_value(&arr(ba)));
        // Sets: order-independent as well.
        let set = |vs: Vec<Value>| Value::Set(Rc::new(RefCell::new(vs)));
        assert_eq!(
            hash_value(&set(vec![Value::Int(1), Value::Str("x".into())])),
            hash_value(&set(vec![Value::Str("x".into()), Value::Int(1)]))
        );
    }

    #[test]
    fn a_cyclic_value_hashes_without_recursing_forever() {
        let a = Value::new_array(vec![]);
        if let Value::Array(rc) = &a {
            rc.borrow_mut().push(a.clone());
        }
        let _ = hash_value(&a);
        if let Value::Array(rc) = &a {
            rc.borrow_mut().clear(); // break the Rc cycle
        }
    }

    #[test]
    fn known_answer_for_the_empty_chain() {
        // A verifier recomputing h_0 for an empty program by hand.
        let p = Program { code: vec![], consts: vec![], ..assemble("HALT", None, None).unwrap() };
        let mut ph = Sha256::new();
        ph.update(0u32.to_be_bytes());
        ph.update(0u32.to_be_bytes());
        let mut h0 = Sha256::new();
        h0.update(DOMAIN);
        h0.update(ph.finalize());
        let expect: [u8; 32] = h0.finalize().into();
        assert_eq!(Transcript::new(&p).digest().digest, expect);
        // And one instruction event on top.
        let mut t = Transcript::new(&p);
        t.record_instruction(0, &[0xFF]);
        let mut h1 = Sha256::new();
        h1.update(expect);
        h1.update([0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0xFF]);
        let expect1: [u8; 32] = h1.finalize().into();
        assert_eq!(t.digest().digest, expect1);
        assert_eq!(t.digest().events, 1);
    }

    #[test]
    fn non_transcript_entry_points_do_not_record_into_an_active_one() {
        // A host cap that runs a nested program through the plain entry point
        // while a transcript is active must not leak events into it.
        struct Nested;
        impl HostCap for Nested {
            fn spec(&self) -> HostCapSpec {
                HostCapSpec { name: "test.nested".to_string(), argc: Some(0), returns: true }
            }
            fn call(&self, _args: Vec<Value>) -> Result<Option<Value>, String> {
                let p = assemble(ARITH, None, Some("n")).unwrap();
                let r = crate::vm::run(&p, &Quotas::default()).map_err(|e| e.to_string())?;
                Ok(r.stack.last().cloned())
            }
        }
        let mut h = HostCaps::new();
        h.register(Box::new(Nested));
        let d = sched("CAP_CALL \"test.nested\" 0\nHALT", &["test.nested"], Some(&h));
        assert_eq!(d.events, 3, "CAP_CALL, its cap event, HALT");
    }
}

#[cfg(test)]
mod engine_tests {
    use crate::assembler::assemble;
    use crate::host::{HostCaps, polyglot_gate};
    use crate::portable_vm::{PortableVm, VmYield};
    use crate::scheduler::run_scheduled_with_transcript;
    use crate::vm::Quotas;

    const PMAP: &str = ".func main\n    PUSH 1\n    PUSH 2\n    NEW_ARRAY 2\n    MAKE_CLOSURE double 1 0\n    CAP_CALL \"parallel_map\" 2\n    HALT\n.func double\n    PUSH 2\n    MUL\n    RET";

    #[test]
    fn parallel_map_records_its_results_but_not_its_workers() {
        let p = assemble(PMAP, Some(&["parallel_map"]), Some("main")).unwrap();
        let (r, a) = run_scheduled_with_transcript(&p, &Quotas::default(), None, None);
        let r = r.unwrap();
        assert_eq!(format!("{:?}", r.stack.last()), "Some(Array(RefCell { value: [Int(2), Int(4)] }))");
        let (_, b) = run_scheduled_with_transcript(&p, &Quotas::default(), None, None);
        assert_eq!(a, b);
        // 6 main-thread instructions + the parallel_map cap event; none of
        // the workers' `PUSH 2; MUL; RET`.
        assert_eq!(a.events, 7);
        let mut vm = PortableVm::new(p);
        vm.run().unwrap();
        assert_eq!(vm.transcript_digest().events, 7);
        assert_eq!(vm.transcript_digest(), a, "no task switching here, so the engines agree");
    }

    /// SPAWN/AWAIT are recorded as the instruction stream they produce, in
    /// execution order, task bodies included. Both engines share the
    /// 50-instruction slice and agree on this program; that is measured here,
    /// not promised in general (see the module doc).
    #[test]
    fn spawned_task_instructions_are_recorded_on_both_engines() {
        let src = ".func main\n    PUSH 99\n    PUSH_STR \"double\"\n    SPAWN 1\n    AWAIT\n    HALT\n.func double\n    PUSH 2\n    MUL\n    RET";
        let p = assemble(src, None, Some("main")).unwrap();
        let (r, a) = run_scheduled_with_transcript(&p, &Quotas::default(), None, None);
        assert_eq!(r.unwrap().stack.last(), Some(&crate::vm::Value::Int(198)));
        assert_eq!(a, run_scheduled_with_transcript(&p, &Quotas::default(), None, None).1);
        // main: PUSH, PUSH_STR, SPAWN, AWAIT, HALT; task: PUSH, MUL, RET.
        assert_eq!(a.events, 8);
        let mut vm = PortableVm::new(p);
        vm.run().unwrap();
        assert_eq!(vm.transcript_digest(), a);
    }

    fn exec_lang_program() -> crate::bytecode::Program {
        let spec = serde_json::json!({"lang": "python", "code": "print('hi')", "var_count": 0});
        let src = format!("EXEC_LANG \"{}\"\nHALT", spec.to_string().replace('"', "\\\""));
        assemble(&src, Some(&["polyglot.python"]), Some("t")).unwrap()
    }

    fn granted() -> HostCaps {
        let mut h = HostCaps::new();
        h.register(polyglot_gate("python"));
        h
    }

    #[test]
    fn a_deferred_lang_block_is_recorded_once_like_an_executor_run() {
        // Deferred: the first step yields without executing EXEC_LANG; the
        // re-run after `provide_lang_result` executes it.
        let mut deferred = PortableVm::new(exec_lang_program());
        deferred.set_host_caps(granted());
        deferred.set_lang_deferred(true);
        assert!(matches!(deferred.step().unwrap(), Some(VmYield::HostCall { .. })));
        assert_eq!(deferred.transcript_digest().events, 0, "the yielded EXEC_LANG is un-recorded");
        deferred.provide_lang_result(Ok("hi\n".to_string()));
        deferred.run().unwrap();

        let mut direct = PortableVm::new(exec_lang_program());
        direct.set_host_caps(granted());
        direct.set_lang_executor(Box::new(|_, _, _| Ok("hi\n".to_string())));
        direct.run().unwrap();

        // EXEC_LANG, its polyglot.python cap event, HALT.
        assert_eq!(direct.transcript_digest().events, 3);
        assert_eq!(deferred.transcript_digest(), direct.transcript_digest());

        let mut other = PortableVm::new(exec_lang_program());
        other.set_host_caps(granted());
        other.set_lang_executor(Box::new(|_, _, _| Ok("bye\n".to_string())));
        other.run().unwrap();
        assert_ne!(other.transcript_digest(), direct.transcript_digest(), "a different block result");
    }
}
