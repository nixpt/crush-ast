//! Browser WebAssembly runtime for Crush.
//!
//! Two entry points into `crush-vm`'s portable (green-thread) interpreter
//! (`crush_vm::run`), not a separate reimplementation:
//!
//! - [`execute`]: compiles Crush source via
//!   `crush-lang-sdk::compile_crush_source` (parse + polyglot-marshaling-prep
//!   + CASM lowering + casm-to-VM-program assembly — the same pipeline
//!   `crush-run`/`crushc` use natively) and runs it, in one call.
//! - [`run_blob`]: skips compilation entirely and runs a pre-compiled `.cvm`
//!   blob (`crush_vm::Program::to_blob()`'s format — the same file
//!   `crush-pkg build` writes to `target/<name>.cvm`). Ship that instead of
//!   raw source to avoid recompiling in the browser on every load.
//!
//! Programs that call `io.read` go through [`PortableVm`] stepped by the
//! host instead, because a browser has no stdin:
//!
//! - [`execute_with`]: runs with `io.read` fed from a `stdin` string given up
//!   front (EOF = `""` once it runs out), and keeps output printed before an
//!   error.
//! - [`Session`]: runs until the program needs a line, pauses, and resumes
//!   when the page calls `provide(line)` — an interactive terminal.
//!
//! Every entry point registers the same capabilities `crush-run` gives a
//! program with no grant flags (see [`browser_caps`]): the pure standard
//! library (`math.*`, `str.*`, `system.*`, …), `sys.args`/`sys.exit` and
//! `caison.parse`. Nothing that reaches outside the VM (`fs`, `time`, `env`,
//! `process`, `net`) is registered.
//!
//! `@lang{}` polyglot blocks are unsupported either way: `EXEC_LANG` needs to
//! spawn a subprocess, which doesn't exist in a browser sandbox. The VM
//! returns a capability-gated `VmError` for those, same as running with no
//! `--polyglot` grant natively — not a silent no-op.

use crush_vm::{HostCaps, InputSource, PortableVm, VmError, VmYield};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

/// The capabilities every browser run gets: what `crush-run` registers when
/// given no grant flags. That is the pure standard library (`math.*`,
/// `str.*`, `system.*`, …; the `stdlib` feature, on by default), `sys.args`
/// (answering `args`), `sys.exit` and `caison.parse`. None of them reaches
/// outside the VM, so a page needs no grant to offer them; `fs`, `time`,
/// `env`, `process` and `net` stay absent.
pub fn browser_caps(args: Vec<String>) -> HostCaps {
    let builder = crush_lang_sdk::HostCapsBuilder::new().args(args);
    #[cfg(feature = "stdlib")]
    let builder = builder.stdlib(true);
    builder.build()
}

#[derive(Serialize)]
struct ExecutionResult {
    output: String,
    steps: usize,
    halted: bool,
    /// The status passed to `sys.exit`, when the program called it.
    #[serde(skip_serializing_if = "Option::is_none")]
    exit_code: Option<i32>,
}

fn run_program(program: &crush_vm::Program) -> Result<JsValue, JsValue> {
    let quotas = crush_vm::Quotas::default();
    let caps = browser_caps(Vec::new());
    // Collected through the streaming sink so that output printed before a
    // `sys.exit` survives it (the exit arrives as an `Err`).
    let mut output = String::new();
    let out =
        match crush_vm::vm::run_with_caps_streaming(program, &quotas, Some(&caps), &mut |part| {
            output.push_str(part)
        }) {
            Ok(result) => ExecutionResult {
                output: result.output,
                steps: result.steps,
                halted: result.halted,
                exit_code: None,
            },
            Err(VmError::Exit(code)) => ExecutionResult {
                output,
                steps: 0,
                halted: true,
                exit_code: Some(code),
            },
            Err(e) => return Err(JsValue::from_str(&format!("runtime error: {e}"))),
        };
    serde_wasm_bindgen::to_value(&out).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Compile and run Crush source, returning `{ output, steps, halted,
/// exit_code? }` as a plain JS object, or throwing a string error (compile
/// failure or VmError). `exit_code` is set when the program called
/// `sys.exit`; `steps` is then 0 (not reported on that path).
#[wasm_bindgen]
pub fn execute(source: &str) -> Result<JsValue, JsValue> {
    let program = crush_lang_sdk::compile::compile_crush_source(source)
        .map_err(|e| JsValue::from_str(&format!("compile error: {e}")))?;
    run_program(&program)
}

/// Run a pre-compiled `.cvm` blob (`crush-pkg build`'s `target/<name>.cvm`,
/// or any `crush_vm::Program::to_blob()` output). No parsing or compilation
/// happens here — this is strictly cheaper than [`execute`] when the source
/// was already compiled ahead of time. Returns `{ output, steps, halted }`,
/// or throws a string error (bad blob format or VmError).
#[wasm_bindgen]
pub fn run_blob(bytes: &[u8]) -> Result<JsValue, JsValue> {
    let program = crush_vm::Program::from_blob(bytes)
        .map_err(|e| JsValue::from_str(&format!("blob error: {e}")))?;
    run_program(&program)
}

/// Compile Crush source without running it. Returns nothing on success;
/// throws the compiler's error message on failure. Cheaper than `execute`
/// for a "check as you type" editor use case.
#[wasm_bindgen]
pub fn check(source: &str) -> Result<(), JsValue> {
    crush_lang_sdk::compile::compile_crush_source(source)
        .map(|_| ())
        .map_err(|e| JsValue::from_str(&format!("compile error: {e}")))
}

/// Options shared by [`execute_with`] and [`Session::new`]. Every field is
/// optional; `undefined`/`null` means all defaults.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct RunOptions {
    /// Text served to `io.read`, line by line (`execute_with` only). Absent
    /// means empty: the first read already returns `""` (EOF).
    pub stdin: Option<String>,
    /// Instruction budget. For a session it covers the whole session, not
    /// each call. Defaults to `crush_vm::Quotas::default().max_steps`.
    #[serde(alias = "maxSteps")]
    pub max_steps: Option<usize>,
    /// What `sys.args()` returns. Absent means none.
    pub args: Option<Vec<String>>,
}

fn parse_options(options: JsValue) -> Result<RunOptions, JsValue> {
    if options.is_undefined() || options.is_null() {
        return Ok(RunOptions::default());
    }
    serde_wasm_bindgen::from_value(options)
        .map_err(|e| JsValue::from_str(&format!("invalid options: {e}")))
}

/// Where a stepped run stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// The program called `io.read` and no line is pending.
    NeedInput,
    /// The program finished.
    Done,
    /// Compile or runtime error; see `error`.
    Error,
}

