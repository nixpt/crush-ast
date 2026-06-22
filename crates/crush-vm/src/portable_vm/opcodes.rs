//! opcode dispatch extracted from portable_vm.rs (CRUSHPVMSPLIT-1).
//!
//! Private submodule of `crush_vm::portable_vm` -- exposes two
//! `pub(super)` chokepoint fns: `execute_instruction` + `dispatch_cap`.
//! Both take `&mut super::PortableVm` so parent state is mutated in place.
//! No pub-surface change at the crate root.

use super::*;
use crate::vm::{Value, VmError};

pub(super) fn execute_instruction(vm: &mut super::PortableVm, opcode: u8, next_ip: usize) -> Result<(), VmError> {
        use crate::bytecode::*;

        match opcode {
            NOP => {}
            PUSH => {
                let val = i64::from_be_bytes(
                    vm.program.code[vm.ip + 1..vm.ip + 9]
                        .try_into()
                        .unwrap(),
                );
                vm.push(Value::Int(val));
            }
            PUSH_F64 => {
                let val = f64::from_be_bytes(
                    vm.program.code[vm.ip + 1..vm.ip + 9]
                        .try_into()
                        .unwrap(),
                );
                vm.push(Value::Float(val));
            }
            PUSH_STR => {
                let idx = u16::from_be_bytes(
                    vm.program.code[vm.ip + 1..vm.ip + 3]
                        .try_into()
                        .unwrap(),
                ) as usize;
                let s = vm
                    .program
                    .consts
                    .get(idx)
                    .ok_or(VmError::ConstOutOfRange(idx))?;
                vm.push(Value::Str(s.clone()));
            }
            PUSH_BOOL => {
                let v = i64::from_be_bytes(
                    vm.program.code[vm.ip + 1..vm.ip + 9]
                        .try_into()
                        .unwrap(),
                );
                vm.push(Value::Bool(v != 0));
            }
            PUSH_NULL => {
                vm.push(Value::Null);
            }
            POP => {
                vm.pop()?;
            }
            DUP => {
                let v = vm.peek()?.clone();
                vm.push(v);
            }
            SWAP => {
                let a = vm.pop()?;
                let b = vm.pop()?;
                vm.push(a);
                vm.push(b);
            }
            ROT => {
                let a = vm.pop()?;
                let b = vm.pop()?;
                let c = vm.pop()?;
                vm.push(b);
                vm.push(c);
                vm.push(a);
            }
            PICK => {
                let n = u16::from_be_bytes(
                    vm.program.code[vm.ip + 1..vm.ip + 3].try_into().unwrap(),
                ) as usize;
                if n >= vm.stack.len() {
                    return Err(VmError::StackUnderflow);
                }
                vm.push(vm.stack[vm.stack.len() - 1 - n].clone());
            }
            ROLL => {
                let n = u16::from_be_bytes(
                    vm.program.code[vm.ip + 1..vm.ip + 3].try_into().unwrap(),
                ) as usize;
                if n >= vm.stack.len() {
                    return Err(VmError::StackUnderflow);
                }
                let idx = vm.stack.len() - 1 - n;
                let v = vm.stack.remove(idx);
                vm.push(v);
            }
            ADD | SUB | MUL | DIV | MOD => {
                let b = vm.pop()?;
                let a = vm.pop()?;
                let is_float = matches!((&a, &b), (Value::Float(_), _) | (_, Value::Float(_)));
                let af = to_f64_p(&a);
                let bf = to_f64_p(&b);
                let result = match opcode {
                    ADD => {
                        if is_float {
                            Value::Float(af + bf)
                        } else {
                            Value::Int(
                                to_i64(&a)
                                    .checked_add(to_i64(&b))
                                    .ok_or(VmError::ArithmeticOverflow)?,
                            )
                        }
                    }
                    SUB => {
                        if is_float {
                            Value::Float(af - bf)
                        } else {
                            Value::Int(
                                to_i64(&a)
                                    .checked_sub(to_i64(&b))
                                    .ok_or(VmError::ArithmeticOverflow)?,
                            )
                        }
                    }
                    MUL => {
                        if is_float {
                            Value::Float(af * bf)
                        } else {
                            Value::Int(
                                to_i64(&a)
                                    .checked_mul(to_i64(&b))
                                    .ok_or(VmError::ArithmeticOverflow)?,
                            )
                        }
                    }
                    DIV => {
                        if bf == 0.0 {
                            return Err(VmError::DivByZero);
                        }
                        if is_float {
                            Value::Float(af / bf)
                        } else {
                            Value::Int(to_i64(&a) / to_i64(&b))
                        }
                    }
                    MOD => {
                        if bf == 0.0 {
                            return Err(VmError::DivByZero);
                        }
                        if is_float {
                            Value::Float(af % bf)
                        } else {
                            let ai = to_i64(&a);
                            let bi = to_i64(&b);
                            Value::Int(ai - bi * (ai / bi))
                        }
                    }
                    _ => unreachable!(),
                };
                vm.push(result);
            }
            NEG => {
                let a = vm.pop()?;
                match a {
                    Value::Int(i) => vm.push(Value::Int(-i)),
                    Value::Float(f) => vm.push(Value::Float(-f)),
                    other => {
                        return Err(VmError::TypeError {
                            expected: "numeric",
                            got: value_type_name(&other),
                        });
                    }
                }
            }
            EQ | NE => {
                let b = vm.pop()?;
                let a = vm.pop()?;
                vm.push(match opcode {
                    EQ => Value::Bool(a == b),
                    NE => Value::Bool(a != b),
                    _ => unreachable!(),
                });
            }
            LT | GT | LE | GE => {
                let b = vm.pop()?;
                let a = vm.pop()?;
                let is_float = matches!((&a, &b), (Value::Float(_), _) | (_, Value::Float(_)));
                let af = to_f64_p(&a);
                let bf = to_f64_p(&b);
                let result = match opcode {
                    LT => Value::Bool(if is_float {
                        af < bf
                    } else {
                        to_i64(&a) < to_i64(&b)
                    }),
                    GT => Value::Bool(if is_float {
                        af > bf
                    } else {
                        to_i64(&a) > to_i64(&b)
                    }),
                    LE => Value::Bool(if is_float {
                        af <= bf
                    } else {
                        to_i64(&a) <= to_i64(&b)
                    }),
                    GE => Value::Bool(if is_float {
                        af >= bf
                    } else {
                        to_i64(&a) >= to_i64(&b)
                    }),
                    _ => unreachable!(),
                };
                vm.push(result);
            }
            AND | OR => {
                let b = vm.pop()?;
                let a = vm.pop()?;
                vm.push(match opcode {
                    AND => Value::Bool(value_is_truthy(&a) && value_is_truthy(&b)),
                    OR => Value::Bool(value_is_truthy(&a) || value_is_truthy(&b)),
                    _ => unreachable!(),
                });
            }
            BITAND | BITOR | BITXOR | SHL | SHR => {
                let b = vm.pop()?;
                let a = vm.pop()?;
                let ai = to_i64(&a);
                let bi = to_i64(&b);
                let result = match opcode {
                    BITAND => Value::Int(ai & bi),
                    BITOR => Value::Int(ai | bi),
                    BITXOR => Value::Int(ai ^ bi),
                    SHL => Value::Int(
                        ai.checked_shl(bi as u32)
                            .ok_or(VmError::ArithmeticOverflow)?,
                    ),
                    SHR => Value::Int(
                        ai.checked_shr(bi as u32)
                            .ok_or(VmError::ArithmeticOverflow)?,
                    ),
                    _ => unreachable!(),
                };
                vm.push(result);
            }
            BITNOT => {
                let a = vm.pop()?;
                vm.push(Value::Int(!to_i64(&a)));
            }
            NOT => {
                let a = vm.pop()?;
                vm.push(Value::Bool(!value_is_truthy(&a)));
            }
            TYPEOF => {
                let v = vm.pop()?;
                vm.push(Value::Str(value_type_name(&v).to_string()));
            }
            CAST => {
                let idx = u16::from_be_bytes(
                    vm.program.code[vm.ip + 1..vm.ip + 3].try_into().unwrap(),
                ) as usize;
                let type_name = vm.program.consts.get(idx).ok_or(VmError::ConstOutOfRange(idx))?.clone();
                let v = vm.pop()?;
                match type_name.as_str() {
                    "str" | "string" => vm.push(Value::Str(value_to_text(&v))),
                    "int" | "i64" => {
                        vm.push(match v {
                            Value::Int(_) => v,
                            Value::Float(f) => Value::Int(f as i64),
                            Value::Str(s) => Value::Int(s.parse().unwrap_or(0)),
                            Value::Bool(b) => Value::Int(if b { 1 } else { 0 }),
                            _ => Value::Int(0),
                        });
                    }
                    "float" | "f64" => {
                        vm.push(match v {
                            Value::Float(_) => v,
                            Value::Int(i) => Value::Float(i as f64),
                            Value::Str(s) => Value::Float(s.parse().unwrap_or(0.0)),
                            Value::Bool(b) => Value::Float(if b { 1.0 } else { 0.0 }),
                            _ => Value::Float(0.0),
                        });
                    }
                    "bool" => vm.push(Value::Bool(value_is_truthy(&v))),
                    _ => vm.push(v),
                }
            }
            LOAD => {
                let slot = u16::from_be_bytes(
                    vm.program.code[vm.ip + 1..vm.ip + 3]
                        .try_into()
                        .unwrap(),
                );
                let frame = vm.call_stack.last().ok_or(VmError::StackUnderflow)?;
                let v = frame.memory.get(&slot).ok_or(VmError::UninitSlot(slot))?;
                vm.push(v.clone());
            }
            STORE => {
                let slot = u16::from_be_bytes(
                    vm.program.code[vm.ip + 1..vm.ip + 3]
                        .try_into()
                        .unwrap(),
                );
                let v = vm.pop()?;
                let frame = vm.call_stack.last_mut().ok_or(VmError::StackUnderflow)?;
                frame.memory.insert(slot, v);
            }
            JMP | JZ | JNZ => {
                let target = u32::from_be_bytes(
                    vm.program.code[vm.ip + 1..vm.ip + 5]
                        .try_into()
                        .unwrap(),
                ) as usize;
                if target > vm.program.code.len() {
                    return Err(VmError::BadJump(target));
                }
                let take = match opcode {
                    JMP => true,
                    JZ => !value_is_truthy(&vm.pop()?),
                    JNZ => value_is_truthy(&vm.pop()?),
                    _ => unreachable!(),
                };
                if take {
                    vm.ip = target;
                }
            }
            PRINT => {
                let s = value_to_text(&vm.pop()?);
                vm.out_len += s.len();
                if vm.out_len > vm.quotas.max_output {
                    return Err(VmError::OutputQuota(vm.quotas.max_output));
                }
                vm.out_parts.push(s);
            }
            CAP_CALL => {
                let idx = u16::from_be_bytes(
                    vm.program.code[vm.ip + 1..vm.ip + 3]
                        .try_into()
                        .unwrap(),
                ) as usize;
                let argc = vm.program.code[vm.ip + 3] as usize;

                let cap = vm
                    .program
                    .consts
                    .get(idx)
                    .ok_or(VmError::ConstOutOfRange(idx))?
                    .clone();

                let mut args = Vec::with_capacity(argc);
                for _ in 0..argc {
                    args.push(vm.pop()?);
                }
                args.reverse();

                let result = dispatch_cap(vm, &cap, args)?;
                if let Some(v) = result {
                    vm.push(v);
                }
            }
            CALL => {
                let idx = u16::from_be_bytes(
                    vm.program.code[vm.ip + 1..vm.ip + 3]
                        .try_into()
                        .unwrap(),
                ) as usize;
                let fname = vm
                    .program
                    .consts
                    .get(idx)
                    .ok_or(VmError::ConstOutOfRange(idx))?;

                let func_entry = vm.get_function_entry(fname)?;
                if vm.call_stack.len() >= vm.quotas.max_call_depth {
                    return Err(VmError::CallDepthQuota(vm.quotas.max_call_depth));
                }

                // Arguments stay on the stack (main VM convention).
                // Callee accesses them via stack operations or LOAD/STORE slots.
                vm.call_stack.push(Frame::new(Some(next_ip)));
                vm.ip = func_entry;
            }
            RET => {
                let frame = vm.call_stack.pop().ok_or(VmError::StackUnderflow)?;
                match frame.return_ip {
                    None => {
                        vm.halted = true;
                    }
                    Some(ret_ip) => {
                        vm.ip = ret_ip;
                    }
                }
            }
            NEW_ARRAY => {
                let count = u16::from_be_bytes(
                    vm.program.code[vm.ip + 1..vm.ip + 3]
                        .try_into()
                        .unwrap(),
                ) as usize;
                let mut vals = Vec::with_capacity(count);
                for _ in 0..count {
                    vals.push(vm.pop()?);
                }
                vals.reverse();
                vm.push(Value::new_array(vals));
            }
            ARR_GET => {
                let idx_v = vm.pop()?;
                let arr_v = vm.pop()?;
                let idx = need_array_index(&idx_v)?;
                let arr_rc = need_array(arr_v)?;
                let arr = arr_rc.borrow();
                let len = arr.len();
                let actual = wrap_index(idx, len)?;
                vm.push(arr[actual].clone());
            }
            ARR_SET => {
                let val = vm.pop()?;
                let idx_v = vm.pop()?;
                let arr_v = vm.pop()?;
                let idx = need_array_index(&idx_v)?;
                let arr_rc = need_array(arr_v)?;
                {
                    let mut arr = arr_rc.borrow_mut();
                    let len = arr.len();
                    let actual = wrap_index(idx, len)?;
                    arr[actual] = val;
                }
                vm.push(Value::Array(arr_rc));
            }
            ARR_LEN => {
                let v = vm.pop()?;
                let arr_rc = need_array(v)?;
                vm.push(Value::Int(arr_rc.borrow().len() as i64));
            }
            ARR_PUSH => {
                let val = vm.pop()?;
                let arr_rc = need_array(vm.pop()?)?;
                arr_rc.borrow_mut().push(val);
                vm.push(Value::Array(arr_rc));
            }
            ARR_POP => {
                let arr_rc = need_array(vm.pop()?)?;
                let val = arr_rc.borrow_mut().pop().unwrap_or(Value::Null);
                vm.push(Value::Array(arr_rc.clone()));
                vm.push(val);
            }
            NEW_OBJ => {
                vm.push(Value::new_map(std::collections::HashMap::new()));
            }
            SET_FIELD => {
                let idx = u16::from_be_bytes(
                    vm.program.code[vm.ip + 1..vm.ip + 3]
                        .try_into()
                        .unwrap(),
                ) as usize;
                let field = vm
                    .program
                    .consts
                    .get(idx)
                    .ok_or(VmError::ConstOutOfRange(idx))?
                    .clone();
                let val = vm.pop()?;
                let map_rc = match vm.pop()? {
                    Value::Map(m) => m,
                    other => {
                        return Err(VmError::TypeError {
                            expected: "map",
                            got: value_type_name(&other),
                        });
                    }
                };
                map_rc.borrow_mut().insert(field, val);
                vm.push(Value::Map(map_rc));
            }
            GET_FIELD => {
                let idx = u16::from_be_bytes(
                    vm.program.code[vm.ip + 1..vm.ip + 3]
                        .try_into()
                        .unwrap(),
                ) as usize;
                let field = vm
                    .program
                    .consts
                    .get(idx)
                    .ok_or(VmError::ConstOutOfRange(idx))?
                    .clone();
                let map_rc = match vm.pop()? {
                    Value::Map(m) => m,
                    other => {
                        return Err(VmError::TypeError {
                            expected: "map",
                            got: value_type_name(&other),
                        });
                    }
                };
                let val = map_rc.borrow().get(&field).cloned().unwrap_or(Value::Null);
                vm.push(val);
            }
            ENTER_TRY => {
                let target = u32::from_be_bytes(
                    vm.program.code[vm.ip + 1..vm.ip + 5]
                        .try_into()
                        .unwrap(),
                ) as usize;
                if target > vm.program.code.len() {
                    return Err(VmError::BadJump(target));
                }
                vm.try_stack.push(target);
            }
            EXIT_TRY => {
                vm.try_stack.pop();
            }
            THROW => {
                let err_val = vm.pop()?;
                if let Some(handler_ip) = vm.try_stack.pop() {
                    vm.ip = handler_ip;
                    vm.push(err_val);
                    return Ok(());
                }
                return Err(VmError::UnknownCap(format!(
                    "uncaught error: {}",
                    value_to_text(&err_val)
                )));
            }
            STR_CONTAINS => {
                let needle = vm.pop()?;
                let haystack = vm.pop()?;
                vm.push(Value::Bool(
                    value_to_text(&haystack).contains(&value_to_text(&needle)),
                ));
            }
            STR_SPLIT => {
                let delim = vm.pop()?;
                let s = vm.pop()?;
                let text = value_to_text(&s);
                let d = value_to_text(&delim);
                let parts: Vec<Value> = if d.is_empty() {
                    text.chars().map(|c| Value::Str(c.to_string())).collect()
                } else {
                    text.split(&d).map(|p| Value::Str(p.to_string())).collect()
                };
                vm.push(Value::new_array(parts));
            }
            STR_REPLACE => {
                let to = vm.pop()?;
                let from = vm.pop()?;
                let s = vm.pop()?;
                vm.push(Value::Str(
                    value_to_text(&s).replace(&value_to_text(&from), &value_to_text(&to)),
                ));
            }
            STR_JOIN => {
                let delim = vm.pop()?;
                let arr_v = vm.pop()?;
                let d = value_to_text(&delim);
                match arr_v {
                    Value::Array(elems) => {
                        let parts: Vec<String> = elems.borrow().iter().map(|v| value_to_text(v)).collect();
                        vm.push(Value::Str(parts.join(&d)));
                    }
                    other => {
                        return Err(VmError::TypeError {
                            expected: "array",
                            got: value_type_name(&other),
                        });
                    }
                }
            }
            MAKE_RANGE => {
                let end_v = vm.pop()?;
                let start_v = vm.pop()?;
                let start = match start_v {
                    Value::Int(i) => i,
                    other => {
                        return Err(VmError::TypeError {
                            expected: "int",
                            got: value_type_name(&other),
                        });
                    }
                };
                let end = match end_v {
                    Value::Int(i) => i,
                    other => {
                        return Err(VmError::TypeError {
                            expected: "int",
                            got: value_type_name(&other),
                        });
                    }
                };
                let mut elems = Vec::new();
                if start < end {
                    for i in start..end {
                        elems.push(Value::Int(i));
                    }
                }
                vm.push(Value::new_array(elems));
            }
            EXEC_LANG => {
                let idx = u16::from_be_bytes(
                    vm.program.code[vm.ip + 1..vm.ip + 3].try_into().unwrap(),
                ) as usize;
                let spec_json = vm.program.consts.get(idx).ok_or(VmError::ConstOutOfRange(idx))?.clone();
                let spec: std::collections::HashMap<String, serde_json::Value> =
                    serde_json::from_str(&spec_json)
                        .map_err(|_| VmError::UnknownCap("exec_lang: invalid args JSON".to_string()))?;
                let lang = spec.get("lang").and_then(|v| v.as_str()).unwrap_or("?");
                let code_str = spec.get("code").and_then(|v| v.as_str()).unwrap_or("");
                let var_count = spec.get("var_count").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let mut var_values: Vec<Value> = Vec::with_capacity(var_count);
                for _ in 0..var_count {
                    var_values.push(vm.pop()?);
                }
                var_values.reverse();
                let mut cmd = std::process::Command::new(lang);
                cmd.arg("-c").arg(code_str);
                for (i, val) in var_values.iter().enumerate() {
                    let key = format!("var_{}", i);
                    if let Some(name) = spec.get(&key).and_then(|v| v.as_str()) {
                        cmd.env(name, value_to_text(val));
                    }
                }
                let output = cmd.output()
                    .map_err(|e| VmError::UnknownCap(format!("exec_lang({lang}): {e}")))?;
                if output.status.success() {
                    let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    vm.out_len += s.len();
                    if vm.out_len > vm.quotas.max_output {
                        return Err(VmError::OutputQuota(vm.quotas.max_output));
                    }
                    vm.out_parts.push(s.clone());
                    vm.push(Value::Str(s));
                } else {
                    let err = String::from_utf8_lossy(&output.stderr);
                    return Err(VmError::UnknownCap(format!("exec_lang({lang}): {err}")));
                }
            }
            SPAWN => {
                let fn_name = value_to_text(&vm.pop()?);
                let task_id = vm.next_task_id;
                vm.next_task_id += 1;
                vm.scheduled_tasks.insert(task_id, fn_name);
                vm.push(Value::Int(task_id as i64));
            }
            YIELD => {
                std::thread::yield_now();
            }
            AWAIT => {
                let handle = vm.pop()?;
                if let Value::Int(task_id) = handle {
                    if let Some(fn_name) = vm.scheduled_tasks.remove(&(task_id as u64)) {
                        if let Some(&entry) = vm.func_entry.get(&fn_name) {
                            if vm.call_stack.len() >= vm.quotas.max_call_depth {
                                return Err(VmError::CallDepthQuota(vm.quotas.max_call_depth));
                            }
                            vm.call_stack
                                .push(Frame::new(Some(next_ip)));
                            vm.ip = entry;
                            return Ok(());
                        }
                    }
                }
                vm.push(Value::Null);
            }
            HALT => {
                vm.halted = true;
            }
            _ => return Err(VmError::UnknownOpcode(opcode, vm.ip)),
        }

        Ok(())
}

