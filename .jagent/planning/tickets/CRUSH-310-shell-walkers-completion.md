# CRUSH-310 — Bash/Zsh walkers: `[[ ]]`, arrays, `${…}` operators, `case` globs, `printf` formats, `read`

| Field | Value |
|-------|-------|
| **ID** | CRUSH-310 |
| **Priority** | P1 |
| **Status** | Done (pending review) |
| **Phase** | M6 |
| **Assignee** | buffy |
| **Dependencies** | CRUSH-202 (shared shell IR) |
| **Estimated effort** | L |
| **Filed by** | buffy — 2026-10-30, branch `agent/buffy/shell-walkers-completion` |

## Problem

CRUSH-202 gave bash and zsh one shared lowering (`crush_walker_core::shell`) with
an honest contract: every construct either prints what the shell prints or is
refused with `unsupported: <construct> at line N`. That left the walkers
covering a small subset of real scripts. Writing ordinary shell code still
failed on:

```
[[ $x -gt 3 ]]        unsupported: `[[ … ]]` (use `[ … ]`)
arr=(a b c)           unsupported: array assignment
v=${x:-default}       unsupported: parameter expansion `${x:-default}`
echo -e 'a\tb'        unsupported: `echo -e`
[[ $f == *.txt ]]     unsupported: pathname expansion (unquoted `*`)
printf '%5s|%05d'     unsupported: `printf` conversion `%5s`
case $f in *.txt)     unsupported: pathname expansion (unquoted `*`)
$((0x10)) ${x##*/}    unsupported: number `0x10` / parameter expansion
```

None of these need a process, a file or a new capability — they are lowering
work. Only `$(…)`, pipelines, redirections and subshells genuinely do.

## Success criteria

- [x] The listed constructs lower, or are refused loudly.
- [x] No construct is silently wrong: the oracle corpus has zero
      `SILENTLY WRONG` verdicts for either dialect.
- [x] Real-shell output tests for every new construct, in both dialects.
- [x] A whole multi-construct script produces identical stdout *and* exit
      status to real `bash` and real `zsh -f`.

## Result

All of it landed in the shared IR (`crates/crush-walker-core/src/shell.rs`), so
bash and zsh get each construct once, and the places where the shells really
differ are explicit `Dialect` checks (each one probed against the real shells
before coding).

### Lowered

- **`[[ … ]]`** (bash `ExtendedTest`, zsh `Cond`): `==` `=` `!=` with pattern
  matching, `-z` `-n`, string `<` `>` by scalar value, `-eq -ne -lt -le -gt -ge`,
  `!`, `&&`, `||`, parentheses. No field splitting, no pathname expansion, and
  a quoted right-hand side stays literal.
- **Arrays**: `a=(…)`, `a+=(…)`, `a=()`, `a[i]=v`, `a[i]+=v`, `${a[i]}`
  (negative indices included), `${a[@]}`, `${a[*]}`, `${#a[@]}`, `${#a[*]}`,
  `${#a[i]}`, `$a`, quoted/unquoted expansion in `for` and in argument lists.
- **`${…}` operators**: `:- - := = :+ + :? ?`, `# ## % %%` (patterns, so
  `${f##*/}` and `${f%%.tar.gz}` work), `/ //` substitution with a glob
  pattern, `/#pat` and `/%pat` anchors, `:off` and `:off:len` substrings
  (negative offsets and lengths), `${#x}`, and positional defaults such as
  `${1:-world}`.
