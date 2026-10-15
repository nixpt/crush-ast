# CRUSH-202 — Bash/Zsh walkers: `$((…))`, `$(…)`, `${#s}` printed literally; `[ a -op b ]` always true

| Field | Value |
|-------|-------|
| **ID** | CRUSH-202 |
| **Priority** | P0 |
| **Status** | Done (pending merge) |
| **Phase** | M6 |
| **Assignee** | panini |
| **Dependencies** | none |
| **Estimated effort** | M |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

- `a=7; echo $((a+1))` → `$((a+1))`; `v=$(echo hi); echo $v` → `$(echo hi)`;
  `s=hello; echo ${#s}` → `${#s}`.
- Zsh: `x=15; if [ $x -gt 20 ]; then echo big; fi` → `big`. The closing `]` makes 4
  args, which falls through to `BoolLiteral true` (`crush-lang-zsh/src/lowerer.rs:~743-771`).
  `while [ … ]` loops forever for the same reason.
- Bash: `[ -lt ]` unsupported; `echo a b` prints `ab`; function arguments broken.
- Zsh expands variables inside single quotes.

## Success criteria

- [x] Arithmetic, command substitution and length expansions lowered (or rejected loudly).
- [x] `[ … ]`/`test` comparison operators correct; unknown forms are errors, not `true`.
- [x] Output tests against `bash` (and `zsh` where installed).

## Result (2026-10-13, panini, branch `agent/panini/CRUSH-202`)

Both walkers now convert their parser's AST into one small shell IR and lower it
with a shared implementation, `crush_walker_core::shell` (new, additive module;
no existing walker-core API changed). Every construct is lowered so the program
prints what the shell prints, or rejected with `unsupported: <construct> at line N`.
Checks that need run-time values (a word that splits into several fields where
one is required, a non-integer in arithmetic or `-gt`, a glob character coming
out of an unquoted expansion, zsh `echo` of a backslash) throw an
`unsupported: …` error from the program.

**Oracle corpus** — `crates/crush-lang-bash/tests/oracle/cases.rs` (88 cases),
run by `shell_oracle.rs` in both crates against `/usr/bin/bash` and
`/usr/bin/zsh -f` (zsh column skipped with a notice when zsh is missing).
Each crush run has a 5M-step quota and a 30 s wall-clock limit.

| | match | loud-unsupported | silently wrong |
|---|---|---|---|
| bash, `main` 1f92f6d | 7 | 0 | 28 (+53 other failures) |
| zsh, `main` 1f92f6d | 8 | 0 | 22 (+58 other failures; `while [ … ]` ran until the output quota) |
| bash, this branch | 57 | 31 | **0** |
| zsh, this branch | 57 | 31 | **0** |

**Lowered:** `echo` (args joined by spaces; `-n` → `io.write`), `printf` (`%s %d %%`,
`\n \t \\`, format reuse), single/double quotes and backslashes, bash field
splitting and zsh's no-split / drop-empty rule, `$x ${x} $1..$9 ${N} $# $@ "$@" $* $? ${#x}`,
`$((…))` and `((…))` (C precedence, comparisons → 1/0, `++`/`+=` at the top of `((…))`),
`test`/`[` (POSIX argument-count rules; `= == != -z -n ! ( ) -a -o -eq -ne -gt -ge -lt -le`),
`if/elif/else`, `while`, `until` (no longer lowered as `while`), `for … in`, `for ((…))`,
`case` (literal patterns, `|`, `*`), `break`/`continue`, functions with arguments,
`return N`/`exit N`, `local`, globals shared with functions, `&& || !`, and the
exit status (`$?`, conditions, functions' return, the script's exit code via `sys.exit`).

**Rejected loudly:** `$(…)` and backticks (no capability runs commands — none was
added), external commands, pipelines, redirections/heredocs, subshells, `[[ ]]`,
arrays, unquoted globs, `~`, brace expansion, other `${…}` forms, environment
variables (a variable the script never assigns), octal/hex/`**`/bitwise arithmetic,
assignments inside `$((…))`, `echo -e`, `printf %f`/widths, `read`, `source`, zsh
`$x[…]`/`$x:…`/`=cmd`, a function called above its definition, a name that is
`local` in one function and global in another (dynamic scoping).

**Value-producing catch-alls removed:**
- zsh `command_to_expr` `_ => BoolLiteral true` and empty-command `true` (the infinite `while`)
- zsh `expr_from_test_call` `else => BoolLiteral true` (4-arg `[ … ]` always true)
- `expr_from_statement` `_ => BoolLiteral true` (bash and zsh) and zsh `expr_from_last_stmt` default `true`
- bash `compound_list_to_expr` default `true`; bash `expr_from_simple` binary `_ => "=="` operator fallback, `test_?` unary fallback
- bash `lower_compound_command` `_ => Ok(Vec::new())`; `Command::Function`/`ExtendedTest` → no-op
- array assignment → `""` (bash); unknown word → literal text (`$((…))`, `$(…)`, `${#s}`, `${x:-y}`)
- dropped redirections and process substitutions (bash `IoRedirect`/`ProcessSubstitution` → `{}`; zsh `Redirected` → inner command)
- zsh `untokenize` on words, which removed quote markers (why `'$x'` expanded)
- bash/zsh `UntilClause` sharing the `While` arm (bash); `echo`/`printf` → one `io.print` of concatenated args

**Also:** brush now parses in bash mode, not `sh_mode` (which parsed `((…))` as nested
subshells and rejected `for ((…))` / `a=(…)`).

`echo -n` uses `io.write`, which was already on `main` (PR #170), so no follow-up is needed for it.

**Live-verified** with `crush-walk-run` on three multi-line scripts (FizzBuzz with a
classifier function + `case`; primes with a function used as a condition, `for ((…))`,
`until`, `printf`; recursive gcd with `$#`, `local`, `!`/`||`, a Collatz `((steps++))`
loop): output and exit status identical to bash and to zsh for all six runs.
