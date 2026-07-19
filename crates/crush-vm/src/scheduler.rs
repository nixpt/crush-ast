//! Green-thread scheduler for the CVM1 interpreter.
//!
//! Runs multiple green threads cooperatively, round-robin, SLICE_SIZE
//! instructions at a time.  SPAWN creates a new green thread, AWAIT parks
//! the caller until the target finishes, YIELD gives up the remaining slice.
//!
//! The Program is shared (read-only) across all green threads — each thread
//! owns its own ip + stack + call_stack + try_stack.

use std::collections::HashMap;

use crate::bytecode::{self, *};
use crate::caps::capabilities;
use crate::host::HostCaps;
use crate::vm::{Frame, GreenThread, Quotas, Value, VmError, VmResult};

const SLICE_SIZE: usize = 50;

/// Marks the line in a polyglot block's stdout that carries its marshaled
/// return value, as JSON, so it can be told apart from the block's own
/// ordinary prints (which must still flow through untouched — stdout is a
/// shared channel, and the two must never be conflated). NUL bytes can't
/// appear in ordinary program output, so this cannot collide by accident.
pub const CRUSH_RESULT_SENTINEL: &str = "\u{0}CRUSH_RESULT\u{0}";

/// Map an `@<lang>` polyglot block tag to the interpreter binary and the
/// flag it uses to execute a code string (this is NOT uniform across
/// languages — e.g. Node's `-c` means "check syntax only", not "execute";
/// `-e` is Node's equivalent of Python's `-c`). Returns `None` for
/// languages with no registered executor — callers must surface that as a
/// loud error, never a silent no-op (an unknown/misspelled language should
/// fail the same way whether or not one happens to exist on PATH under its
/// bare tag name).
/// Canonical capability-name suffix for a @lang tag: `python`/`py`/`python3` all map to the
/// single grant `polyglot.python`. This is what a host must grant to allow that language.
pub(crate) fn canonical_lang(lang: &str) -> Option<&'static str> {
    match lang {
        "python" | "python3" | "py" => Some("python"),
        "javascript" | "js" | "es6" | "ecmascript" | "node" => Some("javascript"),
        "bash" | "sh" => Some("bash"),
        _ => None,
    }
}

/// The @lang → (binary, exec-flag) allowlist. SHARED with portable_vm so the two backends can
/// never drift on which languages run or how (found drifting by crush-diff: portable used the raw
/// tag `javascript` with `-c`, scheduler mapped it to `node -e`).
pub(crate) fn resolve_lang_binary(lang: &str) -> Option<(&'static str, &'static str)> {
    match lang {
        "python" | "python3" | "py" => Some(("python3", "-c")),
        "javascript" | "js" | "es6" | "ecmascript" | "node" => Some(("node", "-e")),
        "bash" | "sh" => Some(("bash", "-c")),
        _ => None,
    }
}

/// Build a `VmError::LangRuntimeError` for an `EXEC_LANG` guest failure
/// (non-zero subprocess exit). SHARED between scheduler.rs and
/// portable_vm.rs so the two backends can't drift on how a guest exception
/// is classified — before CRUSH-18 both independently misclassified this
/// as `VmError::UnknownCap`, the same variant used for actual
/// capability-grant failures.
pub(crate) fn lang_runtime_error(lang: &str, stderr: &[u8], crush_line: Option<u32>) -> VmError {
    let guest_message = String::from_utf8_lossy(stderr).trim().to_string();
    let message = match crush_line {
        Some(l) => format!("(at .crush line {l}) {guest_message}"),
        None => guest_message,
    };
    VmError::LangRuntimeError {
        lang: lang.to_string(),
        message,
        crush_line,
    }
}

/// Outcome of a wall-clock-bounded subprocess run.
pub(crate) enum CommandOutcome {
    Output(std::process::Output),
    TimedOut,
}

/// Run `cmd` to completion, but kill it and return `TimedOut` if it hasn't
/// exited within `limit_ms`. SHARED with portable_vm — both backends spawn
/// the same kind of polyglot subprocess and need the same bound.
///
/// `Child::wait_with_output()` alone has no timeout, and `Command::output()`
/// (what both `EXEC_LANG` handlers used before this) blocks unboundedly —
/// a hung `python3 -c` (or, eventually, a stalled network fetch inside a
/// bucket-sandboxed capability) would hang the whole interpreter with
/// nothing to stop it. Fixed by spawning, moving each pipe onto its own
/// reader thread immediately (so a chatty child can't deadlock by filling
/// an undrained pipe buffer while we poll), and polling `try_wait()` in
/// this thread against a deadline. `child` itself is never moved out of
/// this function, so it can still be killed here on timeout — only the
/// pipe handles (`ChildStdout`/`ChildStderr`, plain `Send` OS handles, not
/// `Value`) cross the thread boundary.
pub(crate) fn run_with_wall_clock_limit(
    mut cmd: std::process::Command,
    limit_ms: u64,
) -> std::io::Result<CommandOutcome> {
    use std::io::Read;
    use std::process::Stdio;

    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    // Put the child in its own new process group (unix only — see the
    // `libc` dependency comment in Cargo.toml). Killing a single tracked
    // PID is not enough: `bash -c "sleep 30"` forks `sleep` as bash's own
    // child, which inherits bash's stdout/stderr pipe write-ends. Kill
    // bash alone and `sleep` keeps running, still holding those fds open —
    // the reader threads below never see EOF until `sleep` exits on its
    // own, defeating the whole timeout. A process group lets us kill the
    // entire subtree at once.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = cmd.spawn()?;

    let stdout_handle = child.stdout.take();
    let stderr_handle = child.stderr.take();
    let stdout_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut out) = stdout_handle {
            let _ = out.read_to_end(&mut buf);
        }
        buf
    });
    let stderr_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut err) = stderr_handle {
            let _ = err.read_to_end(&mut buf);
        }
        buf
    });

    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(limit_ms);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        if std::time::Instant::now() >= deadline {
            break None;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    };

    match status {
        Some(status) => {
            let stdout = stdout_reader.join().unwrap_or_default();
            let stderr = stderr_reader.join().unwrap_or_default();
            Ok(CommandOutcome::Output(std::process::Output {
                status,
                stdout,
                stderr,
            }))
        }
        None => {
            // Timed out: kill the whole process group (not just `child`)
            // so no descendant survives to keep the output pipes open,
            // then reap and join the reader threads.
            #[cfg(unix)]
            {
                // SAFETY: plain libc call, no pointers/aliasing involved.
                // `child.id()` is the pgid too, since we spawned it with
                // `process_group(0)` above (new group, leader = itself).
                unsafe {
                    libc::kill(-(child.id() as i32), libc::SIGKILL);
                }
            }
            #[cfg(not(unix))]
            {
                let _ = child.kill();
            }
            let _ = child.wait();
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            Ok(CommandOutcome::TimedOut)
        }
    }
}

/// Actions a green thread can request from the scheduler.
pub(crate) enum StepAction {
    /// Normal execution: ip advanced to next_ip.
    Continue,
    /// Control flow changed ip (CALL, JMP, RET, THROW).
    Jump,
    /// Spawn a new green thread for this function name.
    Spawn(String, Vec<Value>),
    /// Main thread halted.
    Finish(VmResult),
    /// Thread finished (RET on non-entry frame → pop; on entry → done).
    Done(Option<Value>),
}

