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

- **`EXEC_LANG` checks declared caps and `allowed_caps` (CRUSH-226).** A
  `@lang` block's `polyglot.<lang>` gate now passes the same two checks as
  every `CAP_CALL`: an embedder's `Quotas::allowed_caps` that leaves it out
  denies the block (`CapDenied`), and the program must declare it
  (`CapNotDeclared`). The compiler declares it for every block, and
  `crush-run --polyglot` declares it for hand-written `.casm`. A `.cvm1`
  compiled before this change has no polyglot declaration: recompile it.
- **`use @lang` imports reach later blocks (CRUSH-224).** `use @lang python
  "math" as m` is written into every later `@python` block as
  `import math; import math as m` (selective form: `from m import a, b`;
  JavaScript: `const x = require("m")`). It no longer lowers to a throwaway
  `exec_lang`. `LangBlock.imports` is filled. The import shares the block's
  first line, so guest line numbers still map to `.crush` lines. `use @lang
  bash` is a compile error. The unused `crush_frontend::polyglot_imports`
  module is removed.

### Added — declared capabilities (CRUSH-232)
- `@capabilities [fs.cat, time.now]` (or `capabilities: [...]` inside `@module`)
  declares what a program may use beyond the ambient built-ins. Compiling rejects
  a program that uses a capability its declaration doesn't cover. Entries are names
  (`fs.cat`), families (`fs`, `fs.*`) or scoped names (`fs.read:/var/log`). A program
  without a declaration compiles as before.
- `crush-run caps FILE [--json]` lists the capabilities a program can use and the
  grant each needs, without running it.
- `Runtime::missing_grants` and `crush_lang_sdk::effects::{covers, undeclared,
  missing_grants}` for hosts that want the same check.

### Changed
- `crush-run run` checks grants **before** running: a program that can reach a
  capability the run doesn't grant is refused with every missing grant listed
  (`[capabilities] … nothing was run`), instead of failing at the first one
  mid-run with `unknown capability`. Programs that ran before run the same.
- `@module { purpose: "x", exports: [a] }` accepts commas between keys on one line.

## [0.4.1] - 2026-10-09

- Merge pull request #125 from nixpt/ccr-12236bee-uj4oar
- docs(dejavue): sync skill copies with dejavue 2.2.0; fix the not-on-PATH line
- docs(dejavue): append 2026-10-09 state and CRUSH-187/189 handoff
- docs(dejavue): record CRUSH-187/189 decisions and optimizer invariant
- fix(frontend): implicit return after early return; sound optimizer (CRUSH-187, CRUSH-189)
- Merge pull request #124 from nixpt/ccr-12236bee-uj4oar
- chore(planning): file CRUSH-213..221 (engine divergence); index CRUSH-179..221 in TASKS and BACKLOG-INDEX
- docs: crush-run needs the run subcommand; document the ../buckets sibling clone; file CRUSH-211..212
- chore(planning): file CRUSH-179..210 from the 2026-10-09 test-drive sweep; fix clippy deny + stale example
- test(crush-lang-sdk): check PATH instead of USER in dashboard test
- Merge pull request #123 from nixpt/agent/foreman/publish-env-dup
- ci(publish): fix duplicate job env key; show cargo's error on metadata failure
- Merge pull request #122 from nixpt/agent/foreman/publish-tag-input
- ci(publish): check out the buckets sibling; publish a tag given as input



## [0.4.0] - 2026-10-08

