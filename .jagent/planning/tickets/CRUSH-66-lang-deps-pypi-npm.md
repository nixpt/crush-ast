# CRUSH-66 — `@lang[deps]` via buckets `pypi:` / `npm:` (CRUSH-20 follow-on)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-66 |
| **Priority** | P2 |
| **Status** | Ready |
| **Phase** | M4 / buckets consumer arc |
| **Assignee** | unassigned |
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

- [ ] `@python[pypi:six] { … }` works under `sandboxed-polyglot` (live bwrap test)
- [ ] `@javascript[npm:is-number@7] { … }` works under `sandboxed-polyglot`
- [ ] Guest keeps `allow_network: false` (provisioning is host-side)
- [ ] Bare `@python[numpy]` still fails loudly (no silent auto-prefix in v1)
- [ ] Scoped `npm:@scope/name` rejected with a clear SandboxSetup error
- [ ] `LangBlock.deps` / lexer comments updated (drop “NOT PyPI/npm”)
- [ ] Design doc linked from TASKS / this ticket

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