/// One stepped-run report, serialized to JS as
/// `{ status, output, error?, exit_code?, steps }`.
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub status: Status,
    /// Output printed during this call only (see [`Session`]).
    pub output: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// The status passed to `sys.exit` (the run is then `done`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    pub steps: usize,
}

fn new_vm(program: crush_vm::Program, options: &RunOptions, input: InputSource) -> PortableVm {
    let mut quotas = crush_vm::Quotas::default();
    if let Some(max_steps) = options.max_steps {
        quotas.max_steps = max_steps;
    }
    let mut vm = PortableVm::new(program);
    vm.set_quotas(quotas);
    vm.set_input(input);
    vm.set_host_caps(browser_caps(options.args.clone().unwrap_or_default()));
    vm
}

/// Where [`drive`] stopped: the status, the error for `Status::Error`, and
/// the `sys.exit` status if the program called it.
type Stop = (Status, Option<String>, Option<i32>);

/// Step `vm` until it finishes, errors, exits, or pauses for input.
fn drive(vm: &mut PortableVm) -> Stop {
    loop {
        if vm.is_halted() {
            return (Status::Done, None, None);
        }
        match vm.step() {
            Ok(None) => {}
            Ok(Some(VmYield::HostCall { capability, .. })) if capability == "io.read" => {
                return (Status::NeedInput, None, None);
            }
            Ok(Some(other)) => {
                return (
                    Status::Error,
                    Some(format!("unexpected VM pause: {other:?}")),
                    None,
                );
            }
            Err(VmError::Exit(code)) => return (Status::Done, None, Some(code)),
            Err(e) => return (Status::Error, Some(format!("runtime error: {e}")), None),
        }
    }
}

/// Result of [`execute_with`], serialized as
/// `{ ok, output, error?, exit_code?, steps }`.
#[derive(Debug, Clone, Serialize)]
pub struct ExecuteWithResult {
    /// The program finished without an error and, if it called `sys.exit`,
    /// with status 0.
    pub ok: bool,
    /// Everything printed, including output before an error.
    pub output: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// The status passed to `sys.exit`, when the program called it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    pub steps: usize,
}

/// Pure-Rust core of [`execute_with`].
pub fn execute_with_options(source: &str, options: &RunOptions) -> ExecuteWithResult {
    let program = match crush_lang_sdk::compile::compile_crush_source(source) {
        Ok(program) => program,
        Err(e) => {
            return ExecuteWithResult {
                ok: false,
                output: String::new(),
                error: Some(format!("compile error: {e}")),
                exit_code: None,
                steps: 0,
            };
        }
    };
    let stdin = options.stdin.clone().unwrap_or_default();
    let mut vm = new_vm(program, options, InputSource::supplied(stdin));
    let (status, error, exit_code) = drive(&mut vm);
    ExecuteWithResult {
        ok: status == Status::Done && exit_code.unwrap_or(0) == 0,
        output: vm.take_output(),
        error,
        exit_code,
        steps: vm.steps(),
    }
}