/// Run a program with the cooperative green-thread scheduler.
pub fn run_scheduled(
    program: &crate::bytecode::Program,
    quotas: &Quotas,
    host_caps: Option<&HostCaps>,
) -> Result<VmResult, VmError> {
    let code = &program.code;
    let n = code.len();
    if n == 0 {
        // A genuinely empty program (e.g. a source file that lowers to no CAST
        // statements at all) is a degenerate but valid input, not an error —
        // there's nothing to run. Every other exit path in this scheduler
        // indexes into `code`/`threads` assuming at least one instruction
        // exists; short-circuit here instead of panicking on `code[0]`.
        return Ok(VmResult {
            output: String::new(),
            steps: 0,
            halted: true,
            stack: Vec::new(),
        });
    }
    let declared: std::collections::HashSet<&str> = program
        .manifest
        .permissions
        .iter()
        .map(|s| s.as_str())
        .collect();

    let func_entry: HashMap<&str, usize> = program
        .manifest
        .functions
        .iter()
        .map(|(k, v)| (k.as_str(), v.entry))
        .collect();

    let start_ip = program
        .manifest
        .entry
        .as_deref()
        .and_then(|e| func_entry.get(e).copied())
        .unwrap_or_else(|| func_entry.values().copied().next().unwrap_or(0));

    let mut threads: Vec<GreenThread> = vec![GreenThread::new(start_ip)];
    let mut current: usize = 0;
    let mut slice_remaining = SLICE_SIZE;

    loop {
        // --- round-robin: find a runnable thread ---
        let mut any_runnable = false;
        let start_scan = current;
        loop {
            let t = &threads[current];
            if !t.done && t.waiting_for.is_none() {
                any_runnable = true;
                break;
            }
            current = (current + 1) % threads.len();
            if current == start_scan {
                break;
            }
        }
        if !any_runnable {
            // Every thread is done or blocked — return main thread result
            let main = &threads[0];
            return Ok(VmResult {
                output: main.out_parts.concat(),
                steps: main.steps,
                halted: true,
                stack: main.stack.clone(),
            });
        }

        // --- run one instruction on the current thread ---
        let ip = threads[current].ip;
        let isize = bytecode::instruction_size(code[ip]).ok_or(VmError::UnknownOpcode(code[ip], ip))?;
        if ip + isize > n {
            return Err(VmError::TruncatedInstruction(ip));
        }
        let next_ip = ip + isize;

        threads[current].steps += 1;
        if threads[current].steps > quotas.max_steps {
            return Err(VmError::StepQuota(quotas.max_steps));
        }

        // Borrow ends when execute_one returns — index access is safe in the match below.
        let action = execute_one(
            &mut threads[current], code, ip, next_ip, n, program, quotas,
            &declared, host_caps, &func_entry,
        )?;

        match action {
            StepAction::Continue => {
                threads[current].ip = next_ip;
            }
            StepAction::Jump => {
                // ip was already set inside execute_one (CALL, JMP, etc.)
            }
            StepAction::Spawn(fn_name, args) => {
                threads[current].ip = next_ip;
                let new_id = threads.len() as u64;
                if let Some(&entry) = func_entry.get(fn_name.as_str()) {
                    threads.push(GreenThread::with_args(entry, args));
                    threads[current].stack.push(Value::Handle(new_id));
                } else {
                    threads[current].stack.push(Value::Null);
                }
            }
            StepAction::Finish(result) => {
                return Ok(result);
            }
            StepAction::Done(ret_val) => {
                threads[current].done = true;
                threads[current].return_value = ret_val;
                if current == 0 {
                    return Ok(VmResult {
                        output: threads[0].out_parts.concat(),
                        steps: threads[0].steps,
                        halted: true,
                        stack: threads[0].stack.clone(),
                    });
                }
                // Capture return value before iterating to avoid aliasing.
                let rv = threads[current].return_value.clone().unwrap_or(Value::Null);
                for i in 0..threads.len() {
                    if i != current && threads[i].waiting_for == Some(current as u64) {
                        threads[i].waiting_for = None;
                        threads[i].stack.push(rv.clone());
                    }
                }
            }
        }

        // --- slice management ---
        slice_remaining -= 1;
        if slice_remaining == 0 || threads[current].yielded {
            threads[current].yielded = false;
            current = (current + 1) % threads.len();
            slice_remaining = SLICE_SIZE;
        }
    }
}

