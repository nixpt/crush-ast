# CRUSH-311 — Shell walkers: what a script may touch — process + fs-write for `$(…)`, pipelines and redirections

| Field | Value |
|-------|-------|
| **ID** | CRUSH-311 |
| **Priority** | P1 |
| **Status** | Proposed — needs a decision, not a patch |
| **Phase** | M6 |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-202, CRUSH-310 (the shell IR is where this lands) |
| **Estimated effort** | L (A below is M; B is L; C is S but wrong) |
| **Filed by** | buffy — 2026-10-30, branch `agent/buffy/shell-walkers-completion` |

## Why this exists

The bash/zsh walkers are honest: a construct either prints what the shell prints or
is refused with `unsupported: <construct> at line N`. 168 oracle cases, 0 silently
wrong. Everything still refused falls in two classes — ordinary lowering work
(CRUSH-310's follow-ups) and this one:

```
v=$(date +%Y)              unsupported: command substitution `$(…)` at line 1
echo hi | wc -l            unsupported: a pipeline (`|`) at line 1
echo x > out.txt           unsupported: a redirection at line 1
cat notes.txt              unsupported: command `cat` (only a fixed set of …)
a=$(cat <<'EOT' …)         unsupported: a redirection at line 1        (heredoc)
(cd sub && ls)             unsupported: a subshell `( … )` at line 1
```

(Pathname expansion used to be in this list; CRUSH-310 lowered it over
`fs.exists` / `fs.list` / `fs.is_dir` and it is no longer a decision that is
waiting.)

Every one of them needs the same two things a shell script has and a crush program
so far has not: **the ability to run something else**, and **the ability to write a
file**. So this is not a lowering decision; it is a decision about what a shell
script is allowed to do when crush runs it. CRUSH-310 deliberately filed it as a
follow-up instead of guessing.

The four constructs above are the difference between "crush can run a shell script
that computes" and "crush can run an ordinary shell script". Nothing else blocks
that gap as much.

## What already exists (so the options are real, not hypothetical)

| Piece | Where | What it does today |
|---|---|---|
| `process.exec(cmd, argv)` | `crush-lang-sdk/src/host_caps.rs` | `std::process::Command`, **argv only, no shell**, captures stdout+stderr+exit code into one JSON string. Not streamed, no timeout, no cwd/env/uid control, whole output buffered in memory. |
| `fs.read` `fs.write` `fs.exists` `fs.list` | `host_caps.rs` | One `FsSandbox` rooted at `--fs-root` (`fs_root(dir)`); `..` and symlink escapes are refused and tested. |
| `fs.ls` `fs.cat` `fs.pwd` `fs.cd` `fs.mkdir` `fs.rm` `fs.cp` `fs.mv` `fs.touch` `fs.find`, `text.head` `text.tail` `text.wc` `text.cut` `text.grep` | `fs_tools.rs`, `text_tools.rs` | A **builtin command surface already exists** over the same sandbox. `cat`, `ls`, `cp`, `mv`, `rm`, `mkdir`, `find`, `head`, `tail`, `wc`, `cut`, `grep` need no process at all. |
| `fs_memory()` + `FsBackend` | `host_caps.rs`, `fs_backend.rs` | The same caps over an **in-memory filesystem** — hermetic, no disk. |
| `--fs` / `--fs-root` / `--process` | `crush-lang-sdk/src/bin/crush-run.rs` | The generic runner already asks the host for exactly these. |
| `crush-walk-run` | `crates/crush-aot/src/bin/walk_run.rs` | Grants `fs.exists` + `fs.list` + `fs.is_dir` only (pathname expansion); no write, no process. |

So the capability layer is not the work. The work is policy: what the walkers emit,
what the CLI grants by default, and what an embedder must opt into.

## The decision

> When crush runs a shell script, may that script run programs and write files — and
> if so, who says yes?

Answering that sentence settles the rest of this ticket.

## Options

### A. Builtin-only pipelines and redirections (no process at all)

Lower `|` to an in-VM buffer handoff between the two lowered commands, `>` `>>` `<`
to `fs.write` / `fs.read`, heredocs to a string literal, and a fixed set of external
commands (`cat ls cp mv rm mkdir find head tail wc cut grep sort uniq tr sed`… those
with a `fs.*`/`text.*` cap today) to those caps. Anything else is still refused.

- **Good:** deterministic, hermetic (`fs_memory()` makes the whole thing testable
  against fixtures), no arbitrary code execution, no new security surface, keeps the
  oracle corpus meaningful (it can run a pipeline and compare to bash), unblocks the
  majority of real scripts (log processing, file munging, `$(cat …)`, `$(wc -l < f)`).
- **Bad:** a script that calls something without a cap (`date`, `git`, `python`,
  `jq`) still fails; `|` between two processes has different buffering and SIGPIPE
  behaviour than a buffer handoff — a script that depends on early exit of the left
  side will differ; `sort`/`grep` semantics are ours to get right (locale, `-o`,
  binary data).
- **Cost:** M. Mostly lowering plus a small pipeline runner over `Value`s.

### B. Real `process.exec` for external commands

Lower a simple command that is not a cap-backed builtin to `process.exec(argv)`;
`$(…)` is that plus capturing stdout; a pipeline spawns each stage.

- **Good:** correctness by construction for `date`/`git`/`python` — whatever is on
  `PATH` behaves exactly as it does in bash.
- **Bad:** `process.exec` as it stands cannot express a pipeline: it buffers the
  whole output and returns a JSON string, so a real `a | b` needs a streaming
  spawn API (child stdin/stdout wired, no capture) — new SDK surface, plus
  `crush_vm` re-entrancy questions if a cap call blocks the scheduler. Also:
  arbitrary code execution from a script, PATH/environment dependence, output that
  differs per machine, and an oracle corpus that can no longer be hermetic.
- **Cost:** L. Streaming spawn + pipeline lowering + timeout/kill policy
  (see CRUSH-19, cap calls have no wall-clock timeout today).

### C. Hand the command line to `sh -c`

- **Good:** S effort, every construct works immediately.
- **Bad:** it is a shell calling a shell — the walker adds nothing, output depends on
  the host's `/bin/sh`, and the whole point (a static, capability-checked program)
  is gone. Only worth naming so it can be rejected explicitly.

### D. Orthogonal: capability-gated lowering, whichever of A/B is chosen

The walker always emits the program with the capability requirement it has
(`fs.write`, `process.exec`); the host grants or refuses. A host that grants nothing
gets `unknown capability: process.exec` — loud, exactly like today's `fs.list` for
globs — and a host that wants a sandbox grants `fs_memory()` or `fs_root(dir)`.

- **Good:** one story for every language, not a shell special case; embedders keep
  full control; the refusal is a capability error, not a mis-lowered program.
- **Bad:** nothing on its own — it is the frame A and B both need.

**Recommendation: D framing + A first, then B behind an explicit opt-in** for the
commands A cannot cover. That keeps `crush-walk-run` hermetic by default (pipelines
and `>` work, nothing arbitrary executes), makes the corpus able to compare real
pipelines against bash, and leaves `--allow-process` as a deliberate, documented
choice rather than a default.

## What the decision must also settle

- **Default grants.** `crush-walk-run` currently grants `fs.exists` + `fs.list` over
  the script's start directory. Write and process should not join that default
  silently — a flag (`--allow-fs-write`, `--allow-process`) keeps "runs a script" and
  "lets the script rewrite my tree" as separate consents.
- **Streams and ordering.** Today output is streamed through
  `run_with_caps_streaming`; a pipeline stage writing to a buffer while the other
  prints must not reorder what the script already printed.
- **Exit status plumbing.** `$?` of a pipeline is the last stage's status in both
  shells; `pipefail` is not supported and should stay refused rather than silently
  ignored. `set -e` does not exist as an option here at all.
- **Subshells.** `( … )` is a child environment (own cwd, own variable state, output
  still inherited). Under A this is a scoped variable snapshot, not a process —
  cheap, and it also gives `cd` in a subshell the right semantics.
- **Heredocs.** Pure lowering once `>`/`<` exist: the body is a literal.
- **Binary safety.** `fs.write` takes bytes; shell redirection is textual except
  through `cat`. Decide whether `a > f` on non-UTF-8 output is refused or written raw.
- **Timeouts.** A `process.exec` stage needs a wall-clock limit and a kill path
  before it is enabled by default (CRUSH-19).
- **Corpus hermeticity.** Any case that writes must run in its own temp dir (the
  harness already gives each case its own working directory for globs); any case that
  spawns must either be excluded from the corpus or pinned to a command every CI
  image has.

## Success criteria

- [ ] The decision above is recorded (this ticket updated, or a `docs/design/` note).
- [ ] Whichever of A/B is chosen: pipelines, `>`/`>>`/`<`, heredocs and `$(…)` lower
      with a `Dialect`-correct exit status, or are still refused loudly.
- [ ] Corpus cases for each construct, run against `/usr/bin/bash` and
      `/usr/bin/zsh -f`, with **0 silently wrong** for both dialects.
- [ ] A host granting nothing fails with a capability error, not a wrong answer —
      pinned by a test (the pattern `glob_test.rs` already uses).
- [ ] `crush-walk-run`'s defaults are documented in its `--help`: what a script may
      read, write and execute, and the flag that widens it.
- [ ] CRUSH-310's follow-up list is updated to drop what this lands.

## Out of scope

- `source` / `eval` / `trap`, functions above their definition, dynamic scoping.
- Job control (`&`, `wait`, `jobs`) — background processes are a separate decision
  and, under A, meaningless.
- `set -e` / `-u` / `pipefail` / `nullglob` and the rest of the option surface.
- A POSIX `sh`/`dash` dialect (its own follow-up in CRUSH-310).

## Open questions

1. Is a shell script that writes files acceptable at all in the default CLI, or must
   every write and every spawn be an explicit flag? (Recommendation: explicit.)
2. Does A's buffer handoff need to model SIGPIPE, or is "left side runs to
   completion, right side consumes the buffer" an acceptable documented difference?
3. Should cap-backed builtins be spelled as shell commands (`cat`, `grep`) or as
   crush caps with a shim? The former is what scripts write; the latter is what the
   IR already knows how to emit.
4. Who owns the streaming spawn API in the SDK if B is approved — and does it belong
   to `crush_vm` (async) or to the host cap (blocking)?
