# EXEC_LANG pluggable executor — letting an embedder swap in a sandboxed backend

> Status: scoping / pre-design. Written 2026-07-14 while investigating
> surfer-browser's Crush integration. Not yet built — this is the design
> to react to, not a committed plan.

---

## 1. The concrete gap that prompted this

Confirmed empirically in `surfer-browser` (its `crush-lang-sdk`/`crush-vm`
are unpinned path deps into this repo, so they run whatever's checked out
here):

```
fn main() { @python { result = 1 + 1 } print(result); }
```
```
Runtime error: unknown capability: @python requires the 'polyglot.python'
capability (run with --polyglot to grant it); refusing to spawn
```

Nothing in surfer-browser ever grants `polyglot.*` — so `@python { ... }`
blocks, the natural syntax with the free-variable marshaling built this
session, are dead there. The only Python execution path that actually
works in surfer today is `proc.run_capsule("python@3.11", script)` — a
surfer-specific `HostCap` that resolves and runs the interpreter through
`buckets` (`bwrap`-sandboxed, dependency-pinned), entirely independent of
`EXEC_LANG`. It requires writing the script as a raw string argument (no
`@python { ... }` syntax) and returns only `{stdout, stderr, exit_code}` —
no input/output variable marshaling.

So surfer has two disconnected Python-execution mechanisms:

| | `@python { ... }` (EXEC_LANG) | `proc.run_capsule(...)` |
|---|---|---|
| Syntax | natural | raw string arg |
| Marshaling | yes (this session's work) | no |
| Sandboxing | none (host subprocess) — which is presumably *why* it's never granted | real (`bwrap` via `buckets`) |
| Works in surfer today | **no** | yes |

The fix isn't "grant `polyglot.python` in surfer" — that would make
`@python { ... }` work, but unsandboxed, which is a real security
regression matching `CRUSH-2`'s exact concern (host subprocess, full
ambient authority). The fix is letting `EXEC_LANG` delegate its actual
subprocess-spawning to an embedder-supplied backend, so surfer can plug
`buckets` in underneath the syntax it already has.

## 2. Why this is more tractable than it looks: marshaling and sandboxing are already separate layers

This was the open question worth checking before scoping anything: does
unifying these require touching the marshaling protocol? No.

`crush-lang-sdk::compile::prepare_polyglot_blocks` does free-variable
analysis and rewrites the Python source (JSON-decode-from-env-var
prologue, sentinel-prefixed JSON-encoded result epilogue) **at compile
time**, before the program ever reaches a VM. The compiled `EXEC_LANG`
CASM instruction just carries the *already-rewritten* code string, the
input variable names, and their values. Nothing about *how* that code
string gets executed — host subprocess vs. `bwrap`-sandboxed subprocess —
is baked into the marshaling step.

Symmetrically, `scheduler.rs`'s `EXEC_LANG` handler does the *decode*
side (scan stdout for `CRUSH_RESULT_SENTINEL`, split visible output from
the marshaled payload, JSON-decode into a `Value`) after getting output
back — also independent of who ran the subprocess.

So the only thing that actually needs to become pluggable is the middle:
*take (lang, code, input vars) → produce (stdout, stderr, success)*. Swap
that piece and the marshaling protocol on both sides is untouched.

## 3. Proposed shape: an executor `HostCap`, checked before the hardcoded path

Reuse the existing `HostCap` trait (`crates/crush-vm/src/host.rs`) rather
than inventing new plumbing — `host_caps` is already threaded through
`execute_one`/`EXEC_LANG` for the capability gate check, so this is an
addition to a path that already exists, not new wiring.

```rust
/// If a host registers a capability under this name, EXEC_LANG delegates
/// *how* the subprocess runs to it instead of the built-in
/// resolve_lang_binary + Command::new(host binary) path. The embedder is
/// responsible for ALL languages once registered — see §5 on why partial
/// fallback is deliberately not supported.
pub const POLYGLOT_EXECUTOR_CAP: &str = "polyglot.executor";
```

In `EXEC_LANG`'s handler, *after* the existing `polyglot.<lang>` gate
check (unchanged — the executor override doesn't bypass authorization,
it changes what "authorized" actually runs):

```rust
if let Some(executor) = host_caps.and_then(|h| h.get(POLYGLOT_EXECUTOR_CAP)) {
    // args: [lang, code, var_names_and_values...] — same shape EXEC_LANG
    // already has in hand; no new marshaling needed on this side either.
    let result = executor.call(exec_args)?;
    // result: Value::Map{"stdout": Str, "stderr": Str, "success": Bool}
    // Sentinel-scanning below is UNCHANGED — same code path regardless
    // of which executor produced the stdout.
} else {
    // existing resolve_lang_binary + Command::new(...) + run_with_wall_clock_limit
}
```

The sentinel-scan / JSON-decode-into-`Value` logic that already exists in
`EXEC_LANG` stays exactly where it is and runs on the executor's stdout
the same way it runs on the built-in path's stdout. One scanning
implementation, two ways to produce the bytes it scans.

## 4. What surfer's implementation would look like

