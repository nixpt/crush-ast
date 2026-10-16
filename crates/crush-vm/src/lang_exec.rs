//! Embedder-supplied runners for `@lang` polyglot blocks (CRUSH-271).
//!
//! Crush owns the polyglot *syntax*, the variable marshaling, and the
//! `polyglot.<lang>` grant check. *Running* a granted block belongs to the
//! embedder: a CLI spawns a local interpreter, a browser may hand the block
//! to a sandboxed worker, an OS may run it in a capsule. An embedder says how
//! by giving its [`HostCaps`](crate::HostCaps) a [`PolyglotExecutor`] with
//! [`HostCaps::set_polyglot_executor`](crate::HostCaps::set_polyglot_executor).
//!
//! Both VM engines consult it: the scheduler VM (`run_with_caps`,
//! `run_scheduled*`) and [`PortableVm`](crate::PortableVm). With none set they
//! use [`SubprocessExecutor`], the built-in local-interpreter runner (and its
//! buckets sandbox under the `sandboxed-polyglot` feature), so behaviour
//! without an executor is unchanged.
//!
//! The grant check is never delegated: a block whose `polyglot.<lang>` grant
//! is missing, undeclared or outside `Quotas::allowed_caps` is refused before
//! any executor is called.
//!
//! ```
//! use std::sync::Arc;
//! use crush_vm::{HostCaps, PolyglotError, PolyglotRequest, polyglot_executor_fn};
//!
//! let mut caps = HostCaps::new();
//! caps.grant_polyglot(&["python"]);
//! caps.set_polyglot_executor(polyglot_executor_fn(|req: &PolyglotRequest<'_>| {
//!     if req.lang != "python" {
//!         return Err(PolyglotError::Failed(format!("no runner for {}", req.lang)));
//!     }
//!     // Run `req.code` somewhere with `req.vars` in its environment, inside
//!     // `req.deadline_ms`, and return everything it printed on stdout.
//!     Ok(String::from("hello from the embedder"))
//! }));
//! assert!(caps.polyglot_executor().is_some());
//! ```

use std::sync::Arc;

use crate::scheduler::{ExecLangOutcome, lang_runtime_error, parse_exec_lang_stdout};
use crate::vm::VmError;

/// One granted `@lang` block, as the VM hands it to a [`PolyglotExecutor`].
///
/// It carries everything the built-in subprocess path gets. New fields may be
/// added, so embedders read it but never build one.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct PolyglotRequest<'a> {
    /// The block's language tag as written (`python`, `py`, `js`, …). The
    /// VM has already checked it is a known language and that its
    /// `polyglot.<lang>` grant is held.
    pub lang: &'a str,
    /// The block's guest source, with the marshaling prologue/epilogue the
    /// compiler spliced in. Python and JavaScript blocks read their inputs
    /// from the environment and print their outputs on a
    /// [`CRUSH_RESULT_SENTINEL`](crate::scheduler::CRUSH_RESULT_SENTINEL)
    /// line; an executor only has to run the code and return its stdout.
    pub code: &'a str,
    /// Input variables as `(name, value)` environment pairs, already encoded
    /// for the language (JSON for Python and JavaScript, plain text for
    /// bash) — exactly what the built-in path puts in the subprocess env.
    pub vars: &'a [(String, String)],
    /// Names the block declares as outputs (`@python -> a, b { … }`), in
    /// order. The VM reads them from the sentinel line; they are here for
    /// executors that need to know what the block will produce.
    pub outputs: &'a [String],
    /// Pinned dependencies from `@lang[pypi:six, npm:is-number@7] { … }`:
    /// buckets package specs, bare bottle aliases or `pypi:`/`npm:` specs.
    pub deps: &'a [String],
    /// The `polyglot.<lang>` grant the block passed.
    pub gate: &'a str,
    /// Wall-clock budget for the block in milliseconds
    /// (`Quotas::max_wall_time_ms`). The VM cannot preempt an executor, so
    /// an executor that can block must enforce it itself and return
    /// [`PolyglotError::Timeout`].
    pub deadline_ms: u64,
    /// The `.crush` source line of the block, when the compiler recorded it.
    pub crush_line: Option<u32>,
}

