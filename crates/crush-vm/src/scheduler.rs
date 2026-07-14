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
        ADD | SUB | MUL | DIV | MOD | LT | GT | LE | GE => {
            let b = need_num!(pop!());
            let a = need_num!(pop!());
            let is_float = matches!((&a, &b), (Value::Float(_), _) | (_, Value::Float(_)));
            let af = to_f64(&a);
            let bf = to_f64(&b);
            let result = match opcode {
                ADD => {
                    if is_float { Value::Float(af + bf) }
                    else { Value::Int(to_i64(&a).checked_add(to_i64(&b)).ok_or(VmError::ArithmeticOverflow)?) }
                }
                SUB => {
                    if is_float { Value::Float(af - bf) }
                    else { Value::Int(to_i64(&a).checked_sub(to_i64(&b)).ok_or(VmError::ArithmeticOverflow)?) }
                }
                MUL => {
                    if is_float { Value::Float(af * bf) }
                    else { Value::Int(to_i64(&a).checked_mul(to_i64(&b)).ok_or(VmError::ArithmeticOverflow)?) }
                }
                DIV => {
                    if bf == 0.0 { return Err(VmError::DivByZero); }
                    if is_float { Value::Float(af / bf) }
                    else { Value::Int(trunc_div(to_i64(&a), to_i64(&b))) }
                }
                MOD => {
                    if bf == 0.0 { return Err(VmError::DivByZero); }
                    if is_float { Value::Float(af % bf) }
                    else {
                        let ai = to_i64(&a); let bi = to_i64(&b);
                        Value::Int(ai - bi * trunc_div(ai, bi))
                    }
                }
                LT => Value::Bool(af < bf),
                GT => Value::Bool(af > bf),
                LE => Value::Bool(af <= bf),
                GE => Value::Bool(af >= bf),
                _ => unreachable!(),
            };
            push!(result);
        }
        NEG => {
            let v = need_num!(pop!());
            push!(match v {
                Value::Int(i) => Value::Int(-i),
                Value::Float(f) => Value::Float(-f),
                _ => unreachable!(),
            });
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
            let arr_rc = need_array(arr_v)?;
            let arr = arr_rc.borrow();
            let len = arr.len();
            let actual = wrap_index(idx, len)?;
            push!(arr[actual].clone());
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
            let mut cmd = std::process::Command::new(lang);
            cmd.arg("-c").arg(code_str);
            for (name, val) in var_names.iter().zip(var_values.iter()) {
                cmd.env(name, val.as_text());
            }
            let output = cmd
                .output()
                .map_err(|e| VmError::UnknownCap(format!("exec_lang({lang}): {e}")))?;
            if output.status.success() {
                let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
                *out_len += s.len();
                if *out_len > quotas.max_output {
                    return Err(VmError::OutputQuota(quotas.max_output));
                }
                out_parts.push(s.clone());
                push!(Value::Str(s));
            } else {
                let err = String::from_utf8_lossy(&output.stderr);
                return Err(VmError::UnknownCap(format!("exec_lang({lang}): {err}")));
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
                let s: String = args.iter().map(|a| a.as_text()).collect::<Vec<_>>().concat();
                *out_len += s.len();
                if *out_len > quotas.max_output {
                    return Err(VmError::OutputQuota(quotas.max_output));
                }
                out_parts.push(s);
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
                    Value::Array(elems) => { elems.borrow_mut().push(args[1].clone()); Ok(None) }
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
                        Ok(None)
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
        return handler.call(args).map_err(|msg| VmError::UnknownCap(format!("{cap}: {msg}")));
    }

    Err(VmError::UnknownCap(cap.to_string()))
}
