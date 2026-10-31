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

- `crush-pkg` (CRUSH-348/349/350): `run` now builds and runs the same combined program as a bare
  `crush-pkg` (path dependencies visible); `sys.exit(n)` ends `crush-pkg` with status `n` and keeps
  output printed before it (no `E-RUN` banner; real runtime errors keep it); the unused-dependency
  note no longer fires for a dependency whose functions the entry calls.
- `crush-pkg` git dependencies (CRUSH-351): `[[dependencies]]` takes `git = URL` plus one of
  `tag` / `rev` / `branch`; fetched with the `git` CLI into `$CRUSH_PKG_CACHE`
  (default `~/.cache/crush-pkg`), pinned to a commit in `capsule.lock`, `--offline` uses the
  lock and cache only. See `crates/crush-pkg/MANIFEST.md`.
- `crush-pkg new NAME --cli` (CRUSH-352): scaffolds a facet-based command-line program that runs
  out of the box (`--facet-path DIR` for a local facet checkout).
- `crush update` / `crush-installer update` (CRUSH-344): update a tarball or installer install from
  the GitHub release binaries — SHA256SUMS-verified before extraction, all-or-nothing rename swap,
  `--check` (exit 10 = update available), `--version` pin, `--dir`, refuses cargo-managed installs
  unless `--force`. x86_64 Linux only.

## [0.9.0] - 2026-10-31

