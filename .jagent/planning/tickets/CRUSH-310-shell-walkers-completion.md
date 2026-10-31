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
- **File tests**: `-e`, `-f`, `-d` in `[[ … ]]` and in `test`/`[`, onto
  `fs.exists`/`fs.is_dir` (`-f` is `exists` and not `is_dir`). `-nt`/`-ot`/
  `-ef` and the rarer tests stay refused.
- **Builtin commands** (CRUSH-311): `cat ls wc head tail grep cp mv rm mkdir
  touch sort uniq cut tr`, in-VM over `fs.read`/`fs.write` — no process is
  ever started.
- **Glob options**: `shopt -s/-u nullglob failglob` in bash,
  `setopt`/`unsetopt null_glob nomatch` in zsh, consulted at run time by
  `__sh_globexp`.

### Still refused loudly (unchanged classes)

`$(…)`, backticks, subshells, `~`, brace expansion, external commands outside
the builtin set (`date`, `sed`, `awk`, …), `source`/`eval`/`trap`, environment
variables the script never assigns, `=~`, `-nt`/`-ot`/`-ef` and the rarer file
tests (`-f`, `-d` and `-e` do lower, onto `fs.exists`/`fs.is_dir`), `${!x}`,
`${x^^}` in zsh, `**`, an absolute path with a glob in it (`/usr/*` — the
filesystem sandbox is relative to the script's directory), `extglob`,
`globstar` and every `shopt`/`setopt` option other than
`nullglob`/`failglob`/`null_glob`/`nomatch`, `%a` and any other unlisted
`printf` conversion, a float argument that is not a number, negative
`%x`/`%o`/`%u`, `declare`/`typeset`, associative arrays, `IFS` assignment, and a
name used both as an array and as a plain value.

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

- Oracle corpus: **270 cases** (was 88) in
  `crates/crush-lang-bash/tests/oracle/cases.rs`, run against `/usr/bin/bash`
  and `/usr/bin/zsh -f`, each case in a fresh working directory holding its
  fixture files (`okf`/`perf` cases) so a glob sees the same files either way:
  - bash: 220 match, 50 loud-unsupported, **0 failures**
  - zsh: 210 match, 60 loud-unsupported, **0 failures**
  - (CRUSH-202 was 57 match / 31 loud per dialect.)
- `crates/crush-lang-bash/tests/read_test.rs`: 13 tests, driving `read`
  through `PortableVm` with `InputSource::supplied` so nothing touches the
  harness's stdin.
- `crates/crush-lang-bash/tests/real_script.rs` and
  `crates/crush-lang-zsh/tests/real_script.rs`: one 88-line script
  (`crates/crush-lang-bash/tests/scripts/kitchen_sink.sh`) run through each
  walker and through the real shell, comparing stdout and exit status.
- `cargo test -p crush-walker-core -p crush-lang-bash -p crush-lang-zsh`: all
  pass. `cargo clippy` on the same three crates plus `crush-aot`: no new
  warnings.
- Live, through the real CLI:
  `crush-walk-run kitchen_sink.sh` is byte-identical to `bash kitchen_sink.sh`
  (and the zsh-walker path matches `zsh -f`).

## Pathname expansion

`echo *.txt`, `for f in *`, `rm a?.log` — the one place a shell script looks at
the filesystem. This is the construct CRUSH-310 originally refused as needing a
capability decision; it now lowers onto `fs.exists` + `fs.list`.

Words are expanded after field splitting, per field, the way bash does it:

- `*`, `?` and `[…]` match through the same pattern matcher `${x#pat}` and
  `[[ $x == pat ]]` use.
- **Every path component globs**, not just the last one: `*/*.txt`,
  `sub/*/[ab]*`, `*/*/*/x` and `echo */` all walk the tree. Each component is
  matched against the entries of the directories built so far.
  - a component with no metacharacter is appended as written but has to exist,
    so `a*/missing` stays one word instead of inventing `a1/missing`;
  - a component is only listed when it is a directory, so a *file* matched by
    `*` is skipped rather than failing with "not a directory" (`*/bar` with a
    `notes.txt` in the way);
  - a trailing `/` keeps the slash and keeps only directories (`echo */`);
  - an absolute pattern is refused: the sandbox root is the script's directory,
    so `/usr/*` could only be answered by guessing.
- A name starting with `.` is only matched by a pattern that starts with one,
  in every component (`.h*/*.txt` reaches `.hid/z.txt`, `*/*.txt` does not).
  Matches come back sorted in byte order, and a pattern that matches nothing
  stays as written in bash.
- zsh rules are kept apart: it does not glob the text a *variable* contributes
  (`x='*'; echo $x` prints `*`), and an unmatched pattern is an error there
  (`no matches found`).
- What an unmatched pattern does is an option, not a constant:
  `shopt -s nullglob` drops the word in bash, `shopt -s failglob` skips the
  command with status 1 and lets the script carry on, and zsh's `setopt
  null_glob` / `setopt no_nomatch` do the equivalent. The other `shopt` /
  `setopt` options stay refused, so a script asking for `extglob` hears about
  it instead of getting bash's default behaviour.
- Quoted text inside a glob word is escaped before matching and unescaped on the
  way out, so `a"*"b*` matches `a*bx.txt` and `echo "*.txt"` stays literal.
- The capabilities this needs are the filesystem: `fs.exists`, `fs.list` and
  `fs.is_dir` (the last one is new here, and exists because `fs.list` on a file
  is an error — a walk into `*/*.txt` has to know which matches it may descend
  into). All three are granted by
  `HostCapsBuilder::fs(true).fs_root(dir)`, which the oracle harness and
  `crush-walk-run` do, the latter over the directory the script was started in.
  A host that grants no filesystem gets `unknown capability: fs.exists` (or
  `fs.is_dir`, if it grants only the two the first version of this needed) —
  loud, never a guess.

## Known limitations

**`read` and EOF.** `io.read` reports end of input as an empty string, so `read`
cannot tell a blank line from EOF. `read` therefore treats an empty line as end
of input (status 1, variables empty). `while read` loops terminate instead of
spinning forever, at the cost of stopping at a blank line. Documented in
`lower_read`'s doc comment and in `read_test.rs`.

**Globs sort in byte order.** Matches are ordered like `LC_ALL=C` (`z1.txt`
before `z10.txt`, and `a.b/` before `a/` for `echo */`); under a UTF-8
collation locale bash and zsh order by collation, which can put `z10.txt` first
and `a/` before `a.b/` — probed both ways. The sort is a merge sort over the
matches. A script that relies on collation order will notice; the ordering is
never silently random.

**An unmatched glob ends the run in zsh, and that is correct.** Probed against
`zsh -f`: an unmatched pattern in a *script* prints `no matches found` and
stops it with status 1 — it does not fail just that command. So this walker's
`no matches found` abort matches zsh, and `shopt -s failglob` in bash is the
case that only skips the command (status 1, script continues), which is
implemented separately. Scripts that want bash's keep-the-word behaviour in
zsh write `setopt no_nomatch`; both shells' `nullglob`/`null_glob` drop the
word entirely.

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
- `**` (recursive in zsh, `*` in bash), absolute-path globs, and the
  `extglob` / `globstar` options.
- Locale collation for glob order (the sort is `LC_ALL=C` byte order).
- A zero-field word reaching `printf` after `nullglob` drops it: `printf`'s
  arguments are lowered one word each, so a word that expands to no fields (or
  to several) is refused rather than reusing the format, as bash does.
- `printf %a` (hex float) and `declare`/`typeset` with associative arrays.
- `$(…)`/backticks — needs a process or a builtin-only capture decision;
  **CRUSH-311** chose builtin-only, which leaves `$(date)` refused.
- Pipelines, redirections, heredocs and the builtin command set landed in
  **CRUSH-311** (`|`, `<`, `>`, `>>`, `<<`, `cat ls wc head tail grep cp mv rm
  mkdir touch`), after this ticket.
- A POSIX `sh`/`dash` dialect, so `.sh` stops being read with bash rules.
