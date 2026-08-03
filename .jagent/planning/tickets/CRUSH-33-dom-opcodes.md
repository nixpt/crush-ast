# CRUSH-33 — DOM opcodes wire-up: `dom_mutate`, `dom_event_listener`, `dom_query`

| Field | Value |
|-------|-------|
| **ID** | CRUSH-33 |
| **Priority** | P2 — depends on CRUSH-32 (parallel — same VM-side dispatch shape); unblocks browser-extension / surfer scripting paths |
| **Status** | Superseded |
| **Phase** | M5 |
| **Assignee** | unassigned |
| **Dependencies** | none — parallels CRUSH-32 (shared design pattern for VM dispatch + cap-provider registration; can be developed in either order; the parallelism is on the development side, not a hard upstream dependency) |
| **Estimated effort** | M |

> Superseded s412: canonical file is `CRUSH-33-dom-opcodes-vm-wire-up.md` (landed: df5f59f/b4c81a8/4f5c0a7).

## Origin

Filed s394 (2026-07-23) from `.jagent/planning/ROADMAP.md` M5 section.
`TODO.md` priority 1 ("Make AI opcodes real") line 3:
> Wire DOM opcodes (dom_mutate, dom_event_listener, dom_query)

This ticket realizes that line.

## Problem

Three DOM-shaped opcodes named in `TODO.md` and `ai-native-roadmap.md` are
currently NOP everywhere. The M5 thesis ("agents querying and mutating DOM
via agent-native ops") requires that DOM opcodes execute. Today they fall
through to the default error arm at every VM tier.

Browser-extension / surfer-style scripting is the principal downstream
consumer: a Crush program should be able to invoke DOM operations through
the same agent-native-opcode surface as AI calls, not through a
separate FFI path.

## Success criteria

- [ ] `dom_mutate`, `dom_event_listener`, `dom_query` defined in
      `casm::Instruction` (existing or new — re-verify against
      `f49ece5`'s salvage or extend).
- [ ] Dispatched at all 5 execution tiers (scheduler, portable_vm,
      fastvm, aot-rust, aot-c) — same differential-coverage bar
      as CRUSH-32.
- [ ] `crush.run` of a `.crush` program containing
      `dom_query("body").inner_text` returns the expected string
      (mock DOM provider in tests; production provider hooks into
      surfer/browser target).
- [ ] Provider impl in `crush-lang-sdk` — minimally a
      "TestDomProvider" backed by a `HashMap<String, Node>` tree for
      testing; in production, would route to a browser-extension or
      WebView target (parallels CRUSH-32's `ai_*` provider pattern).
- [ ] All 3 opcodes included in the extended differential harness
      from CRUSH-32 (unified per-tier parity assertion).

## Technical approach

1. **Opcode definitions.** Confirm `casm::Instruction` has placeholders
   (or add 3 new variants — re-base against `f49ece5` codegen output).
2. **Dispatch.** 3 branches per tier × 5 tiers = **15 dispatch sites**,
   patterned after CRUSH-32. Each branch delegates to a
   `host_cap::dom_*` family registered alongside the `ai_*` family.
3. **Test provider.** `crush-lang-sdk::dom_caps::TestDomProvider`
   backed by a simple `HashMap<String, Node>` tree.
4. **Differential coverage.** Extend the CRUSH-32-then-this harness
   with 3 new opcodes (incremental commit, not a single 13-op batch).

## Files to modify

- `crates/crush-vm/src/scheduler.rs` — 3 dispatch branches
- `crates/crush-vm/src/portable_vm.rs` — 3 mirror branches
- `crates/crush-vm/src/fastvm/` — 3 lowering entries
- `crates/crush-jit/src/compiler.rs` — 3 Cranelift lowering
- `crates/crush-lang-sdk/src/host_caps.rs` — `dom_*` provider
  registration (next to `ai_*` from CRUSH-32)
- `crates/crush-lang-sdk/src/dom_caps.rs` (new) — TestDomProvider
  + production stub
- `crates/crush-aot/src/codegen.rs` + `codegen_c.rs` — confirm stubs
  match (likely adds 3 new stubs, parallel to AOT AI stub merge at
  `f49ece5`)

## Non-goals

- **No real browser integration.** Stub provider, not WebView.
- **No DOM spec completeness.** `dom_mutate("id", "set-attr")` works
  only for the test provider's internal model; full DOM-as-spec
  is a separate ticket (post-M5).
- **Not a `webdriver`/`puppeteer`-style interface.** This is opcode-
  level, not protocol-level.

## Cross-references

- `.jagent/planning/ROADMAP.md` — M5 ticket 7 of 8
- `TODO.md` priority 1 line 3
- CRUSH-32 (parallel — VM dispatch shape and cap-provider patterns;
  primary parallel-track sibling; **no hard ordering** — either ticket
  can be implemented first or both in parallel)
- `f49ece5` AOT stubs — may need parallel salvage (DOM stubs weren't
  part of `CRUSHAST-CRUSH-1`'s AOT work; they're added here)
- `surfer`-browser scripting scenario (downstream consumer per
  `PROJECT.md`'s "Powers surfer-browser scripting, crush-notebook
  cells, crush-pkg ecosystem")