- **`${x/pat/rep}` with glob patterns**: `*`, `?`, `[…]` and `\`, using the
  same matcher `${x#pat}` uses. Matching is leftmost-longest, as both shells do
  it; `${x//pat/rep}` replaces every non-overlapping match and never replaces
  an empty one (`${x//a*/Y}` on `bab` is `bY`), `/#pat` / `/%pat` match only at
  the start / end (and an empty pattern there inserts the replacement), and an
  escaped `\*` stays literal.
- **`case` patterns**: `*`, `?`, `[…]` (ranges, `[!…]`/`[^…]`, POSIX classes
  `[[:alpha:]]` …), `|` alternatives, and quoted text that must stay literal.
- **`echo`**: `-n`, `-e`, `-E` (repeatable and combined, as both shells read
  them from the expanded argv), with `\n \t \\ \a \b \e \f \r \v \0NNN \xHH \c`.
- **Arithmetic**: `0x1f`, `0XFF`, `010` (octal in bash, decimal in zsh), `base#digits`
  up to base 64 (36 in zsh), and integer `**` (right-associative, binding looser
  than unary minus, so `-2**2` is `4`).
- **`printf`**: flags `-`, `0`, `+`, space; width; precision; conversions
  `%s %d %i %u %c %x %X %o %b %f %e %E %g %G`; format reuse and
  missing-argument defaults.
- **`printf` floats**: `%f` (6 decimals by default), `%e`/`%E` (one digit, the
  point, precision digits, then `e±NN` with at least two exponent digits) and
  `%g`/`%G` (`prec` significant digits, no trailing zeros, exponent form when
  the decimal exponent is below -4 or not less than the precision), with
  rounding half to even like C (`%.0f` of 2.5 is `2`, of 3.5 is `4`), `0`
  padding after the sign (`%08.2f` of -3.5 is `-0003.50`), and the accepted
  input forms `.5`, `5.`, `1e3`, `+2.0`, surrounding spaces.
- **`read`**: `-r`, `-s`, `-p prompt`, one or more names (the last gets the rest
  of the line with its spacing), `REPLY` default, and bash's backslash
  handling / line continuation without `-r`.

### Still refused loudly (unchanged classes)

`$(…)`, backticks, pipelines, redirections and heredocs, subshells, globs as
arguments, `~`, brace expansion, external commands, `source`/`eval`/`trap`,
environment variables the script never assigns, `=~`, file tests (`-f`, `-d`,
`-nt`, …) in `[[ … ]]` and `test`, `${!x}`, `${x^^}` in zsh, `%a` and any other
unlisted `printf` conversion, a float argument that is not a number,
negative `%x`/`%o`/`%u`, `declare`/`typeset`, associative arrays, `IFS`
assignment, and a name used both as an array and as a plain value.

### Dialect differences this pinned down (probed, then encoded)

| Construct | bash | zsh |
|---|---|---|
| `$((010))` | 8 | 10 |
| `base#digits` above 36 | allowed | error |
| `2**-1` | error | `0.5` |
| `${p}` in a `case`/`[[ ]]` pattern | glob-active | literal |
| `${y//""/x}` | unchanged | inserts around every character |
| `${y/#/a}` `${y/%/z}` | `a…z` | `a…z` |
| `${x^^}` / `${x,,}` | supported | bad substitution |
| `echo 'a\tb'` | literal | escape |
| `$a` where `a` is an array | element 0 | all elements joined |
| `${a[0]}` | first element | out of range |
| `${#a}` where `a` is an array | length of element 0 | element count |
| `a[3]=v` past the end | appended (dense) | pads with empty elements |
| unquoted `${a[@]}` / `${a[*]}` | each element field-split | elements, empties dropped |

### Also fixed

`crush-walk-run` (`crates/crush-aot/src/bin/walk_run.rs`) never registered
`sys.exit`, so every shell script that used `exit`, or whose last command
failed, died with `unknown capability: sys.exit`. It now registers the SDK's
`SysExitCap`, streams output (so what was printed before an exit survives) and
exits with the requested status.

## Verification

- Oracle corpus: **153 cases** (was 88) in
  `crates/crush-lang-bash/tests/oracle/cases.rs`, run against `/usr/bin/bash`
  and `/usr/bin/zsh -f`:
  - bash: 118 match, 35 loud-unsupported, **0 failures**
  - zsh: 117 match, 36 loud-unsupported, **0 failures**
  - (CRUSH-202 was 57 match / 31 loud per dialect.)
- `crates/crush-lang-bash/tests/read_test.rs`: 13 tests, driving `read`
  through `PortableVm` with `InputSource::supplied` so nothing touches the
  harness's stdin.
- `crates/crush-lang-bash/tests/real_script.rs` and
  `crates/crush-lang-zsh/tests/real_script.rs`: one 70-line script
  (`crates/crush-lang-bash/tests/scripts/kitchen_sink.sh`) run through each
  walker and through the real shell, comparing stdout and exit status.
- `cargo test -p crush-walker-core -p crush-lang-bash -p crush-lang-zsh`: all
  pass. `cargo clippy` on the same three crates plus `crush-aot`: no new
  warnings.
- Live, through the real CLI:
  `crush-walk-run kitchen_sink.sh` is byte-identical to `bash kitchen_sink.sh`
  (and the zsh-walker path matches `zsh -f`).

## Known limitations

**`read` and EOF.** `io.read` reports end of input as an empty string, so `read`
cannot tell a blank line from EOF. `read` therefore treats an empty line as end
of input (status 1, variables empty). `while read` loops terminate instead of
spinning forever, at the cost of stopping at a blank line. Documented in
`lower_read`'s doc comment and in `read_test.rs`.

**`printf` floats round the decimal you wrote, not a binary double.** The
formatter works on the written decimal (`__sh_fparse` → digits + decimal
exponent + `__sh_fround`), so it is exact for any length of input, but C's
`printf` first converts to `double`, and a decimal like `2.675` is not
representable in binary (the nearest double is just under it). Where a value
sits exactly on a rounding tie, bash prints `2.67` for `%.2f` of `2.675` and
this walker prints `2.68`. Everything the corpus covers matches bash and zsh
byte for byte; emulating `strtod` + exact binary-to-decimal conversion is a
much larger change and is listed as a follow-up.

## Follow-ups (not in this ticket)

- Exact `double` semantics for `printf` floats (needs `strtod` plus exact
  binary→decimal conversion) — see the limitation above.
- `printf %a` (hex float) and `declare`/`typeset` with associative arrays.
- Pipelines, redirections and `$(…)` — these need a process/fs capability and a
  decision about what a shell script is allowed to do (M6 has no ticket for
  this yet).
- A POSIX `sh`/`dash` dialect, so `.sh` stops being read with bash rules.