/// Execute one instruction on the given thread. Returns the action for the
/// scheduler to handle.
#[allow(clippy::too_many_arguments)]
fn execute_one(
    thread: &mut GreenThread,
    code: &[u8],
    ip: usize,
    next_ip: usize,
    n: usize,
    program: &crate::bytecode::Program,
    quotas: &Quotas,
    declared: &std::collections::HashSet<&str>,
    host_caps: Option<&HostCaps>,
    func_entry: &HashMap<&str, usize>,
) -> Result<StepAction, VmError> {
    // Convenience aliases for thread state
    let stack = &mut thread.stack;
    let call_stack = &mut thread.call_stack;
    let try_stack = &mut thread.try_stack;
    let out_parts = &mut thread.out_parts;
    let out_len = &mut thread.out_len;

    macro_rules! pop {
        () => {{ stack.pop().ok_or(VmError::StackUnderflow)? }};
    }
    macro_rules! push {
        ($v:expr) => {{
            if stack.len() >= quotas.max_stack {
                return Err(VmError::StackQuota(quotas.max_stack));
            }
            stack.push($v);
        }};
    }
    macro_rules! need_num {
        ($v:expr) => {{
            let v = $v;
            if !v.is_numeric() {
                return Err(VmError::TypeError {
                    expected: "numeric",
                    got: v.type_name(),
                });
            }
            v
        }};
    }

    let opcode = code[ip];

    match opcode {
        NOP => {}
        PUSH => {
            let v = i64::from_be_bytes(code[ip + 1..ip + 9].try_into().unwrap());
            push!(Value::Int(v));
        }
        PUSH_F64 => {
            let v = f64::from_be_bytes(code[ip + 1..ip + 9].try_into().unwrap());
            push!(Value::Float(v));
        }
        PUSH_BOOL => {
            let v = i64::from_be_bytes(code[ip + 1..ip + 9].try_into().unwrap());
            push!(Value::Bool(v != 0));
        }
        PUSH_NULL => {
            push!(Value::Null);
        }
        PUSH_STR => {
            let idx = u16::from_be_bytes(code[ip + 1..ip + 3].try_into().unwrap()) as usize;
            let s = program
                .consts
                .get(idx)
                .ok_or(VmError::ConstOutOfRange(idx))?;
            push!(Value::Str(s.clone()));
        }
        POP => {
            pop!();
        }
        DUP => {
            let v = pop!();
            push!(v.clone());
            push!(v);
        }
        SWAP => {
            let a = pop!();
            let b = pop!();
            push!(a);
            push!(b);
        }
        ROT => {
            let a = pop!();
            let b = pop!();
            let c = pop!();
            push!(b);
            push!(c);
            push!(a);
        }
        PICK => {
            let n = u16::from_be_bytes(code[ip+1..ip+3].try_into().unwrap()) as usize;
            let len = stack.len();
            if n >= len { return Err(VmError::StackUnderflow); }
            push!(stack[len - 1 - n].clone());
        }
        ROLL => {
            let n = u16::from_be_bytes(code[ip+1..ip+3].try_into().unwrap()) as usize;
            let len = stack.len();
            if n >= len { return Err(VmError::StackUnderflow); }
            let idx = len - 1 - n;
            let v = stack.remove(idx);
            push!(v);
        }

        EQ => {
            let b = pop!();
            let a = pop!();
            push!(Value::Bool(a == b));
        }
        NE => {
            let b = pop!();
            let a = pop!();
            push!(Value::Bool(a != b));
        }
        // `+` is overloaded: numeric addition, and string concatenation when EITHER side is a
        // string. `io.print("Python says 5^3 is: " + result)` — mixing a string with a number —
        // is the single most common thing anyone writes, and it was a hard type error.
        // (`"a" + "b"` already worked; only the MIXED case failed.)
        ADD if matches!(stack.last(), Some(Value::Str(_)))
            || matches!(stack.len().checked_sub(2).and_then(|k| stack.get(k)), Some(Value::Str(_))) =>
        {
            let b = pop!();
            let a = pop!();
            let joined = format!("{}{}", a.as_text(), b.as_text());
            if joined.len() > quotas.max_output {
                return Err(VmError::OutputQuota(quotas.max_output));
            }
            push!(Value::Str(joined));
        }
        ADD | SUB | MUL | DIV | MOD | LT | GT | LE | GE => {
            let b = pop!();
            let a = pop!();
            let result = match opcode {
                ADD => crate::arithmetic::add_values(&a, &b)?,
                SUB => crate::arithmetic::sub_values(&a, &b)?,
                MUL => crate::arithmetic::mul_values(&a, &b)?,
                DIV => crate::arithmetic::div_values(&a, &b)?,
                MOD => crate::arithmetic::mod_values(&a, &b)?,
                LT => crate::arithmetic::compare_values(&a, &b, |x, y| x < y)?,
                GT => crate::arithmetic::compare_values(&a, &b, |x, y| x > y)?,
                LE => crate::arithmetic::compare_values(&a, &b, |x, y| x <= y)?,
                GE => crate::arithmetic::compare_values(&a, &b, |x, y| x >= y)?,
                _ => unreachable!(),
            };
            push!(result);
        }
        NEG => {
            let v = pop!();
            push!(crate::arithmetic::neg_value(&v)?);
        }
        MATH_POW => {
            let exp = need_num!(pop!());
            let base = need_num!(pop!());
            let base_f = match base {
                Value::Int(x) => x as f64,
                Value::Float(x) => x,
                _ => unreachable!(),
            };
            let exp_f = match exp {
                Value::Int(x) => x as f64,
                Value::Float(x) => x,
                _ => unreachable!(),
            };
            push!(Value::Float(base_f.powf(exp_f)));
        }
        MATH_SQRT | MATH_ABS | MATH_ROUND | MATH_FLOOR | MATH_CEIL => {
            let a = need_num!(pop!());
            let af = match a {
                Value::Int(x) => x as f64,
                Value::Float(x) => x,
                _ => unreachable!(),
            };
            let res = match opcode {
                MATH_SQRT => af.sqrt(),
                MATH_ABS => af.abs(),
                MATH_ROUND => af.round(),
                MATH_FLOOR => af.floor(),
                MATH_CEIL => af.ceil(),
                _ => unreachable!(),
            };
            push!(Value::Float(res));
        }
        VEC_ADD => {
            let right = pop!();
            let left = pop!();
            if let (Value::Array(a), Value::Array(b)) = (&left, &right) {
                let a_ref = a.borrow();
                let b_ref = b.borrow();
                let len = a_ref.len().min(b_ref.len());
                let mut res = Vec::with_capacity(len);
                for i in 0..len {
                    let va = match &a_ref[i] { Value::Int(x) => *x as f64, Value::Float(x) => *x, _ => 0.0 };
                    let vb = match &b_ref[i] { Value::Int(x) => *x as f64, Value::Float(x) => *x, _ => 0.0 };
                    res.push(Value::Float(va + vb));
                }
                push!(Value::new_array(res));
            } else {
                return Err(VmError::TypeError { expected: "array", got: "non-array".into() });
            }
        }
        VEC_DOT => {
            let right = pop!();
            let left = pop!();
            if let (Value::Array(a), Value::Array(b)) = (&left, &right) {
                let a_ref = a.borrow();
                let b_ref = b.borrow();
                let len = a_ref.len().min(b_ref.len());
                let mut sum = 0.0;
                for i in 0..len {
                    let va = match &a_ref[i] { Value::Int(x) => *x as f64, Value::Float(x) => *x, _ => 0.0 };
                    let vb = match &b_ref[i] { Value::Int(x) => *x as f64, Value::Float(x) => *x, _ => 0.0 };
                    sum += va * vb;
                }
                push!(Value::Float(sum));
            } else {
                return Err(VmError::TypeError { expected: "array", got: "non-array".into() });
            }
        }
        MAT_MUL => {
            let right = pop!();
            let left = pop!();
            if let (Value::Array(a), Value::Array(b)) = (&left, &right) {
                let a_ref = a.borrow();
                let b_ref = b.borrow();
                let mut res = Vec::new();
                if a_ref.is_empty() || b_ref.is_empty() {
                    push!(Value::new_array(vec![]));
                } else {
                    let rows_a = a_ref.len();
                    let cols_a = if let Value::Array(first) = &a_ref[0] { first.borrow().len() } else { 0 };
                    let cols_b = if let Value::Array(first) = &b_ref[0] { first.borrow().len() } else { 0 };
                    for i in 0..rows_a {
                        let mut row_res = Vec::new();
                        for j in 0..cols_b {
                            let mut sum = 0.0;
                            for k in 0..cols_a {
                                let va = if let Value::Array(r) = &a_ref[i] { match &r.borrow()[k] { Value::Int(x) => *x as f64, Value::Float(x) => *x, _ => 0.0 } } else { 0.0 };
                                let vb = if let Value::Array(r) = &b_ref[k] { match &r.borrow()[j] { Value::Int(x) => *x as f64, Value::Float(x) => *x, _ => 0.0 } } else { 0.0 };
                                sum += va * vb;
                            }
                            row_res.push(Value::Float(sum));
                        }
                        res.push(Value::new_array(row_res));
                    }
                    push!(Value::new_array(res));
                }
            } else {
                return Err(VmError::TypeError { expected: "array", got: "non-array".into() });
            }
        }
        AND | OR => {
            let b = pop!();
            let a = pop!();
            push!(match opcode {
                AND => Value::Bool(a.is_truthy() && b.is_truthy()),
                OR => Value::Bool(a.is_truthy() || b.is_truthy()),
                _ => unreachable!(),
            });
        }
        BITAND | BITOR | BITXOR | SHL | SHR => {
            let b = need_num!(pop!());
            let a = need_num!(pop!());
            let ai = to_i64(&a);
            let bi = to_i64(&b);
            let result = match opcode {
                BITAND => Value::Int(ai & bi),
                BITOR  => Value::Int(ai | bi),
                BITXOR => Value::Int(ai ^ bi),
                SHL    => Value::Int(ai.checked_shl(bi as u32).ok_or(VmError::ArithmeticOverflow)?),
                SHR    => Value::Int(ai.checked_shr(bi as u32).ok_or(VmError::ArithmeticOverflow)?),
                _ => unreachable!(),
            };
            push!(result);
        }
        BITNOT => {
            let a = need_num!(pop!());
            push!(Value::Int(!to_i64(&a)));
        }
        NOT => {
            let v = pop!();
            push!(Value::Bool(!v.is_truthy()));
        }
        TYPEOF => {
            let v = pop!();
            push!(Value::Str(v.type_name().to_string()));
        }
        CAST => {
            let idx = u16::from_be_bytes(code[ip + 1..ip + 3].try_into().unwrap()) as usize;
            let type_name = program.consts.get(idx).ok_or(VmError::ConstOutOfRange(idx))?.clone();
            let v = pop!();
            match type_name.as_str() {
                "str" | "string" => push!(Value::Str(v.as_text())),
                "int" | "i64" => match v {
                    Value::Int(_) => push!(v),
                    Value::Float(f) => push!(Value::Int(f as i64)),
                    Value::Str(s) => push!(Value::Int(s.parse().unwrap_or(0))),
                    Value::Bool(b) => push!(Value::Int(if b { 1 } else { 0 })),
                    _ => push!(Value::Int(0)),
                },
                "float" | "f64" => match v {
                    Value::Float(_) => push!(v),
                    Value::Int(i) => push!(Value::Float(i as f64)),
                    Value::Str(s) => push!(Value::Float(s.parse().unwrap_or(0.0))),
                    Value::Bool(b) => push!(Value::Float(if b { 1.0 } else { 0.0 })),
                    _ => push!(Value::Float(0.0)),
                },
                "bool" => push!(Value::Bool(v.is_truthy())),
                _ => push!(v),
            }
        }
        NEW_ARRAY => {
            let count = u16::from_be_bytes(code[ip + 1..ip + 3].try_into().unwrap()) as usize;
            let mut vals = Vec::with_capacity(count);
            for _ in 0..count {
                vals.push(pop!());
            }
            vals.reverse();
            push!(Value::new_array(vals));
        }
        ARR_GET => {
            let idx_v = pop!();
            let arr_v = pop!();
            let idx = need_array_index(&idx_v)?;
            match arr_v {
                Value::Array(arr_rc) => {
                    let arr = arr_rc.borrow();
                    let len = arr.len();
                    let actual = wrap_index(idx, len)?;
                    push!(arr[actual].clone());
                }
                Value::Str(s) => {
                    let len = s.chars().count();
                    let actual = wrap_index(idx, len)?;
                    let ch = s.chars().nth(actual).map(|c| c.to_string()).unwrap_or_default();
                    push!(Value::Str(ch));
                }
                _ => return Err(VmError::TypeError { expected: "array or string", got: arr_v.type_name() }),
            }
        }
        ARR_SET => {
            let val = pop!();
            let idx_v = pop!();
            let arr_v = pop!();
            let idx = need_array_index(&idx_v)?;
            let arr_rc = need_array(arr_v)?;
            {
                let mut arr = arr_rc.borrow_mut();
                let len = arr.len();
                let actual = wrap_index(idx, len)?;
                arr[actual] = val;
            }
            push!(Value::Array(arr_rc));
        }
        ARR_LEN => {
            let v = pop!();
            let arr_rc = need_array(v)?;
            let len = arr_rc.borrow().len();
            push!(Value::Int(len as i64));
        }
        ARR_PUSH => {
            let val = pop!();
            let arr_rc = need_array(pop!())?;
            arr_rc.borrow_mut().push(val);
            push!(Value::Array(arr_rc));
        }
        ARR_POP => {
            let arr_rc = need_array(pop!())?;
            let val = arr_rc.borrow_mut().pop().unwrap_or(Value::Null);
            push!(Value::Array(arr_rc.clone()));
            push!(val);
        }
        NEW_TUPLE => {
            let count = u16::from_be_bytes(code[ip + 1..ip + 3].try_into().unwrap()) as usize;
            let mut vals = Vec::with_capacity(count);
            for _ in 0..count {
                vals.push(pop!());
            }
            vals.reverse();
            push!(Value::new_tuple(vals));
        }
        TUPLE_PUSH => {
            let val = pop!();
            let mut t = need_tuple(pop!())?;
            t.push(val);
            push!(Value::Tuple(t));
        }
        NEW_LIST => {
            let count = u16::from_be_bytes(code[ip + 1..ip + 3].try_into().unwrap()) as usize;
            let mut vals = Vec::with_capacity(count);
            for _ in 0..count {
                vals.push(pop!());
            }
            vals.reverse();
            push!(Value::new_list(vals));
        }
        LIST_PUSH => {
            let val = pop!();
            let l_rc = need_list(pop!())?;
            l_rc.borrow_mut().push_back(val);
            push!(Value::List(l_rc));
        }
        NEW_VECTOR => {
            let count = u16::from_be_bytes(code[ip + 1..ip + 3].try_into().unwrap()) as usize;
            let mut vals = Vec::with_capacity(count);
            for _ in 0..count {
                vals.push(pop!());
            }
            vals.reverse();
            push!(Value::new_vector(vals));
        }
        VECTOR_PUSH => {
            let val = pop!();
            let v_rc = need_vector(pop!())?;
            v_rc.borrow_mut().push(val);
            push!(Value::Vector(v_rc));
        }
        NEW_SET => {
            let count = u16::from_be_bytes(code[ip + 1..ip + 3].try_into().unwrap()) as usize;
            let mut vals = Vec::with_capacity(count);
            for _ in 0..count {
                vals.push(pop!());
            }
            vals.reverse();
            push!(Value::new_set(vals));
        }
        SET_PUSH => {
            let val = pop!();
            let s_rc = need_set(pop!())?;
            s_rc.borrow_mut().push(val);
            push!(Value::Set(s_rc));
        }
        LOAD => {
            let slot = u16::from_be_bytes(code[ip + 1..ip + 3].try_into().unwrap());
            let v = call_stack
                .last()
                .unwrap()
                .memory
                .get(&slot)
                .ok_or(VmError::UninitSlot(slot))?
                .clone();
            push!(v);
        }
        STORE => {
            let slot = u16::from_be_bytes(code[ip + 1..ip + 3].try_into().unwrap());
            let v = pop!();
            call_stack.last_mut().unwrap().memory.insert(slot, v);
        }
        JMP | JZ | JNZ => {
            let target = u32::from_be_bytes(code[ip + 1..ip + 5].try_into().unwrap()) as usize;
            if target > n {
                return Err(VmError::BadJump(target));
            }
            let take = match opcode {
                JMP => true,
                JZ => !pop!().is_truthy(),
                JNZ => pop!().is_truthy(),
                _ => unreachable!(),
            };
            if take {
                thread.ip = target;
                return Ok(StepAction::Jump);
            }
        }
        PRINT => {
            let s = pop!().as_text();
            *out_len += s.len();
            if *out_len > quotas.max_output {
                return Err(VmError::OutputQuota(quotas.max_output));
            }
            out_parts.push(s);
        }
        CAP_CALL => {
            let idx = u16::from_be_bytes(code[ip + 1..ip + 3].try_into().unwrap()) as usize;
            let argc = code[ip + 3] as usize;
            let cap = program
                .consts
                .get(idx)
                .ok_or(VmError::ConstOutOfRange(idx))?
                .clone();
            let mut args = Vec::with_capacity(argc);
            for _ in 0..argc {
                args.push(pop!());
            }
            args.reverse();
            let result = dispatch_cap(
                &cap, args, declared, quotas, out_parts, out_len, host_caps,
            )?;
            if let Some(v) = result {
                push!(v);
            }
        }
        CALL => {
            let idx = u16::from_be_bytes(code[ip + 1..ip + 3].try_into().unwrap()) as usize;
            let fname = program
                .consts
                .get(idx)
                .ok_or(VmError::ConstOutOfRange(idx))?;
            let entry = func_entry
                .get(fname.as_str())
                .copied()
                .ok_or_else(|| VmError::UnknownFunction(fname.clone()))?;
            if call_stack.len() >= quotas.max_call_depth {
                return Err(VmError::CallDepthQuota(quotas.max_call_depth));
            }
            call_stack.push(Frame {
                return_ip: Some(next_ip),
                memory: HashMap::new(),
            });
            thread.ip = entry;
            return Ok(StepAction::Jump);
        }
        RET => {
            let frame = call_stack.pop().expect("call stack invariant");
            match frame.return_ip {
                None => {
                    return Ok(StepAction::Done(stack.pop()));
                }
                Some(ret) => {
                    thread.ip = ret;
                    return Ok(StepAction::Jump);
                }
            }
        }
        EXEC_LANG => {
            let idx = u16::from_be_bytes(code[ip + 1..ip + 3].try_into().unwrap()) as usize;
            let spec_json = program
                .consts
                .get(idx)
                .ok_or(VmError::ConstOutOfRange(idx))?
                .clone();
            let spec: HashMap<String, serde_json::Value> = serde_json::from_str(&spec_json)
                .map_err(|_| VmError::UnknownCap("exec_lang: invalid args JSON".to_string()))?;
            let lang = spec.get("lang").and_then(|v| v.as_str()).unwrap_or("?");
            let code_str = spec.get("code").and_then(|v| v.as_str()).unwrap_or("");
            let crush_line = spec.get("crush_line").and_then(|v| v.as_u64()).map(|v| v as u32);
            let var_count = spec.get("var_count").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let mut var_names: Vec<String> = Vec::with_capacity(var_count);
            let mut var_values: Vec<Value> = Vec::with_capacity(var_count);
            for i in 0..var_count {
                let key = format!("var_{}", i);
                if let Some(name) = spec.get(&key).and_then(|v| v.as_str()) {
                    var_names.push(name.to_string());
                    var_values.push(pop!());
                }
            }
            var_values.reverse();
            let (binary, exec_flag) = resolve_lang_binary(lang).ok_or_else(|| {
                VmError::UnknownCap(format!("no executor registered for language '{lang}'"))
            })?;
            // CAPABILITY GATE. A polyglot block spawns a real interpreter with the host process's
            // full authority — `@python { import os; os.system(...) }` is arbitrary code exec. In a
            // capability-based language that MUST be granted, exactly like fs.read or net.get.
            // The grant is `polyglot.<lang>` in the host-caps registry (crush-run: --polyglot;
            // exo-light: derived from the CapabilitySet). No grant → refuse, loudly.
            let gate = canonical_lang(lang)
                .map(|c| format!("polyglot.{c}"))
                .unwrap_or_else(|| format!("polyglot.{lang}"));
            if host_caps.map(|h| h.get(&gate).is_none()).unwrap_or(true) {
                return Err(VmError::UnknownCap(format!(
                    "@{lang} requires the '{gate}' capability (run with --polyglot to grant it); refusing to spawn"
                )));
            }
            #[cfg(target_arch = "wasm32")]
            {
                let _ = (binary, exec_flag, code_str, &var_names, &var_values);
                return Err(VmError::UnknownCap(format!(
                    "@{lang}: polyglot subprocess execution is not supported on wasm32 targets"
                )));
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                let mut cmd = std::process::Command::new(binary);
                cmd.arg(exec_flag).arg(code_str);
                for (name, val) in var_names.iter().zip(var_values.iter()) {
                    cmd.env(name, val.as_text());
                }
                let output = match run_with_wall_clock_limit(cmd, quotas.max_wall_time_ms)
                    .map_err(|e| VmError::UnknownCap(format!("exec_lang({lang}): {e}")))?
                {
                    CommandOutcome::Output(output) => output,
                    CommandOutcome::TimedOut => {
                        return Err(VmError::CapTimeout {
                            cap: gate,
                            limit_ms: quotas.max_wall_time_ms,
                        });
                    }
                };
                if output.status.success() {
                    let raw = String::from_utf8_lossy(&output.stdout);
                    let mut visible_lines: Vec<&str> = Vec::new();
                    let mut result_payload: Option<&str> = None;
                    for line in raw.lines() {
                        match line.strip_prefix(CRUSH_RESULT_SENTINEL) {
                            // Last one wins, matching "the block's final bound
                            // output" if it somehow printed the sentinel more
                            // than once.
                            Some(payload) => result_payload = Some(payload),
                            None => visible_lines.push(line),
                        }
                    }
                    let visible = visible_lines.join("\n").trim().to_string();
                    *out_len += visible.len();
                    if *out_len > quotas.max_output {
                        return Err(VmError::OutputQuota(quotas.max_output));
                    }
                    out_parts.push(visible.clone());
                    let result_value = match result_payload {
                        Some(payload) => serde_json::from_str::<Value>(payload)
                            .unwrap_or_else(|_| Value::Str(payload.to_string())),
                        None => Value::Str(visible),
                    };
                    push!(result_value);
                } else {
                    return Err(lang_runtime_error(lang, &output.stderr, crush_line));
                }
            }
        }
        AI_QUERY | AI_SYNTHESIZE | AI_AGENT_DELEGATION | AI_SEMANTIC_MATCH | AI_LEARNING_LOOP | AI_CONTEXT_AWARE | AI_TOOLCHAIN => {
            // Scheduler VM does not support async AI opcodes natively yet. Stub to Null.
            push!(Value::Null);
        }
        ENTER_TRY => {
            let target = u32::from_be_bytes(code[ip + 1..ip + 5].try_into().unwrap()) as usize;
            if target > n {
                return Err(VmError::BadJump(target));
            }
            try_stack.push(target);
        }
        EXIT_TRY => {
            try_stack.pop();
        }
        THROW => {
            let err_val = pop!();
            if let Some(handler_ip) = try_stack.pop() {
                thread.ip = handler_ip;
                push!(err_val);
                return Ok(StepAction::Jump);
            }
            return Err(VmError::UnknownCap(format!("uncaught error: {}", err_val.as_text())));
        }
        STR_CONTAINS | STR_STARTS_WITH | STR_ENDS_WITH => {
            let needle = pop!();
            let haystack = pop!();
            let haystack_str = haystack.as_text();
            let pattern_str = needle.as_text();
            let res = match opcode {
                STR_CONTAINS => haystack_str.contains(&pattern_str),
                STR_STARTS_WITH => haystack_str.starts_with(&pattern_str),
                STR_ENDS_WITH => haystack_str.ends_with(&pattern_str),
                _ => unreachable!(),
            };
            push!(Value::Bool(res));
        }
        STR_TO_UPPER | STR_TO_LOWER | STR_TRIM => {
            let s = pop!();
            let text = s.as_text();
            let res = match opcode {
                STR_TO_UPPER => text.to_uppercase(),
                STR_TO_LOWER => text.to_lowercase(),
                STR_TRIM => text.trim().to_string(),
                _ => unreachable!(),
            };
            push!(Value::Str(res));
        }
        STR_SPLIT => {
            let delim = pop!();
            let s = pop!();
            let text = s.as_text();
            let d = delim.as_text();
            let parts: Vec<Value> = if d.is_empty() {
                text.chars().map(|c| Value::Str(c.to_string())).collect()
            } else {
                text.split(&d).map(|p| Value::Str(p.to_string())).collect()
            };
            push!(Value::new_array(parts));
        }
        STR_REPLACE => {
            let to = pop!();
            let from = pop!();
            let s = pop!();
            push!(Value::Str(s.as_text().replace(&from.as_text(), &to.as_text())));
        }
        STR_JOIN => {
            let delim = pop!();
            let arr_v = pop!();
            let d = delim.as_text();
            match arr_v {
                Value::Array(elems) => {
                    let parts: Vec<String> = elems.borrow().iter().map(|v| v.as_text()).collect();
                    push!(Value::Str(parts.join(&d)));
                }
                other => {
                    return Err(VmError::TypeError {
                        expected: "array",
                        got: other.type_name(),
                    });
                }
            }
        }
        MAKE_RANGE => {
            let end_v = pop!();
            let start_v = pop!();
            let start = match start_v {
                Value::Int(i) => i,
                other => { return Err(VmError::TypeError { expected: "int", got: other.type_name() }); }
            };
            let end = match end_v {
                Value::Int(i) => i,
                other => { return Err(VmError::TypeError { expected: "int", got: other.type_name() }); }
            };
            let mut elems = Vec::new();
            if start < end {
                for i in start..end {
                    elems.push(Value::Int(i));
                }
            }
            push!(Value::new_array(elems));
        }
        NEW_OBJ => {
            push!(Value::new_map(std::collections::HashMap::new()));
        }
        SET_FIELD => {
            let idx = u16::from_be_bytes(code[ip + 1..ip + 3].try_into().unwrap()) as usize;
            let field = program.consts.get(idx).ok_or(VmError::ConstOutOfRange(idx))?.clone();
            let val = pop!();
            let map_rc = match pop!() {
                Value::Map(m) => m,
                other => { return Err(VmError::TypeError { expected: "map", got: other.type_name() }); }
            };
            map_rc.borrow_mut().insert(field, val);
            push!(Value::Map(map_rc));
        }
        GET_FIELD => {
            let idx = u16::from_be_bytes(code[ip + 1..ip + 3].try_into().unwrap()) as usize;
            let field = program.consts.get(idx).ok_or(VmError::ConstOutOfRange(idx))?.clone();
            let map_rc = match pop!() {
                Value::Map(m) => m,
                other => { return Err(VmError::TypeError { expected: "map", got: other.type_name() }); }
            };
            let val = map_rc.borrow().get(&field).cloned().unwrap_or(Value::Null);
            push!(val);
        }
        SPAWN => {
            let argc = u16::from_be_bytes(code[ip + 1..ip + 3].try_into().unwrap()) as usize;
            let mut args = Vec::with_capacity(argc);
            let fn_name = pop!().as_text();
            for _ in 0..argc {
                args.push(pop!());
            }
            args.reverse();
            return Ok(StepAction::Spawn(fn_name, args));
        }
        YIELD => {
            thread.yielded = true;
        }
        AWAIT => {
            let handle = pop!();
            if let Value::Handle(target_id) = handle {
                thread.waiting_for = Some(target_id);
                // Scheduler will skip this thread until target finishes
                return Ok(StepAction::Continue);
            }
            // Non-handle: push null
            push!(Value::Null);
        }
        HALT => {
            return Ok(StepAction::Finish(VmResult {
                output: out_parts.concat(),
                steps: thread.steps,
                halted: true,
                stack: stack.clone(),
            }));
        }
        _ => return Err(VmError::UnknownOpcode(opcode, ip)),
    }

    Ok(StepAction::Continue)
}