/// Compile and run Crush source with options `{ stdin?, max_steps?, args? }`,
/// returning `{ ok, output, error?, exit_code?, steps }`. `io.read` takes lines from
/// `stdin` and returns `""` once it runs out. Compile and runtime errors come
/// back as `ok: false` with output printed before the error kept; only
/// malformed options throw.
#[wasm_bindgen]
pub fn execute_with(source: &str, options: JsValue) -> Result<JsValue, JsValue> {
    let options = parse_options(options)?;
    let result = execute_with_options(source, &options);
    serde_wasm_bindgen::to_value(&result).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// An interactive run: the program executes until it calls `io.read` with no
/// line pending, then pauses (`status: "need_input"`) with all its state kept
/// until the page supplies a line.
///
/// Every method returns `{ status, output, error?, steps }` where `output` is
/// only what was printed *during that call* — append it to your terminal.
/// [`Session::transcript`] returns everything so far. `steps` is cumulative
/// and the `max_steps` budget covers the whole session. A compile error is
/// reported by the first `run()` as `status: "error"`, not thrown. Once a
/// session is `done` or `error`, further calls return that status again with
/// empty output.
///
/// ```js
/// const s = new Session(source, { max_steps: 1_000_000 });
/// let r = s.run();
/// while (r.status === "need_input") {
///   term.write(r.output);
///   r = s.provide(await nextLine()); // or s.close() for EOF
/// }
/// term.write(r.output);
/// ```
#[wasm_bindgen]
pub struct Session {
    vm: Option<PortableVm>,
    status: Option<Status>,
    error: Option<String>,
    exit_code: Option<i32>,
    transcript: String,
}

#[wasm_bindgen]
impl Session {
    /// Compile `source` with options `{ max_steps?, args? }` (`stdin` is ignored —
    /// use `provide`). Nothing runs until [`Session::run`].
    #[wasm_bindgen(constructor)]
    pub fn new(source: &str, options: JsValue) -> Result<Session, JsValue> {
        Ok(Self::with_options(source, &parse_options(options)?))
    }

    /// Start, or continue, the program.
    pub fn run(&mut self) -> Result<JsValue, JsValue> {
        to_js(&self.run_report())
    }

    /// Supply one line for the pending `io.read` (no trailing newline
    /// needed) and continue.
    pub fn provide(&mut self, line: &str) -> Result<JsValue, JsValue> {
        to_js(&self.provide_report(line))
    }

    /// Signal end of input (`io.read` then returns `""`) and continue.
    pub fn close(&mut self) -> Result<JsValue, JsValue> {
        to_js(&self.close_report())
    }

    /// Everything the program has printed so far.
    pub fn transcript(&self) -> String {
        self.transcript.clone()
    }
}

fn to_js(report: &Report) -> Result<JsValue, JsValue> {
    serde_wasm_bindgen::to_value(report).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Pure-Rust surface of [`Session`], usable from native tests.
impl Session {
    pub fn with_options(source: &str, options: &RunOptions) -> Self {
        match crush_lang_sdk::compile::compile_crush_source(source) {
            Ok(program) => Session {
                vm: Some(new_vm(program, options, InputSource::interactive())),
                status: None,
                error: None,
                exit_code: None,
                transcript: String::new(),
            },
            Err(e) => Session {
                vm: None,
                status: Some(Status::Error),
                error: Some(format!("compile error: {e}")),
                exit_code: None,
                transcript: String::new(),
            },
        }
    }

    pub fn run_report(&mut self) -> Report {
        let Some(vm) = self.vm.as_mut() else {
            return self.terminal_report();
        };
        if matches!(self.status, Some(Status::Done | Status::Error)) {
            return self.terminal_report();
        }
        let (status, error, exit_code) = drive(vm);
        let output = vm.take_output();
        self.transcript.push_str(&output);
        self.status = Some(status);
        self.error = error.clone();
        self.exit_code = exit_code;
        Report {
            status,
            output,
            error,
            exit_code,
            steps: vm.steps(),
        }
    }

    pub fn provide_report(&mut self, line: &str) -> Report {
        if let Some(vm) = self.vm.as_mut()
            && self.status != Some(Status::Done)
            && self.status != Some(Status::Error)
            && let Err(e) = vm.provide_input(line)
        {
            self.status = Some(Status::Error);
            self.error = Some(format!("cannot provide input: {e}"));
        }
        self.run_report()
    }

    pub fn close_report(&mut self) -> Report {
        if let Some(vm) = self.vm.as_mut() {
            vm.close_input();
        }
        self.run_report()
    }

    fn terminal_report(&self) -> Report {
        Report {
            status: self.status.unwrap_or(Status::Error),
            output: String::new(),
            error: self.error.clone(),
            exit_code: self.exit_code,
            steps: self.vm.as_ref().map_or(0, PortableVm::steps),
        }
    }
}

/// Route Rust panics to `console.error` instead of an opaque wasm trap.
/// Call once at startup.
#[wasm_bindgen]
pub fn init_panic_hook() {
    #[cfg(feature = "panic-hook")]
    console_error_panic_hook::set_once();
}
