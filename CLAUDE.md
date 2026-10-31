# crush-ast

Standalone Crush language toolchain: CAST IR, tree-sitter grammar, polyglot
walkers, compiler frontend, VM runtime (CVM1 / FastVM / JIT / AOT), debugger,
GUI, package manager, installer. Published to crates.io as `crush-lang-sdk` and
friends (the toolchain is **not** the unrelated `crush` crate).

Full contribution rules live in `CONTRIBUTING.md` — read that first for
anything beyond a small fix. This file is the condensed working-context
version for an agent mid-task. (Hand-authored 2026-08-21, not yet generated
via `dejavue export --target claude` — running that later will append a
managed block below this content without clobbering it; see CONTRIBUTING.md.
`AGENTS.md` carries the same content for non-Claude tools.)

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
- Never run an example with stdin left open. `examples/crush/blackjack_interactive.crush`
  calls `io.read()`, which blocks forever on an inherited pipe nobody writes to, and that
  hangs the whole agent session. Run examples (singly or in a loop over
  `examples/crush/*.crush`) as `timeout 60 crush-run run … < /dev/null`: at EOF `io.read()`
  returns `""` and the game exits. To play it, pipe the moves in (`printf '10\ns\nq\n' | …`).
  The same goes for any program using `io.read`. Tests must set `Stdio::null()` or piped stdin
  (as `examples_parity.rs` does).
- Ticket IDs: get one with `cargo run -p xtask --bin ticket-id -- next` (it
  counts ticket files, branch names on origin and locally — every worktree —
  and PR titles). CI fails on a duplicate (`xtask/tests/ticket_ids.rs`).
- `.dejavue/*` (and `CHANGELOG.md`) are appended to by almost every PR. The
  `.dejavue` files are `merge=union` in `.gitattributes`, which **GitHub does
  not apply**: a PR whose only overlap is there shows "conflicting" on GitHub
  while a local merge is clean. Merge `origin/main` into the branch locally and
  push.
- Live-verify before calling something done: compile it, run it against real
  input, check the actual output. Tests passing and CI green are both
  necessary, neither is sufficient — this repo's own history (a trailing-
  return codegen bug, a `@decision` field-name parser hang, the `len()`
  divergence above) was found by agents choosing to run something rather
  than trust that it compiled.

## Build / Test

`crush-pkg` and `crush-vm` path-depend on a sibling `nixpt/buckets` checkout
(`git clone https://github.com/nixpt/buckets ../buckets`; CI does the same).

```bash
cargo build                    # default members: everything but crush-gui
cargo test
cargo clippy --all-targets
cargo test -p crush-gui        # needs pkg-config + libfontconfig1-dev
cargo run --bin crushc -- --help
cargo run --bin crush-run -- run my-program.crush
```

`--workspace` includes `crush-gui` (arniko/bliss: Stylo, vello, winit), so CI
runs every `--workspace` step with `--exclude crush-gui` and has a separate
`Test (gui)` job.

Releases are manual: dispatch `release.yml`. It bumps the workspace version
from conventional-commit prefixes since the last tag (`feat:` → minor, else
patch; a breaking change on 0.x bumps **minor** — 1.0.0 needs the `allow_major`
input), tags `vX.Y.Z`, creates that tag's GitHub **pre-release** and dispatches
`artifacts.yml` (Linux binaries + crush-web pkg + `SHA256SUMS`). Then dispatch
`publish.yml` with the tag to publish to crates.io (Trusted Publishing; a crate
never published before needs a manual first `cargo publish`). Never hand-edit
the version in `Cargo.toml`.

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

44 workspace members as of 2026-10-31 (`cargo metadata --no-deps`: 43 under
`crates/` plus `xtask`; 43 default members — all but `crush-gui`; see
`README.md`'s Repository Structure for the full annotated list). Not members: `crates/crush-web` and `crates/crush-vm-py`
(`exclude`d in the root `Cargo.toml`) and `crates/crush-bucketspike` (its own
`[workspace]`; a throwaway spike, not release surface).

## Examples

`examples/crush/` — 50+ `.crush` programs, from language-feature smoke tests
to full self-playing programs (games, two hosted-language interpreters)
written by several different LLMs given the same prompt. Story + per-model
findings: [`nixpt/awesome-crush`](https://github.com/nixpt/awesome-crush).

## Memory

This repo uses [dejavue](https://github.com/nixpt/dejavue) for persistent
architectural context — decisions, invariants, constraints not derivable
from the code alone. Run `dejavue context` before non-trivial changes,
`dejavue recall <query>` to search, `dejavue decision "<title>" --reason
"..."` to record a real one as you make it. Not on PATH? Use a resolver
such as `jagent-dejavue context`, or install it:
`pipx install git+https://github.com/nixpt/dejavue`. `.jagent/planning/` holds the *what/when*
(ROADMAP, TASKS, `CRUSH-N` tickets) as a separate concern from dejavue's *why*.