// ---- helper functions (inlined from vm.rs for independence) ----

#[inline]
fn to_f64(v: &Value) -> f64 {
    match v {
        Value::Int(i) => *i as f64,
        Value::Float(f) => *f,
        _ => 0.0,
    }
}

#[inline]
fn to_i64(v: &Value) -> i64 {
    match v {
        Value::Int(i) => *i,
        Value::Float(f) => *f as i64,
        _ => 0,
    }
}

#[inline]
fn trunc_div(a: i64, b: i64) -> i64 {
    a / b
}

fn need_array(v: Value) -> Result<std::rc::Rc<std::cell::RefCell<Vec<Value>>>, VmError> {
    match v {
        Value::Array(a) => Ok(a),
        other => Err(VmError::TypeError { expected: "array", got: other.type_name() }),
    }
}

fn need_tuple(v: Value) -> Result<Vec<Value>, VmError> {
    match v {
        Value::Tuple(t) => Ok(t),
        other => Err(VmError::TypeError { expected: "tuple", got: other.type_name() }),
    }
}

fn need_list(v: Value) -> Result<std::rc::Rc<std::cell::RefCell<std::collections::LinkedList<Value>>>, VmError> {
    match v {
        Value::List(l) => Ok(l),
        other => Err(VmError::TypeError { expected: "list", got: other.type_name() }),
    }
}

