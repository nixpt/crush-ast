# CRUSH-2 — Polyglot blocks bypass the capability system entirely

| Field | Value |
|-------|-------|
| **ID** | CRUSH-2 |
| **Priority** | P0 |
| **Status** | **Done** — verified s388 (2026-07-16) |
| **Phase** | M0 |
| **Assignee** | fixed via `polyglot_gate()` (crush-vm `host.rs`), merged `agent/kai/CRUSHAST-RELEASE-1` |
| **Dependencies** | none |
| **Estimated effort** | M (design work; the fix itself is likely small once the design is settled) |

## Resolution (verified s388)

Re-ran this ticket's own repro verbatim: `crushc /tmp/escape.crush -o /tmp/escape.cvm1 &&
crush-run run /tmp/escape.cvm1` with **zero capabilities granted**. Result:

```
[runtime] unknown capability: @bash requires the 'polyglot.bash' capability (run with --polyglot to grant it); refusing to spawn
```

No probe file created — matches every success criterion below (loud, named
denial via `polyglot.<lang>`, `--polyglot` grants it explicitly, no silent
no-op). `scheduler.rs` and `portable_vm.rs` both gate `EXEC_LANG` through
`crush_vm::polyglot_gate(lang)` before `Command::new` is ever reached
(confirmed both call sites, `crush-diff` proves no divergence between them).
Sandboxing depth (bwrap/seccomp) is still open — see the buckets-integration
opportunity in TASKS.md — but the capability-bypass itself is closed.

## Problem

Crush's core pitch is capability-based execution: a program gets exactly the
authority it's granted (`--fs`, `--net`, `--process`, ...) and nothing more.
`@python { ... }` / `@bash { ... }` / `@javascript { ... }` polyglot blocks
completely bypass this. `EXEC_LANG` (`crates/crush-vm/src/scheduler.rs`)
spawns a real interpreter subprocess (`python3 -c`, `bash -c`, `node -e`) via
`resolve_lang_binary` + `std::process::Command`, with **no capability check
of any kind** — the child inherits the full ambient authority of the
`crush-run` process itself, regardless of what was or wasn't granted on the
command line.

`crush-website/example.crush` currently ships text claiming the opposite:
*"Held: io, str, math. Not fs. Not net. There is no ambient authority to
escape from."* — one screen above a `@bash` block that, unmitigated, can
read/write/execute anything the host user can. The security claim is false
the moment any polyglot block runs.

The binary-selection side is fine — `resolve_lang_binary` is a fixed
allowlist (`python3`/`node`/`bash`/etc.), so the interpreter itself isn't
attacker-controlled. The bypass is entirely about *whether a block should be
allowed to run at all*, and what it can touch once it does.

## Steps to reproduce

```
$ crush-run run escape.crush            # NO --fs, NO --net, NO --process granted
```
```crush
fn main() {
    @bash { touch /tmp/crush_escape_probe }
}
```
Result: `/tmp/crush_escape_probe` is created. No error, no capability
prompt, no denial — despite zero capabilities being granted on the command
line.

`@python { import os; os.system("...") }` is arbitrary code execution behind
a language whose whole pitch is that code execution is capability-gated.

Found by khukuri (SAST) flagging the `Command::new` call site in
`scheduler.rs`; the capability bypass was then confirmed by hand.

## Expected behavior

A polyglot block should require an explicit capability grant before it's
allowed to spawn a subprocess at all (e.g. a `polyglot` capability, or a
per-language grant mirroring `--fs`/`--net`/`--process`), and — ideally —
the child process should run inside the same authority boundary the
capability model implies (a real sandbox), not with the parent's full
ambient authority just because the language binary itself is on an
allowlist.

## Actual behavior

Any `@<lang> { ... }` block runs unconditionally, with the full authority of
the host `crush-run` process, regardless of granted capabilities.

## Why this blocks more than it looks like

This isn't cosmetic. It's the security story for the polyglot feature
(CRUSHAST-POLYGLOT-1, which made `@python`/`@javascript` blocks actually
*work* end-to-end for the first time — see `docs/design/python-lowering-coverage.md`
and the merged `agent/cece/CRUSHAST-POLYGLOT-1` branch) and it directly
contradicts the headline pitch on crushlang.org. It gates any honest launch
that sells capabilities as the differentiator — right now the pitch and the
implementation disagree.

## Success criteria

- [ ] A polyglot block cannot spawn a subprocess without an explicit,
      capability-checked grant (name/shape TBD — design decision).
- [ ] The existing capability-check pattern (`declared: HashSet<&str>` in
      `scheduler.rs`, used elsewhere for host caps) is the natural place to
      look for how other capabilities are already gated — reuse that
      pattern rather than inventing a parallel mechanism if it fits.
- [ ] Denial is loud (a named `CapabilityDenied`-style error), never a
      silent no-op — consistent with this session's repeated finding that
      silent fallthroughs are the recurring disease in this codebase.
- [ ] `crush-website/example.crush`'s capability claims become true again —
      either by granting `@bash`/`@python` real capabilities explicitly in
      the demo, or by the demo no longer implying zero ambient authority
      when it uses polyglot blocks.
- [ ] Ideally: the child process itself is constrained (not just gated),
      e.g. dropped privileges / restricted env / no filesystem access
      beyond what was granted — the allowlist-of-binaries protection alone
      (`resolve_lang_binary`) stops arbitrary-binary injection but does
      nothing to constrain what an *allowed* binary can do once it runs
      with a `-c`/`-e` argument the user fully controls.

## Technical approach (starting points, not a committed plan)

1. Add a `polyglot` (or per-language: `python`, `js`, `bash`, ...)
   capability, checked in `scheduler.rs`'s `EXEC_LANG` handler the same way
   other host capabilities are checked, before `Command::new` is ever
   reached.
2. Decide granularity: one blanket "polyglot" grant, or per-language grants
   (a program that only needs `@python` shouldn't implicitly also be able
   to run `@bash`).
3. Decide on sandboxing depth for a later phase — OS-level (seccomp/Landlock
   on Linux, or similar), a restricted subprocess environment (empty `PATH`,
   scrubbed env, no inherited fds), or accept subprocess-with-capability-gate
   as "good enough for v1" and revisit if abuse patterns show up.
4. Update `crush-website/example.crush` and the guide once the grant shape
   is settled, so the demo's own claims stay true.

## Files likely involved

- `crates/crush-vm/src/scheduler.rs` — `EXEC_LANG` handler, `resolve_lang_binary`, the `declared` capability set already threaded through `execute_one`.
- `crates/crush-vm/src/caps.rs` / `crates/crush-vm/src/host.rs` — wherever the existing capability-declaration/check pattern lives, to extend rather than duplicate.
- `crates/crush-lang-sdk/src/bin/crush-run.rs` — CLI flag surface (`--fs`/`--net`/`--process` precedent) for however the new grant is exposed.
- `crush-website/example.crush`, `crush-language-guide` — once the fix lands, the demo/docs need to stop claiming zero ambient authority while using polyglot blocks ungated.

## Notes

Not urgent for whoever is deep in the polyglot-marshaling ticket
(CRUSHAST-POLYGLOT-1) to fix personally — that work is correct and needed
regardless of this gap. This is a separate design question, flagged for
captain/foreman to scope. Filed here instead of left as a TASKS.md line
because it's launch-blocking and needs its own success criteria, not just
a parking-lot mention.
