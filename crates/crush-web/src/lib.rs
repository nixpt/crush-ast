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
//! `caison.parse`. `execute_with`/`Session` add what the page grants: `time`
//! (the clock, see `clock`), `env` (the environment, which in a browser is
//! empty) and polyglot languages (below). `fs`, `process` and `net` are never
//! registered.
//!
//! Like `crush-run`, every entry point refuses a program that can reach a
//! capability the run does not grant before running any of it (`nothing was
//! run`), rather than failing when the call is reached.
//!
//! `@lang{}` polyglot blocks (#91): a browser can't spawn a subprocess, so a
//! block runs only when the page (1) grants it — `execute_with`/`Session`
//! option `grants: ["polyglot.javascript"]` (or `polyglot.python`; bash is never
//! grantable here) — and (2) supplies an executor with [`set_lang_executor`]
//! that runs the code where the page chooses (a Worker or sandboxed iframe), so
//! it never runs with the page's own authority. Without both, the VM refuses
//! with the same capability error as native with no `--polyglot` grant.
//! `execute`/`run_blob` take no options and so never run polyglot blocks.
//!
//! Or the page can have `@python` / `@javascript` blocks **lowered** instead
//! (option `polyglot: "lower"`, CRUSH-272): each block's body is compiled into
//! the program through its language's walker and runs as Crush, with no grant
//! and no executor. A construct the walker can't lower is a compile error
//! naming it and the block's line; nothing falls back to the executor.

use std::cell::RefCell;
use std::rc::Rc;

use crush_vm::{HostCaps, InputSource, PortableVm, VmError, VmYield};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

mod clock;

/// A host runner for `@lang` blocks: `(lang, code, env)` → the block's stdout
/// (including the result-sentinel line) or an error message.
pub type HostLangExecutor = Rc<dyn Fn(&str, &str, &[(String, String)]) -> Result<String, String>>;

thread_local! {
    static LANG_EXECUTOR: RefCell<Option<HostLangExecutor>> = const { RefCell::new(None) };
}

/// Install (or with `None`, remove) the runner for granted `@lang` blocks.
/// Pure-Rust counterpart of [`set_lang_executor`], usable from native tests.
pub fn set_host_lang_executor(executor: Option<HostLangExecutor>) {
    LANG_EXECUTOR.with(|e| *e.borrow_mut() = executor);
}

/// Install the page's runner for granted `@lang` blocks (#91):
/// `(lang, code, env) => stdout`, called synchronously. `env` maps the
/// block's input variables to JSON strings (what native passes as environment
/// variables, read by the marshaling prologue as `process.env[name]`); return
/// everything the block printed, including the result-sentinel line. Throw to
/// fail the block. Pass `null` to remove it.
#[wasm_bindgen]
pub fn set_lang_executor(executor: JsValue) -> Result<(), JsValue> {
    if executor.is_null() || executor.is_undefined() {
        set_host_lang_executor(None);
        return Ok(());
    }
    let func: js_sys::Function = executor
        .dyn_into()
        .map_err(|_| JsValue::from_str("set_lang_executor expects a function or null"))?;
    set_host_lang_executor(Some(Rc::new(move |lang: &str, code: &str, env: &[(String, String)]| {
        let env_obj = js_sys::Object::new();
        for (k, v) in env {
            let _ = js_sys::Reflect::set(&env_obj, &JsValue::from_str(k), &JsValue::from_str(v));
        }
        match func.call3(&JsValue::NULL, &JsValue::from_str(lang), &JsValue::from_str(code), &env_obj) {
            Ok(out) => out.as_string().ok_or_else(|| "executor must return a string".to_string()),
            Err(err) => Err(err
                .as_string()
                .or_else(|| js_sys::Reflect::get(&err, &JsValue::from_str("message")).ok().and_then(|m| m.as_string()))
                .unwrap_or_else(|| "executor threw".to_string())),
        }
    })));
    Ok(())
}

/// Grants a page may give a program in the browser, each the counterpart of a
/// `crush-run` flag: `polyglot.<lang>` (#91, `--polyglot`; a granted block runs
/// through [`set_lang_executor`]), `time` (`--time`, the browser's clock) and
/// `env` (`--env`; a browser has no environment, so `env.get` returns null and
/// `env.all` is empty) and `fs` (`--fs`: an empty in-memory directory of the
/// run's own, see `HostCapsBuilder::fs_memory`). `polyglot.bash`, `process` and
/// `net` are never grantable here.
const GRANTABLE: &[&str] = &["polyglot.javascript", "polyglot.python", "time", "env", "fs"];

/// [`browser_caps`] plus `grants`, or an error naming a grant a browser can't give.
fn granted_caps(args: Vec<String>, grants: &[String]) -> Result<HostCaps, String> {
    if let Some(bad) = grants.iter().find(|g| !GRANTABLE.contains(&g.as_str())) {
        return Err(format!(
            "`{bad}` can't be granted in a browser (grantable: {})",
            GRANTABLE.join(", ")
        ));
    }
    let granted = |name: &str| grants.iter().any(|g| g == name);
    let builder = crush_lang_sdk::HostCapsBuilder::new().args(args).env(granted("env"));
    // `fs` (crush-run's `--fs`): the file caps over an empty in-memory root of
    // this run's own, with the same results and errors as an empty --fs-root.
    let builder = if granted("fs") { builder.fs_memory() } else { builder };
    #[cfg(feature = "stdlib")]
    let builder = builder.stdlib(true);
    let mut caps = builder.build();
    if granted("time") {
        clock::register(&mut caps);
    }
    for lang in grants.iter().filter_map(|g| g.strip_prefix("polyglot.")) {
        caps.register(crush_vm::host::polyglot_gate(lang));
    }
    Ok(caps)
}