fn need_vector(v: Value) -> Result<std::rc::Rc<std::cell::RefCell<Vec<Value>>>, VmError> {
    match v {
        Value::Vector(v_rc) => Ok(v_rc),
        other => Err(VmError::TypeError { expected: "vector", got: other.type_name() }),
    }
}

fn need_set(v: Value) -> Result<std::rc::Rc<std::cell::RefCell<Vec<Value>>>, VmError> {
    match v {
        Value::Set(s) => Ok(s),
        other => Err(VmError::TypeError { expected: "set", got: other.type_name() }),
    }
}

fn need_array_index(v: &Value) -> Result<i64, VmError> {
    match v {
        Value::Int(i) => Ok(*i),
        other => Err(VmError::BadIndex(other.type_name())),
    }
}

fn wrap_index(idx: i64, len: usize) -> Result<usize, VmError> {
    let ilen = len as i64;
    if idx >= -ilen && idx < ilen {
        Ok(if idx < 0 { (ilen + idx) as usize } else { idx as usize })
    } else {
        Err(VmError::ArrayBounds { index: idx, len })
    }
}

fn dispatch_cap(
    cap: &str,
    args: Vec<Value>,
    declared: &std::collections::HashSet<&str>,
    quotas: &Quotas,
    out_parts: &mut Vec<String>,
    out_len: &mut usize,
    host_caps: Option<&HostCaps>,
) -> Result<Option<Value>, VmError> {
    if !declared.contains(cap) {
        return Err(VmError::CapNotDeclared(cap.to_string()));
    }
    if let Some(allowed) = &quotas.allowed_caps
        && !allowed.iter().any(|a| a == cap)
    {
        return Err(VmError::CapDenied(cap.to_string()));
    }

    if let Some(spec) = capabilities().get(cap) {
        if let Some(expected) = spec.argc
            && args.len() != expected
        {
            return Err(VmError::CapArity { cap: cap.to_string(), expected, got: args.len() });
        }
        return match cap {
            "io.print" => {
                let parts: Vec<String> = args.iter().map(|a| a.as_text()).collect();
                let line = crate::io_print::format_io_print_line(&parts);
                *out_len += line.len();
                if *out_len > quotas.max_output {
                    return Err(VmError::OutputQuota(quotas.max_output));
                }
                out_parts.push(line);
                Ok(None)
            }
            "str.concat" => {
                let s: String = args.iter().map(|a| a.as_text()).collect::<Vec<_>>().concat();
                Ok(Some(Value::Str(s)))
            }
            "str.len" => {
                let s = args[0].as_text();
                Ok(Some(Value::Int(s.len() as i64)))
            }
            "str.contains" => {
                let haystack = args[0].as_text();
                let needle = args[1].as_text();
                Ok(Some(Value::Bool(haystack.contains(&needle))))
            }
            "str.split" => {
                let s = args[0].as_text();
                let delim = args[1].as_text();
                let parts: Vec<Value> = if delim.is_empty() {
                    s.chars().map(|c| Value::Str(c.to_string())).collect()
                } else {
                    s.split(&delim).map(|p| Value::Str(p.to_string())).collect()
                };
                Ok(Some(Value::new_array(parts)))
            }
            "str.replace" => {
                let s = args[0].as_text();
                let from = args[1].as_text();
                let to = args[2].as_text();
                Ok(Some(Value::Str(s.replace(&from, &to))))
            }
            "str.join" => {
                let delim = args[1].as_text();
                match &args[0] {
                    Value::Array(elems) => {
                        let parts: Vec<String> = elems.borrow().iter().map(|v| v.as_text()).collect();
                        Ok(Some(Value::Str(parts.join(&delim))))
                    }
                    other => Err(VmError::TypeError { expected: "array", got: other.type_name() }),
                }
            }
            "arr_slice" => {
                if args.len() < 2 { return Err(VmError::CapArity { cap: cap.to_string(), expected: 2, got: args.len() }); }
                match &args[0] {
                    Value::Array(elems) => {
                        let arr = elems.borrow();
                        let len = arr.len() as i64;
                        let start = match &args[1] {
                            Value::Int(i) => *i,
                            Value::Null => 0i64,
                            _ => return Err(VmError::TypeError { expected: "int or null", got: args[1].type_name() }),
                        };
                        let end = if args.len() > 2 {
                            match &args[2] {
                                Value::Int(i) => *i,
                                Value::Null => len,
                                _ => return Err(VmError::TypeError { expected: "int or null", got: args[2].type_name() }),
                            }
                        } else {
                            len
                        };
                        let start = start.max(0).min(len);
                        let end = end.max(start).min(len);
                        let sliced: Vec<Value> = arr[start as usize..end as usize].to_vec();
                        Ok(Some(Value::new_array(sliced)))
                    }
                    _ => Err(VmError::TypeError { expected: "array", got: args[0].type_name() }),
                }
            }
            "make_range" => {
                let (start, end) = match args.len() {
                    0 => (0i64, 100i64),  // large default, for-loop break handles exit
                    1 => {
                        let end = match &args[0] { Value::Int(i) => *i, _ => 100 };
                        (0, end.max(0))
                    }
                    _ => {
                        let s = match &args[0] { Value::Int(i) => *i, _ => 0 };
                        let e = match &args[1] { Value::Int(i) => *i, _ => 0 };
                        (s, e)
                    }
                };
                let mut elems = Vec::new();
                if start < end { for i in start..end { elems.push(Value::Int(i)); } }
                Ok(Some(Value::new_array(elems)))
            }
            "append" | "push" => {
                if args.len() < 2 { return Err(VmError::CapArity { cap: cap.to_string(), expected: 2, got: args.len() }); }
                match &args[0] {
                    Value::Array(elems) => { elems.borrow_mut().push(args[1].clone()); Ok(Some(args[0].clone())) }
                    _ => Err(VmError::TypeError { expected: "array", got: args[0].type_name() }),
                }
            }
            "arr_set" => {
                if args.len() < 3 { return Err(VmError::CapArity { cap: cap.to_string(), expected: 3, got: args.len() }); }
                match &args[0] {
                    Value::Array(elems) => {
                        let idx = match &args[1] { Value::Int(i) => *i as usize, _ => 0 };
                        let mut arr = elems.borrow_mut();
                        if idx < arr.len() { arr[idx] = args[2].clone(); }
                        Ok(Some(args[0].clone()))
                    }
                    _ => Err(VmError::TypeError { expected: "array", got: args[0].type_name() }),
                }
            }
            "arr_get" => {
                if args.len() < 2 { return Err(VmError::CapArity { cap: cap.to_string(), expected: 2, got: args.len() }); }
                match &args[0] {
                    Value::Array(elems) => {
                        let idx = match &args[1] { Value::Int(i) => *i as usize, _ => 0 };
                        Ok(Some(elems.borrow().get(idx).cloned().unwrap_or(Value::Null)))
                    }
                    _ => Err(VmError::TypeError { expected: "array", got: args[0].type_name() }),
                }
            }
            _ => Err(VmError::UnknownCap(cap.to_string())),
        };
    }

    if let Some(host) = host_caps
        && let Some(handler) = host.get(cap)
    {
        let spec = handler.spec();
        if let Some(expected) = spec.argc
            && args.len() != expected
        {
            return Err(VmError::CapArity { cap: cap.to_string(), expected, got: args.len() });
        }
        return match handler.call_with_deadline(args, quotas.max_wall_time_ms) {
            Ok(v) => Ok(v),
            Err(crate::host::HostCapError::Timeout) => Err(VmError::CapTimeout {
                cap: cap.to_string(),
                limit_ms: quotas.max_wall_time_ms,
            }),
            Err(crate::host::HostCapError::Message(msg)) => {
                Err(VmError::UnknownCap(format!("{cap}: {msg}")))
            }
        };
    }

    Err(VmError::UnknownCap(cap.to_string()))
}

