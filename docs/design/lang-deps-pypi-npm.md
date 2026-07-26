# Design: `@lang[deps]` → buckets `pypi:` / `npm:` (CRUSH-66)

**Status:** proposed  
**Date:** 2026-07-25  
**Depends on:** CRUSH-20 (shipped), [BUCKETS-15](https://github.com/nixpt/buckets/pull/4)  
**Ticket:** `.jagent/planning/tickets/CRUSH-66-lang-deps-pypi-npm.md`

## Why now

CRUSH-20 deliberately deferred the “numpy reframe”: buckets could only
provision bare language runtimes, so `@python[numpy]` could not mean a
PyPI package. BUCKETS-15 adds `pypi:` / `npm:` resolvers that install into
the **shared buckets cellar** via `resolve_multi` — the same path
`bucket_exec` already uses for `python@3.11` / `node@20`.

That collapses the old plan (“`pip install` inside bwrap with
`allow_network: true`”) into the bottle pattern:

1. Host: `resolve_multi(["python@3.11", "pypi:numpy@1.26"])` → cellar
2. Guest: RO-bind installations + `PYTHONPATH` / `NODE_PATH` from
   `compose_env`; **keep `allow_network: false`**

No in-sandbox package manager. No persistent writable site-packages in
the wick. Cold resolve still uses CRUSH-19’s deadline shape.

## Syntax

Keep the existing `@lang[dep, …] { … }` grammar. Change **what a dep
string means**:

| Form | Meaning | Example |
|------|---------|---------|
| `pypi:<pkg>[@ver]` | Buckets PyPI install | `@python[pypi:numpy@1.26]` |
| `npm:<pkg>[@ver]` | Buckets npm install | `@javascript[npm:is-number@7]` |
| `cargo:<crate>[@ver]` | Buckets cargo binary (optional stretch) | `@bash[cargo:ripgrep]` |
| bare bottle alias | Existing CRUSH-20 behavior | `@python[openssl@^1.1]` |

**v1 rule — no silent auto-prefix.** Bare `numpy` stays a bottle-spec
lookup and fails loudly if unknown (today’s behavior). Callers must write
`pypi:numpy`. Rationale: auto-prefixing by lang (`@python` → assume
PyPI) hides mistakes and fights bottle companion names.

Optional later convenience (out of scope for CRUSH-66): a lint that
suggests `pypi:` when a bare name fails resolve.

## Layering (unchanged from CRUSH-20)

- `crush-vm` already owns the `buckets` path-dep behind
  `sandboxed-polyglot`.
- Extend `bucket_exec::build_sandboxed_command` only:
  - pass deps through `resolve_multi` unchanged (prefix already in the
    string)
  - ensure `compose_env`’s `PYTHONPATH` / `NODE_PATH` survive into the
    sandbox env (already copied from `resolved.env`)
  - keep `allow_network: false` for the guest
- Parser / `LangBlock.deps` need **doc + comment updates only** — no new
  AST field. Specs are opaque strings today.

## Validation

Light validation in `bucket_exec` (or a small helper) before resolve:

1. Reject empty dep strings.
2. Reject scoped npm (`npm:@types/node`) until buckets supports it
   (BUCKETS-15 v1 limitation) — map to a clear `LangRuntimeError` /
   `SandboxSetup` phase message.
3. Optional: warn when `@python[npm:…]` / `@javascript[pypi:…]` (cross-
   ecosystem) — allow but document as unusual.

## Failure modes (CRUSH-18 shapes)

| Phase | Example |
|-------|---------|
| `SandboxSetup` | unknown `pypi:` package (404), resolve deadline, bwrap missing |
| guest exception | `import numpy` succeeds but user code raises |

Provisioning failures stay `SandboxSetup`; do not look like guest
exceptions.

## Feature gate

Remains behind `sandboxed-polyglot`. Without the feature, deps stay
silently unused (CRUSH-20 behavior) — document that in the ticket and in
`LangBlock` docs so notebook authors know to enable the feature.

## Non-goals

- Auto-`pip install` / `npm install` inside the guest namespace
- Writable, mutable site-packages across sparks
- Full transitive lockfiles / poetry / package-lock as first-class input
- Changing default-on for `sandboxed-polyglot`
- Surfer / exo-light unification

## Acceptance sketch

- [ ] `@python[pypi:six] { import six; print(six.__version__) }` under
      `sandboxed-polyglot` + bwrap succeeds (live test, like CRUSH-20’s)
- [ ] Guest sandbox still network-isolated (`allow_network: false`)
- [ ] Bare `@python[numpy]` still fails with a resolve/unknown-package
      error (no silent success)
- [ ] `@javascript[npm:is-number@7]` live smoke
- [ ] Docs: `LangBlock.deps`, CRUSH-20 ticket “numpy reframe” → superseded
      by CRUSH-66, `docs/design/` this file linked from TASKS

## Implementation touch list

- `crates/crush-vm/src/bucket_exec.rs` — comments + optional validation;
  confirm env passthrough for PYTHONPATH/NODE_PATH
- `crates/crush-cast/src/lib.rs` — update `deps` doc comment
- `crates/crush-frontend/src/parser/lexer.rs` — update “NOT PyPI/npm”
  comment
- `crates/crush-vm` tests — live sandbox proof with `pypi:six` /
  `npm:is-number`
- Depends on buckets main containing BUCKETS-15 (`../../../buckets`)
