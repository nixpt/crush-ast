//! C API shared library for embedding the CrushVM in C/C++ programs.
//!
//! This crate is built as a `cdylib` (`libcrush_vm_capi.so`) so that the
//! main `crush-vm` crate can remain a plain `lib`, avoiding duplicate
//! compilation of `casm` in workspace builds.
//!
//! Follows the exosphere `crush-abi-c` pattern:
//! - Opaque handle types
//! - Integer error codes (0 = success)
//! - `extern "C"` + `#[no_mangle]` for all entry points
//!
//! Generate the C header via cbindgen or use the handwritten `crush_vm.h`.

use std::ffi::{CStr, c_char};
use std::sync::{Mutex, LazyLock};

/// Opaque VM state.
struct CrushVmState {
    output: Vec<String>,
    last_error: Option<String>,
}

static VM_STATE: LazyLock<Mutex<CrushVmState>> = LazyLock::new(|| {
    Mutex::new(CrushVmState {
        output: Vec::new(),
        last_error: None,
    })
});

/// Initialize the CrushVM runtime. Call once before any other function.
/// Returns 0 on success.
#[unsafe(no_mangle)]
pub extern "C" fn crush_vm_init() -> i32 {
    let mut state = VM_STATE.lock().unwrap();
    state.output.clear();
    state.last_error = None;
    0
}

/// Load and execute a CASM JSON program.
///
/// `casm_json` must be a valid null-terminated UTF-8 C string containing
/// a CASM JSON program.
///
/// Returns 0 on success, -1 on parse error, -2 on execution error.
///
/// # Safety
///
/// `casm_json` must point to a null-terminated C string that stays valid for
/// the duration of the call, or be null (which is rejected with `-1`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn crush_vm_run_casm(casm_json: *const c_char) -> i32 {
    if casm_json.is_null() {
        let mut state = VM_STATE.lock().unwrap();
        state.last_error = Some("null pointer passed to crush_vm_run_casm".to_string());
        return -1;
    }

    let c_str = unsafe { CStr::from_ptr(casm_json) };
    let json_str = match c_str.to_str() {
        Ok(s) => s,
        Err(e) => {
            let mut state = VM_STATE.lock().unwrap();
            state.last_error = Some(format!("Invalid UTF-8: {}", e));
            return -1;
        }
    };

    let program: casm::Program = match serde_json::from_str(json_str) {
        Ok(p) => p,
        Err(e) => {
            let mut state = VM_STATE.lock().unwrap();
            state.last_error = Some(format!("CASM parse error: {}", e));
            return -1;
        }
    };

    match run_casm_program(&program) {
        Ok(rendered) => {
            let mut state = VM_STATE.lock().unwrap();
            state.last_error = None;
            state.output.push(rendered);
            0
        }
        Err(msg) => {
            let mut state = VM_STATE.lock().unwrap();
            state.last_error = Some(msg);
            -2
        }
    }
}

/// A stdout `io.print` capability for the C API (the bare FastVM has none), so
/// `cap_call io.print` in an embedded program actually prints.
#[derive(Debug)]
struct StdoutPrintCap;

impl crush_vm::fastvm::Capability for StdoutPrintCap {
    fn name(&self) -> &str {
        "io.print"
    }
    fn call(
        &self,
        arena: &mut crush_vm::memory::Arena,
        args: Vec<crush_vm::value::RuntimeValue>,
        _hal: std::sync::Arc<dyn crush_vm::fastvm::Hal>,
    ) -> anyhow::Result<crush_vm::value::RuntimeValue> {
        let parts: Vec<String> = args
            .iter()
            .map(|v| crush_vm::io_print::runtime_value_to_text(v, arena))
            .collect();
        print!("{}", crush_vm::io_print::format_io_print_line(&parts));
        Ok(crush_vm::value::RuntimeValue::Null)
    }
}

/// Run `program` on the FastVM with an `io.print` capability, returning the
/// finished value's text.
///
/// Only a run that *finished* is success. A runtime error, an exhausted budget,
/// or a host request the C API cannot service is an error (CRUSH-205) — before,
/// any `Ok(..)` was reported as success, so a program that raised a runtime
/// error returned 0 and `last_error` stayed null.
fn run_casm_program(program: &casm::Program) -> Result<String, String> {
    use crush_vm::fastvm::{Capability, FastYield};
    let caps: Vec<std::sync::Arc<dyn Capability>> = vec![std::sync::Arc::new(StdoutPrintCap)];
    match crush_vm::vm::run_fastvm_with_caps(program, caps) {
        Ok(FastYield::Finished(v)) => Ok(format!("{v:?}")),
        Ok(FastYield::Value(v)) => Ok(format!("{v:?}")),
        Ok(FastYield::Error(e)) => Err(format!("VM execution error: {e:?}")),
        Ok(other) => Err(format!("VM did not complete: {other:?}")),
        Err(e) => Err(format!("VM execution error: {e:?}")),
    }
}

/// Assemble a CASM text source (`.casm` text format) and execute it.
///
/// Returns 0 on success, -1 on assembly error, -2 on execution error.
///
/// # Safety
///
/// `asm_source` must point to a null-terminated C string that stays valid for
/// the duration of the call, or be null (which is rejected with `-1`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn crush_vm_run_asm(asm_source: *const c_char) -> i32 {
    if asm_source.is_null() {
        let mut state = VM_STATE.lock().unwrap();
        state.last_error = Some("null pointer passed to crush_vm_run_asm".to_string());
        return -1;
    }

    let c_str = unsafe { CStr::from_ptr(asm_source) };
    let source = match c_str.to_str() {
        Ok(s) => s,
        Err(e) => {
            let mut state = VM_STATE.lock().unwrap();
            state.last_error = Some(format!("Invalid UTF-8: {}", e));
            return -1;
        }
    };

    let program = match crush_vm::assemble(source, None, None) {
        Ok(p) => p,
        Err(e) => {
            let mut state = VM_STATE.lock().unwrap();
            state.last_error = Some(format!("Assembly error: {}", e));
            return -1;
        }
    };

    let quotas = crush_vm::vm::Quotas::default();

    match crush_vm::vm::run(&program, &quotas) {
        Ok(result) => {
            let mut state = VM_STATE.lock().unwrap();
            state.last_error = None;
            state.output.push(format!("{:?}", result));
            0
        }
        Err(e) => {
            let mut state = VM_STATE.lock().unwrap();
            state.last_error = Some(format!("VM execution error: {}", e));
            -2
        }
    }
}

/// Get the last error message. Returns a pointer to a null-terminated string,
/// or null if no error occurred. The pointer is valid until the next API call.
///
/// The caller must NOT free this pointer.
#[unsafe(no_mangle)]
pub extern "C" fn crush_vm_last_error() -> *const c_char {
    static ERROR_BUF: Mutex<Vec<u8>> = Mutex::new(Vec::new());
    let state = VM_STATE.lock().unwrap();
    match &state.last_error {
        Some(err) => {
            let mut buf = ERROR_BUF.lock().unwrap();
            buf.clear();
            buf.extend_from_slice(err.as_bytes());
            buf.push(0); // null terminator
            buf.as_ptr() as *const c_char
        }
        None => std::ptr::null(),
    }
}

/// Get the CrushVM library version string.
#[unsafe(no_mangle)]
pub extern "C" fn crush_vm_version() -> *const c_char {
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr() as *const c_char
}
