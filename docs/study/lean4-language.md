# Study: Lean 4 for the Crush language side, and an io_uring backend for crush-ast

Status: study note, design only. No code changes. Tickets below are **proposed, not filed**;
filing waits on the captain.
Date: 2026-11-20. Author: altair (crush lane).
Sources: crush-ast `origin/main` 9891781a; Lean 4 at leanprover/lean4 @ 450d6b1 (shallow clone,
read only, nothing built or run). Lean refs are `path:line` into that clone.

Parts: 1 io_uring proposal, 2 Lean lessons (adopt/adapt/skip), 3 proposed tickets T1-T14, 4 proof as a
lever (P1-P5, tickets T15-T18, and the "260k" source check).

Ranking key: **ADOPT** = take the idea as is; **ADAPT** = take the idea, reshape it for Crush;
**SKIP** = understood, not worth it for us.

---

## Part 1: io_uring for crush-ast

### 1.1 Where crush-ast does IO today

| Area | Where | How |
|---|---|---|
| File caps (`fs.*`) | `crates/crush-lang-sdk/src/fs_tools.rs:32` (`register`) | Every call goes through `FsSandbox` → `Arc<dyn FsBackend>` |
| Backend trait | `crates/crush-lang-sdk/src/fs_backend.rs:49` (`trait FsBackend`) | 16 sync methods (canonicalize … copy_file) |
| Disk backend | `fs_backend.rs:84` / `:98` (`DiskFs`) | Plain `std::fs` |
| Memory backend | `fs_backend.rs:289` (`MemFs`) | In-memory tree; error text matches Linux on purpose (`:174-201`) |
| Backend choice | `crates/crush-lang-sdk/src/host_caps.rs:98-125` (`fs`, `fs_root`, `fs_memory*`), `:319` | `HostCapsBuilder` picks the backend; `FsSandbox::new` defaults to `DiskFs` (`:611`) |
| Text caps (`text.*`) | `crates/crush-lang-sdk/src/text_tools.rs:142` (`walk_files`), `:164` (`text.grep`) | Recursive sorted `read_dir`, then a serial open+read per file |
| TCP caps (`tcp.*`) | `crates/crush-net/src/caps.rs` (header `:18`: "Caps are synchronous"), `tcp.accept` `:282` | Blocking calls on the VM thread |
| TCP accept | `crates/crush-net/src/tcp.rs:239` (`accept_until`) | Non-blocking accept in a loop with `sleep(5 ms)` (`:257`) |
| TCP recv | `tcp.rs:102-110` | Toggles the socket to blocking + `read_timeout` |
| Net reactor | `crates/crush-net/src/reactor.rs:29` (`POLL_INTERVAL` 10 ms), `:170`, `:215` | Zero-dependency polling loop: `try_read`/`try_write`/`try_accept`, then sleep |
| Green threads | `crates/crush-vm/src/scheduler.rs` | `spawn` = green threads on one scheduler; a blocking cap stalls all of them (issue #506) |
| Wall-clock limit | `scheduler.rs:175` (`run_with_wall_clock_limit`), sleep at `:226` | Polls a child with a 10 ms sleep |

crush-ast has no io_uring, tokio, mio, glommio or monoio today. The two places where IO cost or
latency is set by our own code, not by the kernel, are **the sleep-polled network path** and
**`text.grep`'s serial per-file reads**.

### 1.2 Proposal

One rule for every piece: **io_uring is a backend, never a new capability.** Scripts see the same caps,
with the same grants and the same sandbox checks. The backend sits below `FsSandbox` and below the
`tcp.*` caps, so capability gating does not change and cannot be bypassed by choosing a backend.

Gating:
- Cargo feature `io-uring` on `crush-lang-sdk` and `crush-net`, off by default.
- `#[cfg(target_os = "linux")]` on top of the feature; other platforms build the fallback only.
- Runtime probe: if `io_uring_setup` fails (old kernel, seccomp, container with io_uring disabled,
  `kernel.io_uring_disabled=2`), fall back silently to the current path and record it once
  (`crush doctor` shows which backend is live).
- Dependency: the `io-uring` crate (raw, no runtime). It fits the zero-dep stance of crush-net better
  than tokio-uring/glommio/monoio, which bring their own executors and would fight the VM scheduler.

**Seam A: `crush-net` completion backend (first, tied to #506).**
- Replace the `sleep`-poll in `reactor.rs` and in `accept_until` with a ring: `ACCEPT` (multishot where
  the kernel has it), `RECV`/`SEND`, and `LINK_TIMEOUT` for the deadlines the caps already take.
- Keep `trait Source` as the seam; the existing poller stays as the fallback implementation.
- The bigger prize is #506: with completions, a green thread that calls `tcp.recv` can **park** in the
  scheduler until its completion arrives, instead of blocking the whole VM. That needs the design choice
  #506 asks for (non-blocking caps integrated with the scheduler vs host-cap worker VMs). This proposal
  takes side 1 of #506 and gives it a concrete backend; it does not replace the #506 design note.
- Honest note: most of the latency win comes from being event-driven at all. epoll would remove the
  5–10 ms sleeps too. io_uring adds batched submission, completion-based recv into owned buffers,
  multishot accept and one model for both file and net IO. If the captain wants the smallest step, an
  epoll backend behind the same `Source` seam is a valid first stage; the test below is the same.

**Seam B: `UringFs: FsBackend` (second).**
- A third backend next to `DiskFs` and `MemFs`, chosen by a new `HostCapsBuilder::fs_uring()` (or by the
  feature flipping the default in `FsSandbox::new`, `host_caps.rs:611`).
- Path resolution, canonicalisation and the sandbox checks stay where they are (`host_caps.rs:420-533`),
  so a uring backend cannot widen what a script can reach.
- Win: batched `OPENAT` + `STATX` + `READ` + `CLOSE` chains for `text.grep` over a tree and for
  `fs.cat`/`text.head|tail|wc` on many files. Needs a small trait addition such as
  `read_many(&[PathBuf]) -> Vec<io::Result<String>>` with a default that loops `read_to_string`, so
  `DiskFs` and `MemFs` need no changes.
- Not a win: `fs.find`/`fs.ls`. io_uring has no getdents op, and `find` (`fs_tools.rs:265-297`) already
  uses entry kinds with no stat calls. Leave them on `read_dir`.

**Seam C: child wait via pidfd (minor).** `run_with_wall_clock_limit` (`scheduler.rs:175`) can wait on a
pidfd (`POLL_ADD` on the ring, or plain `poll`) instead of a 10 ms sleep loop. Small, and independent of
the ring work.

### 1.3 First measurable win, and the test that guards it

**Win: TCP round-trip latency on the loopback.** Today `tcp.accept` + `tcp.recv` latency is bounded
below by the 5 ms accept sleep and the 10 ms reactor poll. Benchmark: a Crush echo server plus a Rust
client, 1000 round trips of 64 B and 64 KiB, p50/p99, both backends, same binary.
Expected: p50 drops from milliseconds to tens of microseconds. That is a large, easy-to-measure change,
which is why it goes first.

**Parity test (identical results):** `crates/crush-net/tests/backend_parity.rs`, run under
`--features io-uring` on Linux:
- the same echo script on both backends; assert byte-identical transcripts, identical cap error values
  for timeouts, refused connections and peer resets, and identical deadline behaviour (within a band);
- skipped (not failed) when the runtime probe falls back, with the skip reason printed.

Second win, for Seam B: `text.grep` over a 10k-file synthetic tree (cold and warm cache), wall time
`UringFs` vs `DiskFs`, with a parity test that runs the whole existing fs/text cap test set against
`DiskFs`, `MemFs` and `UringFs` and asserts identical results **and identical error text** (the
`MemFs` precedent at `fs_backend.rs:174-201`).

CI note: GitHub-hosted Linux runners allow io_uring today, but some container setups do not. The parity
tests must treat "probe fell back" as a skip so CI on those hosts stays green, and one job must assert
the probe succeeded so we know the uring path really ran.

---

## Part 2: Lean 4 lessons for the Crush language and toolchain

Lean paths are relative to `src/` in the clone; Crush paths are relative to `crates/`. Lean's compiler is
part-way through a migration: the new LCNF pipeline (`Lean/Compiler/LCNF/`) carries ExplicitRC,
ResetReuse and EmitC, and the old `Lean/Compiler/IR/` is still there. Refs below cite LCNF.
Not covered: `crush-lsp` is not checked out on this box, so the LSP comparison (F9) only describes Lean's side.

### Summary

| # | Lesson | Verdict |
|---|---|---|
| F6 | Small trusted core: re-check the cap set in front of every engine | **ADOPT** |
| F12 | `#guard_msgs`-style golden diagnostic fixtures | **ADOPT** |
| F13 | Error explanations keyed by code (`crush explain`) | **ADOPT** |
| F1 | Hygienic user macros (narrow form) | ADAPT |
| F2 | Borrowed-parameter inference → fewer value clones | ADAPT (measure first) |
| F4 | Optimizer as a pass list + dead-branch pass | ADAPT |
| F5 | IR checker between lowerings | ADAPT |
| F7 | Lake content traces → crush-pkg build cache | ADAPT |
| F8 | Lockfile carries a tree hash | ADAPT |
| F10 | Cooperative cancel token for `spawn`/`parallel_map` | ADAPT |
| F11 | Purity inferred from cap inference | ADAPT |
| F3 | Reset/reuse (FBIP) | SKIP |
| F9 | Server snapshots, reuse by syntax | SKIP for now |

### ADOPT

**F6: small trusted core (the kernel lesson applied to capabilities).**
Lean re-checks every declaration the untrusted elaborator produces before adding it to the environment
(`kernel/environment.cpp:131-185`, `check_constant_val` runs a `type_checker`). A separate checker replays a
compiled library from `.olean` files without running the elaborator at all (`LeanChecker.lean:17-44`,
`replayFromImports`).
Crush has three cap layers, and they are uneven:
- static inference over bytecode (`crush-vm/src/cap_inference.rs`, `capabilities_used`), compared with
  `capsule.toml` by crush-pkg (`crush-pkg/src/capcheck.rs`, called at `builder.rs:487`);
- the VM's per-call gate (`crush-vm/src/scheduler.rs:89-101`, `check_cap_permitted`), spread across at least five
  call sites in `portable_vm.rs` (around `:1230`, `:1266`, `:1991`, `:2283`, `:2328`);
- **no gate in AOT**: `cap_call` is emitted straight to code (`crush-aot/src/codegen.rs:1221`), and nothing in
  `crush-aot/src` reads the manifest permissions or the privileged list.

Lean's shape: one small, independent checker in front of every consumer. For us that is
`verify_caps(&Program, &Grants)`, built on the existing `capabilities_used` plus `is_privileged` and called
at VM load, JIT load and AOT compile. Keep the per-call checks as defence in depth.
Residual gap, to be stated, not hidden: the inference under-reports `spawn` targets computed at run time
(header of `cap_inference.rs`). So the gate is complete for static programs, not a proof.

**F12: golden diagnostic fixtures.** In Lean, the expected messages sit in the source above the command,
and the test fails with a diff (`Lean/Elab/GuardMsgs.lean:1-30`); Lean's whole test suite works this way.
Crush has coded diagnostics (`E-MUT-001/002` in `crush-frontend/src/mutation_check.rs`, `E-EXH-001`,
`E-VIS01`, `E-TP01`, `E-IM01`, `E-RT01..05`, …; about 14 in use), but the tests are Rust asserts on
message substrings. Proposal: `.crush` fixtures with `// expect:` headers, one harness, `UPDATE_EXPECT=1`.

**F13: error explanations.** Lean stores a long explanation with examples for each named error
(`Lean/ErrorExplanation.lean:27-54`). Crush has the codes but no `explain` command. Proposal: `crush explain
<CODE>`, one markdown page per code. The page's bad/good snippets double as F12 fixtures, so the docs
cannot rot.

### ADAPT

**F1: hygienic macros.** Lean registers a `Macro` per syntax kind through an attribute
(`Lean/Elab/Util.lean:134-151`) and runs each expansion under a fresh macro scope (`:160-169`,
`withFreshMacroScope`). Identifiers the macro introduces get the scope appended
(`Init/Prelude.lean:5896`, `addMacroScope`).
Crush has no user-level macros. `crush-macros` (`crush-macros/src/lib.rs:28-50`) embeds Crush in Rust
(`crush!`, `crush_file!`), which is a different thing. The parser is one hand-written recursive descent
(`crush-frontend/src/parser/mod.rs`).
Take only the narrow part: `macro name(pattern) => template`, expanded between parse and compile, with
per-expansion fresh binders and a recursion limit. Do **not** take open-ended syntax extension
(`declare_syntax_cat`): tree-sitter-crush and the polyglot walkers need one fixed grammar.

**F2: borrowed parameters.** `Lean/Compiler/LCNF/InferBorrow.lean:19-30` assumes every parameter is borrowed
and marks it owned only where it is consumed; ExplicitRC then skips the `inc`.
Crush containers are already `Rc` aliases, but `Value::Str`, `Tuple` and `Bytes` are by value
(`crush-vm/src/vm.rs:384-415`) and get cloned on load and dup (`portable_vm.rs:863`, `:906`).
A read-only-parameter analysis over CAST could avoid those clones. The gain is unmeasured, so count the
clones first.

**F4: optimizer as a pass list.** LCNF is a registered list of small passes (`LCNF/Passes.lean`,
`PassManager.lean`), including `ElimDeadBranches`, `CSE` and `ExtractClosed`. `crush-frontend/src/optimizer.rs`
is one recursive constant-folding pass. Split it into named passes that can each be switched off (so a
miscompile can be bisected), and add dead-branch elimination on literal `if`/`match`.

**F5: IR checker between lowerings.** Lean re-validates IR between passes (`Lean/Compiler/IR/Checker.lean`,
`LCNF/Check.lean`). crush-aot finds bad CASM late, at codegen (`crush-aot/src/codegen.rs:1227`, `unsupported`).
Add `casm::check(&Program)` (stack balance, branch targets, `cap_call` argc, const indexes), run after
lowering in tests and debug builds, and always before AOT.

**F7: Lake build traces.** Lake rebuilds a target only if the mixed hash of its inputs differs from the saved
trace (`lake/Lake/Build/Trace.lean:64`, `:318-322`; `lake/Lake/Build/Common.lean:213-276`,
`checkHashUpToDate'`, `replayIfUpToDate`). crush-pkg recompiles on every `build`: `crush-pkg/src/builder.rs`
has no up-to-date logic. Take the trace file (sha256 over sources, dep traces, git revs, compiler version and
features), not Lake's job framework.

**F8: lockfile tree hash.** The Lake manifest says exactly how to materialise each dependency and keeps old
schema versions readable (`lake/Lake/Load/Manifest.lean:55-90`). `capsule.lock` (`crush-pkg/src/gitdep.rs:14-46`)
records url/ref/rev only. `crush-pkg/src/merkle.rs` already computes tree hashes but is "not yet wired into
the CLI" (`merkle.rs:1-12`). Add `tree_sha256`, so both a moved tag and a tampered cache are detected.

**F10: cooperative cancellation.** `IO.CancelToken` (`Init/System/CancelToken.lean`) is a first-class value,
and long loops poll it (`runtime/io.cpp:1613-1648`). Crush cancels only by kill or deadline: one wall-clock
deadline per run (`crush-vm/src/scheduler.rs:535-538`), `task.stop` kills a process, and `parallel_map`
workers cannot be stopped (`crush-vm/src/parallel.rs:1-9`). Add `cancel(handle)`, checked at loop back-edges and
`cap_call` (where quotas are already checked).

**F11: purity from cap inference.** Lean puts effects in types (`IO` vs pure). Lean is not
capability-secure, so we take the discipline, not the model. `cap_inference.rs` already collects per-function
facts. Expose `is_pure(fn)` (empty transitive cap set), warn when `parallel_map` gets an impure function, and
let F4 CSE pure calls.

### SKIP

- **F3 reset/reuse** (`LCNF/ResetReuse.lean:20-37`, `ExpandResetReuse.lean`): an answer to value semantics plus
  RC. Crush containers are shared `Rc<RefCell>` cells, so there is nothing to reuse.
- **F9 server snapshots** (`Lean/Server/Snapshots.lean`, `Lean/Language/Lean.lean:84-140`): can't compare
  without crush-lsp, and Crush files are small. Revisit if LSP latency shows up.
- Dependent types, tactics, the kernel type checker itself, the `.olean` format, `Specialize`, and the libuv
  bindings (`runtime/libuv.cpp`; Lean's async IO is libuv, not io_uring).

---

## Part 3: proposed tickets (NOT filed; waiting on the captain)

Overlap was checked against the 87 open crush-ast issues on 2026-11-20.

| # | Title | Area / owner | Overlap |
|---|---|---|---|
| T1 | io_uring: Linux-only, feature-gated completion backend for crush-net (accept/recv/send, deadlines), poller kept as fallback; loopback echo p50/p99 bench + byte-identical parity test | engines (tulya) | **#506**: implements side 1 of its design question. File after, or as part of, the #506 design note, not before |
| T2 | io_uring: `UringFs: FsBackend` + `read_many`; `text.grep` 10k-file bench; DiskFs/MemFs/UringFs parity over the whole fs/text cap tests, including error text | engines (tulya) | none |
| T3 | Wall-clock child wait via pidfd instead of a 10 ms sleep (`scheduler.rs:175-226`) | engines | #401/#485 touch the same deadline code; sequence after them |
| T4 | `verify_caps(program, grants)` gate in front of VM, JIT and AOT (F6) | engines + me (contract) | **#473** (AOT refuses undeclared caps): T4 is the way to do #473. Propose as a comment on #473, not a new issue. Also #401 |
| T5 | Golden diagnostic fixtures (F12) | frontend (panini) | #456 (loud fallback in the checker) benefits; no duplicate |
| T6 | `crush explain <CODE>` (F13) | frontend + toolchain | none |
| T7 | content-trace build cache for crush-pkg (F7) | toolchain (upkar) | none |
| T8 | `capsule.lock` tree_sha256 + wire up merkle.rs (F8) | toolchain (upkar) | **#479** (lockfile version enforced): do it with #479's version bump |
| T9 | CASM well-formedness checker before AOT (F5) | engines | #493 (frozen-format corpus) gives it a corpus to run on |
| T10 | `cancel(handle)` for spawn / parallel_map (F10) | engines | #506, #161 (spawn on other engines), #544 |
| T11 | Optimizer pass list + dead-branch pass (F4) | frontend | none |
| T12 | Purity from cap inference (F11) | frontend | depends on T4's per-function facts |
| T13 | Hygienic `macro` design + spike (F1) | frontend (panini) | none; design note first |
| T14 | Measure Str/Tuple/Bytes clones per call, then decide (F2) | engines | none; measurement only |

Each finding's scope and done-when, as drafted by the study, follows the pattern in Parts 1 and 2. Full
ticket bodies get written when the captain picks which ones to file.

---

## Part 4: proof as a lever (the captain's angle)

The question: where can a Lean-checked spec let us **delete** work, or **prove** something we now only test?

### 4.1 How Lean turns a proof into speed

- Array indexing takes a proof of the bound as an argument:
  `instance : GetElem (Array α) Nat α fun xs i => i < xs.size` (`src/Init/GetElem.lean:391`).
- The proof is erased at compile time. The access compiles to `lean_array_fget` / `lean_array_uget`, which
  do **no bounds check** (`src/include/lean/lean.h:1099-1112`).
- `@[implemented_by …Unsafe]` (`src/Init/Data/Array/Basic.lean:557, 610, 679`) runs fast unsafe code that
  stands in for a safe definition.

So the pattern is: **check once, statically, then run without the check.** That is the lever.

There is a limit. Lean is not capability-secure, and its compiler is trusted, not proved. So for Crush,
Lean is a place to write and check **specs and checkers**. The runtime stays Rust. A Lean proof covers our
Rust code only through one of two routes:
- a Rust checker that is differential-tested against the Lean one; or
- a Lean model whose traces the Rust code replays.

### 4.2 The "260k" source

**No Lean entry scored 260k.** Source: twelve-dollar-server-challenge
(https://github.com/arjaythedev/twelve-dollar-server-challenge), `LEADERBOARD.md` at 8ad6f30 (2026-11-20).
Scored holds are on the challenge box (1 vCPU / 2 GB) for 5 minutes. I read the table; I reproduced nothing.

- **260,000 is an F# claim.** The PR body of #142 (fsharp-kestrel-delneg) says 260,000 users held for
  5 minutes. The leaderboard scores that entry at its follow-up #201: **247,000** (rank 43).
- **The Lean entries:**

| Entry | PR | Proofs | Scored on the leaderboard | Self-reported in the PR |
|---|---|---|---|---|
| lean-derive-hargup | #153 (merged) | none shown; Lean contract, generated C, io_uring | **280,000** (rank 11; p99 316 ms) | 280k |
| lean-verified-soonhokong | #286 (merged 2026-11-05, e7acdcc; replaces closed #271) | 819 theorems "about the code that runs", no `sorry` | **270,000** (rank 22; p99 282.8 ms, 954 MB peak) | 290k at 6d5b097; 340-360k on the author's own VMs (#271) |

- For scale: the top rows score 310,000. Those are rust-iouring-kensac #264 and fsharp-uring-delneg #437,
  both io_uring + SQLite B-tree reads.
- So the proved Lean server is about 13% behind the leader. Its proofs didn't make it fast; io_uring and the
  B-tree reads did. Every top entry shares that design.

#286 matters for us because of **what it trusts**. Its own body says the C runtime, SQLite, the io_uring
transport and Lean's compiler are **trusted and tested, not proved**. The hot C leaves, behind `@[extern]`,
are checked against their Lean definitions on 20,000 random inputs in the build's self-test. Its B-tree C
helpers are reused, with credit, from c-iouring-pavlyhalim #245 (LEADERBOARD.md notes). That is the split
this section proposes: the proof covers the spec and the logic, while the fast unsafe parts stay trusted
code with a differential test against the spec.

Other leads, unverified:
- segment-tree/LeanServer: 914 theorems, 0 `sorry`, by its README.
- A Rust HTTP server verified with Lean 4 (Medium, Koukyosyumei).
- Hale/Warp (typednotes.github.io/hale/Warp).

### 4.3 Candidates

| # | Candidate | What we'd prove | What it lets us delete or relax | Cost | Payoff |
|---|---|---|---|---|---|
| P1 | **Bytecode verifier (CVM1 stack discipline)** | For a verified program, every instruction finds enough operands on the stack, jump targets are in range, and locals and constant indices are in bounds (a Lean spec of CVM1 typing) | The ~10 `StackUnderflow` checks in `crates/crush-vm/src/portable_vm.rs` (e.g. :904, :915, :1158) and the `Result` on `pop()` (:2804) on the verified path: about 106 pop sites. Today crush-vm has **0** `get_unchecked` and **no** bytecode verifier. This is the direct analogue of GetElem → `uget` | **M** | Hot-loop speed in PortableVm (measure first: the interpreter dispatch loop with checks vs. without). One verifier also guards JIT and AOT inputs. Ties to T9 |
| P2 | **Capability soundness** | If `verify_caps(program, grants)` (T4) accepts, then no execution reaches a cap call outside `grants`. This is stated over a Lean model of `capabilities_used` (`crates/crush-vm/src/cap_inference.rs:40`) and the VM call semantics | The per-call `check_cap_permitted` (`crates/crush-vm/src/scheduler.rs:89`; callers at `portable_vm.rs:1228, 1264, 1989`, `dom_args.rs:53`, `ai_args.rs:87`) becomes **defence in depth** for statically verified programs. It can't go away fully, because quotas and dynamic targets (spawn of a computed function, polyglot blocks) still need it. AOT gets the gate it lacks today (`crush-aot/src/codegen.rs:1221`) | **M-L** | Mainly **safety**: closes the AOT cap gap with a proof, not a test. The speed win is small (one check per cap call). Honest residual: dynamic dispatch and polyglot need a conservative over-approximation, or stay checked at runtime |
| P3 | **io_uring backend state machine** (T1/T2) | Each submitted SQE completes exactly once (completion, error or cancel); no buffer is reused while the kernel owns it; a timeout racing a completion yields one result; fallback gives the same observable results as the poller | Defensive bookkeeping in the backend (double-completion guards, "should not happen" panics), and most hand-written race tests. These are replaced by a Lean model plus a **trace-replay oracle**: the Rust backend logs its SQE/CQE sequence, and a checker validates it against the model in CI | **S-M** | Safety of the riskiest new code in Part 1, at low cost: the model is small, and the oracle runs on the parity tests we already planned |
| P4 | **Type checker and effect rules** | Effect soundness: a function inferred pure (F11/T12) performs no cap call and has no observable side effect | Lets the compiler **reorder, dedupe, memoize and parallelize** pure calls without a conservative fallback (T11's dead-branch pass, `parallel_map` fusion) | **M** | Speed from optimizations we'd otherwise not dare to ship. Depends on T4/T12 |
| P5 | **CAST → CASM → CVM1 lowering preservation** | Lowering preserves meaning (a simulation proof over a Lean semantics of CAST and CVM1) | Full proof: none of our code goes away cheaply. Practical form: a Lean **reference interpreter** used as the oracle in the differential harness (#453), which could replace part of the hand-maintained expected outputs | **L** (full proof) / **M** (oracle only) | Confidence across five engines. The oracle form is worth it; the full CompCert-style proof isn't, before 1.0 |

### 4.4 Recommendation

1. **P1 first.** It is the one where a proof buys speed directly, the same way GetElem does in Lean. Gate it
   on a measurement: if removing the checks in a branch saves under ~5% on the dispatch-loop bench, do the
   verifier for safety only and keep the checks.
2. **P3 together with T1.** It is cheap, and it covers the code most likely to have race bugs.
3. **P2 after T4 lands.** It turns T4 from "tested" into "proved". Keep the runtime check either way.
4. P4 and P5 (oracle form) are later; they depend on T12 and #453.

Proposed proof tickets (**NOT filed**, same rule as Part 3):

| # | Title | Area | Depends on |
|---|---|---|---|
| T15 | Lean spec of CVM1 stack discipline + Rust bytecode verifier, differential-tested against it; bench the dispatch loop with checks vs. without | engines (tulya) + a tester | T9 |
| T16 | Lean model of the io_uring backend state machine + CQE trace-replay checker in the parity tests | engines | T1 |
| T17 | Lean statement of cap soundness over `capabilities_used` + VM call semantics; proof for the static subset | engines + me (contract) | T4 |
| T18 | Lean reference interpreter for CAST as a #453 oracle (no preservation proof) | tester (kasauti if minted) | #453 |

Lean would be a new toolchain in the lane: `lake`, and an `elan` pin. These are proof and CI dependencies
only. Nothing ships in crush-ast's runtime, and no Lean code runs in the VM.
