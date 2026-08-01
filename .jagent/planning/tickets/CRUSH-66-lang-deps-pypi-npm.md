# CRUSH-66 — `@lang[deps]` via buckets `pypi:` / `npm:` (CRUSH-20 follow-on)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-66 |
| **Priority** | P2 |
| **Status** | Done |
| **Phase** | M4 / buckets consumer arc |
| **Assignee** | cece |
| **Dependencies** | CRUSH-20 ✅; [BUCKETS-15](https://github.com/nixpt/buckets/pull/4) (must land on buckets `main` first) |
| **Estimated effort** | M |
| **Design** | [`docs/design/lang-deps-pypi-npm.md`](../../../docs/design/lang-deps-pypi-npm.md) |

## Problem

CRUSH-20 shipped sandboxed polyglot execution (`sandboxed-polyglot`) and
`@lang[dep,…]` syntax, but deps are **bare bottle specs only**. The ticket’s
“numpy reframe” deferred PyPI/npm because buckets had no registry resolvers.

BUCKETS-15 adds `pypi:` / `npm:` to `resolve_multi`. The follow-on is no longer
“pip inside bwrap”; it is “pass `pypi:numpy` through the same host
resolve → RO-bind cellar → network-isolated guest path bottles already use.”

## Success criteria

- [x] `@python[pypi:six] { … }` works under `sandboxed-polyglot` (live bwrap test)
- [x] `@javascript[npm:is-number@7] { … }` works under `sandboxed-polyglot`
- [x] Guest keeps `allow_network: false` (provisioning is host-side)
- [x] Bare `@python[numpy]` still fails loudly (no silent auto-prefix in v1)
- [x] Scoped `npm:@scope/name` rejected with a clear SandboxSetup error
- [x] `LangBlock.deps` / lexer comments updated (drop “NOT PyPI/npm”)
- [x] Design doc linked from TASKS / this ticket

## Non-goals

- Auto-prefix bare names by language
- In-sandbox `pip`/`npm` or writable site-packages
- Turning `sandboxed-polyglot` on by default
- Full lockfile / poetry / package-lock support

## Technical approach

See design doc. Short version:

1. Wait for BUCKETS-15 on `../../../buckets` (or path-patch for local verify).
2. Treat dep strings as opaque buckets specs in `build_sandboxed_command`
   (already `specs.extend(deps)` — may need zero code if env already carries
   PYTHONPATH/NODE_PATH; verify + add validation + tests).
3. Update docs/comments that still claim PyPI/npm are impossible.

## Files likely involved

- `crates/crush-vm/src/bucket_exec.rs`
- `crates/crush-cast/src/lib.rs` (doc only)
- `crates/crush-frontend/src/parser/lexer.rs` (doc only)
- `crates/crush-vm` live tests (`--features sandboxed-polyglot`)
- `docs/design/lang-deps-pypi-npm.md`


## Resolution (2026-08-01, cece)

**BUCKETS-15** (`pypi:`/`npm:` registry resolvers) was merged to buckets
`master` (fast-forward of `origin/agent/nixp/BUCKETS-15-lang-specs` @
`c2394a4`). With that in place, the crush-ast changes were:

1. **Lexer fix** (`crush-frontend/src/parser/lexer.rs`): the CRUSH-20
   `read_dep_list` char class (`[a-zA-Z0-9_.-]`) silently rejected `:` `@`
   `/` and semver-constraint chars, so `@python[pypi:six]` parsed as a
   malformed list and the dep was DROPPED. Added `is_dep_spec_char`
   (alphanumerics + `_ - . : @ / ^ ~ * = < >`) so every `resolve_multi`
   spec form lexes intact. 5 new lexer unit tests.

2. **Validation** (`crush-vm/src/bucket_exec.rs`): `validate_deps` rejects
   empty entries and scoped npm (`npm:@scope/name` — BUCKETS-15 v1 is
   unscoped only) up front, surfacing a clear `SandboxSetup`-phase
   `LangRuntimeError` instead of an opaque buckets resolve failure. 5 unit
   tests + 2 fast EXEC_LANG integration tests.

3. **Runtime passthrough** was already correct (CRUSH-20's
   `specs.extend(deps)` + `env = resolved.env` carry `pypi:`/`npm:` through
   `resolve_multi` → `compose_env`'s `PYTHONPATH`/`NODE_PATH` into the
   network-isolated guest). No change needed beyond validation + docs.

4. **Doc/comment updates** in `crush-cast/src/lib.rs` (`LangBlock.deps`),
   `crush-vm/src/scheduler.rs`, `crush-vm/src/vm.rs`,
   `crush-vm/src/bucket_exec.rs` — dropped the stale "buckets has no
   PyPI/npm resolution" claims.

5. **Live proof** (`--features sandboxed-polyglot --ignored`):
   `exec_lang_provisions_a_pypi_dep_into_a_network_isolated_sandbox`
   (`import six` under `allow_network: false`) and
   `exec_lang_provisions_an_npm_dep_into_a_network_isolated_sandbox`
   (`require('is-number')`) both provision cold through buckets + bwrap.

**Verified:** `cargo test -p crush-frontend --lib` 11/11 · `cargo test -p
crush-vm --features sandboxed-polyglot --lib` 138+7 pass, 3 ignored (live) ·
live tests pass with cold cache.
