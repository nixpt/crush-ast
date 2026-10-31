# CRUSH-311 — Shell walkers: what a script may touch — process + fs-write for `$(…)`, pipelines and redirections

| Field | Value |
|-------|-------|
| **ID** | CRUSH-311 |
| **Priority** | P1 |
| **Status** | Done — option A (builtin-only), on `agent/buffy/shell-walkers-completion` |
| **Phase** | M6 |
| **Assignee** | buffy |
| **Dependencies** | CRUSH-202, CRUSH-310 (the shell IR is where this lands) |
| **Estimated effort** | M for A (as estimated); B is still open |
| **Filed by** | buffy — 2026-10-30, branch `agent/buffy/shell-walkers-completion` |

## Why this exists

The bash/zsh walkers are honest: a construct either prints what the shell prints or
is refused with `unsupported: <construct> at line N`.168 oracle cases (229 by the time this was implemented), 0 silently wrong. Everything still refused falls in two classes — ordinary lowering work
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

**Decided 2026-10-30: D framing + A, as recommended.** A pipeline is an in-VM
buffer handoff between two lowered commands, `<` `>` `>>` are `fs.read` /
`fs.write` over the existing filesystem sandbox, a here-document is a string,
and a fixed builtin set covers `cat ls wc head tail grep cp mv rm mkdir touch`.
No process is ever spawned: `$(…)`, subshells, `2>&1` and any command outside
the builtin set stay refused loudly. `crush-walk-run` keeps glob reading on by
default and gates file contents behind `--allow-fs`; B (`--allow-process`)
remains unbuilt and unplanned here.

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

- [x] The decision above is recorded (this ticket updated, or a `docs/design/` note).
- [x] Pipelines, `>`/`>>`/`<` and heredocs lower with a `Dialect`-correct exit
      status; `$(…)` and the rest are still refused loudly.
- [x] Corpus cases for each construct, run against `/usr/bin/bash` and
      `/usr/bin/zsh -f`, with **0 silently wrong** for both dialects.
- [x] A host granting nothing fails with a capability error, not a wrong answer —
      pinned by a test (the pattern `glob_test.rs` already uses; here, the live
      run without `--allow-fs` fails with `unknown capability: fs.read`).
- [x] `crush-walk-run`'s defaults are documented in its `--help`: what a script may
      read, write and execute, and the flag that widens it.
- [x] CRUSH-310's follow-up list is updated to drop what this lands.

## What landed

All in the shared IR, so bash and zsh get one implementation
(`crates/crush-walker-core/src/shell.rs`); the lowerers only map parser nodes onto
it.

- **IR**: `Pipeline { stages }` (one entry per `|` stage), `Cmd::Redirected
  { inner, redirs }`, `Redir { kind: In | Out | Append | HereDoc, word, expand }`.
- **Streams**: the stream a command writes to is state in `__g`, not a parameter —
  `__sh_stream` (0 = the host, 1 = `__sh_buf`), `__sh_buf`, `__sh_in` /
  `__sh_in_set` (redirected stdin, with "set but empty" kept apart from "no
  redirection"), `__sh_in_file` (so `wc` pads like GNU `wc` for `< file` and
  pipes), `__sh_outf` / `__sh_append`. Keeping it in the globals map is what
  makes a function inherit its caller's redirection, exactly as fds do.
- **Pipelines**: each stage but the last fills the buffer, which becomes the next
  stage's stdin; the last stage writes wherever the pipeline itself was pointed.
  Nothing reaches the host until the last stage prints, so a pipeline cannot
  reorder what the script already printed. `$?` is the last stage's status and
  `!` negates it.
- **Subshell semantics of a stage**: a stage's writes to globals are rolled back
  afterwards — for every stage in bash and for the non-last stages in zsh, which
  keeps the last stage's (zsh runs that one in the current shell). So
  `printf x | read a` leaves `a` unset in bash and set in zsh, as both shells do.
- **Redirections**: `<` is `fs.read`, `>` is `fs.write` (truncating), `>>` reads
  the old contents back and writes old + new, a here-document is a literal (or,
  for an unquoted delimiter, a normal word expansion) string. The filesystem caps
  are only touched when a redirection is present.
- **`exit`** flushes a redirected command's buffer before leaving, so
  `f() { echo x; exit 3; }; f > log` leaves `x` in `log` like the shell does. In
  a pipeline stage it is refused (see below), because there a real shell exits
  only that stage and the script continues — which a lowered program cannot
  resume halfway through a stage.