pub(super) fn dispatch_cap(vm: &mut super::PortableVm, cap: &str, args: Vec<Value>) -> Result<Option<Value>, VmError> {
        // Check permission
        if !vm.declared_caps.contains(cap) {
            return Err(VmError::CapNotDeclared(cap.to_string()));
        }
        if let Some(allowed) = &vm.quotas.allowed_caps
            && !allowed.iter().any(|a| a == cap)
        {
            return Err(VmError::CapDenied(cap.to_string()));
        }

        // Built-in portable capabilities
        if let Some(spec) = crate::caps::capabilities().get(cap) {
            if let Some(expected) = spec.argc
                && args.len() != expected
            {
                return Err(VmError::CapArity {
                    cap: cap.to_string(),
                    expected,
                    got: args.len(),
                });
            }
            return match cap {
                "io.print" => {
                    let s: String = args.iter().map(value_to_text).collect::<Vec<_>>().concat();
                    vm.out_len += s.len();
                    if vm.out_len > vm.quotas.max_output {
                        return Err(VmError::OutputQuota(vm.quotas.max_output));
                    }
                    vm.out_parts.push(s);
                    Ok(None)
                }
                "str.concat" => {
                    let s: String = args.iter().map(value_to_text).collect::<Vec<_>>().concat();
                    Ok(Some(Value::Str(s)))
                }
                "str.len" => {
                    let s = value_to_text(&args[0]);
                    Ok(Some(Value::Int(s.len() as i64)))
                }
                "str.contains" => {
                    let haystack = value_to_text(&args[0]);
                    let needle = value_to_text(&args[1]);
                    Ok(Some(Value::Bool(haystack.contains(&needle))))
                }
                "str.split" => {
                    let s = value_to_text(&args[0]);
                    let delim = value_to_text(&args[1]);
                    let parts: Vec<Value> = if delim.is_empty() {
                        s.chars().map(|c| Value::Str(c.to_string())).collect()
                    } else {
                        s.split(&delim).map(|p| Value::Str(p.to_string())).collect()
                    };
                    Ok(Some(Value::new_array(parts)))
                }
                "str.replace" => {
                    let s = value_to_text(&args[0]);
                    let from = value_to_text(&args[1]);
                    let to = value_to_text(&args[2]);
                    Ok(Some(Value::Str(s.replace(&from, &to))))
                }
                "str.join" => {
                    let delim = value_to_text(&args[1]);
                    match &args[0] {
                        Value::Array(elems) => {
                            let parts: Vec<String> =
                                elems.borrow().iter().map(|v| value_to_text(v)).collect();
                            Ok(Some(Value::Str(parts.join(&delim))))
                        }
                        other => Err(VmError::TypeError {
                            expected: "array",
                            got: value_type_name(other),
                        }),
                    }
                }
                "make_range" => {
                    let start = match &args[0] {
                        Value::Int(i) => *i,
                        other => {
                            return Err(VmError::TypeError {
                                expected: "int",
                                got: value_type_name(other),
                            });
                        }
                    };
                    let end = match &args[1] {
                        Value::Int(i) => *i,
                        other => {
                            return Err(VmError::TypeError {
                                expected: "int",
                                got: value_type_name(other),
                            });
                        }
                    };
                    let mut elems = Vec::new();
                    if start < end {
                        for i in start..end {
                            elems.push(Value::Int(i));
                        }
                    }
                    Ok(Some(Value::new_array(elems)))
                }
                _ => Err(VmError::UnknownCap(cap.to_string())),
            };
        }

        // Privilege check for host-provided capabilities
        if !vm.privileged_allowed && crate::caps::is_privileged(cap) {
            return Err(VmError::CapDenied(format!(
                "privileged cap requires elevated grant: {cap}"
            )));
        }

        // Host-provided capabilities
        if let Some(host) = &vm.host_caps
            && let Some(handler) = host.get(cap)
        {
            let spec = handler.spec();
            if let Some(expected) = spec.argc
                && args.len() != expected
            {
                return Err(VmError::CapArity {
                    cap: cap.to_string(),
                    expected,
                    got: args.len(),
                });
            }
            return handler
                .call(args)
                .map_err(|msg| VmError::UnknownCap(format!("{cap}: {msg}")));
        }

        Err(VmError::UnknownCap(cap.to_string()))
}