#[cfg(test)]
mod wall_clock_limit_tests {
    use super::*;

    /// CRUSH-19 regression fixture: a `HostCap` that self-enforces a
    /// wall-clock deadline via `call_with_deadline` — it genuinely blocks
    /// (polls in a sleep loop) past the deadline it's given, then returns
    /// `HostCapError::Timeout` rather than completing. This is the shape a
    /// blocking `HostCap` (network, cold provisioning) is expected to use
    /// per CRUSH-19's chosen design (cooperative timeout, not generic
    /// `CAP_CALL` preemption — `Value` isn't `Send`).
    struct SelfEnforcingBlockingCap;

    impl crate::host::HostCap for SelfEnforcingBlockingCap {
        fn spec(&self) -> crate::host::HostCapSpec {
            crate::host::HostCapSpec {
                name: "test.slow_blocking_cap".to_string(),
                argc: Some(0),
                returns: true,
            }
        }

        fn call(&self, _args: Vec<Value>) -> Result<Option<Value>, String> {
            // A HostCap that never overrides call_with_deadline would land
            // here with no bound at all — this impl deliberately overrides
            // call_with_deadline below instead, so this arm should never
            // execute in the regression test.
            std::thread::sleep(std::time::Duration::from_secs(30));
            Ok(Some(Value::Null))
        }

