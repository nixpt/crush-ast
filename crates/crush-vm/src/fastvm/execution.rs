//! Instruction execution logic for FastVM.

use super::instructions::SymbolTables;
use super::instructions::{FastInstr, FastOp};
use super::operations::{binary_op, compare_op, current_locals_base, is_truthy};
use super::similarity::calculate_similarity;
use super::types::{FastError, FastFrame, FastYield, HostRequest};
use crate::memory::{Arena, Object};
use crate::value::RuntimeValue;
use super::Capability;
use std::sync::Arc;

/// Execute a single instruction
#[inline(always)]
pub fn execute_one(
    instr: FastInstr,
    pc: &mut usize,
    stack: &mut Vec<RuntimeValue>,
    locals: &mut Vec<RuntimeValue>,
    call_stack: &mut Vec<FastFrame>,
    symbols: &SymbolTables,
    capabilities: &[Arc<dyn Capability>],
    hal: &Arc<dyn crate::fastvm::Hal>,
    arena: &mut Arena,
) -> Result<Option<FastYield>, FastError> {
    match instr.op {
        // ===== Stack operations =====
        FastOp::PushInt => {
            stack.push(RuntimeValue::Int(instr.arg as i64));
        }

        FastOp::PushFloat => {
            stack.push(RuntimeValue::Float(f64::from_bits(instr.arg)));
        }

        FastOp::PushBool => {
            stack.push(RuntimeValue::Bool(instr.arg != 0));
        }

        FastOp::PushNull => {
            stack.push(RuntimeValue::Null);
        }

        FastOp::PushStr => {
            let s = &symbols.strings[instr.arg as usize];
            let ptr = arena.alloc(Object::Str(s.clone()));
            stack.push(RuntimeValue::Ref(ptr));
        }

        FastOp::Pop => {
            stack.pop().ok_or(FastError::StackUnderflow)?;
        }

        FastOp::Dup => {
            let val = stack.last().ok_or(FastError::StackUnderflow)?.clone();
            stack.push(val);
        }

        // ===== Local variable access =====
        FastOp::LoadLocal => {
            let base = current_locals_base(call_stack);
            let idx = base + instr.arg as usize;
            let val = locals.get(idx).cloned().unwrap_or(RuntimeValue::Null);
            stack.push(val);
        }

        FastOp::StoreLocal => {
            let val = stack.pop().ok_or(FastError::StackUnderflow)?;
            let base = current_locals_base(call_stack);
            let idx = base + instr.arg as usize;

            // Extend locals if needed
            while locals.len() <= idx {
                locals.push(RuntimeValue::Null);
            }
            locals[idx] = val;
        }

        // ===== Control flow =====
        FastOp::Jump => {
            *pc = instr.arg as usize;
        }

        FastOp::JumpIf => {
            let cond = stack.pop().ok_or(FastError::StackUnderflow)?;
            if is_truthy(&cond) {
                *pc = instr.arg as usize;
            }
        }

        FastOp::JumpIfNot => {
            let cond = stack.pop().ok_or(FastError::StackUnderflow)?;
            if !is_truthy(&cond) {
                *pc = instr.arg as usize;
            }
        }

        FastOp::Call => {
            let func_name = &symbols.strings[instr.arg as usize];
            let (target_pc, _, _arity) = symbols
                .functions
                .get(func_name)
                .copied()
                .ok_or(FastError::InvalidFunction(instr.arg as u32))?;

            let argc = instr.arg2 as usize;
            let locals_base = locals.len();

            // Pop args from stack (top = last pushed = last arg)
            if stack.len() < argc {
                return Err(FastError::StackUnderflow);
            }
            let split_at = stack.len() - argc;
            let call_args: Vec<RuntimeValue> = stack.drain(split_at..).collect();
            // call_args is [first_arg, ..., last_arg]
            // Callee's 'store param1' pops from top, so first_arg must be on top.
            // Push last_arg first, ..., first_arg last:
            for arg in call_args.iter().rev() {
                stack.push(arg.clone());
            }

            // Push call frame
            call_stack.push(FastFrame {
                return_pc: *pc,
                locals_base,
                locals_count: argc,
                handlers: Vec::new(),
            });

            *pc = target_pc;
        }

        FastOp::Return => {
            if let Some(frame) = call_stack.pop() {
                // Truncate locals to caller's frame
                locals.truncate(frame.locals_base);
                *pc = frame.return_pc;
            } else {
                // Return from main
                let result = stack.pop();
                return Ok(Some(FastYield::Finished(result)));
            }
        }

        // ===== Capability calls =====
        FastOp::CapCall => {
            let cap_idx = instr.arg as usize;
            let argc = instr.arg2 as usize;

            let cap = capabilities
                .get(cap_idx)
                .ok_or(FastError::InvalidCapability(cap_idx as u32))?
                .clone();

            // Collect args from stack
            let mut args = Vec::with_capacity(argc);
            for _ in 0..argc {
                args.push(stack.pop().ok_or(FastError::StackUnderflow)?);
            }
            args.reverse();

            // Call capability (this is where we exit the fast path to native code)
            match cap.call(arena, args, hal.clone()) {
                Ok(result) => stack.push(result),
                Err(e) => {
                    // TODO: Better error handling
                    let err_str = arena.alloc(Object::Str(e.to_string()));
                    stack.push(RuntimeValue::Ref(err_str));
                }
            }
        }

        // ===== Arithmetic =====
        FastOp::Add => binary_op(stack, |a, b| a + b, |a, b| a + b)?,
        FastOp::Sub => binary_op(stack, |a, b| a - b, |a, b| a - b)?,
        FastOp::Mul => binary_op(stack, |a, b| a * b, |a, b| a * b)?,
        FastOp::Div => {
            let b = stack.pop().ok_or(FastError::StackUnderflow)?;
            let a = stack.pop().ok_or(FastError::StackUnderflow)?;
            match (&a, &b) {
                (RuntimeValue::Int(x), RuntimeValue::Int(y)) => {
                    if *y == 0 {
                        return Err(FastError::DivisionByZero);
                    }
                    stack.push(RuntimeValue::Int(x / y));
                }
                (RuntimeValue::Float(x), RuntimeValue::Float(y)) => {
                    stack.push(RuntimeValue::Float(x / y));
                }
                (RuntimeValue::Int(x), RuntimeValue::Float(y)) => {
                    stack.push(RuntimeValue::Float(*x as f64 / y));
                }
                (RuntimeValue::Float(x), RuntimeValue::Int(y)) => {
                    stack.push(RuntimeValue::Float(x / *y as f64));
                }
                _ => return Err(FastError::TypeMismatch),
            }
        }
        FastOp::Mod => {
            let b = stack.pop().ok_or(FastError::StackUnderflow)?;
            let a = stack.pop().ok_or(FastError::StackUnderflow)?;
            match (&a, &b) {
                (RuntimeValue::Int(x), RuntimeValue::Int(y)) => {
                    if *y == 0 {
                        return Err(FastError::DivisionByZero);
                    }
                    stack.push(RuntimeValue::Int(x % y));
                }
                _ => return Err(FastError::TypeMismatch),
            }
        }
        FastOp::Neg => {
            let a = stack.pop().ok_or(FastError::StackUnderflow)?;
            match a {
                RuntimeValue::Int(x) => stack.push(RuntimeValue::Int(-x)),
                RuntimeValue::Float(x) => stack.push(RuntimeValue::Float(-x)),
                _ => return Err(FastError::TypeMismatch),
            }
        }

        // ===== Comparison =====
        FastOp::Eq => compare_op(stack, |a, b| a == b, |a, b| a == b)?,
        FastOp::Ne => compare_op(stack, |a, b| a != b, |a, b| a != b)?,
        FastOp::Lt => compare_op(stack, |a, b| a < b, |a, b| a < b)?,
        FastOp::Le => compare_op(stack, |a, b| a <= b, |a, b| a <= b)?,
        FastOp::Gt => compare_op(stack, |a, b| a > b, |a, b| a > b)?,
        FastOp::Ge => compare_op(stack, |a, b| a >= b, |a, b| a >= b)?,

        // ===== Logical =====
        FastOp::And => {
            let b = stack.pop().ok_or(FastError::StackUnderflow)?;
            let a = stack.pop().ok_or(FastError::StackUnderflow)?;
            stack.push(RuntimeValue::Bool(is_truthy(&a) && is_truthy(&b)));
        }
        FastOp::Or => {
            let b = stack.pop().ok_or(FastError::StackUnderflow)?;
            let a = stack.pop().ok_or(FastError::StackUnderflow)?;
            stack.push(RuntimeValue::Bool(is_truthy(&a) || is_truthy(&b)));
        }
        FastOp::Not => {
            let a = stack.pop().ok_or(FastError::StackUnderflow)?;
            stack.push(RuntimeValue::Bool(!is_truthy(&a)));
        }

        // ===== Bitwise =====
        FastOp::BitAnd => {
            let b = stack.pop().ok_or(FastError::StackUnderflow)?;
            let a = stack.pop().ok_or(FastError::StackUnderflow)?;
            match (&a, &b) {
                (RuntimeValue::Int(x), RuntimeValue::Int(y)) => {
                    stack.push(RuntimeValue::Int(x & y));
                }
                _ => return Err(FastError::TypeMismatch),
            }
        }
        FastOp::BitOr => {
            let b = stack.pop().ok_or(FastError::StackUnderflow)?;
            let a = stack.pop().ok_or(FastError::StackUnderflow)?;
            match (&a, &b) {
                (RuntimeValue::Int(x), RuntimeValue::Int(y)) => {
                    stack.push(RuntimeValue::Int(x | y));
                }
                _ => return Err(FastError::TypeMismatch),
            }
        }
        FastOp::BitXor => {
            let b = stack.pop().ok_or(FastError::StackUnderflow)?;
            let a = stack.pop().ok_or(FastError::StackUnderflow)?;
            match (&a, &b) {
                (RuntimeValue::Int(x), RuntimeValue::Int(y)) => {
                    stack.push(RuntimeValue::Int(x ^ y));
                }
                _ => return Err(FastError::TypeMismatch),
            }
        }
        FastOp::BitNot => {
            let a = stack.pop().ok_or(FastError::StackUnderflow)?;
            match a {
                RuntimeValue::Int(x) => stack.push(RuntimeValue::Int(!x)),
                _ => return Err(FastError::TypeMismatch),
            }
        }
        FastOp::Shl => {
            let b = stack.pop().ok_or(FastError::StackUnderflow)?;
            let a = stack.pop().ok_or(FastError::StackUnderflow)?;
            match (&a, &b) {
                (RuntimeValue::Int(x), RuntimeValue::Int(y)) => {
                    stack.push(RuntimeValue::Int(x << (*y as u32)));
                }
                _ => return Err(FastError::TypeMismatch),
            }
        }
        FastOp::Shr => {
            let b = stack.pop().ok_or(FastError::StackUnderflow)?;
            let a = stack.pop().ok_or(FastError::StackUnderflow)?;
            match (&a, &b) {
                (RuntimeValue::Int(x), RuntimeValue::Int(y)) => {
                    stack.push(RuntimeValue::Int(x >> (*y as u32)));
                }
                _ => return Err(FastError::TypeMismatch),
            }
        }

        // ===== Stack manipulation =====
        FastOp::Swap => {
            let len = stack.len();
            if len < 2 {
                return Err(FastError::StackUnderflow);
            }
            stack.swap(len - 1, len - 2);
        }
        FastOp::Rot => {
            // Rotate top 3: [a, b, c] -> [b, c, a]
            let len = stack.len();
            if len < 3 {
                return Err(FastError::StackUnderflow);
            }
            stack.swap(len - 3, len - 2);
            stack.swap(len - 2, len - 1);
        }
        FastOp::Pick => {
            let n = instr.arg as usize;
            let len = stack.len();
            if n >= len {
                return Err(FastError::StackUnderflow);
            }
            let val = stack[len - 1 - n].clone();
            stack.push(val);
        }
        FastOp::Roll => {
            let n = instr.arg as usize;
            let len = stack.len();
            if n >= len {
                return Err(FastError::StackUnderflow);
            }
            let val = stack.remove(len - 1 - n);
            stack.push(val);
        }

        // ===== Loop control =====
        FastOp::Break => {
            // Break target should be patched by compiler to a jump target
            // If arg is set, jump there; otherwise treat as halt (shouldn't happen in valid code)
            if instr.arg != 0 {
                *pc = instr.arg as usize;
            }
        }
        FastOp::Continue => {
            if instr.arg != 0 {
                *pc = instr.arg as usize;
            }
        }

        // ===== Type operations =====
        FastOp::TypeOf => {
            let val = stack.pop().ok_or(FastError::StackUnderflow)?;
            let type_name = match &val {
                RuntimeValue::Int(_) => "int",
                RuntimeValue::Float(_) => "float",
                RuntimeValue::Bool(_) => "bool",
                RuntimeValue::Null => "null",
                RuntimeValue::Ref(ptr) => match arena.get(*ptr) {
                    Some(Object::Str(_)) => "string",
                    Some(Object::Array(_)) => "array",
                    Some(Object::Map(_)) => "map",
                    Some(Object::Object { .. }) => "object",
                    Some(Object::Bytes(_)) => "bytes",
                    Some(Object::Buffer(_)) => "buffer",
                    _ => "unknown",
                },
                _ => "unknown",
            };
            let ptr = arena.alloc(Object::Str(type_name.to_string()));
            stack.push(RuntimeValue::Ref(ptr));
        }
        FastOp::Cast => {
            let type_name = &symbols.strings[instr.arg as usize];
            let val = stack.pop().ok_or(FastError::StackUnderflow)?;
            let casted = match type_name.as_str() {
                "int" => match &val {
                    RuntimeValue::Int(_) => val,
                    RuntimeValue::Float(f) => RuntimeValue::Int(*f as i64),
                    RuntimeValue::Bool(b) => RuntimeValue::Int(if *b { 1 } else { 0 }),
                    RuntimeValue::Ref(ptr) => match arena.get(*ptr) {
                        Some(Object::Str(s)) => RuntimeValue::Int(s.parse::<i64>().unwrap_or(0)),
                        _ => return Err(FastError::TypeMismatch),
                    },
                    _ => return Err(FastError::TypeMismatch),
                },
                "float" => match &val {
                    RuntimeValue::Float(_) => val,
                    RuntimeValue::Int(i) => RuntimeValue::Float(*i as f64),
                    RuntimeValue::Bool(b) => RuntimeValue::Float(if *b { 1.0 } else { 0.0 }),
                    RuntimeValue::Ref(ptr) => match arena.get(*ptr) {
                        Some(Object::Str(s)) => {
                            RuntimeValue::Float(s.parse::<f64>().unwrap_or(0.0))
                        }
                        _ => return Err(FastError::TypeMismatch),
                    },
                    _ => return Err(FastError::TypeMismatch),
                },
                "string" => {
                    let s = match &val {
                        RuntimeValue::Int(i) => i.to_string(),
                        RuntimeValue::Float(f) => f.to_string(),
                        RuntimeValue::Bool(b) => b.to_string(),
                        RuntimeValue::Null => "null".to_string(),
                        RuntimeValue::Ref(ptr) => match arena.get(*ptr) {
                            Some(Object::Str(s)) => s.clone(),
                            _ => return Err(FastError::TypeMismatch),
                        },
                        _ => return Err(FastError::TypeMismatch),
                    };
                    let ptr = arena.alloc(Object::Str(s));
                    RuntimeValue::Ref(ptr)
                }
                "bool" => RuntimeValue::Bool(is_truthy(&val)),
                _ => return Err(FastError::TypeMismatch),
            };
            stack.push(casted);
        }

        // ===== Structured data =====
        FastOp::MakeList => {
            let count = instr.arg as usize;
            let mut items = Vec::with_capacity(count);
            for _ in 0..count {
                items.push(stack.pop().ok_or(FastError::StackUnderflow)?);
            }
            items.reverse();
            let ptr = arena.alloc(Object::Array(items));
            stack.push(RuntimeValue::Ref(ptr));
        }

        FastOp::MakeMap => {
            let count = instr.arg as usize;
            let mut map = std::collections::HashMap::new();
            for _ in 0..count {
                let val = stack.pop().ok_or(FastError::StackUnderflow)?;
                let key = stack.pop().ok_or(FastError::StackUnderflow)?;
                if let RuntimeValue::Ref(ptr) = &key {
                    if let Some(Object::Str(s)) = arena.get(*ptr) {
                        map.insert(s.clone(), val);
                    }
                }
            }
            let ptr = arena.alloc(Object::Map(map));
            stack.push(RuntimeValue::Ref(ptr));
        }

        FastOp::Index => {
            let key = stack.pop().ok_or(FastError::StackUnderflow)?;
            let container = stack.pop().ok_or(FastError::StackUnderflow)?;

            if let RuntimeValue::Ref(ptr) = container {
                match arena.get(ptr) {
                    Some(Object::Array(items)) => {
                        if let RuntimeValue::Int(idx) = key {
                            let val = items
                                .get(idx as usize)
                                .cloned()
                                .unwrap_or(RuntimeValue::Null);
                            stack.push(val);
                        } else {
                            stack.push(RuntimeValue::Null);
                        }
                    }
                    Some(Object::Map(map)) => {
                        if let RuntimeValue::Ref(key_ptr) = &key {
                            if let Some(Object::Str(s)) = arena.get(*key_ptr) {
                                let val = map.get(s).cloned().unwrap_or(RuntimeValue::Null);
                                stack.push(val);
                            } else {
                                stack.push(RuntimeValue::Null);
                            }
                        } else {
                            stack.push(RuntimeValue::Null);
                        }
                    }
                    Some(Object::Str(s)) => {
                        if let RuntimeValue::Int(idx) = key {
                            if let Some(c) = s.chars().nth(idx as usize) {
                                let c_ptr = arena.alloc(Object::Str(c.to_string()));
                                stack.push(RuntimeValue::Ref(c_ptr));
                            } else {
                                stack.push(RuntimeValue::Null);
                            }
                        } else {
                            stack.push(RuntimeValue::Null);
                        }
                    }
                    _ => stack.push(RuntimeValue::Null),
                }
            } else {
                stack.push(RuntimeValue::Null);
            }
        }

        FastOp::Len => {
            let val = stack.pop().ok_or(FastError::StackUnderflow)?;
            if let RuntimeValue::Ref(ptr) = val {
                let len = match arena.get(ptr) {
                    Some(Object::Array(a)) => a.len(),
                    Some(Object::Map(m)) => m.len(),
                    Some(Object::Str(s)) => s.len(),
                    Some(Object::Bytes(b)) => b.len(),
                    Some(Object::Buffer(b)) => b.len(),
                    _ => return Err(FastError::TypeMismatch),
                };
                stack.push(RuntimeValue::Int(len as i64));
            } else {
                return Err(FastError::TypeMismatch);
            }
        }

        FastOp::MakeRange => {
            let end = stack.pop().ok_or(FastError::StackUnderflow)?;
            let start = stack.pop().ok_or(FastError::StackUnderflow)?;
            if let (RuntimeValue::Int(s), RuntimeValue::Int(e)) = (start, end) {
                let range = (s..e).map(RuntimeValue::Int).collect();
                let ptr = arena.alloc(Object::Array(range));
                stack.push(RuntimeValue::Ref(ptr));
            } else {
                return Err(FastError::TypeMismatch);
            }
        }

        FastOp::ArrayPop => {
            let arr_ref = stack.pop().ok_or(FastError::StackUnderflow)?;
            if let RuntimeValue::Ref(ptr) = arr_ref {
                if let Ok(Object::Array(arr)) = arena.get_mut(ptr) {
                    if let Some(val) = arr.pop() {
                        stack.push(val);
                    } else {
                        stack.push(RuntimeValue::Null);
                    }
                } else {
                    return Err(FastError::TypeMismatch);
                }
            } else {
                return Err(FastError::TypeMismatch);
            }
        }

        FastOp::ArrayPush => {
            let val = stack.pop().ok_or(FastError::StackUnderflow)?;
            let arr_ref = stack.pop().ok_or(FastError::StackUnderflow)?;
            if let RuntimeValue::Ref(ptr) = arr_ref {
                if let Ok(Object::Array(arr)) = arena.get_mut(ptr) {
                    arr.push(val);
                } else {
                    return Err(FastError::TypeMismatch);
                }
            } else {
                return Err(FastError::TypeMismatch);
            }
        }

        FastOp::TuplePush => {
            let val = stack.pop().ok_or(FastError::StackUnderflow)?;
            let arr_ref = stack.pop().ok_or(FastError::StackUnderflow)?;
            if let RuntimeValue::Ref(ptr) = arr_ref {
                if let Ok(Object::Tuple(arr)) = arena.get_mut(ptr) {
                    arr.push(val);
                } else {
                    return Err(FastError::TypeMismatch);
                }
            } else {
                return Err(FastError::TypeMismatch);
            }
        }

        FastOp::ListPush => {
            let val = stack.pop().ok_or(FastError::StackUnderflow)?;
            let arr_ref = stack.pop().ok_or(FastError::StackUnderflow)?;
            if let RuntimeValue::Ref(ptr) = arr_ref {
                if let Ok(Object::List(arr)) = arena.get_mut(ptr) {
                    arr.push_back(val);
                } else {
                    return Err(FastError::TypeMismatch);
                }
            } else {
                return Err(FastError::TypeMismatch);
            }
        }

        FastOp::VectorPush => {
            let val = stack.pop().ok_or(FastError::StackUnderflow)?;
            let arr_ref = stack.pop().ok_or(FastError::StackUnderflow)?;
            if let RuntimeValue::Ref(ptr) = arr_ref {
                if let Ok(Object::Vector(arr)) = arena.get_mut(ptr) {
                    arr.push(val);
                } else {
                    return Err(FastError::TypeMismatch);
                }
            } else {
                return Err(FastError::TypeMismatch);
            }
        }

        FastOp::SetPush => {
            let val = stack.pop().ok_or(FastError::StackUnderflow)?;
            let arr_ref = stack.pop().ok_or(FastError::StackUnderflow)?;
            if let RuntimeValue::Ref(ptr) = arr_ref {
                if let Ok(Object::Set(arr)) = arena.get_mut(ptr) {
                    arr.push(val);
                } else {
                    return Err(FastError::TypeMismatch);
                }
            } else {
                return Err(FastError::TypeMismatch);
            }
        }

        FastOp::NewObj => {
            let ptr = arena.alloc(Object::Object {
                lang: "crush".to_string(),
                class_name: "Object".to_string(),
                fields: std::collections::HashMap::new(),
            });
            stack.push(RuntimeValue::Ref(ptr));
        }

        FastOp::NewStruct => {
            let name = &symbols.strings[instr.arg as usize];
            let ptr = arena.alloc(Object::Object {
                lang: "crush".to_string(),
                class_name: name.clone(),
                fields: std::collections::HashMap::new(),
            });
            stack.push(RuntimeValue::Ref(ptr));
        }

        FastOp::GetField => {
            let name = &symbols.strings[instr.arg as usize];
            let target = stack.pop().ok_or(FastError::StackUnderflow)?;
            if let RuntimeValue::Ref(ptr) = target {
                if let Some(Object::Object { fields, .. }) = arena.get(ptr) {
                    stack.push(fields.get(name).cloned().unwrap_or(RuntimeValue::Null));
                } else {
                    stack.push(RuntimeValue::Null);
                }
            } else {
                stack.push(RuntimeValue::Null);
            }
        }

        FastOp::SetField => {
            let name = &symbols.strings[instr.arg as usize];
            let val = stack.pop().ok_or(FastError::StackUnderflow)?;
            let target = stack.pop().ok_or(FastError::StackUnderflow)?;
            if let RuntimeValue::Ref(ptr) = target {
                if let Ok(Object::Object { fields, .. }) = arena.get_mut(ptr) {
                    fields.insert(name.clone(), val);
                } else {
                    return Err(FastError::TypeMismatch);
                }
            } else {
                return Err(FastError::TypeMismatch);
            }
        }

        FastOp::NewArray => {
            let size = instr.arg as usize;
            let ptr = arena.alloc(Object::Array(Vec::with_capacity(size)));
            stack.push(RuntimeValue::Ref(ptr));
        }

        FastOp::NewTuple => {
            let size = instr.arg as usize;
            let ptr = arena.alloc(Object::Tuple(Vec::with_capacity(size)));
            stack.push(RuntimeValue::Ref(ptr));
        }
        FastOp::NewList => {
            let _size = instr.arg as usize;
            let ptr = arena.alloc(Object::List(std::collections::LinkedList::new()));
            stack.push(RuntimeValue::Ref(ptr));
        }
        FastOp::NewVector => {
            let size = instr.arg as usize;
            let ptr = arena.alloc(Object::Vector(Vec::with_capacity(size)));
            stack.push(RuntimeValue::Ref(ptr));
        }
        FastOp::NewSet => {
            let size = instr.arg as usize;
            let ptr = arena.alloc(Object::Set(Vec::with_capacity(size)));
            stack.push(RuntimeValue::Ref(ptr));
        }

        // ===== String Ops =====
        FastOp::StrContains => {
            let pattern_val = stack.pop().ok_or(FastError::StackUnderflow)?;
            let str_val = stack.pop().ok_or(FastError::StackUnderflow)?;

            let pattern = match pattern_val {
                RuntimeValue::Ref(p) => match arena.get(p) {
                    Some(Object::Str(s)) => s.clone(),
                    _ => return Err(FastError::TypeMismatch),
                },
                _ => return Err(FastError::TypeMismatch),
            };

            let res = match str_val {
                RuntimeValue::Ref(p) => match arena.get(p) {
                    Some(Object::Str(s)) => s.contains(&pattern),
                    _ => return Err(FastError::TypeMismatch),
                },
                _ => return Err(FastError::TypeMismatch),
            };
            stack.push(RuntimeValue::Bool(res));
        }

        FastOp::StrSplit => {
            let delim_val = stack.pop().ok_or(FastError::StackUnderflow)?;
            let str_val = stack.pop().ok_or(FastError::StackUnderflow)?;

            let delim = match delim_val {
                RuntimeValue::Ref(p) => match arena.get(p) {
                    Some(Object::Str(s)) => s.clone(),
                    _ => return Err(FastError::TypeMismatch),
                },
                _ => return Err(FastError::TypeMismatch),
            };

            let target_string = if let RuntimeValue::Ref(ptr) = str_val {
                if let Some(Object::Str(s)) = arena.get(ptr) {
                    s.clone()
                } else {
                    return Err(FastError::TypeMismatch);
                }
            } else {
                return Err(FastError::TypeMismatch);
            };

            let parts: Vec<RuntimeValue> = target_string
                .split(&delim)
                .map(|p| {
                    let sp = arena.alloc(Object::Str(p.to_string()));
                    RuntimeValue::Ref(sp)
                })
                .collect();
            let ptr = arena.alloc(Object::Array(parts));
            stack.push(RuntimeValue::Ref(ptr));
        }

        FastOp::StrReplace => {
            let new_val = stack.pop().ok_or(FastError::StackUnderflow)?;
            let old_val = stack.pop().ok_or(FastError::StackUnderflow)?;
            let str_val = stack.pop().ok_or(FastError::StackUnderflow)?;

            let new_str = match new_val {
                RuntimeValue::Ref(p) => match arena.get(p) {
                    Some(Object::Str(s)) => s.clone(),
                    _ => return Err(FastError::TypeMismatch),
                },
                _ => return Err(FastError::TypeMismatch),
            };

            let old_str = match old_val {
                RuntimeValue::Ref(p) => match arena.get(p) {
                    Some(Object::Str(s)) => s.clone(),
                    _ => return Err(FastError::TypeMismatch),
                },
                _ => return Err(FastError::TypeMismatch),
            };

            let target_string = if let RuntimeValue::Ref(ptr) = str_val {
                if let Some(Object::Str(s)) = arena.get(ptr) {
                    s.clone()
                } else {
                    return Err(FastError::TypeMismatch);
                }
            } else {
                return Err(FastError::TypeMismatch);
            };

            let replaced = target_string.replace(&old_str, &new_str);
            let res_ptr = arena.alloc(Object::Str(replaced));
            stack.push(RuntimeValue::Ref(res_ptr));
        }

        FastOp::StrJoin => {
            let delim_val = stack.pop().ok_or(FastError::StackUnderflow)?;
            let arr_val = stack.pop().ok_or(FastError::StackUnderflow)?;

            let delim = match delim_val {
                RuntimeValue::Ref(p) => match arena.get(p) {
                    Some(Object::Str(s)) => s.clone(),
                    _ => return Err(FastError::TypeMismatch),
                },
                _ => return Err(FastError::TypeMismatch),
            };

            let mut strings = Vec::new();
            if let RuntimeValue::Ref(ptr) = arr_val {
                if let Some(Object::Array(arr)) = arena.get(ptr) {
                    for val in arr {
                        if let RuntimeValue::Ref(sp) = val {
                            // Clone string content to avoid long borrow
                            if let Some(Object::Str(s)) = arena.get(*sp) {
                                strings.push(s.clone());
                            } else {
                                return Err(FastError::TypeMismatch);
                            }
                        } else {
                            return Err(FastError::TypeMismatch);
                        }
                    }
                } else {
                    return Err(FastError::TypeMismatch);
                }
            } else {
                return Err(FastError::TypeMismatch);
            };

            let joined = strings.join(&delim);
            let res_ptr = arena.alloc(Object::Str(joined));
            stack.push(RuntimeValue::Ref(res_ptr));
        }

        FastOp::StrSim => {
            let s2_val = stack.pop().ok_or(FastError::StackUnderflow)?;
            let s1_val = stack.pop().ok_or(FastError::StackUnderflow)?;

            let s1 = match s1_val {
                RuntimeValue::Ref(p) => match arena.get(p) {
                    Some(Object::Str(s)) => s,
                    _ => return Err(FastError::TypeMismatch),
                },
                _ => return Err(FastError::TypeMismatch),
            };

            let s2 = match s2_val {
                RuntimeValue::Ref(p) => match arena.get(p) {
                    Some(Object::Str(s)) => s,
                    _ => return Err(FastError::TypeMismatch),
                },
                _ => return Err(FastError::TypeMismatch),
            };

            let sim = calculate_similarity(s1, s2);
            stack.push(RuntimeValue::Float(sim));
        }

        // ===== Exceptions =====
        FastOp::EnterTry => {
            let target_pc = instr.arg as usize;
            if let Some(frame) = call_stack.last_mut() {
                frame.handlers.push(target_pc);
            }
        }

        FastOp::ExitTry => {
            if let Some(frame) = call_stack.last_mut() {
                frame.handlers.pop();
            }
        }

        FastOp::Throw => {
            let err_val = stack.pop().ok_or(FastError::StackUnderflow)?;
            let mut caught = false;
            let mut target_frame_index = 0;

            for (i, frame) in call_stack.iter().enumerate().rev() {
                if !frame.handlers.is_empty() {
                    caught = true;
                    target_frame_index = i;
                    break;
                }
            }

            if caught {
                call_stack.truncate(target_frame_index + 1);
                let frame = call_stack.last_mut().unwrap();
                let target_pc = frame.handlers.pop().unwrap();
                *pc = target_pc;
                stack.push(err_val);
            } else {
                return Err(FastError::Unimplemented(
                    "Uncaught exception (no handler)".to_string(),
                ));
            }
        }

        // ===== VM control =====
        FastOp::Yield => {
            return Ok(Some(FastYield::Yielded));
        }

        FastOp::Halt => {
            let result = stack.pop();
            return Ok(Some(FastYield::Finished(result)));
        }

        FastOp::Nop => {}

        FastOp::Gc => return Ok(Some(FastYield::Request(HostRequest::Gc))),

        FastOp::Spawn => {
            let func_val = stack.pop().ok_or(FastError::StackUnderflow)?;
            let func_name = match func_val {
                RuntimeValue::Ref(p) => match arena.get(p) {
                    Some(Object::Str(s)) => s.clone(),
                    _ => return Err(FastError::TypeMismatch),
                },
                _ => return Err(FastError::TypeMismatch),
            };
            return Ok(Some(FastYield::Request(HostRequest::Spawn {
                func: func_name,
            })));
        }

        FastOp::Restart => {
            let id_val = stack.pop().ok_or(FastError::StackUnderflow)?;
            let task_id = match id_val {
                RuntimeValue::Int(i) => i as usize,
                _ => return Err(FastError::TypeMismatch),
            };
            return Ok(Some(FastYield::Request(HostRequest::Restart { task_id })));
        }

        FastOp::Watchdog => {
            let action_val = stack.pop().ok_or(FastError::StackUnderflow)?;
            let deadline_val = stack.pop().ok_or(FastError::StackUnderflow)?;
            let task_id_val = stack.pop().ok_or(FastError::StackUnderflow)?;

            let task_id = match task_id_val {
                RuntimeValue::Int(i) => i as usize,
                _ => return Err(FastError::TypeMismatch),
            };
            let deadline = match deadline_val {
                RuntimeValue::Int(i) => i as u64,
                _ => return Err(FastError::TypeMismatch),
            };
            let action = match action_val {
                RuntimeValue::Ref(p) => match arena.get(p) {
                    Some(Object::Str(s)) => s.clone(),
                    _ => return Err(FastError::TypeMismatch),
                },
                _ => return Err(FastError::TypeMismatch),
            };

            return Ok(Some(FastYield::Request(HostRequest::Watchdog {
                task_id,
                deadline,
                action,
            })));
        }

        // Variables
        FastOp::ExportVar => {
            let name = symbols.strings[instr.arg as usize].clone();
            let val = stack.pop().ok_or(FastError::StackUnderflow)?;
            return Ok(Some(FastYield::Request(HostRequest::ExportVar {
                name,
                value: val,
            })));
        }

        FastOp::ImportVar => {
            let name = symbols.strings[instr.arg as usize].clone();
            return Ok(Some(FastYield::Request(HostRequest::ImportVar { name })));
        }

        // Host interaction
        FastOp::CallHost => {
            let site = &symbols.host_calls[instr.arg as usize];
            let capsule_name = symbols.strings[site.capsule_idx as usize].clone();
            let method_name = symbols.strings[site.method_idx as usize].clone();

            let mut args = Vec::with_capacity(site.argc as usize);
            for _ in 0..site.argc {
                args.push(stack.pop().ok_or(FastError::StackUnderflow)?);
            }
            args.reverse();

            return Ok(Some(FastYield::Request(HostRequest::CallHost {
                capsule_name,
                method_name,
                ic_id: site.ic_id,
                args,
            })));
        }

        FastOp::CallInterface => {
            let site = &symbols.interface_calls[instr.arg as usize];
            let method_name = symbols.strings[site.method_idx as usize].clone();

            // Get handle from local variable
            let locals_base = current_locals_base(call_stack);
            let handle_idx = locals_base + site.handle_var_idx as usize;
            let handle = locals
                .get(handle_idx)
                .cloned()
                .unwrap_or(RuntimeValue::Null);

            let mut args = Vec::with_capacity(site.argc as usize);
            for _ in 0..site.argc {
                args.push(stack.pop().ok_or(FastError::StackUnderflow)?);
            }
            args.reverse();

            return Ok(Some(FastYield::Request(HostRequest::CallInterface {
                handle,
                method_name,
                args,
            })));
        }

        FastOp::ExecLang => {
            let site = &symbols.exec_lang_calls[instr.arg as usize];
            let lang = symbols.strings[site.lang_idx as usize].clone();
            let code = symbols.strings[site.code_idx as usize].clone();

            let mut variables = std::collections::HashMap::new();

            let locals_base = current_locals_base(call_stack);
            for name_idx in &site.var_names {
                let name_str = &symbols.strings[*name_idx as usize];
                // lookup local slot
                if let Some(&local_slot) = symbols.locals.get(name_str) {
                    let val_idx = locals_base + local_slot as usize;
                    if let Some(val) = locals.get(val_idx) {
                        variables.insert(name_str.clone(), val.clone());
                    }
                }
            }

            return Ok(Some(FastYield::Request(HostRequest::ExecLang {
                lang,
                code,
                variables,
            })));
        }

        FastOp::Await => {
            let event_val = stack.pop().ok_or(FastError::StackUnderflow)?;
            let event_id = match event_val {
                RuntimeValue::Ref(p) => match arena.get(p) {
                    Some(Object::Str(s)) => s.clone(),
                    _ => return Err(FastError::TypeMismatch),
                },
                _ => return Err(FastError::TypeMismatch),
            };
            return Ok(Some(FastYield::Request(HostRequest::Await { event_id })));
        }

        FastOp::CrossLangCall => {
            let site = &symbols.cross_lang_calls[instr.arg as usize];
            let target_lang = symbols.strings[site.target_lang_idx as usize].clone();
            let function_name = symbols.strings[site.function_name_idx as usize].clone();

            // Pop arguments from stack (in reverse order)
            let mut args = Vec::new();
            for _ in 0..site.argc {
                let arg = stack.pop().ok_or(FastError::StackUnderflow)?;
                args.push(arg);
            }
            args.reverse();

            // Call the function via global registry
            // let result = crate::polyglot::call_function_global(&target_lang, &function_name, args);
            return Err(FastError::ExecutionError("Polyglot execution not supported yet".into()));
        }
    }

    Ok(None)
}
