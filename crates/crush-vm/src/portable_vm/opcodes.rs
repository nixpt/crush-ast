//! Opcode dispatch extracted from portable_vm.rs (CRUSHPVMSPLIT-1a).
//!
//! Private submodule of `crush_vm::portable_vm` -- first extract in the
//! CRUSHPVMSPLIT-1a/1b sequencing. This PR (1a) ships ONE chokepoint fn:
//! `dispatch_cap`. The opcode decoder (`execute_instruction`) still lives
//! in `portable_vm.rs` and calls this chokepoint via the sibling-pattern
//! `opcodes::dispatch_cap(self, cap, args)`. CRUSHPVMSPLIT-1b will later
//! additionally extract `execute_instruction` into this submodule.
//!
//! Sized XS -- smaller blast radius than the combined extract attempted
//! in CRUSHPVMSPLIT-1 (PR #11).

use super::*;
use crate::vm::{Value, VmError};

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
