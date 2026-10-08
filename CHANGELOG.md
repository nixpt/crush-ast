# Changelog

All notable changes to this project are documented here.

Format loosely follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versioning is [SemVer](https://semver.org/), applied automatically by
`bump-version` (`scripts/bump-version.sh` + `.github/workflows/release.yml`) —
every push to `main` computes a bump from conventional-commit subjects since
the last `v*` tag and appends a mechanical entry below. Hand-written entries
are welcome for anything the mechanical summary won't capture well (a
behavioral note, a migration hint); the automation appends rather than
overwrites, so edit this file directly for that and let the next auto-bump
add its entry after yours.

## [Unreleased]

- **`ai_native.toolchain` runs tool chains (CRUSH-157).** With `ai_native`
  granted, the toolchain cap is now a strategy engine (sequential, parallel,
  conditional, retry × fail-fast, continue-on-error, retry, fallback) that
  returns `{results, aborted, abort_reason}`. Every step is dispatched
  through the program's own `HostCaps`: a tool (or its
  `required_capability`) that wasn't granted fails its step and never runs.
  Tool arguments come from `parameters.args` (positional) or the parameters
  map; `"$name"` refers to an earlier step's `result_binding`. `HostCaps` is
  now `Clone` (clones share handlers) and `Value::is_truthy` is public.

- **`ai_native.*` caps receive their compiled arguments (CRUSH-156).** Each
  AI opcode now calls its cap with `[payload, operands…]`: the compiled
  payload as a map, then the values the frontend pushed for that kind
  (`context_aware`'s expression, `semantic_match`'s target, `synthesize`'s
  context refs and examples). Specs declare real arities (1, 2, or variadic
  for `synthesize`). Crush programs compiled to CVM1 now execute the ten AI
  ops (they were lowered to `NOP`); statement forms pop their result.
  Ungranted ops still yield `null`, operands consumed. FastVM's
  `resolve_host_request` passes the payload through the same contract
  (`crush_vm::ai_args`) and returns map results as JSON text. Tool lists
  carry `required_capability`, and the `fallback` policy carries its tools.

