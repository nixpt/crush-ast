# CRUSH-122: stdlib convergence — finish absorbing exosphere's `core/base/stdlib` (W10)

**Status**: steps 2–4 landed 2026-09-25 (branch
`claude/stdlib-nanovm-crush-ast-port-477gv7`); step 1 (CRUSH-113) and step 5
(atlas / playbook note, outside this repo) remain — captured 2026-09-24
(captain), from the [main] foreman's nanovm / extraction-playbook review
**Priority**: medium (blocks exosphere's archive gate, W18)
**Relates to**: M9 "STDLIB restoration" (proposed), CRUSH-113 (`stdlib` feature
off by default), exosphere extraction playbook **W10** (exosphere#65).

## Correction first

The workspace atlas (§ exosphere reuse map) says exosphere's stdlib has **no**
crush-ast counterpart. That is wrong: `crates/crush-lang-sdk/src/stdlib.rs`
(2,523 lines, "pure computation capabilities ported from exosphere's stdlib")
exists behind the `stdlib` feature — **off by default** (CRUSH-113). The real
gap is: **partially ported, off by default, the rest planned (M9) but not
scheduled.**

## Inventory (2026-09-24; exosphere `crates/core/base/stdlib/src`, 9,436 Rust lines)

**Ported** (capability families registered in `crush-lang-sdk` stdlib):
`math` (21), `str` (16), `json` (11), `path` (7), `conv` (7), `collections`
(7), `regex` (5) — the pure-computation core.

**Not ported:**

| family | lines | likely home |
|---|---:|---|
| `text`, `result`, `buffer`, `bytes`, `binary`, `time_cap` | ~1.5k | **stdlib** (pure / near-pure) |
| `async_cap`, `task` | ~0.15k | stdlib or crush-vm scheduler — check against `crush-vm::scheduler` |
| `fs`, `storage`, `env`, `http` | ~1.1k | **not stdlib** — host capabilities behind osmosis `HostCapability` (grant-gated: `fs.*`, `store.*`, `net.*`) |
| `dom`, `gfx` | ~0.9k | host capabilities of a UI host (surfer/arniko), not stdlib |
| `ai_capabilities` | 1.5k | host capability (ai-core), not stdlib |
| `polyglot_bridge` | 0.7k | `crush-vm::polyglot` — diff first |
| `ics` | 0.7k | decide: stdlib module or drop |
| `missing_capabilities`, `implementation_plan`, `helpers`, `create_std_registry` | ~0.9k | scaffolding — read, then drop |
| `sbl_core.{casm,crush}` (nanovm) | 0.3k | the "System Bytecode Layer / root of trust": stdlib bootstrap + nakshatra measured boot framing |

## Steps

- [ ] 1. Decide CRUSH-113 (default-on, or a clear error when `--stdlib` is off).
- [x] 2. Port the pure/near-pure families above into `crush-lang-sdk`'s stdlib,
  each with an M5 `@covers` test (M9's rule: no mock markers).
- [x] 3. For each host-capability family, record its home (osmosis
  `HostCapability` + grant name) — do not put I/O in the stdlib.
- [x] 4. Diff `polyglot_bridge` against `crush-vm::polyglot`; port or drop.
- [ ] 5. Fix the atlas row ("no counterpart" → "partial; CRUSH-122") and tell the
  playbook owner (vega) that W10's done-condition is this ticket.

## Landed (2026-09-25)

Source: exosphere `06b68057`, `crates/core/base/stdlib/src` +
`crates/core/vm/nanovm/src/sbl_core.{crush,casm}`. Every family below has
unit tests beside it and a source-pipeline test (Crush source → compile →
run) in `crates/crush-lang-sdk/tests/stdlib_ported_families.rs`.

**Ported into the stdlib** (`--stdlib`, `crates/crush-lang-sdk/src/stdlib/`):

| family | caps | notes |
|---|---|---|
| `collections` (rest) | `keys values entries merge pluck sort_by find any all` | keys/values/entries sorted by key (nanovm used HashMap order — nondeterministic) |
| `bytes`, `buffer` | `bytes.len/slice/from_string/to_string`, `buffer.alloc/write/read/freeze` | buffer = shared `Value::Vector` of byte ints, so `buffer.write` mutates in place like a nanovm `Buffer` ref |
| `binary` | `binary.{read,write}_u{16,32,64}_{le,be}` | nanovm registered them as `binary.ReadU16Le` (`stringify!` bug); u16/u32 writes reject out-of-range values instead of truncating |
| `result` | `ok err is_ok unwrap` | a result is the map `{ok, value}` (CVM1 has no Result object) |
| `text` (pure half) | `text.sort`, `text.uniq` | `text.echo` dropped (= `io.print`) |
| `time` (pure half) | `time.format`, `time.parse` | epoch **ms**, chrono strftime; invalid formats error instead of panicking |
| `env` (pure half) | `env.os`, `env.arch` | compile-time constants, no environment read |
| **SBL** (nanovm `sbl_core`) | `system.path_normalize`, `system.format_node_info`, `system.format_info` | `crates/crush-lang-sdk/sbl/sbl_core.crush` is the implementation: compiled once, each call runs in a fresh quota-bounded VM with only the pure stdlib. nanovm's `.casm` was a stub (`path_normalize` returned its input) and was never wired up |

**Ported as grant-gated host capabilities** (not stdlib — they do I/O):

| family | caps | gate |
|---|---|---|
| `text` (file half) | `text.head tail wc cut grep` | `--fs`, sandboxed by the same `resolve_path` as `fs.*`; `grep` also needs the `stdlib` feature (regex) |
| `time` (clock half) | `time.now_ms now_iso elapsed sleep` | `--time`; `time.now` keeps returning seconds; `time.sleep` honours `max_wall_time_ms` (CapTimeout) |

**Homes recorded, not ported here** (step 3): `fs.*` beyond
`read/write/exists/list` (`ls cd pwd mkdir rm cat touch cp mv find`),
`storage.*`, `http.*`/`net.*` → osmosis `HostCapability` behind `fs.*` /
`store.*` / `net.*` grants (crush-lang-sdk's `net` feature already has
`net.http_get/post`); `dom.*`, `gfx.*` → UI host (surfer / arniko);
`ai_capabilities`, `agent.*`, `learn.*` → ai-core host capability.

**Declined**: `polyglot_bridge` (`polyglot.lib/call/transfer`,
`python.stdlib`, `js.stdlib`) — `execute_polyglot_function` returns
hard-coded mock values; crush-ast's real `@python{}` / `@javascript{}`
EXEC_LANG path supersedes it. `async.sleep` — same synchronous sleep as
`time.sleep`. `task.restart` / `task.watchdog` — stubs that always error.
`ics` (IC records) — nanovm identity layer; `HostCapSpec` plays that role
here. `missing_capabilities`, `implementation_plan`, `helpers`,
`create_std_registry` — scaffolding.

**Language-side fixes found by the port**:
- `array.push(a, x)` / `array.pop(a)` compiled to an unknown
  `cap_call "array.push"` (the parser emits dotted calls as
  `CapabilityCall`; the intrinsics were only in the `Call` branch). Now lowered
  to ARR_PUSH / ARR_POP (`crush-frontend/tests/array_intrinsics.rs`).
- `fs.*` sandbox escape: for a path that did not exist yet, `resolve_path`
  fell back to the un-canonicalized join, and `Path::starts_with` is
  component-wise, so with an absolute `--fs-root` `fs.write("../x", ..)`
  wrote outside the root. Now resolved lexically first, then through the
  deepest existing ancestor (symlinks included); tests in `host_caps.rs`.
- crush-vm: `PortableVm::push_entry_args` (call one function of a compiled
  library directly) and `Value::type_name` made public.

Corpus: `examples/crush/{sbl_core_nanovm,test_sbl,text_tools_test}.crush` went
from expected-failure to verified output; the conformance runner gained
`// caps: stdlib, fs` and file selection (`conformance -- <files>`).

**Still open**: step 1 (CRUSH-113 — stdlib stays off by default) and step 5
(the atlas row + playbook W10 note live outside this repo).