        fn call_with_deadline(
            &self,
            _args: Vec<Value>,
            deadline_ms: u64,
        ) -> Result<Option<Value>, crate::host::HostCapError> {
            let deadline =
                std::time::Instant::now() + std::time::Duration::from_millis(deadline_ms);
            loop {
                if std::time::Instant::now() >= deadline {
                    return Err(crate::host::HostCapError::Timeout);
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
    }

    #[test]
    fn cap_call_returns_a_named_timeout_error_instead_of_hanging() {
        use crate::assembler::assemble;
        use crate::host::HostCaps;
        use crate::vm::{Quotas, VmError, run_with_caps};

        let mut host_caps = HostCaps::new();
        host_caps.register(Box::new(SelfEnforcingBlockingCap));

        let prog = assemble(
            "CAP_CALL \"test.slow_blocking_cap\" 0\nHALT",
            Some(&["test.slow_blocking_cap"]),
            None,
        )
        .unwrap();

        let quotas = Quotas {
            max_wall_time_ms: 100,
            ..Default::default()
        };

        let start = std::time::Instant::now();
        let result = run_with_caps(&prog, &quotas, Some(&host_caps));
        let elapsed = start.elapsed();

        match result {
            Err(VmError::CapTimeout { cap, limit_ms }) => {
                assert_eq!(cap, "test.slow_blocking_cap");
                assert_eq!(limit_ms, 100);
            }
            other => panic!("expected VmError::CapTimeout, got {other:?}"),
        }
        // The real proof: this returned near the 100ms deadline, not after
        // the cap's own 30s `call()` sleep (which call_with_deadline's
        // override preempts before ever reaching).
        assert!(
            elapsed < std::time::Duration::from_secs(5),
            "should return promptly at the deadline, took {elapsed:?}"
        );
    }

    #[test]
    fn fast_command_returns_its_real_output_within_the_limit() {
        let mut cmd = std::process::Command::new("bash");
        cmd.arg("-c").arg("echo hi");
        match run_with_wall_clock_limit(cmd, 5_000).expect("spawn should succeed") {
            CommandOutcome::Output(output) => {
                assert!(output.status.success());
                assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "hi");
            }
            CommandOutcome::TimedOut => panic!("a fast command must not time out"),
        }
    }

    #[test]
    fn slow_command_is_killed_at_the_deadline_not_waited_out() {
        let mut cmd = std::process::Command::new("bash");
        cmd.arg("-c").arg("sleep 30");
        let start = std::time::Instant::now();
        let outcome = run_with_wall_clock_limit(cmd, 150).expect("spawn should succeed");
        let elapsed = start.elapsed();
        assert!(
            matches!(outcome, CommandOutcome::TimedOut),
            "a 30s sleep against a 150ms limit must time out"
        );
        // The real proof: this returned near the 150ms deadline, not after
        // the process's own 30s sleep completed on its own.
        assert!(
            elapsed < std::time::Duration::from_secs(5),
            "should return promptly after killing, took {elapsed:?}"
        );
    }

    #[test]
    fn failing_command_still_reports_its_real_exit_status_and_stderr() {
        let mut cmd = std::process::Command::new("bash");
        cmd.arg("-c").arg("echo oops >&2; exit 1");
        match run_with_wall_clock_limit(cmd, 5_000).expect("spawn should succeed") {
            CommandOutcome::Output(output) => {
                assert!(!output.status.success());
                assert_eq!(String::from_utf8_lossy(&output.stderr).trim(), "oops");
            }
            CommandOutcome::TimedOut => panic!("a fast-failing command must not time out"),
        }
    }

    #[test]
    fn a_chatty_process_does_not_deadlock_while_polling() {
        // Regression guard for the exact bug this function was designed to
        // avoid: if stdout weren't drained on its own thread while the main
        // thread polls `try_wait`, a process writing enough output to fill
        // the OS pipe buffer (~64KiB on Linux) would block on its own
        // write() and never reach exit, hanging forever regardless of the
        // wall-clock limit.
        let mut cmd = std::process::Command::new("bash");
        cmd.arg("-c").arg("head -c 1000000 /dev/zero | tr '\\0' 'a'");
        let start = std::time::Instant::now();
        match run_with_wall_clock_limit(cmd, 5_000).expect("spawn should succeed") {
            CommandOutcome::Output(output) => {
                assert!(output.status.success());
                assert_eq!(output.stdout.len(), 1_000_000);
            }
            CommandOutcome::TimedOut => {
                panic!("a 1MB write should complete well within 5s if pipes are drained correctly")
            }
        }
        assert!(start.elapsed() < std::time::Duration::from_secs(5));
    }

    // CRUSH-18 regression (green-thread scheduler side — see
    // portable_vm.rs's parallel test for the other backend, both sharing
    // `lang_runtime_error` so they can't drift on this classification).
    // Before this fix, a guest program's own non-zero exit was folded into
    // `VmError::UnknownCap`, the same variant used for "the capability
    // doesn't exist"/"wasn't granted" — a category error.
    #[test]
    fn exec_lang_guest_failure_maps_to_lang_runtime_error_not_unknown_cap() {
        use crate::assembler::assemble;
        use crate::host::HostCaps;
        use crate::vm::run_with_caps;

        let spec = serde_json::json!({
            "lang": "bash",
            "code": "echo -n 'boom' >&2; exit 1",
            "var_count": 0,
            "crush_line": 7,
        });
        let src = format!(
            "EXEC_LANG \"{}\"\nHALT",
            spec.to_string().replace('"', "\\\"")
        );
        let prog = assemble(&src, None, None).unwrap();

        let mut host_caps = HostCaps::new();
        host_caps.grant_polyglot(&["bash"]);

        match run_with_caps(&prog, &Quotas::default(), Some(&host_caps)) {
            Err(VmError::LangRuntimeError { lang, message, crush_line }) => {
                assert_eq!(lang, "bash");
                assert!(
                    message.contains("boom"),
                    "message should carry the guest's stderr, got: {message}"
                );
                assert_eq!(crush_line, Some(7), "should surface the .crush-source line");
            }
            other => panic!("expected VmError::LangRuntimeError, got {other:?}"),
        }
    }
}