- Merge pull request #281 from nixpt/agent/vega/1.0-criteria
- docs(planning): R1 done (#348 merged); scoreboard recounted
- Merge remote-tracking branch 'origin/main' into vega-281-update
- Merge pull request #353 from nixpt/agent/vega/crush-errors-bucketspike
- Merge pull request #350 from nixpt/agent/vega/CRUSH-166-exec-lang-callbacks
- Merge pull request #354 from nixpt/agent/satya/CRUSH-165
- Merge pull request #351 from nixpt/agent/nova/CRUSH-164
- Merge remote-tracking branch 'origin/main' into agent/vega/crush-errors-bucketspike
- Merge pull request #352 from nixpt/agent/vega/remove-top-level-stdlib
- Merge pull request #349 from nixpt/agent/vega/docs-refresh-0.9
- Merge pull request #348 from nixpt/agent/vega/no-accidental-1.0
- feat(crush-vm): tamper-evident CVM1 execution transcript (CRUSH-165)
- refactor(crush-errors): drop exosphere-only error modules and version boundaries; remove crush-bucketspike
- chore: remove the top-level stdlib/ (unused transpiled-module experiment)
- feat(crush-vm): wildcard + expiry entries in Quotas::allowed_caps (CRUSH-164)
- docs(planning): migration re-check covers nanovm, casm, crush-cast
- docs(design): EXEC_LANG callbacks design note (CRUSH-166); re-check exosphere migration
- Merge remote-tracking branch 'origin/main' into agent/vega/docs-refresh-0.9
- docs: refresh README, CLAUDE.md/AGENTS.md, .dejavue and .jagent for 0.9
- Merge pull request #347 from nixpt/agent/nixp/unicode-support
- docs(planning): re-score the 1.0 criteria against main 2026-10-31; split CRUSH-75
- Merge remote-tracking branch 'origin/main' into agent/vega/1.0-criteria-rescore
- fix(release): no accidental 1.0 — a breaking change on 0.x bumps minor
- feat(tree-sitter): slices and hex/binary integer literals
- Merge origin/main into unicode-support (CHANGELOG: keep both; .dejavue union files)
- Merge pull request #290 from nixpt/agent/cursor/CRUSH-zorro-44-design
- Merge pull request #346 from nixpt/agent/nova/CRUSH-342
- Merge pull request #345 from nixpt/agent/buffy/shell-walkers-completion
- feat(slices): bracket slices desugar to seq.slice — string slicing + [::step] syntax
- docs(CRUSH-342): ticket lists crush-gui as shipped
- Merge origin/main into agent/nova/CRUSH-342; ship crush-gui in release tarball
- Merge pull request #344 from nixpt/agent/nova/CRUSH-319
- fix(ci): artifacts smoke test - crush-run has no --version
- Merge pull request #277 from pranix/bro/fix-bash-vars-255
- Merge pull request #291 from nixpt/agent/cursor/CRUSH-286-ticket-ids
- docs(CRUSH-310): record what CI runs and the workspace link failure
- ci(release): pre-release per tag with prebuilt artifacts; PR CI checks release instead of building (CRUSH-342)
- Merge pull request #342 from nixpt/agent/nixp/CRUSH-207
- feat(walkers): glob options, file tests and a merge-sorted glob walk (CRUSH-310)
- Merge origin/main (#334) into CRUSH-207 (.dejavue union files)
- Merge pull request #334 from nixpt/agent/buffy/shell-walkers-completion
- ci(gui): keep crush-gui out of the default and --workspace builds; add a Test (gui) job
- Merge origin/main into fix-bash-vars-255
- fix(xtask): ticket-id next also counts local branches; allowlist matches main (CRUSH-286)
- feat(gui): crush-gui - grant-gated ui.* API and arniko desktop backend (CRUSH-319)
- Merge remote-tracking branch 'origin/main' into agent/vega/CRUSH-286-update
- Merge origin/main into CRUSH-207 (CHANGELOG: keep both; .dejavue union files)
- Merge origin/main into shell-walkers-completion
- Merge pull request #343 from nixpt/agent/nova/CRUSH-318
- fix(walkers): refuse multi-column `wc` on a `< file`, whose padding is version-dependent
- Merge pull request #337 from nixpt/agent/nixp/CRUSH-zorro-8
- Merge pull request #335 from nixpt/agent/nixp/CRUSH-203
- docs(CRUSH-311): record builtin-only pipelines as the decision, and what it covers
- feat(walkers): run pipelines, redirections and a builtin command set in-VM (CRUSH-311)
- Merge pull request #341 from nixpt/agent/nova/CRUSH-317
- Merge #337 (with #335, #334, #341, #340, main) into CRUSH-318
- Merge #335 (with #341, #340, #334, main) into CRUSH-zorro-8 (CHANGELOG: keep both)
- Merge #341 (with #340, main) and #334 into CRUSH-203 (CHANGELOG: keep both)
- local base: #341 + #334 (not pushed)
- Merge pull request #340 from nixpt/agent/nova/CRUSH-264
- Merge #340 (with main, incl. #336/#338/#339) into CRUSH-317
- fix(tree-sitter): track the frontend's syntax, and check the corpus in CI (CRUSH-207)
- Merge origin/main (incl. #336, #338, #339) into CRUSH-264 (CHANGELOG: keep both)
- Merge pull request #338 from nixpt/agent/nova/CRUSH-315
- Merge pull request #339 from nixpt/agent/satya/planning-survey
- Merge pull request #336 from nixpt/agent/nova/CRUSH-316
- feat(sdk): wire crush-net in as tcp.* capabilities behind --tcp / --tcp-listen
- feat(debugger): crush debug FILE / crush-debug, grant flags, NDJSON mode (CRUSH-317)
- feat(sdk): net on by default, http.* renamed loudly, document time.now unit (CRUSH-264)
- docs(planning): add SURVEY-2026-10-31 — what changed, judgment calls, unverified items
- docs: fix verified drift in crate counts, version, internal links; point docs/tasks at the board
- refactor(crush-net): depend on crush-vm not crush-lang-sdk; rename caps net.* -> tcp.*; split register into client/listen/io groups
- docs(planning): bring indexes in line with ticket statuses and v0.8.0
- docs(CRUSH-310): point the pipelines follow-up at CRUSH-311
- docs(planning): renumber colliding CRUSH-266 / CRUSH-271 tickets to CRUSH-340 / CRUSH-341
- docs(planning): set ticket statuses from merge evidence; normalise spellings
- fix(diagnostics): one JSON schema and one record per error across crushc and crush-run (CRUSH-315)
- fix(js): a JS/TS dialect comes from the real extension; unknown ones refused (CRUSH-zorro-8)
- fix(crush-lint): report real parser/semantic diagnostics, never a guessed syntax error (CRUSH-316)
- fix(walkers): lower a node or refuse it loudly, never fabricate null/empty (CRUSH-203)
- Merge remote-tracking branch 'origin/main' into agent/buffy/shell-walkers-completion
- feat(walkers): glob every path component, not just the last (CRUSH-310)
- docs: bring the Bash/Zsh walker rows up to date, and file the process/fs-write decision
- Merge pull request #325 from nixpt/agent/nixp/CRUSH-299
- Merge pull request #326 from nixpt/agent/nixp/CRUSH-300
- Merge #326 (with main, incl. #322/#327) into CRUSH-299; combine walk_run's error handling
- Merge origin/main into CRUSH-300 (CHANGELOG: keep both)
- Merge pull request #327 from nixpt/agent/buffy/shell-walkers-completion
- Merge pull request #328 from nixpt/agent/nixp/CRUSH-301
- fix(sdk): kill spawned tasks when the run ends; --task states its process grant (CRUSH-182)
- Merge remote-tracking branch 'origin/main' into agent/buffy/shell-walkers-completion
- feat(walkers): pathname expansion over fs.exists/fs.list (CRUSH-310)
- fix(capi): report runtime errors, route io.print to stdout (CRUSH-205)
- Merge pull request #322 from nixpt/agent/buffy/shell-walkers-completion
- Merge pull request #324 from nixpt/agent/nixp/CRUSH-298
- fix(cli): crush-walk-run keeps output before a runtime error and exits non-zero (CRUSH-184)
- Merge pull request #323 from nixpt/agent/nixp/CRUSH-297
- Merge pull request #321 from nixpt/agent/nixp/CRUSH-296
- docs(walkers): point printf-float divergence at the renumbered ticket
- Merge #323 (with #321 and main) into CRUSH-298 (CHANGELOG: keep both)
- Merge remote-tracking branch 'origin/agent/buffy/shell-walkers-completion' into agent/buffy/shell-walkers-completion
- feat(walkers): glob patterns in ${x/pat/rep} and printf float conversions (CRUSH-294)
- fix(sdk): deterministic CASM function layout (CRUSH-188)
- planning: renumber the shell-walkers ticket CRUSH-298 -> CRUSH-310
- Merge #321 (CRUSH-296, with main) into CRUSH-297 (CHANGELOG: keep both)
- Merge origin/main into CRUSH-296 (CHANGELOG: keep both)
- planning: CRUSH-298 inside the renumbered ticket, too
- planning: renumber the shell-walkers ticket CRUSH-294 -> CRUSH-298
- Merge pull request #319 from nixpt/agent/nixp/CRUSH-294
- fix(go-walker): no silent drops — lower or loudly reject (CRUSH-198)
- Merge pull request #318 from nixpt/agent/nixp/CRUSH-213
- Merge pull request #317 from nixpt/agent/nixp/CRUSH-215
- feat(walkers): lower [[ ]], arrays, ${…} operators, case globs, printf formats, read for bash/zsh (CRUSH-294)
- Merge #318 (CRUSH-213, with #317) into CRUSH-294
- Merge #317 (CRUSH-215) into CRUSH-213; restore the JIT's fcmp helper
- Merge origin/main into CRUSH-215 (keep both CHANGELOG entries and both new tests)
- fix(sdk): stdlib caps reject negative lengths and bound allocations (CRUSH-181)
- Merge pull request #316 from nixpt/agent/nixp/CRUSH-295
- Merge pull request #320 from nixpt/agent/vega/publish-keep-going
- ci(publish): keep going past a crate crates.io refuses; list them all
- fix(jit): frame stride is the program's largest local count (CRUSH-295, GitHub #273)
- fix(vm): FastVM/JIT print heap values, not the arena ref (CRUSH-294, GitHub #267)
- fix(vm,jit): division/remainder edge cases fail loudly on every engine (CRUSH-213)
- fix(vm,jit): one canonical equality — strings/arrays/maps compare by value (CRUSH-215)
- docs: get ticket numbers with ticket-id next (CRUSH-286)
- test(xtask): gate duplicate ticket IDs in CI
- feat(xtask): ticket-id allocator and shared unique ticket-ID check
- docs(design): COSM method dispatch rule for receivers (CRUSH-zorro-44)
- docs(planning): draft Crush 1.0 release criteria (CRUSH-275)
- Fix #255: restore Crush var access in @bash blocks via $name/${name} scan



- **Source-level string slices (and full `xs[start:stop:step]` syntax) via the
  shared `seq.slice` capability:** the parser desugared bracket slices to the
  array-only `arr_slice`, so `s[0:5]` on a string errored at runtime with
  "expected array, got str" on every engine, even though the capability layer
  (`slice_indices` in `seq.rs`) had always been char-based for strings and
  arrays, and both AOT backends already lowered `seq.slice` for strings.
  Bracket slices now lower to `seq.slice` (target, start, stop, step with
  `null = from-the-edge`), which also unlocks Python's full slice syntax —
  `xs[:]`, `xs[::2]`, `xs[::-1]`, `xs[a::b]` were unparsable outright
  (`::` lexes as one DoubleColon token, the Rust path separator). Slicing stays
  Unicode-scalar-safe end to end: `u[::-1]` on `"日本語🎉!"` keeps the
  4-byte emoji intact on the PortableVm, scheduler, both AOT backends and the
  cross-engine conformance corpus (`examples/crush/unicode_slice_test.crush`).
  Regression guards: `crush-lang-sdk` `string_slice_test` (char boundaries,
  no scalar splitting, step-form slot order), `crush-frontend` compiler tests
  (string targets reach `seq.slice`; the step lands in the step slot).

- **Every tag now has a GitHub pre-release with prebuilt artifacts (CRUSH-342):**
  `release.yml` dispatches the new `artifacts.yml`, which builds the tag and
  attaches `crush-<ver>-x86_64-unknown-linux-gnu.tar.gz` (the CLIs),
  `crush-web-<ver>-pkg.tar.gz` (browser wasm package) and `SHA256SUMS`;
  `cargo binstall crush-lang-sdk` / `crush-pkg` use it. Pre-1.0 releases are
  marked pre-release and not latest. PR CI runs `cargo check --release` instead
  of the full release build (which still runs on `main`).

- **`tree-sitter-crush` tracks the frontend again, and CI now checks it
  (CRUSH-207):** the grammar parsed **1 of 51** `examples/crush/*.crush` without
  ERROR nodes. The comment rule knew only `#`, so a file opening with `//` died
  at line 1; assignment accepted only `a.b = c`; there were no return types,
  string escapes, logical operators, object/struct literals, `@`-annotations or
  `semantic_switch`. All of those are now handled — **49 of 51** parse clean.
  A new `crates/tree-sitter-crush/tests/grammar_examples.rs` parses the corpus
  and the crate's fixtures on every `cargo test`, failing on ERROR nodes with a
  `KNOWN_BAD` allowlist that may only shrink (it runs in the existing `test-rest`
  CI job; the grammar corpus still needs the tree-sitter CLI). Note for
  consumers: the `field_assignment` node is now `assignment` (it covers
  identifier/index targets too), and regenerating `parser.c` needs tree-sitter
  CLI ≥ 0.25.

- **Raw TCP/TLS sockets as `tcp.*` capabilities, behind their own grants
  (CRUSH-318):** `crush-net` is now wired into `crush-lang-sdk` (cargo features
  `tcp`, `tcp-tls`, on by default; grants are never on by default). `crush-run
  --tcp` grants outbound client sockets (`tcp.connect`, `tcp.tls_wrap`,
  `tcp.send`/`recv`/`close`/`ping`); `crush-run --tcp-listen` is a separate,
  stronger grant for being a server (`tcp.listen`, `tcp.accept`, plus
  send/recv/close/ping on the connections they return, but not `tcp.connect`).
  `--net` stays HTTP-client-only and grants neither. The caps were renamed
  from `net.*` to `tcp.*` so the `net` family can never cover them, and
  `crush-net` now depends on `crush-vm` instead of `crush-lang-sdk` (the old
  edge was a cycle). Also fixed in `crush-net`: `tcp.send`/`recv`/`close` did
  not work on a `tcp.tls_wrap`ped connection, `tls_wrap` could hand out an id
  that collided with a later plain connection, and `tcp.accept` held the
  shared state lock while blocking.

- **JS/TS dialect handling: unknown extensions are refused, `cts` is TypeScript
  (CRUSH-zorro-8):** `JsLowering::for_ext` treated every unrecognised extension
  as plain JS, so a `.ts` file reached as `.tmp` (or a typo like `typescript`)
  parsed TypeScript as ES. It now accepts exactly
  `js mjs cjs jsx ts tsx mts cts` and refuses anything else, naming the set;
  `cts` is TypeScript; and `SubprocessWalker` names its temp file
  `walker_N.<ext>` so a walked `.ts` keeps its dialect.

- **Stub walkers no longer produce empty programs (CRUSH-203):** the Zig walker
  matched the wrong tree-sitter node names (`IntegerLiteral` vs the real
  `integer`/`float`/`string`/`boolean`, `BinaryExpr` vs `binary_expression`, …),
  so every literal silently became `null` and every program printed nothing; the
  names and wrappers are fixed and unhandled nodes now error with the node kind
  and position. Dart and Java (which returned an empty program / panicked with a
  bare `unreachable!`) fail with "not implemented". The Wasm walker no longer
  drops every non-`call` operator or lowers `fd_write` to an arg-less
  `io.print`; it errors loudly. `crush-walk-run` reads a `.wasm` file as bytes.
  The Nepali walker's false "native Nepali keywords" comment is corrected.

- **`crush-gui`: Crush programs can draw a window (CRUSH-319, phase 1):** a new
  crate with a grant-gated `ui.*` capability API and a desktop backend on
  [arniko](https://crates.io/crates/arniko) 0.3.1 (from crates.io; it is not in
  `crush-lang-sdk`'s default build, since arniko pulls Stylo/vello/winit). A
  program renders HTML with `ui.render(markup)` and *pulls* events in a loop with
  a blocking `ui.next_event()` (a map `{kind, id, value, key}`: `click`, `input`,
  `submit`, `key`, `close`), like `io.read` - no callbacks into the VM. Component
  helpers: `ui.button/input/card/alert/badge/escape`. Run with
  `crush-gui run app.crush --ui` or `crush gui run app.crush --ui`; `--ui` is
  the grant (a program using `ui.*` without it is refused before it runs, naming
  the flag; no other flag grants it), and `crush-run` now points such a program
  at `crush gui`. `crush-installer` installs `crush-gui` when it is built. Tests
  run the examples (`examples/gui/`) headlessly against a real bliss DOM. The
  desktop window itself has not been run on a screen. Design:
  `docs/design/ui-api.md`; the terminal (tui-easy) and browser backends are
  follow-ups behind the same `UiBackend` seam.

- **`crush debug FILE` / `crush-debug`: the debugger is runnable (CRUSH-317):**
  `crush-debugger` had breakpoints, step/next/finish, watchpoints and
  grant-gated inspection but no front end that compiled Crush source, so it could
  only debug hand-written CASM. `crush debug prog.crush --step --inspect` now
  compiles through `crush run`'s pipeline and opens the REPL; `break FILE:LINE`
  resolves in the *source's* lines (the compiler's source map, which used to be
  the generated assembly's), `print total` / `print p.x` / `watch total` /
  `locals` use variable names, `step`/`next` go by source line, and stops show
  the line. Debugging grants nothing by default: `--step` to run and step,
  `--inspect=redacted` / `--inspect` to see values; a refused command names the
  flag. `--message-format json` emits the `DebugEvent`s as NDJSON for an editor or
  agent. `crush-installer` now installs `crush` and `crush-debug`. Fixes found on
  the way: hashing a closure that captures itself (`let f = |n| f(n - 1)`)
  overflowed the stack under `--inspect=redacted`, and `BreakpointSet::remove`
  used `IndexMap::remove`, now a deprecation error under `-D warnings`.

- **`net` on by default, `http.*` refused with its replacement, `time.now` unit
  documented (CRUSH-264):** `crush-lang-sdk`'s `default` features now include
  `net`, so `crush-run --net` grants `net.http_get/post/put/delete/request` out of
  the box (still refused up front without `--net`); `db` stays opt-in
  (`--features db`) and `crush-run caps` says so. exosphere's `http.get/…` have no
  alias: a program calling one is refused before it runs with `http.get was
  renamed to net.http_get (grant it with --net)` (`effects::RENAMED_CAPS`, shared
  by every runner that preflights). `time.now` is seconds, `time.now_ms`
  milliseconds (exosphere's `time.now` was ms) — now stated in `crush-run caps`,
  the SDK README and MIGRATION-INVENTORY. `examples/crush/httpclient.crush` uses
  `net.http_get` and `str.format` and compiles. Embedders that don't want the
  HTTP client: `default-features = false`.

- **Consistent `--message-format json` diagnostics across `crushc` and `crush-run`
  (CRUSH-315):** the same broken program now yields the same NDJSON records
  from both. `crush-run` used to flatten every compile failure to `E-IO` with no
  position; it now emits `E-PP0n` (syntax, with line/col) and `E-TP01`
  (semantic), exactly as `crushc` does. Semantic errors carry line/col for
  undefined variables/functions and type errors (the analyzer now reads the
  AST locations, and the parser stamps `Var`/`Call`/`BinaryOp`/statement
  locations at the construct instead of the token after it). A runtime error's
  text is now in `message` (`E-RT05` used to leave it empty and put it in
  `hint`); a program needing an ungranted capability is reported as `E-CAPS`
  (hint: the grant flag) instead of `E-IO`. `file` is filled on every
  `crushc`/`crush-run` record. The wire struct is defined once, in
  `crush-diagnostics` (`Diagnostic`); `crush_lang_sdk::theme::JsonDiagnostic` is
  an alias, and its `parse_error` / `runtime_error` constructors moved to the
  `JsonDiagnosticExt` trait. New in `crush-frontend`: `ParseErrors`,
  `SemanticError` (typed `anyhow` payloads, `Display` unchanged).

- **`crush-walk-run` keeps output before a runtime error and exits non-zero
  (CRUSH-184):** it buffered the first run's output and replaced it with
  `Error: …` on failure, and returned `Ok`, so a failing script exited 0 and
  everything printed before the error was lost. It now streams the first run's
  output (so nothing is lost) and fails with a non-zero exit. `crush-run` (and
  `crush run`, which delegates to it) already streamed and exited non-zero.

- **C API reports runtime errors and routes `io.print` to stdout (CRUSH-205):**
  `crush_vm_run_casm` treated any `Ok(FastYield)` as success, so a program that
  raised a runtime error (a stack underflow, a bad jump, an unsupported
  capability) returned `0` with `last_error` null; it now returns `-2` with a
  message for a run that did not finish, and clears `last_error` on success.
  The bare FastVM has no capabilities, so the embedded `cap_call io.print` went
  nowhere and `examples/embed.c` printed nothing — `run_casm` now runs with a
  stdout `io.print` capability, so the example prints `42`.

- **Spawned tasks no longer outlive the VM (CRUSH-182):** `TaskState` now
  implements `Drop`, killing and reaping every still-running task when the
  capability registry is dropped at the end of a run — a spawned `sleep 333`
  no longer survives `crush-run`. `--task` spawns host processes, so its help
  text now says plainly that it grants `process/spawn`.

- **Deterministic CASM function layout (CRUSH-188):** `crushc --emit casm` laid
  the functions out into the code buffer by iterating a `HashMap`, so the same
  source compiled to different CASM (and different label numbering) from run to
  run — bad for reproducible builds and the AOT build cache. Functions are now
  emitted in name order.

- **Go walker: no more silent drops (CRUSH-198):** the walker matched
  `short_variable_declaration`, but tree-sitter-go emits `short_var_declaration`,
  so `a := 7` was dropped and `a` read as uninitialised; `var`, assignment,
  `++`/`--`, `for` (all shapes), bare blocks and `if`/`for` initializers were
  silently dropped too, and unknown expressions became `null`. All are lowered
  now, and any construct the walker does not support (`switch`, `defer`, a
  two-variable range, a multi-value `return`, unknown expressions) is a loud
  error instead of a silent drop. The README feature table matches reality.

- **Stdlib caps no longer panic or abort the host on bad arguments
  (CRUSH-181):** `str.repeat`, `str.pad_left`/`pad_right` and
  `collections.chunk` cast negative lengths to `usize` (wrapping to a huge
  value → "capacity overflow" / OOM abort); `conv.parse_int` panicked on a
  radix outside 2..=36; `buffer.alloc` and `str.repeat` could request an
  unbounded allocation. All now give a runtime error, and every
  allocation-growing cap (`str.repeat`/`pad_*`/`join`, `buffer.alloc`) is
  bounded by a fixed 64 MiB limit.

- **FastVM/JIT print heap values, not the arena ref (CRUSH-294, GitHub #267):**
  `io.print` of an array, map or struct printed the arena index (`@259`) on
  FastVM and the JIT and finished successfully, while every other engine
  printed the value. `runtime_value_to_text` (the `io.print` formatter both
  fast engines use) resolved a ref only for strings; it now renders every heap
  kind exactly like the VM's `Display` (`[a, b]`, `{k: v}`, `Vector[…]`, …).
  Covered by an all-engine print test (`stdout_parity::heap_values_print_…`).

- **Division edge cases fail loudly on every engine (CRUSH-213):**
  `i64::MIN / -1` (and `% -1`) panicked in the interpreter and FastVM and
  SIGFPE'd on AOT C; the JIT trapped (SIGILL/SIGFPE) on a zero divisor and
  silently returned `inf` for `1.0 / 0.0`. The interpreter and FastVM now use
  `checked_div`/`checked_rem` (→ `arithmetic overflow`), and the JIT guards
  the divisor before `sdiv`/`srem`/`fdiv`, reporting `division by zero` /
  `arithmetic overflow` like the other engines. AOT already did the right
  thing. All six engines agree (verified by
  `backend_agreement::division_edge_cases_agree_across_every_engine`).

- **FastVM/JIT `==`/`!=` for strings, arrays, maps, bools and null
  (CRUSH-215):** both fast engines reported `false` for every non-numeric
  `==` — so `"ab" == "ab"` was `false` and `"ab" != "ab"` was `false` too
  (FastVM), and the JIT compared strings by their NaN-boxed bits so string
  `==` was always `false` and `!=` always `true`. A single canonical equality
  (`crush_vm::value::runtime_value_eq`) now backs FastVM (`Eq`/`Ne`) and the
  JIT (new `OP_CMP_EQUAL` runtime helper); it mirrors the interpreter's
  `Value::PartialEq`. All six engines now agree on `==`/`!=` for every shared
  value kind (verified by
  `backend_agreement::equality_agrees_across_every_engine`).

- **JIT: a function with more than 8 locals lost them across a call
  (CRUSH-295, GitHub #273):** the JIT addressed each frame with a fixed 8-slot
  stride and did not bounds-check local indices, so a caller using local 8+
  spilled into the callee's frame and had its locals overwritten — the run
  finished with wrong values while VM/FastVM/AOT were correct. The frame stride
  is now the program's largest local count, so frames cannot overlap (a frame
  that does not fit is refused → FastVM fallback).

## [0.8.0] - 2026-10-30

- Merge pull request #315 from nixpt/agent/vega/fs-backend
- Merge pull request #314 from nixpt/agent/vega/crush-web-deferred-lang
- fix(sdk): fs paths use has_root, so the in-memory fs works on wasm32
- feat(sdk,web): the --fs sandbox over a storage backend; an in-memory one for the browser
- fix(crush-web): serialize report maps as plain objects (lang.env)
- feat(crush-web): deferLang, so a page can run a @lang block asynchronously
- Merge pull request #313 from nixpt/agent/vega/312-lower-trim
- fix(polyglot): name an unlowered walker intrinsic by its source construct (#312)
- Merge remote-tracking branch 'origin/main' into agent/vega/312-lower-trim
- Merge pull request #311 from nixpt/agent/vega/crush-web-size
- fix(polyglot): a lowered block prints and fails as EXEC_LANG does (#312)
- docs(crush-web): JS marshaling is boa, not swc; feature sizes; build recipe; #312
- build(crush-web): strip debuginfo only, so wasm-bindgen keeps externref glue
- build(crush-web): drop swc and add a size-tuned release profile (9.79 -> 4.99 MB wasm)
- Merge pull request #310 from nixpt/agent/vega/257-builtin-values
- Merge pull request #309 from nixpt/agent/vega/256-for-var-methods
- Merge pull request #308 from nixpt/agent/vega/254-negative-slices
- Merge pull request #307 from nixpt/agent/vega/253-return-match
- Merge pull request #306 from nixpt/agent/vega/260-crushc-ctor
- fix(sema): a built-in used as a value names itself and its wrapper (#257)
- fix(compiler): method calls on a for loop variable (#256)
- fix(vm): negative slice bounds count from the end (#254)
- fix(parser): return match / new / async.… return their value (#253)
- fix(crushc): constructor calls of @kind(construct: …) Kinds compile (#260)
- Merge pull request #305 from nixpt/agent/vega/304-crush-pkg-run
- fix(pkg): crush-pkg runs Crush CLI programs — args, quotas, .cvm (#304)
- Merge pull request #303 from nixpt/agent/vega/302-fs-write-bytes
- fix(sdk): fs.write writes bytes and buffers raw (#302)
- Merge pull request #301 from nixpt/agent/muse/CRUSH-zorro-7
- Merge main into agent/muse/CRUSH-zorro-7 (resolve against merged boa PRs)
- Merge pull request #300 from nixpt/agent/muse/CRUSH-293-d3
- Merge main into agent/muse/CRUSH-293-d3 (resolve against merged boa PRs)
- Merge pull request #299 from nixpt/agent/muse/CRUSH-293
- Merge main into agent/muse/CRUSH-293 (resolve against merged boa PRs)
- Merge pull request #297 from nixpt/agent/muse/CRUSH-292
- Merge main into agent/muse/CRUSH-292 (resolve against merged boa PRs)
- Merge pull request #294 from nixpt/agent/muse/CRUSH-289
- Merge main into agent/muse/CRUSH-289 (resolve against merged boa PRs)
- Merge pull request #293 from nixpt/agent/muse/CRUSH-288
- Merge main into agent/muse/CRUSH-288 (resolve against merged boa PRs)
- Merge pull request #298 from nixpt/agent/muse/CRUSH-293-spread-probe
- Merge pull request #296 from nixpt/agent/muse/CRUSH-zorro-5-6-probe
- Merge pull request #295 from nixpt/agent/muse/CRUSH-290
- Merge pull request #292 from nixpt/agent/muse/CRUSH-286
- test(js): ctrl_finally is exact on boa — move it to BOA_FIXED (CRUSH-zorro-7)
- CRUSH-zorro-7: boa try/finally runs on every JS-level exit
- CRUSH-293 D3: call_value_apply opcode for value-callee spreads (boa)
- CRUSH-290 review: === is total — identity for instances, classes, functions
- CRUSH-293 (D2): spread arguments expand at known plain signatures
- CRUSH-293: scope spread call arguments for the boa walker (ticket)
- CRUSH-292: logical assignment (&&=, ||=, ??=) in the boa walker
- CRUSH-zorro-5/6 probe: boa bare blocks proven, labeled jumps pinned loud
- CRUSH-290: js.strict_eq capability for JS === / !== and switch case tests
- Merge pull request #289 from nixpt/agent/muse/CRUSH-281
- Merge pull request #288 from nixpt/agent/muse/CRUSH-284
- CRUSH-281 review: pending classes are scoped per statement list and binding
- CRUSH-290: file the === / !== never-compiles bug
- CRUSH-289: optional chaining in the boa walker
- CRUSH-288: new Map() / new Set() host constructors in the boa walker
- CRUSH-287: destructuring declarations in the boa walker
- CRUSH-284: drop unused variable in destructure
- CRUSH-284 review: destructuring defaults fire on absence, not explicit null
- Merge pull request #287 from nixpt/agent/kai/CRUSH-280-bench-fixtures-and-ticket-hygiene
- Merge pull request #284 from nixpt/agent/nixp/CRUSH-275
- CRUSH-281: new C() inside top-level functions sees module classes
- docs(planning): wheel CI ticket is CRUSH-285 (CRUSH-275 is #281's 1.0 criteria)
- docs(planning): load-of-program-function ticket is CRUSH-266a (a residue suffix; CRUSH-284 is #288's)
- CRUSH-284: destructuring parameters in the boa walker
- Merge pull request #286 from nixpt/agent/muse/CRUSH-280
- Merge pull request #285 from nixpt/agent/muse/CRUSH-277-s5
- docs(planning): load-of-program-function ticket is CRUSH-284 (CRUSH-283 is #285's print-hooks ticket)
- CRUSH-286 re-review: kind forgetting covers logical/nested/loop/closure assignment
- CRUSH-286 review 3: instance kind tracking forgets
- CRUSH-286 review 1+2+4: member-call shapes validated after lowering
- CRUSH-280: default/rest parameters on boa class members
- CRUSH-285 re-review: statics chain walk stops at first holder; member_call bail ahead of grant split
- docs(planning): load-of-program-function ticket is CRUSH-283 (CRUSH-281 is #285's)
- Merge origin/main into CRUSH-277-s5 (CRUSH-266 exported declarations)
- Merge remote-tracking branch 'origin/main' into agent/muse/CRUSH-277-s5
- CRUSH-285 review 7+8: walk-time refusals stay walk-time; loader validates opcodes
- CRUSH-285 review 6/6: class fields define, don't set; capture/branch guards
- CRUSH-285 review 5/6: optimizer sees writes inside new arguments
- CRUSH-285 review 3/6: statics are inherited, shared, and class-only
- CRUSH-285 review 4/6: a missing method is a refusal, not a granted capability
- CRUSH-285 review 2/6: block-bound instances call their own binding
- CRUSH-285 review 1/6: accessors run on the caller's own stack
- docs(planning): bench fixture ticket is CRUSH-282 (CRUSH-280 is #286's)
- docs(planning): ticket hygiene — zorro-40/41 done, CRUSH-275 collision, zorro-44 status
- test(bench): cast_compile fixtures re-recorded; CAST_FIXTURES_UPDATE mode (CRUSH-280)
- Merge pull request #283 from nixpt/agent/muse/CRUSH-266
- CRUSH-277 S5: super.m(), class fields, statics, corpus
- CRUSH-277 S4: accessors run on the COSM VMs
- CRUSH-277 S3: new, this and method calls end to end (boa)
- feat(casm,vm): prototype_chain kinds run on the COSM VMs (CRUSH-277 S2)
- test(sdk): a class field is the refused JS construct now (CRUSH-277 S1)
- feat(cast,frontend,js): COSM transport + boa class emission (CRUSH-277 S1)
- docs(planning): scope COSM K3 JS slice as CRUSH-277
- docs: record the wheel decision and file CRUSH-275 (wheel CI coverage)
- docs(crush-vm-py): document the ABI switch and the return_value contract
- test(crush-vm-py): add a wheel smoke test
- feat(crush-vm-py): surface VmResult.return_value from run_blob
- fix(crush-vm-py): build the wheel against the stable ABI
- test(js): boa exported function end-to-end; file CRUSH-276 blocker queue
- fix(crush-lang-js): boa lowering keeps exported function declarations (CRUSH-266)
- docs(planning): CRUSH-267 superseded by CRUSH-250 function values
- fix(crush-lang-js): keep exported declarations when lowering to CAST (CRUSH-266)



- **CASM/CVM1 versioning (CRUSH-277/278/280, CRUSH-285 review):** every
  program's CASM is now stamped with the current format version
  (CRUSH-278) — this applies to *all* programs, not only ones using
  classes. CVM1 gained the `LOAD_KIND` opcode (`0xCB`) and the blob
  loader now validates the code section at load time: a byte that
  isn't an opcode of the format is refused while loading
  (`unknown opcode … at code offset …`) instead of surfacing
  mid-execution. Blobs written by older versions still load.
- **JS classes (CRUSH-277, boa walker):** classes, `new`, `this`,
  methods, `extends`/`super`, accessors, class fields and statics run
  on the COSM prototype-chain VMs (scheduler + portable). Accessors
  execute as ordinary calls on the running thread (the CRUSH-285
  review): the caller's step/call-depth quotas and `try`/`catch`
  apply to them. Default and rest parameters work on constructors,
  methods and statics (CRUSH-280).

## [0.7.1] - 2026-10-20

- Merge pull request #282 from nixpt/agent/vega/fix-bump-aliased-deps
- fix(release): bump renamed workspace deps; refuse to tag on a stale member requirement



## [0.7.0] - 2026-10-20

- Merge pull request #280 from nixpt/agent/vega/ci-release-artifacts
- ci: upload release binaries as an artifact (14 days)
- Merge pull request #276 from nixpt/nixpt-patch-1
- Update README.md
- Merge pull request #263 from nixpt/agent/buffy/CRUSH-quickwins-11-35
- Merge pull request #272 from nixpt/agent/nixp/CRUSH-xps-13-method-contracts
- Merge main into agent/buffy/CRUSH-quickwins-11-35
- Merge pull request #274 from nixpt/agent/claude/CRUSH-272
- Merge commit 'refs/trial/main-now' into kai-s481-272
- Merge pull request #259 from nixpt/agent/vega/CRUSH-zorro-55-fastvm-jit-unassigned
- Merge pull request #275 from nixpt/agent/buffy/CRUSH-111
- docs(CRUSH-111): record the resolution, the two extra divergences, and the evidence
- feat(frontend): one builtin table shared by the checker and the compiler
- feat(aot): AotCompiler::cached_casm_path — ask whether a program is already compiled
- docs(tickets): point CRUSH-111's dispatch build dir at /build after clearing it
- docs(tickets): CRUSH-111 dispatch metadata + linted prompt pointer
- docs(tickets): mint CRUSH-271 for refused unary `+`/`~` from the Python/JS walkers
- feat(sdk): compile_lowered_script — a whole Python / JS script, exact or loud (CRUSH-272)
- docs(planning): fold the make_range entry-point drift into CRUSH-111
- docs(planning): CRUSH-272 inline lowering; file CRUSH-273 and CRUSH-274
- feat(crush-web): `polyglot: "lower"` run option and a `polyglot-python` feature (CRUSH-272)
- feat(sdk): lower @python / @javascript blocks into the program (PolyglotMode::Lower, CRUSH-272)
- build(crush-lang-python): build for wasm32 by taking crush-walker-core without tree-sitter
- docs(planning): correct CRUSH-35 table to runtime-verified rows (PR #263 review)
- docs(planning): close CRUSH-255 as superseded (crush-vm 0.6.0 is published) (#271)
- docs: correct README, examples/README and the CAST cookbook against 3ae0b4a (#264)
- build: dev container for GitHub Codespaces / Dev Containers (#262)
- feat(rust): share mapped method semantics across VM and AOT (CRUSH-xps-13)
- fix(crush-web): browser runs match crush-run: refuse before running, time/env grants, native guest errors (#270)
- feat(vm): embedder-supplied polyglot executor on the scheduler VM (CRUSH-271) (#269)
- perf(aot): Test (aot) 16 min to under 4.5 min: -O0 test builds, parallel harnesses, sharded CI, deterministic cached builds (#268)
- fix(vm): await on a non-handle returns the value, not null (#265)
- docs(tickets): close CRUSH-30 by scope ruling, mint CRUSH-30a residue ticket
- docs(quick-wins): CRUSH-11 repro re-verified, CRUSH-35 VISION.md table refreshed
- docs: CRUSH-zorro-55 resolution and decision record
- fix(aotc): crush-aotc fails a read of an unassigned local instead of reading 0 (CRUSH-zorro-55)
- fix(jit): JIT fails a read of an unassigned local, and a reused frame can't leak the last call's (CRUSH-zorro-55)
- fix(vm): FastVM fails a read of an unassigned local instead of reading null (CRUSH-zorro-55)
- refactor(casm): move the definite-assignment pass from crush-aot to casm
- feat(rust): shared typed Debug formatter for VM and Rust AOT (CRUSH-xps-9) (#252)
- fix(rust): typed literal Debug and macro formatting boundaries (CRUSH-xps-9) (#247)
- feat(rust): proven dynamic casts and bounded unsigned values (CRUSH-xps-8) (#246)
- fix(aot): refuse green tasks (spawn/yield/await) by name (#161) (#250)
- fix(aot): C backend compares maps and arrays by value, not identity (#249)
- fix(aot): fail a read of an unassigned local instead of reading null (CRUSH-zorro-54) (#248)
- feat(polyglot): explicit @lang block outputs (CRUSH-270) (#245)
- fix(rust): preserve literal integer casts and reject unproven conversions (#244)
- fix(js): refuse default parameters at walk time on the swc lowering (CRUSH-zorro-18) (#243)
- feat(js): default and rest parameters on function declarations (CRUSH-zorro-18) (#242)
- fix(python): detect free-variable reads inside comprehensions and nested scopes (#241)
- feat(js): `push` takes several values and returns the new length (CRUSH-250) (#240)
- docs(rust): spec and refine CRUSH-xps-8 through CRUSH-xps-13 (#237)
- feat(js): script variables captured by top-level functions; hoisted closures see later lets (CRUSH-250 step 4) (#236)
- feat(js): the builtin/method table; unbound calls and names fail at walk time (CRUSH-250 step 3) (#234)
- test(examples): fail loudly when an example stops compiling (#229)
- fix(frontend): pass method-call arguments to capabilities in order (CRUSH-235) (#238)
- fix(vm): member and Kind names are not capabilities (COSM K1.4 follow-up to #222) (#239)
- fix(rust): make flat value copies exact and reject unproven ownership (CRUSH-xps-7) (#233)
- fix(rust): preserve lexical binding identity (CRUSH-xps-6) (#224)
- test(rust): add SDK and AOT conformance gate (CRUSH-xps-5) (#220)
- feat(sdk): add walker-backed source compilation (#213)
- feat(js): the boa lowering with a scope pass, at parity on the harness (CRUSH-250 step 2) (#231)
- feat(js): boa_parser 0.22 and the js-boa / ts feature layout (CRUSH-250 step 1) (#228)
- feat: COSM K1.4 — Records, Kinds, member ops, CONSTRUCT and receivers on the VMs (CRUSH-zorro-43, closes CRUSH-zorro-37) (#222)
- planning: @lang blocks declare outputs explicitly, @python -> names (CRUSH-270, #230) (#232)
- planning: JS number model decided, option (a), Int promotes to Float on overflow (CRUSH-zorro-13) (#227)
- fix: fail loudly on declined AI statements instead of silent NOP (CRUSH-265) (#226)
- planning: close M1 correctness — CRUSH-1/7/13 done; file CRUSH-267/268 slicing follow-ups (#221)
- feat: COSM K1.3 — CASM and CVM1 carry the kind table; FastVM/JIT/AOT refuse it (CRUSH-zorro-42) (#219)
- docs(design): COSM — the Crush Object Semantic Model: objects lowered like code (Kinds; CRUSH-251) (#206)
- planning: Python .py gap tickets (CRUSH-zorro-28..36) (#201)
- planning: crush-ptx header bug (CRUSH-zorro-50) + gpu_map design (CRUSH-zorro-51) (#212)



- Rust mapped string and proven numeric methods now share pure kernels across
  VM, Rust AOT and native Linux C AOT. Integer pow/abs retain source widths and
  optimized-profile wrapping; f64 powi/powf are distinct. Prefix/suffix methods
  return Rust Bool without changing generic SDK predicate flags or math.pow's
  Float contract (CRUSH-xps-13).
- C AOT links a private Rust scalar/string support library with UTF-8 lengths,
  owned result/free pairs and panic containment. It refuses NUL/unproven input
  representations and generic string coercions without proof before building.
  Dynamic Debug's sequence bridge remains xps-9 work. Compiler/target/options
  and support source now participate in native artifact cache keys.
- AOT loader results preserve numeric-looking String values and non-finite
  Float kinds. Potential NUL root text results need a length-aware host ABI and
  are refused before generation; internal VM/Rust AOT NUL text remains supported.
- Async Rust functions are refused instead of eagerly running deferred Future
  bodies. Typed numeric method programs also refuse unproven arithmetic and
  wide integer ordering until exact source proofs exist.
- Non-finite constant float operations remain runtime operations, preserving
  infinity/NaN through the JSON-based compiler pipeline. Numeric method call
  arguments require canonical value proofs; f32, written/unknown/closure flows
  and unsupported unsigned result domains remain source-located refusals.

- Add the opt-in pure `rust.debug(value, descriptor)` stdcap with a single
  bounded kernel shared by SDK VM and Rust AOT. Validate descriptors and values,
  detect cyclic sequences, and enforce depth/item/output limits. C refuses
  dynamic Debug compilation until a shared bridge is available (CRUSH-xps-9).
- Rust Debug now supports proven immutable dynamic scalars and homogeneous
  sequences, retaining char/String/unit distinctions and owned formatter results.
  Unknown producers, mutable flows, unsupported control/call contracts and
  iterators remain explicit source refusals.
- Rust typed literal Debug now quotes/escapes strings and chars exactly, with
  bounded literal aggregate traversal. Dynamic Debug is refused pending the
  shared formatter (CRUSH-xps-9 slice 1).
- Rust formatting refuses potentially effectful repeated/reordered arguments,
  named argument materialization, and unsupported print/stderr routing. Empty
  and literal-only `format!` retain their owned String identity. Previously
  accepted dynamic Debug and newline-free/stderr macros now fail explicitly.
- Rust dynamic integer casts now use lexical numeric proofs and a shared CASM
  expansion with one operand evaluation. Proven lengths, bounded range indices,
  small u64 values and portable bounded usize conversions are supported;
  unproven producers are refused (CRUSH-xps-8 slice 2).
- Rust free-function calls evaluate arguments left to right and honor source
  functions that share builtin/helper names. Nested lowering errors retain one
  innermost source location.
- Rust casts of literal integers now preserve truncation and signedness for
  i8/i16/i32/i64/u8/u16/u32. Other casts fail with source locations pending
  numeric analysis; casts are no longer discarded. Signed minimum literals
  parse correctly, explicitly suffixed integer bounds are checked, and wide /
  pointer integer suffixes and f32 suffixes are rejected (CRUSH-xps-8 slice 1).

- Rust flat Vec/array clones now create independent sequences through the shared
  `seq.slice` operation. Fresh value arguments/reassignment and String clones
  with local `+=` have differential VM/AOT coverage (CRUSH-xps-7).
- Rust owned binding transfers, nested/unknown clones, escaping borrowed views,
  explicit references, owned captures, indexed writes, and unproven ownership
  adapters now fail with source locations. `pop`/`unwrap` are rejected pending
  Option semantics; valid Rust moves may require an explicit supported clone.

- Resolve Rust lexical locals to distinct storage names, preserving outer
  bindings across blocks, branches, loops, match arms, and closures. Binding
  metadata retains source identity; nested functions now fail explicitly until
  local item resolution is supported (CRUSH-xps-6).

- Add a shared Rust-source conformance audit for the SDK VM and both AOT
  backends, with rustc freshness checks, byte-exact output, ticket-linked
  exceptions, and source locations on Rust lowering errors (CRUSH-xps-5).

### Added

- `crush-lang-sdk::compile::compile_source(language_or_extension, source)`
  compiles enabled walker languages through the shared compiler into a CVM1
  program for `Runtime`. The default SDK features include `polyglot-rust`;
  `compile_rust_source` is a convenience wrapper over the same registry.

### Changed

- Rust move/async closures and unsupported closure parameter patterns now fail
  during lowering. Multi-statement or semicolon-ending blocks in value positions
  report their source location instead of dropping statements; function and
  closure bodies continue to lower their complete statement sequences.

### Fixed

- Walker assignment intrinsics and loop bindings pass semantic validation, and
  optimization preserves assignment targets and loop-carried values. Rust
  mapped methods use host-registered standard-library capabilities even when
  source bindings are named `array`, `str`, or `math`.

## [0.6.0] - 2026-10-15

- fix(publish): a crate with a versioned dev-dep waits on that crate's first publish (#218)
- fix(rust): typed closure parameters were dropped — a closure could capture the wrong x (silently) (#217)
- feat(frontend): Crush impl / trait / @kind(…) lowered to COSM (CRUSH-zorro-41) (#214)
- feat(cosm): crush-cosm crate — COSM schema, serde, validation, C3 (CRUSH-zorro-40) (#211)
- planning: CRUSH-zorro-3 — drop a reference to private zorro work (public crush-ast) (#216)
- planning: file Rust SDK correctness tickets (CRUSH-xps-10..12) (#215)
- docs: phase the Rust ↔ Crush pipeline (#204)
- feat(crush-pkg): add host-approved Rust plugins (#196)
- fix(python): keyword arguments are honoured or loud, never dropped (CRUSH-zorro-38) (#209)
- feat(vm,frontend): spawn a function value; parallel_map (CRUSH-251 step 5, #175) (#207)
- planning: COSM K1 tickets (CRUSH-zorro-40..44); zorro-40..49 reserved for COSM (#210)
- planning: CRUSH-zorro-37 (match ignores struct names), CRUSH-zorro-38 (Python drops kwargs) (#208)
- Merge pull request #205 from nixpt/agent/vega/CRUSH-251-step4b-js
- Merge remote-tracking branch 'origin/main' into agent/vega/CRUSH-251-step4b-js
- Merge pull request #203 from nixpt/agent/vega/CRUSH-251-step4a-python
- Merge remote-tracking branch 'origin/main' into agent/vega/CRUSH-251-step4a-python
- Merge pull request #202 from nixpt/agent/vega/CRUSH-251-step3-captures
- feat(js): let/const are block bindings; per-iteration for-let/of/in (CRUSH-251 step 4b)
- fix(python): comprehension loop variables get their own scope (CRUSH-251 step 4a)
- test(python): pin late binding of closures (CRUSH-251 step 4a)
- feat(python): lambdas, nested-def closures and nonlocal (CRUSH-251 step 4a)
- fix(frontend): a Crush `let` is a fresh binding each time it runs (CRUSH-251 step 3)
- test(js): update the parity harness lists after #197/#199/#194 (fixes CI walkers + aot) (#200)
- feat(vm,frontend): closures capture enclosing variables (CRUSH-251 step 3)
- planning: CRUSH-zorro-24 (call expressions on values), CRUSH-zorro-25 (crush-aotc silent op drop) (#195)
- test(js): update the parity harness lists after #197/#199/#194 (fixes CI walkers + aot)
- feat(vm,frontend): function values — MAKE_CLOSURE / CALL_VALUE (CRUSH-251 step 2) (#194)
- planning: CRUSH-251 closures design (slice 1) — agreed vega + foreman (#191)
- fix(js): JS runtime semantics — + ToString, && / || values, == coercion, console.log inspect (CRUSH-zorro-9, -22, -10) (#199)
- fix(js): make silently mis-lowered constructs fail loudly (CRUSH-zorro-23) (#197)
- test(js): JS/TS parity harness — feature corpus vs node, AOT exact-or-loud, TS twin (CRUSH-zorro-4) (#198)
- planning: JS/TS parity spike: baseline, parser decision (boa), 21 tickets (CRUSH-zorro-4..23, 26) (#193)
- Merge pull request #192 from nixpt/agent/vega/CRUSH-251-step1-unique-lambdas
- fix(frontend): inner bindings shadow outer lambda aliases (tombstones)
- fix(frontend): reassigning a lambda binding is a compile error until CRUSH-251 step 2
- fix(frontend): lift every lambda to a unique function; let bindings are scoped aliases (CRUSH-251 step 1)
- chore(crush-ptx): follow the workspace version; CRUSH-zorro-3 done (#190)
- feat(gpu): write a kernel in .crush and run it on the GPU (CRUSH-zorro-3) (#186)
- fix: JS / is true division; JS and Rust print numbers like node/rustc; AOT io.print prints every argument (#189)
- fix: task-local HALT; collections.* and native maps in deterministic insertion order (#188)
- feat: Rust and Crush interoperability MVP (CRUSH-266) (#187)
- Merge pull request #185 from nixpt/agent/vega/concurrency-decisions
- Merge remote-tracking branch 'origin/main' into agent/vega/concurrency-decisions
- Merge pull request #183 from nixpt/agent/vega/capsule-target-ticket
- Merge pull request #184 from nixpt/agent/vega/python-parity
- Merge pull request #182 from nixpt/agent/vega/portable-vm-spawn
- planning: record owner decisions on spawn HALT, #176 scope, #175 order, collections order
- test+planning: Python e2e parity corpus (VM exact, AOT exact-or-loud); decline CRUSH-47
- feat: Python repr printing; dict ops, ordered maps and string ops on AOT
- feat(vm): insertion-ordered maps (Value::Map is an IndexMap)
- feat(python): method calls on any value; fix array.pop leaving the array on the stack
- feat: `**` (math.power) and Python slicing (seq.slice) in every engine
- planning: file CRUSH-zorro-1 — crush AOT target for native Antarikshya capsules
- fix(portable-vm): real green tasks for spawn/yield/await, matching the scheduler (#161)
- Merge pull request #181 from nixpt/agent/vega/spawn-regression-tests
- test(crush-vm): regression tests for #177 — spawned-task output order and exit semantics
- Merge pull request #177 from pranix/bro/spawn-join-handles
- Merge pull request #173 from nixpt/agent/panini/CRUSH-202
- Merge remote-tracking branch 'origin/main' into agent/panini/CRUSH-202
- Merge pull request #180 from nixpt/agent/vega/js-walker-silent-bugs
- fix(walkers): JS switch/ternary/member-assign/console.log silently wrong; lazy conditionals in JS, Python, Rust (CRUSH-196)
- fix: address #177 review feedback
- Merge pull request #179 from nixpt/agent/vega/planning-rendered-views
- planning: archive the hand-written TASKS.md verbatim
- planning: TASKS.md/STATE.md become rendered views (jagent-ticket migrate)
- chore(planning): reviewed MILESTONES.caison (milestones/1) — MILE-16 (#178)
- fix: spawned tasks run to completion instead of being abandoned (#174)
- Merge remote-tracking branch 'origin/main' into agent/panini/CRUSH-202
- Merge pull request #172 from nixpt/agent/gorkhali/CRUSH-180
- Merge remote-tracking branch 'origin/main' into agent/panini/CRUSH-202
- Merge remote-tracking branch 'origin/main' into agent/gorkhali/CRUSH-180
- Merge pull request #171 from nixpt/agent/vega/planning-hygiene-stdlib
- Merge branch 'main' into agent/vega/planning-hygiene-stdlib
- docs(planning): CRUSH-202 result, decision, and captured follow-ups
- chore(bash-walker): CRUSH-202 drop a redundant cast (clippy)
- fix(walkers): CRUSH-202 bash/zsh lower correctly or fail loudly
- test(walkers): CRUSH-202 bash/zsh oracle corpus (red)
- docs(planning): CRUSH-180 result, decision, and a captured error-prefix issue
- fix(sdk): CRUSH-180 — fs sandbox: never follow a symlink out of --fs-root
- Fix debugger integration tests for invalid grant cases by avoiding stdin writes when the process exits during validation.
- planning: decide CRUSH-55 D-1/D-2 — close as ast ⊇ exo, retire exo runtimes
- planning: tick the TASKS issue about the duplicate CRUSH-108 ID
- planning: stdlib migration hygiene; file CRUSH-264/265
- Merge pull request #170 from nixpt/agent/vega/CRUSH-201-c-walker-loud
- fix(c-walker): fail loudly instead of silently wrong; printf, else, arrays, loops (CRUSH-201)



### Breaking changes

- `crush-vm::VmResult` now includes `return_value`; downstream Rust code that
  constructs this public struct with a full struct literal must provide the new
  field (or use `..Default::default()`). This changes the public Rust API and
  requires a major-version release under SemVer.

### Changed

- Rust walker inputs containing unsupported top-level items such as `use`
  declarations or structs now fail with diagnostics instead of compiling while
  silently ignoring those items.

## [0.5.0] - 2026-10-12

- Merge pull request #169 from nixpt/agent/vega/fix-main-dup-unparen
- fix(js-walker): drop duplicate unparen — main fails to compile after #164 + #167
- Merge pull request #167 from nixpt/agent/nixp/CRUSH-200
- Merge pull request #166 from nixpt/agent/vega/CRUSH-197-rust-walker
- Merge pull request #165 from nixpt/agent/vega/CRUSH-257-typed-locals
- Merge pull request #164 from nixpt/agent/vega/CRUSH-260-js-length-numbers
- fix(python): address CRUSH-200 review — overflow, map keys, range end
- Merge branch 'pr-168' into agent/nixp/CRUSH-200
- fix(js-walker): C-style for loops, do-while, comma sequences, multi-declarator var
- fix(rust-walker): println!/format! args, tail-expr returns, inclusive ranges; match/loop/vec! (CRUSH-197)
- fix(python): CRUSH-200 walker semantics + AOT vs CPython bench (CRUSH-262)
- feat(aot-c): sound CFG local-type inference; re-enable unboxed int64/double locals (CRUSH-257)
- planning: CRUSH-199 += array method calls, compile_cast vs walker helpers, JS number printing
- fix(js): `x.length` lowers to `len` on strings and arrays (CRUSH-260)
- Merge pull request #163 from nixpt/agent/vega/js-aot-compute-gap
- planning+bench: JS → AOT vs Node compute gap (CRUSH-257..261)
- Merge pull request #146 from pranix/bro/fix-inclusive-range
- Merge pull request #160 from nixpt/agent/vega/CRUSH-256-drop-ort
- Merge pull request #162 from nixpt/agent/vega/CRUSH-91-web-polyglot-grants
- Merge pull request #158 from nixpt/agent/vega/aot-value-text
- Merge pull request #159 from nixpt/agent/vega/CRUSH-256-gc-model-no-ort
- feat(crush-web): run @lang blocks through a host executor; capability grants (#91)
- build: drop ort (ONNX Runtime) from crush-vm and crush-lint (CRUSH-256)
- planning: CRUSH-256 — GC predictor without ONNX Runtime (generated rules / tract); ort out of crush-vm defaults
- fix(aot): arrays/objects as text match the VM; parity reports the outputs
- Merge pull request #157 from nixpt/agent/vega/parity-string-stress
- test(aot): drop stale string_stress KNOWN_DIVERGENCES (main is red)
- fix(frontend): repair test_lex_dotdoteq / test_lex_dotdot_still_works after the main merge
- Merge branch 'main' into bro/fix-inclusive-range
- Merge pull request #156 from nixpt/agent/vega/CRUSH-78-lambdas
- Merge pull request #155 from nixpt/agent/vega/CRUSH-137-struct-literals
- Merge pull request #154 from nixpt/agent/vega/CRUSH-148-aot-string-index
- Merge pull request #153 from nixpt/agent/vega/CRUSH-142-quotes-bytes
- Merge pull request #152 from nixpt/agent/vega/CRUSH-138-and-or-not
- Merge pull request #151 from nixpt/agent/vega/bozo-js-plan
- Merge pull request #150 from pranix/bro/fix-for-over-map-keys
- Merge pull request #149 from nixpt/agent/vega/CRUSH-143-match-arms
- Merge pull request #145 from pranix/bro/add-stress-tests
- Merge pull request #144 from pranix/bro/fix-hex-binary-literals
- test: move for-over-map e2e tests to crush-lang-sdk/tests
- test(vm): e2e tests for for-over-map iteration
- fix(frontend): lambdas `|x| => expr` parse and run; captures are compile errors (#78)
- test(frontend): parser + lexer tests for inclusive range
- test(frontend): lexer tests for hex/binary literals
- feat(frontend): struct literals `Name { field: expr }` (#137)
- fix(aot): string indexing s[i] returns the character, like the VM (#148)
- fix(frontend): single-quoted strings; clear error for b"..." bytes literals (#142)
- fix(frontend): annotation kebab names may contain and/or/not
- fix(frontend): `and` / `or` / `not` keywords (#138)
- fix(aot): map_keys codegen used RuntimeValue::Map; the AOT runtime variant is Object
- planning: CRUSH-255 — publish crush-vm 0.4.1 (shared HostCap trait for exo-light in surfer)
- planning: CRUSH-250..254 — browser JS on crush (Boa parser default, object model, real dom_native, python-dom, job queue)
- feat: map_keys opcode for for-over-map iteration (all backends)
- wip: map_keys opcode for for-over-map (CVM1, PortableVM, FastVM)
- test(aot): mark string_stress as known divergence (CRUSH-217)
- examples: add language stress tests (strings, arrays, maps, control flow, arithmetic)
- fix: match arms — void call underflow (#143) and lost assignments under the optimizer (#147)
- fix(frontend): support inclusive range 1..=5
- fix(frontend): support hex (0x) and binary (0b) integer literals
- Merge pull request #136 from nixpt/ccr-12236bee-uj4oar
- chore: ignore crush-web's own target dir
- feat(crush-web): browser runs get the stdlib, sys.args and sys.exit (CRUSH-244)
- Merge pull request #134 from nixpt/ccr-12236bee-uj4oar
- fix(sdk): net host caps handle HostCapError::Exit (all-features build)
- test(scripting): pin the bash side to LC_ALL=C so sort order doesn't depend on the host locale
- Merge remote-tracking branch 'origin/main' into agent/foreman/pr134-fix
- chore: renumber declared-capabilities ticket CRUSH-232 -> CRUSH-243 (CRUSH-232 landed as the casm_to_vm lowering ticket)
- Merge pull request #135 from nixpt/agent/panini/CRUSH-232
- feat(sdk): casm_to_vm lowers 33 more CASM ops; completeness test over every OpCode (CRUSH-232)
- feat(scripting): sys.args, sys.exit, streamed output; void host caps return null (CRUSH-183)
- Merge origin/main into ccr-12236bee-uj4oar
- test(aot): examples_parity skips interactive examples instead of reading its own stdin
- Merge pull request #133 from nixpt/agent/foreman/examples-stdin
- docs(agents): never run io.read examples with stdin open — blackjack_interactive hangs the session
- Merge pull request #132 from nixpt/agent/naka/CRUSH-231
- feat: programs declare capabilities; crush-run checks grants before running (CRUSH-232)
- Merge remote-tracking branch 'origin/main' into agent/naka/CRUSH-231
- ci: run every crate's tests; drop the dead 1.85 pin; add an MSRV job (CRUSH-231)
- Merge pull request #131 from nixpt/agent/panini/CRUSH-224-226
- docs(dejavue): record CRUSH-226 and CRUSH-224 decisions
- fix(polyglot): carry use @lang imports into later blocks (CRUSH-224)
- chore(planning): file CRUSH-232..238 from the agents + polyglot use-case review
- fix(vm): EXEC_LANG enforces declared caps and allowed_caps (CRUSH-226)
- Merge pull request #129 from nixpt/agent/foreman/CRUSH-231
- Merge pull request #128 from nixpt/ccr-12236bee-uj4oar
- chore(planning): CRUSH-231 — note PR #128 covers crush-aot
- Merge origin/main into ccr-12236bee-uj4oar (keep one CRUSH-227 row, checked)
- chore(planning): file CRUSH-231 (CI toolchain pin dead, 24 crates never run tests)
- Merge pull request #127 from nixpt/agent/foreman/CRUSH-224-226
- docs: close CRUSH-227; record the C collector decision and its invariant
- feat(aot): C runtime frees memory during a run; CI runs the AOT tests (CRUSH-227)
- Merge pull request #126 from nixpt/ccr-12236bee-uj4oar
- chore(planning): file CRUSH-224..226 (polyglot imports, session state, EXEC_LANG cap check)
- chore(planning): file CRUSH-227..230 from the PR #126 review
- fix(aot): address PR #126 review — mat_mul bounds, long string results, name mangling, C overflow
- test(aot): every example the VM runs must print the same under AOT (CRUSH-220)
- fix(aot): C backend stack, arrays and objects grow instead of dropping values (CRUSH-223)
- fix(aot): C backend strings, io.print stack contract, float text (CRUSH-216)
- fix(aot): Rust backend compiles int / and %, and keyword function names (CRUSH-214)
- chore(planning): file CRUSH-222..223 from the AOT test drive; add evidence to 214/216/217/220



- **`crush-web`: the browser gets the standard library, `sys.args` and
  `sys.exit` (CRUSH-244).** Every entry point now registers what `crush-run`
  gives a program with no grant flags: `math.*`, `str.*`, `system.*` and the
  rest of the stdlib, `sys.args` (answering a new `args` option), `sys.exit`
  and `caison.parse`. Before, the browser had no capabilities at all, so
  `examples/crush/math_test.crush` and `test_sbl.crush` failed there while
  passing natively. `sys.exit(n)` is reported as `exit_code` with earlier
  output kept (a `Session` ends `done`; `execute_with` sets `ok: false` for a
  non-zero code). Nothing outside the VM (`fs`, `time`, `env`, `process`,
  `net`) is registered. The stdlib roughly doubles the module (~0.5 → ~0.95 MB
  gzipped); the new default-on `stdlib` feature leaves it out.
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

### Added — scripting basics
- `sys.args()` returns the program's arguments: `crush run script.crush a "b c"`
  (flags may follow; `--` passes an argument starting with `-`). Always registered.
- `sys.exit(code)` ends the program with exit status 0–255; output printed before it
  is kept.
- `crush-run` writes output as the program prints it instead of all at the end, and
  output printed before a runtime error is no longer lost (`Runtime::run_streaming`,
  `crush_vm::vm::run_with_caps_streaming`).
- `tests/scripting_parity.rs`: 13 everyday shell jobs in bash and Crush, compared.

### Fixed
- `fs.write`, `akg.write`, `message_bus.publish`/`subscribe` and `task.stop` no longer
  end every program with `stack underflow`; they return null (CRUSH-183).

### Added — declared capabilities (CRUSH-243)
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