Almost entirely a reuse of the already-hardened `proc_capsule.rs` logic
(see `CRUSHAST-CAPTIMEOUT-1` / `SURF-CAPSULE-TIMEOUT-1`, same session —
the pipe-drain-safe, process-group-kill-safe `wait_with_timeout`), adapted
to accept `(lang, code, env_vars)` instead of `(spec, script, ext)` and
return a structured `Value::Map` instead of `CapsuleOutput`:

```rust
struct BucketPolyglotExecutor;
impl HostCap for BucketPolyglotExecutor {
    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        // parse lang/code/vars out of args (shape TBD — mirror EXEC_LANG's
        // existing internal spec JSON shape, or take structured Value args)
        // buckets::resolve(...) + buckets::sandbox::sandboxed_command(...)
        // + the SAME reader-thread + process-group-kill wait logic already
        // proven in proc_capsule.rs
        // return Value::Map{"stdout", "stderr", "success"}
    }
}
```

Registered once alongside `RunCapsuleCap` in
`super-surfer/src/app/handlers/crush.rs::build_crush_runtime` (and
wherever surfer's own console runtime is built). No change needed to
`capsule_lang_from_spec`'s python/node/bun/deno allowlist — it already
covers everything `resolve_lang_binary` does except `bash`, which is a
real gap either way (see §5).

## 5. Open design questions — need a decision before implementing, not guesses

1. **Naming/granularity**: one generic `polyglot.executor` cap (proposed
   above) that owns every language uniformly, vs. per-language override
   caps (`polyglot.python.executor`, etc.)? Buckets itself is already
   language-generic (`resolve(spec, ...)` takes any alias), so a single
   override matches its own shape — but worth confirming this is actually
   what an embedder wants before committing to the name.

2. **All-or-nothing vs. partial fallback**: if `polyglot.executor` is
   registered but a requested language isn't one the executor supports
   (e.g. `@bash { ... }`, which `buckets`/`capsule_lang_from_spec` doesn't
   cover), what happens? Two options:
   - **(recommended)** The executor owns everything once registered; an
     unsupported language is the *executor's* loud error to return, never
     a silent fallback to the built-in unsandboxed host-subprocess path.
     Falling back silently would mean an embedder who registered a
     sandboxed executor specifically to avoid unsandboxed execution could
     have some requests silently downgrade to exactly what they were
     trying to avoid — a hidden security regression, the same class of
     bug this whole session has been about eliminating (`CRUSH-2`, the
     capability bypass, the silent-fallthrough pattern found repeatedly
     elsewhere in this codebase today).
   - Silent per-language fallback to the built-in path. Not recommended,
     for the reason above — noted only so it's an explicit rejected
     option, not an unconsidered one.

3. **Does this belong in crush-vm at all, or should it be surfer-specific
   patch/fork?** Recommend crush-vm — the "one VM" doctrine
   (`docs/design/crushvm-rustpython.md`) has held throughout this
   project's history specifically to avoid embedders diverging on
   execution semantics; a generic extension point benefits any future
   embedder (crush-ast's own CLI could use it too, e.g. to route through
   `buckets` by default instead of bare host subprocess), not just
   surfer.

4. **js/bash marshaling remains separately out of scope.** Unifying the
   *sandboxing* mechanism is valuable on its own even without extending
   marshaling beyond Python — `@javascript { ... }`/`@bash { ... }` would
   at least run sandboxed via the executor, just without input/output
   variable passing, matching their current (non-marshaled) behavior on
   the built-in path today. Not a blocker on this design.

## 6. Explicitly not scoped here

- Extending `prepare_polyglot_blocks`-style marshaling to JS/bash — a
  separate, larger piece of work (needs a wired JS parser for
  free-variable analysis, already deferred once this session as
  "python-only-and-honest").
- The `numpy`/PyPI-package provisioning gap (`buckets` only resolves bare
  language runtimes, not packages) — separate, already-flagged follow-up
  from the buckets spike work.
- Actually implementing any of this — pending input from Kai's WASM/Crush
  research (may bear directly on "how should embedders customize
  execution"), and pending answers to §5's open questions.

## References

- `crates/crush-vm/src/scheduler.rs` — `EXEC_LANG` handler, the
  `polyglot.<lang>` gate, `CRUSH_RESULT_SENTINEL` scanning.
- `crates/crush-lang-sdk/src/compile.rs::prepare_polyglot_blocks` /
  `rewrite_python_marshaling` — the compile-time marshaling this design
  leaves untouched.
- `surfer-browser/crates/surfer/src/runtime/proc_capsule.rs` — the
  already-hardened `buckets` execution logic a `BucketPolyglotExecutor`
  would largely reuse.
- `docs/design/python-lowering-coverage.md` — the sibling design doc on
  which Python constructs are worth lowering natively vs. leaving on a
  subprocess path; this doc is about *which subprocess path*, not
  native-lowering coverage.
- `.jagent/planning/tickets/CRUSH-2-polyglot-capability-bypass.md` — the
  security finding whose exact failure mode (silent fallback to
  unsandboxed execution) motivates §5's "no silent fallback" rule.
