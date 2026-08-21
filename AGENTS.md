# crush-ast

Standalone Crush language toolchain: CAST IR, tree-sitter grammar, polyglot
walkers, compiler frontend, VM runtime, package manager, installer.
Extracted from `exosphere` on 2026-06-12 as a peer project — no path
dependencies run in either direction; exosphere pins its own `crush-cast`/
`casm` copies separately.

Full contribution rules live in `CONTRIBUTING.md` — read that first for
anything beyond a small fix. This file is the condensed working-context
version for an agent mid-task. (Hand-authored 2026-08-21, not yet generated
via `dejavue export --target codex` — running that later will append a
managed block below this content without clobbering it; see CONTRIBUTING.md.
`CLAUDE.md` carries the same content for Claude specifically.)

## Operating rules (see CONTRIBUTING.md for the full reasoning on each)

- Dependency DAG stays acyclic: `crush-frontend` → `crush-cast` → `casm` →
  `crush-errors`. Never add a back-edge.
- Internal deps use `.workspace = true` with both `path` + `version` in
  `[dependencies]` (not just `path`) — a real dep missing `version` blocks
  that crate from ever publishing to crates.io. Dev-deps are exempt.
- A `[lib]` crate depended on by other in-workspace crates stays
  `crate-type = ["lib"]` — never add `cdylib`/`staticlib` to it (breaks the
  `-C extra-filename` hash Cargo needs when the same crate builds as two
  units, e.g. target-graph + a proc-macro's host-graph copy).
- One capability, one shared implementation, every backend calls into it —
  see `io_print.rs`/`io_read.rs`. `len()` diverging between the VM (errors on
  strings) and the AOT C backend (handles them) is the bug class this
  prevents (CRUSH-114).
- Capability-based security: every VM-external op is an explicit, named,
  opt-in capability (`--cap io.read`) — never ambient.
- Never edit the shared checkout directly — use a worktree
  (`kitchen enter <NAME>` / `kitchen ship --pr` / `kitchen clean`, or a plain
  `git worktree add` if squadron isn't on PATH).
- Live-verify before calling something done: compile it, run it against real
  input, check the actual output. Tests passing and CI green are both
  necessary, neither is sufficient — this repo's own history (a trailing-
  return codegen bug, a `@decision` field-name parser hang, the `len()`
  divergence above) was found by agents choosing to run something rather
  than trust that it compiled.

## Build / Test

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets
cargo run --bin crushc -- --help
cargo run --bin crush-run -- my-program.crush
```

Version is automatic (`bump-version` — every push to `main` bumps + tags from
conventional-commit prefixes since the last tag). Never hand-edit the version
in `Cargo.toml`.

## Architecture map

```
source.(crush|py|rs|go|c|js|sh|zig|wasm|...)
  → walker (tree-sitter → CAST, crates/crush-lang-*)
  → CAST (crush-cast — JSON IR)
  → crush-frontend (parse/semantics/optimize/compile)
  → CASM (casm — assembly, JSON or binary .castb)
  → execution: CVM1 PortableVm (crush-vm) | FastVM | crush-jit (Cranelift)
              | AOT C/Rust (crush-aot, crush-aotc) | PTX (crush-ptx)
```

43 crates total as of 2026-08-21 (see `README.md`'s Repository Structure for
the full annotated list). `crates/crush-bucketspike` is a throwaway spike,
not release surface.

## Examples

`examples/crush/` — 40+ `.crush` programs, from language-feature smoke tests
to full self-playing programs (games, two hosted-language interpreters)
written by several different LLMs given the same prompt. Story + per-model
findings: [`nixpt/awesome-crush`](https://github.com/nixpt/awesome-crush).

## Memory

This repo uses [dejavue](https://github.com/nixpt/dejavue) for persistent
architectural context — decisions, invariants, constraints not derivable
from the code alone. Run `dejavue context` before non-trivial changes,
`dejavue recall <query>` to search, `dejavue decision "<title>" --reason
"..."` to record a real one as you make it. Fallback if not on PATH:
`python3 .dejavue/dejavue context`. `.jagent/planning/` holds the *what/when*
(ROADMAP, TASKS, `CRUSH-N` tickets) as a separate concern from dejavue's *why*.
