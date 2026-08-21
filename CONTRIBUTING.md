# Contributing to crush-ast

This file has two audiences: humans and AI coding agents. Both are welcome
and, in this repo specifically, both have already contributed real, merged
work — several of `examples/crush/`'s programs, and a chunk of the current
capability surface, were written by dispatched LLM agents, not by hand. The
expectations below are mostly the same for both; where they differ, it's
called out.

## Before you start

1. Read `README.md` for the project's actual shape (CAST IR → CASM →
   CVM1/JIT/AOT, the polyglot walker crates, the language frontend).
2. Check `.jagent/planning/ROADMAP.md`, `TASKS.md`, and `tickets/` — most
   non-trivial follow-up work already has a `CRUSH-N` ticket with a problem
   statement, a reproduction (for bugs) or an approach (for new capabilities),
   and explicit success criteria. Read the relevant ticket before starting;
   it may already have decided things you'd otherwise re-litigate, or name a
   dependency/gate you'd otherwise miss.
3. Run `dejavue context` before making a non-trivial change — it surfaces
   recorded decisions, invariants, and constraints that aren't obvious from
   reading the code alone.
4. If what you're about to do isn't covered by an existing ticket, mint one
   (`.jagent/planning/tickets/CRUSH-N-*.md`, next free ID from
   `BACKLOG-INDEX.md`) or open a GitHub issue before writing code, especially
   for anything that adds a new capability, a new backend, or touches more
   than one crate.

## The rules that actually matter (enforced, not just suggested)

- **Dependency DAG stays acyclic**: `crush-frontend` → `crush-cast` → `casm`
  → `crush-errors`. Never add a back-edge.
- **Internal crate deps use `.workspace = true`** in `[dependencies]`, with
  both a `path` and a `version` (see `[workspace.dependencies]` in the root
  `Cargo.toml`) — a real dependency with only a bare `path` and no version
  blocks that crate from ever publishing to crates.io (this exact mistake in
  `crush-ptx` silently stalled the whole workspace's crates.io publish
  pipeline for 50+ consecutive runs before it was caught — see
  `.dejavue/decisions.md` if you want the full story). Dev-only path deps
  don't need a version pin; crates.io allows those unversioned.
- **A crate that's both a real `[lib]` and depended on by other in-workspace
  crates must stay `crate-type = ["lib"]`** — never add `cdylib`/`staticlib`
  to it. Cargo drops the `-C extra-filename` hash suffix for any target whose
  crate-type includes a non-rlib output, which collapses the rlib to one
  fixed path; a workspace that builds two units of the same crate (a normal
  target-graph build plus a host-graph one via a proc-macro crate, for
  example) needs that hash to avoid colliding. See `crush-vm/src/vm.rs`'s own
  `[lib]` comment for the concrete case this bit.
- **A capability implemented for one backend must be implemented identically
  for every backend, from one shared source of truth.** `io_print.rs` and
  `io_read.rs` exist specifically so the scheduler, `PortableVm`, and the AOT
  codegens can't silently disagree — `len()` diverging between the VM (errors
  on strings) and the AOT C backend (handles them fine) is the bug class this
  pattern exists to prevent (CRUSH-114). If you add a capability, add its
  shared logic in one file and have every backend call into it, not
  reimplement it.
- **Capability-based security**: every VM-external operation (I/O, network,
  filesystem, process) is gated behind an explicit, named, opt-in capability
  (`--cap io.read`, etc.) — never ambient. A new capability follows the
  `caps.rs`/`stdlib.rs` registry pattern; it doesn't get a free pass into the
  interpreter without being named there.
- **Never edit the shared checkout directly.** Every non-trivial change goes
  through a worktree (`kitchen enter <NAME>` / `kitchen ship --pr` /
  `kitchen clean` if you have `squadron/bin` on PATH; a plain
  `git worktree add ../crush-ast-<name> -b <branch>` otherwise). The shared
  checkout is for reading, building, and reviewing — not for accumulating
  uncommitted work that the next person to `cd` in there has to work around.

## Workflow

```bash
cargo build --workspace
cargo test --workspace                    # verified 2026-08-21: no exclusions needed
cargo clippy --workspace --all-targets
cargo run --bin crushc -- --help          # or: cargo run --bin crush-run -- my-program.crush
```