- **crush-web: interactive `io.read` in the browser (CRUSH-118, #91).** New
  `execute_with(source, { stdin, max_steps })` feeds `io.read` from a string
  and keeps output printed before an error; new `Session` pauses when the
  program needs a line and resumes on `provide(line)`, so
  `examples/crush/blackjack_interactive.crush` is playable in a page.
  `execute()` is unchanged. Backed by `crush_vm::InputSource` on `PortableVm`
  (`set_input` / `provide_input` / `close_input` / `take_output`); native
  `io.read` still reads process stdin. New CI job runs it in headless Chromium.

## [0.3.9] - 2026-10-07

- Merge pull request #90 from nixpt/agent/panini/CRUSH-104
- docs: CRUSH-104 changelog security entry, ticket evidence, decisions
- fix(deps): CRUSH-104 raise internal dep requirements 0.3.0 -> 0.3.8
- test(sdk): CRUSH-104 pin the polyglot gate for unknown @lang + embedders; run SDK tests in CI
- docs(planning): CRUSH-104 rescope — SDK publish set, order, design call
- Merge pull request #58 from nixpt/claude/status-check-zaai3m
- Merge pull request #89 from nixpt/agent/foreman/CRUSH-148
- tickets: file CRUSH-148 — format the workspace once, then gate cargo fmt --check in CI
- Merge pull request #88 from nixpt/agent/panini/CRUSH-147
- docs(planning): capture fmt-gate gap and missing CRUSH-73/19/11 work
- docs(planning): CRUSH-147 shared-checkout salvage triage
- Merge pull request #87 from nixpt/claude/crush-135-array-any-concat
- feat: mixed array literals are array<any>; `a + b` concatenates arrays (CRUSH-135, #75)
- feat: field access on `any` and `any` conditions; one truthiness rule on every backend (CRUSH-134, #74, #77)
- docs: record the #74–#78 language decisions; close CRUSH-136, CRUSH-143
- feat: `<` `>` `<=` `>=` order two strings lexicographically (CRUSH-136, #76)
- fix(frontend): optimizer dropped assignments made inside `if` branches (CRUSH-143)
- docs(planning): close CRUSH-142
- test(aot): the differential harness is strict on the JIT (CRUSH-142)
- fix(frontend): `array.pop(a)` leaked the array on the stack
- fix(jit): 48-bit ints, not 16-bit (CRUSH-142)
- fix(jit): array_push/array_pop keep FastVM's stack contract (CRUSH-142)
- docs(planning): close CRUSH-138/139/140; file CRUSH-142
- test(aot): unique temp file per JIT subprocess run
- fix(fastvm): ExecLang requests carry the block's input variables (CRUSH-140)
- fix(fastvm, jit, frontend): call arguments bound in reverse (CRUSH-138)
- fix(jit): logical not, not bitwise, for `!` and JumpIfNot (CRUSH-139)
- docs(planning): close CRUSH-130/132/133; file CRUSH-140, CRUSH-141
- fix(jit): don't compile ExecLang to a payload-less yield (CRUSH-133, #73)
- fix(aot): unsupported ops are a compile error, not a silent no-op (CRUSH-132, #72)
- fix(aotc, pkg): compile Crush source with the polyglot pass too (CRUSH-130)
- fix(crushc): run the polyglot marshaling pass (CRUSH-130, #70)
- docs(planning): close CRUSH-125; file CRUSH-138, CRUSH-139
- fix(frontend): `&&` / `||` short-circuit (CRUSH-125, #65)
- docs(planning): close CRUSH-126; file CRUSH-137
- fix(vm): unwind to the try's frame when a throw is caught (CRUSH-126, #66)
- docs(planning): close CRUSH-124/127/128/129/131; fix TASKS ticket links
- test(sdk): source-level regressions for #64, #67, #68, #69
- fix(frontend): don't fold constants across `@lang` blocks (CRUSH-131, #71)
- fix(frontend): top-level `main()` next to `fn main` runs main once (CRUSH-129, #69)
- fix(frontend): prefix `!` / `-` apply to the whole postfix chain (CRUSH-128, #68)
- fix: uncaught throw is not an "unknown capability"; compile errors not `[runtime]` (CRUSH-127, #67)
- fix(vm): CASM assembler decodes every escape `{:?}` emits (CRUSH-124, #64)
- docs(planning): file CRUSH-124…136 for GitHub issues #64–#77
- refactor(crush-cson): repoint onto standalone caison crate (#63)
- fix(walkers): go/zig/wasm walker binaries were never found; add PATH fallback (#62)
- tickets: CRUSH-121 kitchen-shaped GC + CRUSH-123 conformance timeout; CRUSH-122 status after #61 (#59)
- chore: gitignore .jagent/worktrees/ (squadron SQ-204) (#60)
- Merge pull request #61 from nixpt/claude/stdlib-nanovm-crush-ast-port-477gv7
- docs(planning): CRUSH-55 exosphere ↔ crush-ast delta inventory
- docs(planning): CRUSH-122 landed record + dejavue decision
- test(conformance): `// caps:` annotation, file selection; nanovm corpus now passes
- feat(sdk): absorb exosphere's stdlib and nanovm's SBL (CRUSH-122, W10)
- feat(vm): PortableVm::push_entry_args; make Value::type_name public
- fix(sdk): fs sandbox escape via `..` on paths that do not exist yet
- fix(frontend): lower array.push / array.pop to ARR_PUSH / ARR_POP
- Merge pull request #57 from nixpt/agent/nixp/CRUSH-114
- feat(ai-opcodes): parser + compiler support for semantic_switch, ai_synthesize, ai_semantic_match
- fix(parser): handle keyword tokens in annotation field names (issue #38)
- fix(vm): CRUSH-114 len() accepts strings via shared str_len helper
- ops: release workflow on dispatch only (Actions-usage trim) (#56)



### Security

- **The polyglot capability gate now ships in the SDK (CRUSH-104).**
  `crush-lang-sdk` 0.2.x (crates.io, via `crush-vm` 0.2.0) runs `@lang`
  blocks ungated: any `@word { code }` spawns `word -c code` from `PATH` with
  the host's authority, with no capability check and no language allowlist.
  The next SDK release requires a `polyglot.<lang>` grant for every block
  (`crush-run --polyglot`, `HostCapsBuilder::polyglot`) and refuses languages
  outside the python/javascript/bash allowlist even when granted. Upgrade off
  0.2.x; those versions are scheduled to be yanked.
- `crush-lang-python` and `crush-lang-js` are now publishable, so the SDK keeps
  its default `polyglot-python`/`polyglot-javascript` features (typed
  variable marshaling into and out of `@python`/`@javascript` blocks).
- Internal dependency requirements now say `0.3.8` instead of `0.3.0`; several
  0.3.0 releases on crates.io predate APIs their dependents use.
- CI: new `Test (sdk)` job runs `cargo test -p crush-lang-sdk`, including the
  gate tests.

## [0.3.8] - 2026-08-25

- chore(security): ignore agent/MCP artifacts and secret-shaped files



## [0.3.7] - 2026-08-24

- docs(planning): import crush backlog dispatch prompts
- docs(planning): reconcile completed crush backlog



## [0.3.6] - 2026-08-24

- casm: delete dead ecasm.rs + CachedProgram remnants (CRUSH-80)
- Implement conv.chr and conv.ord across runtimes
- merge(agent/nixp): CRUSH-116 deterministic RNG completion
- merge(agent/buffy): CRUSH-119 for-loop fixes
- Complete deterministic math RNG capability integration.
- docs: reconcile milestone and backlog status
- fix: preserve self-referential assignment values in optimizer
- fix: preserve FastVM array mutation values



## [0.3.5] - 2026-08-21

- Add canonical io.read capability across runtimes (#52)



## [0.3.4] - 2026-08-21

- docs: rewrite CONTRIBUTING.md, add CHANGELOG.md, enrich CLAUDE.md/AGENTS.md (#51)



### Added

- `scripts/bump-version.sh` + `.github/workflows/release.yml` — automatic
  version bump + tag on every push to `main`.
- This file.

## [0.3.0] — 2026-0X-XX

Never tagged at the time (see `.jagent/planning/tickets/` / `STATE.md` for
context) — `Cargo.toml` and crates.io both moved to `0.3.0` without a
corresponding `v0.3.0` git tag. Noted here rather than silently starting the
changelog as if `0.3.0` didn't happen; `bump-version`'s automation picks up
cleanly from here regardless; the missing tag is a historical gap, not a
blocker.

## [0.2.0] and earlier

Predates this changelog. See `git log --oneline v0.2.0` and
`.jagent/planning/tickets/` for the historical record.