/// `crush-run`'s preflight: refuse a program that can reach a capability
/// `caps` does not register, before any of it runs, with the same message
/// (one line per missing grant, named by the `crush-run` flag that grants it).
fn refuse_ungranted(program: &crush_vm::Program, caps: &HostCaps) -> Result<(), String> {
    let missing = crush_lang_sdk::effects::missing_grants(program, Some(caps)).map_err(|e| e.to_string())?;
    if missing.is_empty() {
        return Ok(());
    }
    Err(format!(
        "this program needs capabilities this run does not grant; nothing was run:\n{}",
        crush_lang_sdk::effects::describe_missing(&missing)
    ))
}

/// The capabilities every browser run gets: what `crush-run` registers when
/// given no grant flags. That is the pure standard library (`math.*`,
/// `str.*`, `system.*`, …; the `stdlib` feature, on by default), `sys.args`
/// (answering `args`), `sys.exit` and `caison.parse`. None of them reaches
/// outside the VM, so a page needs no grant to offer them; `fs`, `time`,
/// `env`, `process` and `net` stay absent.
pub fn browser_caps(args: Vec<String>) -> HostCaps {
    granted_caps(args, &[]).expect("no grants")
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
    refuse_ungranted(program, &caps).map_err(|e| JsValue::from_str(&e))?;
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
    /// Capabilities the page grants this run: `polyglot.javascript` /
    /// `polyglot.python` (a granted block runs through [`set_lang_executor`]),
    /// `time`, `env`, `fs` (an empty in-memory directory per run). See `GRANTABLE`.
    pub grants: Option<Vec<String>>,
    /// What `@python` / `@javascript` blocks become (CRUSH-272): `"exec"`
    /// (the default) runs each through the executor, behind its grant;
    /// `"lower"` compiles each block's body into the program through its
    /// language's walker, so it runs **as Crush** — Python / JavaScript as the
    /// walker lowers them, not CPython / Node — needs no grant and no
    /// executor, and a construct the walker can't lower is a compile error
    /// naming it. Lowering needs this build's `polyglot-python` /
    /// `polyglot-javascript` feature.
    pub polyglot: Option<String>,
}

/// Compile `source` for a run with `options` (its `polyglot` mode).
fn compile(source: &str, options: &RunOptions) -> Result<crush_vm::Program, String> {
    use crush_lang_sdk::compile::{CompileOptions, PolyglotMode};
    let polyglot = match options.polyglot.as_deref() {
        None | Some("exec") => PolyglotMode::Exec,
        Some("lower") => PolyglotMode::Lower,
        Some(other) => return Err(format!("invalid options: polyglot must be \"exec\" or \"lower\", not {other:?}")),
    };
    crush_lang_sdk::compile::compile_crush_source_with(source, &CompileOptions { polyglot })
        .map_err(|e| format!("compile error: {e}"))
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

fn new_vm(program: crush_vm::Program, options: &RunOptions, input: InputSource) -> Result<PortableVm, String> {
    let mut quotas = crush_vm::Quotas::default();
    if let Some(max_steps) = options.max_steps {
        quotas.max_steps = max_steps;
    }
    let caps = granted_caps(options.args.clone().unwrap_or_default(), options.grants.as_deref().unwrap_or_default())?;
    refuse_ungranted(&program, &caps)?;
    let mut vm = PortableVm::new(program);
    vm.set_quotas(quotas);
    vm.set_input(input);
    vm.set_host_caps(caps);
    // The fs caps that change files (fs.write, mkdir, rm, cp, mv, touch) are
    // privileged in the portable VM; `fs` grants them, as `--fs` does natively.
    // Only registered caps can run, and no other privileged cap (net.*,
    // wallet.*, vm.*) is ever registered here.
    vm.set_privileged_allowed(options.grants.as_deref().unwrap_or_default().iter().any(|g| g == "fs"));
    if let Some(executor) = LANG_EXECUTOR.with(|e| e.borrow().clone()) {
        vm.set_lang_executor(Box::new(move |lang, code, env| executor(lang, code, env)));
    }
    Ok(vm)
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
    let program = match compile(source, options) {
        Ok(program) => program,
        Err(e) => {
            return ExecuteWithResult {
                ok: false,
                output: String::new(),
                error: Some(e),
                exit_code: None,
                steps: 0,
            };
        }
    };
    let stdin = options.stdin.clone().unwrap_or_default();
    let mut vm = match new_vm(program, options, InputSource::supplied(stdin)) {
        Ok(vm) => vm,
        Err(e) => {
            return ExecuteWithResult { ok: false, output: String::new(), error: Some(e), exit_code: None, steps: 0 };
        }
    };
    let (status, error, exit_code) = drive(&mut vm);
    ExecuteWithResult {
        ok: status == Status::Done && exit_code.unwrap_or(0) == 0,
        output: vm.take_output(),
        error,
        exit_code,
        steps: vm.steps(),
    }
}

/// Compile and run Crush source with options `{ stdin?, max_steps?, args?, grants? }`,
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
    /// Compile `source` with options `{ max_steps?, args?, grants? }` (`stdin` is ignored —
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
        let vm = compile(source, options)
            .and_then(|program| new_vm(program, options, InputSource::interactive()));
        match vm {
            Ok(vm) => Session {
                vm: Some(vm),
                status: None,
                error: None,
                exit_code: None,
                transcript: String::new(),
            },
            Err(e) => Session {
                vm: None,
                status: Some(Status::Error),
                error: Some(e),
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