- **Builtin command set** — `cat ls wc head tail grep cp mv rm mkdir touch`,
  lowered inline onto `fs.read` / `fs.write` / `fs.cp` / `fs.mv` / `fs.rm` /
  `fs.mkdir` / `fs.touch` / `fs.list` and the string helpers:
  - `ls` hides dotfiles and sorts (`fs.list` is directory order; `ls` never is);
  - `wc -l -w -c -m` on stdin — `wc -l < f` and `cat f | wc -l` are the exact
    paths; several columns on a `< file` is refused, because coreutils < 9.8
    pads those to 7 and newer ones pad by the file's digit count;
  - `head`/`tail` with `-n N`, `-nN` (default 10) or a variable (`head -n $n`),
    `grep` with `-c` and `-v` and its status wired to `$?`;
  - `cat` with no operand reads stdin, which is what makes a here-document or a
    pipeline stage usable;
  - `cp mv rm mkdir touch`, with `-r`/`-f` for `rm` and `-p` for `mkdir`.
  Anything outside those options is refused with a located error rather than
  silently ignored.
- **`crush-walk-run --allow-fs`** (was going to be `--allow-fs-write`; renamed
  because `<` and `cat` need `fs.read` too) registers the whole fs group over the
  start directory. Without it the script still gets `fs.exists` / `fs.list` /
  `fs.is_dir` for globs, and a read of file *contents* fails with
  `unknown capability: fs.read`. `--help` lists the three tiers: read names
  (always), read/write files (`--allow-fs`), run a program (never).

### Refused loudly, on purpose

At compile time: `$(…)`, backticks, subshells, `&`, `2>&1` / `&>` /
fd-numbered redirections, here-strings, process substitution, `>` on a loop /
`if` / `{ }` (the buffer is flushed after the command returns, so an early
`break`/`return` would lose its output), `wc FILE`, a multi-column `wc < f`,
`ls` with options,
and any command outside the builtin set; two `>`/`>>` (or two `<`) on one
command, because a shell creates the earlier file empty and this could only
write one of them; `exit` inside a pipeline stage. At run time: a `grep` pattern
that uses regular-expression syntax (matching it as literal text would silently
disagree with the shell).

## Verification

- Shared oracle corpus: **234 cases**, each run against real `/usr/bin/bash` and
  `/usr/bin/zsh -f`, **0 failures / 0 silently wrong**:
  - bash: 193 match, 41 loud-unsupported
  - zsh: 187 match, 47 loud-unsupported
  - (CRUSH-310 was 182 cases, 147/35 and 141/41. This ticket takes it from 182 to 234: 47
    new matching cases and 8 new loud ones, minus the two old `pipeline` /
    `redirection` refusals; the `external_command` case was retargeted from `ls`
    to `date`.)
- Each case now runs in a *separate* fixture directory for the shell and for
  crush (`…-sh` / `…-crush`): a case that writes a file must not see the file
  the other side just wrote.
- `pipeline_test.rs`, `read_test.rs`, `glob_test.rs`, the kitchen-sink script and
  the frontend tests are green in both dialects.
- Live through the CLI, in a temp directory: a script with
  `cat in.txt | grep b | wc -l`, `printf 'x\ny\n' > out.txt`, `echo appended >>
  out.txt`, `wc -c < out.txt`, `mkdir -p d/e`, `touch d/e/f`, `ls d/e` and a
  literal here-document prints byte-identical output to bash and creates the same
  files; without `--allow-fs` it fails with `unknown capability: fs.read`.

## Out of scope

- `source` / `eval` / `trap`, functions above their definition, dynamic scoping.
- Job control (`&`, `wait`, `jobs`) — background processes are a separate decision
  and, under A, meaningless.
- `set -e` / `-u` / `pipefail` / `nullglob` and the rest of the option surface.
- A POSIX `sh`/`dash` dialect (its own follow-up in CRUSH-310).

## Open questions — answered

1. **Explicit flag.** `--allow-fs`, off by default. It covers reads of file
   *contents* as well as writes, because `<` and `cat` need `fs.read`; that is why
   the flag is not named `--allow-fs-write` as originally proposed.
2. **No SIGPIPE model.** The left side runs to completion and the right side
   consumes a buffer. A script whose left stage would have died of EPIPE here does
   more work but prints the same bytes; this is a documented difference, not a
   silent one.
3. **Shell spelling.** `cat`, `grep`, … lower inline onto the `fs.*` / `text.*`
   caps; no shim layer, and a script writes what a shell script writes.
4. **Moot.** B was not approved, so there is no streaming spawn API to own.

## Follow-ups

- `$(…)` / backticks — the A-consistent form is a builtin-only capture (run the
  lowered command into the buffer, use the buffer as the word); `$(date)` would
  still be refused.
- The rest of the coreutils: `sort`, `cut`, `uniq`, `tr`, `find`, `sed`. Each
  needs its option surface decided the way `wc`/`grep` were.
- `>` on a loop / `if` / `{ }` (flush the buffer on early exit), `2>&1`, `ls -l`,
  `wc FILE`'s multi-file columns, and a regex engine for `grep`.
- B behind `--allow-process` for the commands A cannot cover, with the timeout /
  kill policy CRUSH-19 asks for first.
