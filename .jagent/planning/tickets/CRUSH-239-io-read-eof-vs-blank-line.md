# CRUSH-239 — `io.read()` can't tell a blank line from end of input

| Field | Value |
|-------|-------|
| **ID** | CRUSH-239 |
| **Priority** | P1 |
| **Status** | In review |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-115 |
| **Estimated effort** | S |
| **Filed by** | claude — 2026-10-09 scripting test drive (`tests/scripting_parity.rs`) |

## Problem

`io.read()` returns `""` both for an empty line and at end of input (the CRUSH-115
convention). The natural loop `while line != "" { … line = io.read() }` stops at the
first blank line, so a script summing a piped CSV with a blank line in the middle gives
a **silently wrong** answer. `tests/scripting_parity.rs` case `t03_csv_sum` pins it.

## Success criteria

- [x] A way to detect end of input distinct from an empty line, e.g. `io.read()`
      returning `null` at EOF (breaking: decide and record it), or `io.read_line()` /
      `io.eof()` alongside.
- [x] Same behaviour on every engine via `io_read.rs` (the CRUSH-114 rule).
- [x] `t03_csv_sum` removed from `KNOWN_GAPS`.

## Resolution

**Design: add `io.read_line()` (null at EOF); keep `io.read()` unchanged.**

Why not change `io.read()` to return `null` at EOF: every existing script ends its
loop with `while line != ""` (`examples/crush/blackjack_interactive.crush`,
`t09_word_freq`, the shell walker's `read`, CLAUDE.md's "at EOF `io.read()` returns
`""` and the game exits"). With `null` at EOF, `null != ""` is true and each of those
loops would spin forever at end of input. A silent wrong answer (this ticket) would
become a hang, and it would be a breaking change. `io.eof()` was rejected too: it
needs a peek on every source, and on the interactive (browser) source "is there more?"
is not knowable without pausing, so it is not the same shape as the pause/resume contract.
A new capability name leaves the old contract intact and lets a script opt in; `null` as
the end-of-data signal matches how the other optional-returning values behave.

What changed:
- `crates/crush-vm/src/io_read.rs`: `read_io_line_opt_from` / `read_io_line_opt` /
  `InputSource::read_line_opt` are the single line reader (`None` at EOF); the old
  `io.read` functions are now `unwrap_or_default()` over it.
- `crates/crush-vm/src/caps.rs`: registry entry `io.read_line`.
- `crates/crush-vm/src/scheduler.rs` (`dispatch_cap`) and `portable_vm.rs`
  (`dispatch_cap`, `awaited_input_cap`): the capability, and the interactive pause now
  reports the actual capability name (`io.read` or `io.read_line`); `crush-web/src/lib.rs`
  `drive` treats both as `NeedInput`.
- AOT Rust `crush-aot/src/codegen.rs` (`io_read_line_opt`), AOT C `crush-aot/src/codegen_c.rs`
  (`io_read_line_opt`), `crush-aotc` (`cap_io_read_line` in `rt_header.rs` + `codegen.rs`).
- FastVM/JIT have no `io.read` today; `io.read_line` fails the same loud way
  (capability not available) rather than returning a default.
- `crush-run --list-caps`, `effects.rs` (`stdin/read`), docs (`docs/design/sbl.md`,
  `crush-web/README.md`), CHANGELOG.
- `t03_csv_sum.crush` now uses `io.read_line()` and was removed from `KNOWN_GAPS`.

Tests: `opt_read_distinguishes_blank_line_from_eof`, `test_portable_io_read_line_*`,
`cap_io_read_line_is_registered_as_zero_arg_capability` (crush-vm);
`rust_aot_io_read_line_yields_null_at_eof`, `test_c_codegen_io_read_line_yields_null_at_eof`,
`c_aot_io_read_line_yields_null_at_eof`; `crush_run_io_read_line_distinguishes_blank_line_from_eof`
and `crush_scripts_do_what_the_bash_scripts_do` (crush-lang-sdk).

Follow-up outside this repo: `crush-language-guide` (`src/crush/capabilities.md`,
`quick_reference.md`) should document `io.read_line`.