- Merge pull request #121 from nixpt/agent/foreman/release-0.4-prep
- docs: cson.parse alias stays until 0.5
- ci(publish): publish everything not waiting on a first publish; report the rest
- ci(release): reinstall bump-version with internal-pin rewrite (squadron#122)
- Merge pull request #120 from nixpt/agent/foreman/publish-workflow
- ci: publish.yml — publish the workspace to crates.io from a release tag (Trusted Publishing)
- Merge pull request #119 from nixpt/agent/foreman/doctor-etxtbsy
- Merge pull request #118 from nixpt/agent/panini/CRUSH-170
- fix(crush-doctor): retry --version launch while the tool is "text file busy"
- Merge pull request #117 from nixpt/agent/panini/CRUSH-172
- Merge remote-tracking branch 'origin/main' into kai/118-update
- dejavue: record CRUSH-170 decision (inference over CVM1 bytecode)
- planning: capture flaky doctor version_falls_back_to_stderr (ETXTBSY) found on CRUSH-170 CI
- feat(crush-pkg): capability inference in `crush-pkg check` (CRUSH-170)
- feat(crush-pkg): squad-bridge-peek example capsule + run grants (CRUSH-172)
- Merge pull request #116 from nixpt/kai/merge-wave-s474
- Merge wave: declare caison.parse's effects
- Merge wave: make lane A's effects wrapper work with lane B's shared registry
- TASKS: dedupe the phase-2 board after the merge wave; renumber CRUSH-149 follow-ups
- Merge remote-tracking branch 'origin/agent/nimbus-b/CRUSH-158' into kai/merge-wave
- Merge remote-tracking branch 'origin/agent/panini-e/CRUSH-175' into kai/merge-wave
- Merge remote-tracking branch 'origin/agent/panini-d/CRUSH-171' into kai/merge-wave
- Merge remote-tracking branch 'origin/agent/nimbus-c/CRUSH-160' into kai/merge-wave
- Merge remote-tracking branch 'origin/agent/nimbus-a/CRUSH-155' into kai/merge-wave
- Merge remote-tracking branch 'origin/agent/panini/CRUSH-149' into kai/merge-wave
- planning: capture missing Crush AI-expression syntax (found CRUSH-158)
- feat(ai_native): QueryProvider / DelegationBackend + delegation selection (CRUSH-158)
- feat(ai_native): toolchain strategy engine, each step grant-gated (CRUSH-157)
- feat(ai_native): caps receive compiled payload + stack operands (CRUSH-156)
- style: rustfmt crush doctor (CRUSH-175)
- feat(crush): crush doctor — polyglot runtime health check (CRUSH-175)
- docs(planning): CRUSH-170 deferred until lane A's caps merge
- docs(planning): CRUSH-171 evidence + decision
- feat(crush-pkg): CRUSH-171 manifest category + platforms metadata
- docs(design): import system, compile pipeline, walker authoring, SBL notes (CRUSH-173)
- docs(planning): CRUSH-167 evidence + decision
- feat(crush-pkg): CRUSH-167 bare crush-pkg builds then runs; build/check refuse non-Crush
- docs(planning): CRUSH-161 evidence, crush-pkg in the publish lane
- fix(crush-pkg): CRUSH-161 build against crates.io crush-buckets 0.1.0
- docs(planning): close archive-zip stdlib restore tickets; record C-2/C-3 decisions (CRUSH-169)
- docs: fix stale exosphere/ecasm references in crush-ast docs (CRUSH-168)
- docs(planning): CRUSH-160 evidence — commit + PR
- feat(debugger): grant-gated debugging, redacted value views, event sink (CRUSH-160)
- feat(vm,sdk): CRUSH-155 — capability effects metadata + crush-run caps --json
- docs(planning): CRUSH-159 evidence — commit + PR
- feat(debugger): step over/out and watchpoints on PortableVm (CRUSH-159)
- docs: CRUSH-154 — decline storage.* (C-6); db.* covers persistence
- docs(planning): CRUSH-176 evidence — commit + PR
- fix(crush-vm): PortableVm skipped a jump landing on its own instruction (CRUSH-176, #94)
- feat(sdk): CRUSH-153 — env.all/env.home_dir, net.http_put/delete/request, deadline-aware HTTP
- feat(sdk): CRUSH-152 — async.sleep under --time, sharing time.sleep's implementation
- feat(sdk): CRUSH-151 — fs coreutils and a VM-local fs.cd inside the --fs sandbox
- feat(sdk): CRUSH-113 — stdlib on by default; --stdlib without the feature is an error
- feat: rename crush-cson to crush-caison, add caison.parse capability (CRUSH-149)
- Merge pull request #95 from nixpt/agent/nimbus/CRUSH-150
- tickets: point CRUSH-55/113/122 at the CRUSH-150 migration inventory
- tickets: file CRUSH-151..175 phase-2 migration tickets + TASKS rows
- docs(planning): CRUSH-150 §5 phase-2 order + decisions, §6 dead weight
- docs(planning): CRUSH-150 §2 stdlib/stdcap/corecap
- docs(planning): CRUSH-150 §1 module map
- docs(planning): CRUSH-150 §4 squeeze
- docs(planning): CRUSH-150 §3 runtimes + captured findings
- docs(planning): CRUSH-150 migration inventory skeleton + prior-art re-check
- Merge pull request #93 from nixpt/agent/nimbus/CRUSH-118
- feat(crush-web): interactive io.read — Session + execute_with stdin (CRUSH-118)
- feat(crush-vm): host-chosen io.read input source on PortableVm (CRUSH-118)



- **`crush-pkg check` checks capabilities (CRUSH-170).** `check` now
  compiles the package the way `build` does (entry plus path deps), so an
  entry that calls a dependency's functions no longer fails it, and then
  compares the capabilities the program uses with `capsule.toml`'s
  `[capabilities]`: used but undeclared is an error, a `required` entry
  nothing uses is a warning, and so is a capability a `platforms = ["web"]`
  capsule can't get in the browser. Findings are `E-CAPS` records under
  `--message-format=json`. Ambient capabilities (VM built-ins, the pure
  stdlib) needn't be declared; families (`"fs"`) and scoped entries
  (`"fs.read:/x"`) count. Details and blind spots: `crates/crush-pkg/MANIFEST.md`.
  New APIs: `crush_vm::capabilities_used(&Program)` (every capability name a
  compiled program can request, from the code reachable from its entry), and
  `crush_lang_sdk::effects::catalog()` (every capability with its effects and
  grant; `crush-run caps --json` prints it and now includes the
  `polyglot.*` gates). `PackageBuilder::check` returns a `CheckReport`.

- **`crush-pkg` grants: `--fs`, `--fs-root`, `--env`, `--time` (CRUSH-172).**
  `crush-pkg run` and bare `crush-pkg` took no grants, so a capsule that
  read a file could not run at all. They now take the same grant flags as
  `crush run`, and register the pure standard library the way `crush run`
  does. New example capsule `examples/crush/capsules/squad-bridge-peek`
  (the first pure-Crush capsule: prints the last entries of a log via
  `fs.cat`, file from `$BRIDGE_PEEK_FILE`, read inside the `--fs` sandbox).
  CI's `Test (sdk)` job now also runs `crush-pkg`'s tests, which no job ran
  before. Library: `runners::get_runner_with` / `get_runner_for_payload_with`
  take the `CrushRunner` to use.

- **`crush-cson` renamed to `crush-caison`; VM capability `caison.parse` (CRUSH-149).**
  CAISON was renamed from CSON on 2026-09-27 ("CSON" already means
  CoffeeScript Object Notation); the crate and capability now match.
  `crates/crush-cson` → `crates/crush-caison` (package `crush-caison`,
  `CsonParseCap` → `CaisonParseCap`); `crush_cast::cson` → `crush_cast::caison`.
  Migration: depend on `crush-caison`; call `caison.parse` from Crush.
  `crush_cast::cson` (a `#[deprecated]` module) and the `cson.parse`
  capability (same handler) keep working until 0.5. The `crush-cson` crate
  name is retired: `crates/crush-cson-shim` publishes a final `crush-cson`
  that only re-exports `crush-caison`. Mechanical rename — no change to the
  value mapping or parser.

- **Capability effects (CRUSH-155).** `crush_vm::HostCap` gains a defaulted
  `effects() -> Option<&'static [&'static str]>` (`None` = undeclared,
  `Some(&[])` = pure; labels like `"fs/read"`, `"env/read"`, `"time/sleep"`,
  `"net/http"`, `"process/spawn"`) — existing implementations compile
  unchanged. Every capability `HostCapsBuilder` registers now declares its
  effects (the stdlib as pure), polyglot gates declare `process/spawn`, and
  `crush-run caps --json` lists name, argc, returns, effects and the granting
  flag for every capability. Informational only: grants still decide access.
  New `HostCaps::into_handlers()`.
- **`env.all` / `env.home_dir` and more HTTP verbs (CRUSH-153).** With
  `--env`: `env.all()` (map of the variables the grant exposes — the host
  environment plus injected values) and `env.home_dir()` (`HOME` /
  `USERPROFILE`, or null). With the `net` feature + `--net`: `net.http_put`,
  `net.http_delete`, and `net.http_request(method, url, body, headers)`, which
  returns `{status, body}` instead of failing on a non-2xx status. All five
  `net.*` verbs now share one request path and honour the VM's wall-clock
  quota (`CapTimeout`) — previously `net.http_get`/`http_post` could block
  past it. exosphere's `http.*` names are not aliased (one name per
  capability).
- **`async.sleep` (CRUSH-152).** Registered with `--time` next to `time.sleep`
  and backed by the same function, so `await async.sleep(ms)` (the exosphere
  / nanovm spelling) works and honours the wall-clock quota (`CapTimeout`). It
  blocks like `time.sleep`; it does not yield to the scheduler. The
  conformance runner learned `// caps: time`; `examples/crush/async_test.crush`
  now passes.
- **fs coreutils + a VM-local working directory (CRUSH-151).** Under `--fs`,
  `crush-lang-sdk` now also registers `fs.ls`, `fs.cat`, `fs.pwd`, `fs.cd`,
  `fs.mkdir`, `fs.rm`, `fs.cp`, `fs.mv`, `fs.touch` and `fs.find`. `fs.cd`
  moves a working directory that belongs to the capability registry (one per
  VM) and that every `fs.*` and `text.*` file cap resolves against; it never
  leaves `--fs-root` and never `chdir`s the process. `fs.pwd` answers relative
  to the root (`.` at the root). Directories need an explicit flag to be
  removed or copied recursively; `fs.rm`/`fs.mv` act on a symlink itself, and
  a recursive `fs.cp` refuses symlinks. `fs.list` now returns sorted names, and
  fs errors show sandbox-relative paths instead of host paths. PortableVm's
  privileged tier now covers `fs.mkdir/rm/cp/mv/touch` as well as `fs.write`.
- **Standard library on by default (CRUSH-113).** The `stdlib` cargo feature
  of `crush-lang-sdk` is now in `default`, and `crush-run` / `crush-repl`
  register the pure stdcaps (`str.*`, `math.*`, `conv.*`, `collections.*`,
  `json.*`, `path.*`, `regex.*`, …) without a flag — they do no I/O and grant
  no authority. `--stdlib` still parses; `--no-stdlib` turns them off. In a
  build without the feature (`default-features = false`), `--stdlib` is now a
  hard error instead of a warning, and `ReplConfig { stdlib: true, .. }` is
  refused instead of silently ignored. `HostCapsBuilder` itself is unchanged:
  embedders still opt in with `.stdlib(true)`. I/O families (`text.head`,
  `time.now`, `fs.*`, …) stay behind their grants.
- **Debugger: grants, redaction and events (CRUSH-160).** Debugging is now a
  granted capability: `debug.step` (control), `debug.inspect.redacted` (values
  as type + per-session hash) and `debug.inspect` (values in full), via
  `HostCaps::grant_debug` or `crush-debugger run --cap debug.*`. **Without a
  debug grant the debugger REPL refuses to run the program** — pass
  `--cap debug.step --cap debug.inspect` for the old behaviour. New
  `DebugEvent`/`DebugEventSink` (a channel sender works) for embedding hosts;
  `PortableVm::frame_snapshot` returns redacted frames. The REPL now shows the
  program's own output.
- **Debugger: step over/out and watchpoints (CRUSH-159).** `PortableVm` gains
  `request_step(StepMode::{Into, Over, Out})` (by call depth),
  `add_watchpoint(slot, WatchScope::{Frame(depth), Top})`, `call_depth()`,
  `local(depth, slot)` and `last_stop()`; stops still surface as
  `VmYield::DebugBreak`. `crush-debugger` adds `next`, `finish`, `watch`,
  `unwatch` and a working `print <slot>`. Bytecode-level: locals are slots
  until the frontend emits a source map.
- **Fix: `PortableVm` diverged from the scheduler on recursive programs
  (CRUSH-176, #94).** A jump that lands on the instruction it came from — a
  recursive call in tail position returning to the caller's own `RET`, or
  `loop: JMP loop` — was treated as "no jump" and fell through. awesome-crush's
  tictactoe, lights_out, blackjack and multi-round blackjack_interactive now
  run the same on `PortableVm` (crush-web `Session`/`execute_with`, the
  debugger, exo-light) as on `crush_vm::run`.
- **Manifest `category` and `platforms` (CRUSH-171).** `[capsule]` takes an
  optional `category` (`cli`, `library`, `app`, `service`, `game`,
  `dev-tool`, `language`, `example`) and `platforms` (any of `linux`,
  `macos`, `windows`, `web`). Unknown values, duplicate platforms, and `web` on
  a non-Crush capsule are load errors that name the accepted values.
  `crush-pkg show` prints both. Manifests without them load as before. The
  schema is documented in `crates/crush-pkg/MANIFEST.md`.

- **`crush-pkg` with no subcommand builds and runs (CRUSH-167, squeeze folded
  in).** A bare `crush-pkg` builds the package (entry + path deps), writes
  `target/<name>.cvm` + `.casm.json`, then runs the program it just built;
  `crush-pkg -- ARGS` passes ARGS through (Script/Native capsules receive
  them; Crush programs have no argv channel yet). Script and native capsules
  skip the build and go straight to the runner. `crush-pkg build`/`check` now
  refuse Script/Native capsules with a clear message (`E-BUILDER`) instead of
  feeding Python or JavaScript to the Crush compiler. Replaces the separate
  `squeeze` tool. New library surface: `crush_pkg::flow` and
  `CrushRunner::run_program`.

- **crush-pkg is publishable (CRUSH-161).** `cargo publish --dry-run -p
  crush-pkg` now verifies against crates.io as it is (every dependency,
  including `crush-buckets` 0.1.0, is live). The script runner's buckets
  sandbox profile no longer names fields that only exist in the unpublished
  buckets checkout. The publish itself is the maintainers' step; see the
  publish lane in `.jagent/planning/tickets/CRUSH-104-publish-lane.md`.
- **`crush doctor` (CRUSH-175).** Reports whether the interpreters polyglot
  blocks spawn (`python3`, `node`, `bash` — taken from `EXEC_LANG`'s own
  allowlist) and the sandbox tools (`bwrap`, `buckets`) are on `PATH`, with
  their versions, plus this build's polyglot features. `--json` for tooling;
  exits 1 when a runtime that `crush run --polyglot` grants is missing (`bwrap`
  counts only in a `sandboxed-polyglot` build). Read-only: it runs `--version`
  and nothing else. New in `crush-vm`: `resolve_lang_binary` is public and
  `SANDBOXED_POLYGLOT` reports that feature.
- **Host backends for `ai_native.query` and `ai_native.agent_delegation`
  (CRUSH-158).** New `ai_native::providers::{QueryProvider,
  DelegationBackend}` traits; `HostCapsBuilder::query_provider(..)` /
  `.delegation_backend(..)` put them behind the gates in place of the echo
  stubs (they take effect only with `ai_native(true)` — a backend is not a
  grant). Delegation picks agents (`first_available`, `broadcast`, `best`,
  `round_robin`; others are an error) from the backend's reported status and
  validates each result against `expected_format` (`json`, `structured`,
  `text`). No real backend ships; `ai_native::register_with` is the
  non-builder entry point.

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