/// Why an executor could not produce a block's output.
#[derive(Debug)]
#[non_exhaustive]
pub enum PolyglotError {
    /// The executor failed. Becomes a VM error carrying this message:
    /// `@<lang> block failed (line N): <message>` — the same error a
    /// [`LangExecutor`](crate::LangExecutor) failure gives on `PortableVm`.
    Failed(String),
    /// The guest code itself failed (an exception, a non-zero exit); the
    /// string is what it wrote to stderr. Becomes
    /// [`VmError::LangRuntimeError`], like a failing subprocess.
    Guest(String),
    /// The block ran past [`PolyglotRequest::deadline_ms`]. Becomes
    /// [`VmError::CapTimeout`] on the block's `polyglot.<lang>` gate.
    Timeout,
    /// A VM error to surface unchanged.
    Vm(VmError),
}

impl From<String> for PolyglotError {
    fn from(message: String) -> Self {
        PolyglotError::Failed(message)
    }
}

impl From<VmError> for PolyglotError {
    fn from(err: VmError) -> Self {
        PolyglotError::Vm(err)
    }
}

impl PolyglotError {
    /// The VM error this executor error surfaces as for `request`.
    pub fn into_vm_error(self, request: &PolyglotRequest<'_>) -> VmError {
        let lang = request.lang;
        match self {
            PolyglotError::Failed(message) => {
                let at = request.crush_line.map(|l| format!(" (line {l})")).unwrap_or_default();
                VmError::UnknownCap(format!("@{lang} block failed{at}: {message}"))
            }
            PolyglotError::Guest(stderr) => {
                lang_runtime_error(lang, stderr.as_bytes(), request.crush_line)
            }
            PolyglotError::Timeout => VmError::CapTimeout {
                cap: request.gate.to_string(),
                limit_ms: request.deadline_ms,
            },
            PolyglotError::Vm(err) => err,
        }
    }
}

/// Runs granted `@lang` blocks for an embedder.
///
/// `execute` returns the block's whole stdout, including the result-sentinel
/// line its marshaling epilogue prints; the VM splits that into the block's
/// visible output and its declared output values, the same way for every
/// executor. Shared across green tasks, so it is `Send + Sync` and takes
/// `&self`; keep any state behind a lock.
pub trait PolyglotExecutor: Send + Sync {
    /// Run one block.
    fn execute(&self, request: &PolyglotRequest<'_>) -> Result<String, PolyglotError>;
}

impl<F> PolyglotExecutor for F
where
    F: Fn(&PolyglotRequest<'_>) -> Result<String, PolyglotError> + Send + Sync,
{
    fn execute(&self, request: &PolyglotRequest<'_>) -> Result<String, PolyglotError> {
        self(request)
    }
}

/// Wrap a closure as a shareable [`PolyglotExecutor`]. (A bare closure
/// already implements the trait; this pins down its signature so the
/// argument type can be inferred, and boxes it into an `Arc`.)
pub fn polyglot_executor_fn<F>(f: F) -> Arc<dyn PolyglotExecutor>
where
    F: Fn(&PolyglotRequest<'_>) -> Result<String, PolyglotError> + Send + Sync + 'static,
{
    Arc::new(f)
}

/// The built-in executor, used when the embedder sets none: runs the block
/// in the local interpreter (`python3 -c`, `node -e`, `bash -c`) with the
/// input variables in its environment, killed at the deadline. With the
/// `sandboxed-polyglot` feature it is provisioned through buckets and run in
/// a bwrap sandbox, `deps` included.
#[derive(Debug, Default, Clone, Copy)]
pub struct SubprocessExecutor;

impl PolyglotExecutor for SubprocessExecutor {
    fn execute(&self, request: &PolyglotRequest<'_>) -> Result<String, PolyglotError> {
        crate::scheduler::run_exec_lang_stdout(
            request.lang,
            request.code,
            request.deps,
            request.vars,
            request.crush_line,
            request.gate,
            request.deadline_ms,
        )
        .map_err(PolyglotError::Vm)
    }
}

/// Run a granted block through `executor`, or [`SubprocessExecutor`] when
/// there is none, and parse its stdout. The grant check has already passed.
pub(crate) fn run_block(
    executor: Option<&dyn PolyglotExecutor>,
    request: &PolyglotRequest<'_>,
) -> Result<ExecLangOutcome, VmError> {
    let executor = executor.unwrap_or(&SubprocessExecutor);
    match executor.execute(request) {
        Ok(stdout) => Ok(parse_exec_lang_stdout(&stdout)),
        Err(err) => Err(err.into_vm_error(request)),
    }
}

/// The names an `exec_lang` spec declares as outputs (`out_0`..), in order.
pub(crate) fn declared_outputs(
    spec: &std::collections::HashMap<String, serde_json::Value>,
) -> Vec<String> {
    let count = spec.get("out_count").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    (0..count)
        .filter_map(|i| spec.get(&format!("out_{i}")).and_then(|v| v.as_str()).map(str::to_string))
        .collect()
}