A feature isn't done when `cargo test` passes — it's done when it also
compiles and runs a real `.crush` program through the real toolchain. Every
program in `examples/crush/` this repo has taken from a dispatched agent was
independently compiled, run, and hand-checked against expected output before
being merged, not accepted on the agent's own "it works" report. PRs are
expected to show the same discipline — paste the actual compile+run output
in the PR description, not just "tests pass."

Versioning is automatic (`bump-version`, installed via
`scripts/bump-version.sh` + `.github/workflows/release.yml`) — every push to
`main` bumps `[workspace.package] version` and tags `vX.Y.Z` based on
conventional-commit prefixes since the last tag (`feat:` → minor,
`fix:`/anything else → patch, `BREAKING CHANGE`/`!` → major). **Never
hand-edit the version in `Cargo.toml`.**

## Opening a PR

- PRs land on `main` via a worktree, not a long-lived integration branch —
  this repo doesn't use a `dev`/`main` split.
- One logical change per PR. A new capability and an unrelated refactor
  don't belong in the same PR.
- Update `README.md`/the relevant `.jagent/planning/` files in the SAME PR as
  the code, not as a follow-up — a stale README claiming a crate or a
  capability doesn't exist (or vice versa) is exactly the class of drift this
  repo has had to clean up more than once.
- If your PR closes a `CRUSH-N` ticket, say so in the description and update
  the ticket's own `Status` field.
- Before opening: check whether your branch is behind `main` by more than a
  commit or two and rebase — a stale base is the single most common reason a
  PR that was mergeable an hour ago shows up `CONFLICTING` (usually just an
  append-only `.dejavue/timeline.jsonl` collision, trivially resolved by
  rebasing, but it blocks the merge until you do).

## For AI coding agents specifically

- Read `CLAUDE.md`/`AGENTS.md` — whichever matches your tool. Both describe
  the same rules as this file, condensed for a working-context load rather
  than a one-time read.
- Run `dejavue context` before non-trivial changes, and record real decisions
  as you make them (`dejavue decision "<title>" --reason "..."`) — a genuine
  architectural choice (which backend a new capability's shared logic lives
  in, why a rewrite instead of a verbatim restore, an EOF convention) is worth
  it; a small mechanical fix isn't.
- **Work in a worktree, not the shared checkout.** If you're dispatched
  directly into a shared clone rather than a fresh worktree, create one
  yourself before writing anything (`kitchen enter <NAME>` if available).
  This isn't a style preference — an in-progress, uncommitted change sitting
  in a shared checkout blocks or confuses whoever needs that checkout clean
  next, agent or human.
- **Live-verify before reporting something as done.** "The tests pass" is
  not the same claim as "I compiled it and ran it against real input and
  checked the output." This repo's own history has multiple real bugs (a
  trailing-return codegen bug, a parser hang on one `@decision` field name,
  `len()` diverging across backends) that were found specifically by an agent
  choosing to run something rather than trust that it compiled.
  CI passing is not the same claim either — a green check can mean the
  feature works, or it can mean the test never actually exercised the path
  that's broken.
- **Don't duplicate in-flight work.** Before starting a `CRUSH-N` ticket,
  check `.jagent/planning/TASKS.md`'s Status field and `gh pr list` for an
  open PR against the same ticket — two independent implementations of the
  same capability landing on the same day is wasted work for whoever has to
  reconcile them.
- Don't invent scope. If a task implies a much bigger change than asked
  (a new backend, a new language walker), say so and confirm before building
  it, rather than silently expanding the PR.

## What NOT to contribute

- A new capability that only implements one backend (VM-only, or AOT-only) —
  see the shared-source-of-truth rule above. `io_read.rs`/`io_print.rs` are
  the template; a capability that skips it will diverge the way `len()` did.
- Dependencies added for convenience rather than necessity, especially in
  `crush-vm` or `crush-frontend` — both are meant to stay lean since they're
  the parts every consumer (exosphere, buckets, the standalone binaries)
  actually links against.
- Any change to a `[lib]` crate's `crate-type` without checking whether
  another in-workspace crate depends on it as an rlib first (see the
  `cdylib`/`staticlib` rule above).
