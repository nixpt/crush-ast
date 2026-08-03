# CRUSHAST-BUCKETSPIKE-2 — results

Throwaway spike, not production work. Branch `agent/cece/CRUSHAST-BUCKETSPIKE-2`,
off `agent/cece/CRUSHAST-BUCKETSPIKE-1` (which itself branches off
`agent/cece/CRUSHAST-POLYGLOT-1`). Extends BUCKETSPIKE-1's proven Python
result to the other two languages `crush-vm::scheduler::resolve_lang_binary`
maps to a host subprocess: `node -e` (for `@javascript`) and `bash -c` (for
`@bash`).

**Branch-base note:** the task brief said to branch off the current HEAD of
`agent/cece/CRUSHAST-POLYGLOT-1`, describing it as already including
BUCKETSPIKE-1's work. In this worktree that isn't quite true — `POLYGLOT-1`
HEAD is `09ea363`, and `BUCKETSPIKE-1` is `09ea363` + one commit
(`16bc76a`, the spike itself). Branched off `BUCKETSPIKE-1` directly instead,
since that's the branch that actually has `crates/crush-bucketspike` to
extend. Same effective base either way.

## Question

Does `buckets::sandbox::sandboxed_command` + piped stdio work the same way
for `node -e <code>` and `bash -c <code>` as it did for `python3 -c <code>`
in BUCKETSPIKE-1 — ordinary output and a NUL-sentinel-bearing line both
surviving intact through bwrap + `Command` capture — and what does cold/warm
provisioning cost for each? (NOT: does node/bash have a real marshaling
protocol like Python's — they don't, and building one is explicitly out of
scope; see `crates/crush-lang-sdk/README.md`'s "Polyglot blocks" section:
"Only Python has this.")

## Answer: yes for both, mechanism generalizes cleanly

- `bwrap` **was** actually exercised for every run (python/node/bash × cold/warm,
  two independent full binary runs = 12 sandboxed executions total) — verified
  the same way as BUCKETSPIKE-1: `bwrap on PATH: true` printed, and the absence
  of `buckets::sandbox`'s fallback stderr warning ("bwrap not found on PATH —
  running WITHOUT sandbox isolation"), which would have printed on every single
  run if it had fallen through. It didn't print once, in either full run.
- `bwrap` is present on this box at `/usr/bin/bwrap` (same as BUCKETSPIKE-1).
- Both `node -e` and `bash -c` produced the same raw byte sequence shape as
  python did: ordinary `console.log`/`echo` output on its own line, followed
  by a line starting with a literal NUL byte, the ASCII text `CRUSH_RESULT`,
  another literal NUL byte, then a JSON-encoded payload (`6`) — piped through
  bwrap's mount namespace and captured by `wait_with_output()` without
  truncation, reordering, or corruption. Both PASS on both independent runs.
- Bash's env-var passthrough bonus also worked: the sandboxed bash process
  read `$base` (a real env var, `base=5`, injected the same way the real
  EXEC_LANG handler injects marshaled inputs via `cmd.env(name, val.as_text())`)
  directly with no decode step, computed `base + 1`, and that computed value
  round-tripped through the sentinel line — proving env-var passthrough itself
  survives bwrap, which any future bash marshaling protocol would depend on.
- Real cold vs warm provisioning latency numbers below, for node and bash
  separately, plus python re-measured on this box for a same-session,
  same-conditions comparison baseline (own numbers, not assumed to match
  BUCKETSPIKE-1's original recorded run).

## A bug this spike found in its own harness (worth flagging)

The first draft of this harness built one shared `buckets::config::Config`
via `Config::new()` at the top of `main()`, then called
`std::env::set_var("BUCKETS_CACHE_DIR", ...)` per-language before each
resolve, expecting that to redirect where `resolve_multi` looked. It didn't:
`Config::new()` reads `BUCKETS_CACHE_DIR` **once**, at construction, into a
struct field (`buckets/src/config.rs`) — calling `set_var` afterward is a
silent no-op against an already-built `Config`. The result: all three
languages were quietly sharing the box's real default `~/.buckets` cache
(which already had leftover state from BUCKETSPIKE-1's own prior runs), not
the fresh isolated per-language tmp dirs this spike intended — so the first
run's "cold" python number came back in 89ms total wall (a warm cache hit
wearing a cold label), which didn't match BUCKETSPIKE-1's documented
~4.4–6.1s and was the tell that something was wrong.

Fix: construct `Config::new()` **inside** `resolve_and_run`, immediately
after the `set_var` call, so every cold/warm pair gets a `Config` that
actually reflects the just-set cache dir. After the fix, python's cold
number on this same box came back in the 4.2–6.6s range across two runs —
consistent with BUCKETSPIKE-1's original recording, confirming the fix
(and that nothing about the box itself had changed). All numbers below are
post-fix, real numbers.

## Setup

- Same crate as BUCKETSPIKE-1: `crates/crush-bucketspike` (binary
  `bucketspike`), still its own `[workspace]` root, still the same
  absolute-path `buckets` dep, still not a member of crush-ast's root
  workspace — same reasons, unchanged, see `SPIKE_RESULTS.md`.
- Same build gotcha, same workaround: crush-ast's `.cargo/config.toml` sets
  `CFLAGS`/`LDFLAGS=-flto`, which breaks `buckets`' `ureq`/`ring` TLS
  linking. Built with `CFLAGS= LDFLAGS= RUSTC_WRAPPER= cargo build`.
- `main.rs` was extended (not replaced): BUCKETSPIKE-1's python-specific
  `resolve_and_run`/scanning logic was generalized into a `run_language`
  harness parameterized over buckets spec / program / exec-flag / source
  code / extra env vars, reused for python (regression re-run, same
  source/logic as BUCKETSPIKE-1) and the two new node/bash runs. The
  stdout-scanning logic (`scan_stdout`) is untouched, verbatim, shared
  across all three — proving the SAME mechanism generalizes, not
  reimplementing per-language logic.
- Buckets index resolution: `node` → `nodejs.org` alias, with **two
  automatic companion packages** (`openssl.org@^1.1`, `unicode.org@^73` —
  node dynamically links against them; see `buckets/src/index.rs`'s
  `companions` map and its own comment on why openssl 1.1 specifically,
  not latest 3.x). `bash` → `gnu.org/bash` alias, no companions. This is
  exactly the "different bottles, different sizes" the task brief warned
  about: node's cold run installs **3 packages**, bash's installs **1**,
  python's installs **1** (but a bigger one — CPython's bottle is the
  biggest of the three).
- Cache isolation: fresh, per-language `BUCKETS_CACHE_DIR` (not shared
  between python/node/bash, and not reused across the two independent
  full-binary runs below — `rm -rf /tmp/bucketspike2-cache-*` between runs).

## Real run output (two independent full-binary invocations)

Command each time:
```
cd crates/crush-bucketspike
rm -rf /tmp/bucketspike2-cache-*
CARGO_TARGET_DIR=<scratch> RUSTC_WRAPPER= CFLAGS= LDFLAGS= cargo build
<scratch>/debug/bucketspike
```

### Run 1 — node section (real captured stdout)

```
=== node ===
fresh BUCKETS_CACHE_DIR: /tmp/bucketspike2-cache-node-587780
--- COLD run (fresh cache, first resolve+install+sandbox+exec) ---
✓ resolved nodejs.org@^20 → v20.20.2
✓ resolved openssl.org@^1.1 → v1.1.1+w
✓ resolved unicode.org@^73 → v73.2.0
↓ installing 3 package(s)...
↓ fetching https://dist.pkgx.dev/openssl.org/linux/x86-64/v1.1.1w.tar.xz
↓ fetching https://dist.pkgx.dev/unicode.org/linux/x86-64/v73.2.0.tar.xz
↓ fetching https://dist.pkgx.dev/nodejs.org/linux/x86-64/v20.20.2.tar.xz
✓ cached openssl.org v1.1.1w
✓ cached nodejs.org v20.20.2
✓ cached unicode.org v73.2.0
cold: success=true resolve_duration=2.343602286s total_wall=2.384270001s
cold stdout (raw, with embedded NUL sentinel bytes shown as \0):
"hello from sandboxed node\n\0CRUSH_RESULT\06\n"
cold visible output: "hello from sandboxed node"
cold decoded sentinel Value: Int(6)
--- WARM run (same cache dir, second resolve = cache hit) ---
✓ resolved nodejs.org@^20 → v20.20.2
✓ resolved openssl.org@^1.1 → v1.1.1+w
✓ resolved unicode.org@^73 → v73.2.0
warm: success=true resolve_duration=16.245999ms total_wall=71.694462ms
warm stdout (raw):
"hello from sandboxed node\n\0CRUSH_RESULT\06\n"
warm visible output: "hello from sandboxed node"
warm decoded sentinel Value: Int(6)
[node] cold resolve_duration: 2.343602286s  (total incl. sandbox exec: 2.384270001s)
[node] warm resolve_duration: 16.245999ms  (total incl. sandbox exec: 71.694462ms)
[node] proof: PASS
```

### Run 1 — bash section (real captured stdout)

```
=== bash ===
fresh BUCKETS_CACHE_DIR: /tmp/bucketspike2-cache-bash-587780
--- COLD run (fresh cache, first resolve+install+sandbox+exec) ---
✓ resolved gnu.org/bash@^5 → v5.3.0
↓ installing 1 package(s)...
↓ fetching https://dist.pkgx.dev/gnu.org/bash/linux/x86-64/v5.3.0.tar.xz
✓ cached gnu.org/bash v5.3.0
cold: success=true resolve_duration=375.55034ms total_wall=382.078046ms
cold stdout (raw, with embedded NUL sentinel bytes shown as \0):
"hello from sandboxed bash\n\0CRUSH_RESULT\06\n"
cold visible output: "hello from sandboxed bash"
cold decoded sentinel Value: Int(6)
--- WARM run (same cache dir, second resolve = cache hit) ---
✓ resolved gnu.org/bash@^5 → v5.3.0
warm: success=true resolve_duration=7.175063ms total_wall=13.100482ms
warm stdout (raw):
"hello from sandboxed bash\n\0CRUSH_RESULT\06\n"
warm visible output: "hello from sandboxed bash"
warm decoded sentinel Value: Int(6)
[bash] cold resolve_duration: 375.55034ms  (total incl. sandbox exec: 382.078046ms)
[bash] warm resolve_duration: 7.175063ms  (total incl. sandbox exec: 13.100482ms)
[bash] proof: PASS
```

### Run 2 (fresh cache dirs again, independent process) — node + bash

```
=== node ===
fresh BUCKETS_CACHE_DIR: /tmp/bucketspike2-cache-node-587879
cold: success=true resolve_duration=2.082814247s total_wall=2.126062279s
cold stdout (raw): "hello from sandboxed node\n\0CRUSH_RESULT\06\n"
cold visible output: "hello from sandboxed node"
cold decoded sentinel Value: Int(6)
warm: success=true resolve_duration=15.671522ms total_wall=56.385099ms
warm stdout (raw): "hello from sandboxed node\n\0CRUSH_RESULT\06\n"
warm visible output: "hello from sandboxed node"
warm decoded sentinel Value: Int(6)
[node] proof: PASS

=== bash ===
fresh BUCKETS_CACHE_DIR: /tmp/bucketspike2-cache-bash-587879
cold: success=true resolve_duration=349.538934ms total_wall=355.988645ms
cold stdout (raw): "hello from sandboxed bash\n\0CRUSH_RESULT\06\n"
cold visible output: "hello from sandboxed bash"
cold decoded sentinel Value: Int(6)
warm: success=true resolve_duration=6.369974ms total_wall=13.998162ms
warm stdout (raw): "hello from sandboxed bash\n\0CRUSH_RESULT\06\n"
warm visible output: "hello from sandboxed bash"
warm decoded sentinel Value: Int(6)
[bash] proof: PASS
```

### Both runs — python (regression re-run, same box/session)

```
Run 1: cold resolve_duration=6.629391521s total_wall=6.669308731s | warm resolve_duration=7.067976ms total_wall=45.084056ms | PASS
Run 2: cold resolve_duration=4.176381223s total_wall=4.211200605s | warm resolve_duration=7.155644ms total_wall=48.374622ms | PASS
```

(Note: `✓`/`↓` glyphs are `buckets`' own `eprintln!` output, UTF-8 rendered
as replacement bytes in the raw terminal capture, same as BUCKETSPIKE-1 —
reproduced as intended glyphs here for readability. Numeric/stdout lines are
copied verbatim.)

## Latency summary — node vs bash vs python (this box, same session, post-fix)

| language | packages installed (cold) | cold (fresh cache) run 1 | cold run 2 | warm (cache hit) run 1 | warm run 2 |
|---|---|---|---|---|---|
| python | 1 (python.org, biggest bottle) | 6.67 s | 4.21 s | 45.1 ms | 48.4 ms |
| node | 3 (nodejs.org + openssl.org + unicode.org) | 2.38 s | 2.13 s | 71.7 ms | 56.4 ms |
| bash | 1 (gnu.org/bash, smallest bottle) | 382 ms | 356 ms | 13.1 ms | 14.0 ms |

**They do NOT match Python's numbers, as the task brief predicted** — and
not in a single consistent direction:

- **Bash is much faster cold** (~350–380 ms) than python (~4.2–6.7 s) despite
  bash being one of the two languages "already" available on most hosts —
  buckets' `gnu.org/bash` bottle is simply much smaller/quicker to fetch
  than python's.
- **Node is faster cold than python despite installing 3 packages**, not 1 —
  counter to a naive "more packages = slower" assumption. Node's combined
  3-bottle cold cost (~2.1–2.4 s) is still under half of python's single-bottle
  cost, meaning python's bottle by itself is larger/slower than node's three
  bottles combined on this box's network path to `dist.pkgx.dev` at the time
  of these runs.
- **Warm numbers are all in the same rough order of magnitude** across all
  three (6–72 ms), dominated by bwrap spawn + interpreter startup rather than
  buckets' own resolve step (which is consistently single-digit ms once
  cached, for all three languages) — this part of BUCKETSPIKE-1's finding
  ("warm latency dominated by sandbox spawn, not buckets") generalizes
  cleanly. Node's warm total_wall (56–72 ms) runs a bit higher than
  python/bash's (13–48 ms), plausibly V8 startup cost being heavier than
  CPython's or bash's, though this spike didn't isolate that specifically
  (out of scope — the point was proving the mechanism, not profiling per-
  interpreter startup cost).

## Mechanism proof (both languages, both runs)

Raw captured stdout, byte for byte, for both node and bash:
```
node: "hello from sandboxed node\n\0CRUSH_RESULT\06\n"
bash: "hello from sandboxed bash\n\0CRUSH_RESULT\06\n"
```

Split via the exact same `scan_stdout` logic BUCKETSPIKE-1 used (shared,
unmodified, across python/node/bash in this spike) — line-split, pull the
`CRUSH_RESULT_SENTINEL`-prefixed line out, everything else is "visible"
output:

- node visible output: `"hello from sandboxed node"` — `console.log`'s own
  output, correctly separated from the sentinel line.
- node sentinel payload: `"6"` — JSON-decoded via `crush_vm::vm::Value`'s
  real `Deserialize` impl into `Value::Int(6)`. Not marshaling — this value
  is a JS literal (`JSON.stringify(6)`) hardcoded in the test source, not
  derived from any Crush variable; it exists purely to prove a NUL-bearing
  sentinel-shaped line survives the same pipe/bwrap/capture path python's
  real marshaling epilogue depends on.
- bash visible output: `"hello from sandboxed bash"` — `echo`'s own output.
- bash sentinel payload: `"6"` — this one **is** computed inside the
  sandbox from a real env var (`base=5`, injected via `cmd.env`, read as
  `$base` with zero decode step, per bash's native env-var exposure) via
  `result=$((base + 1))`, then `printf`'d with the sentinel prefix. Proves
  env-var passthrough end-to-end through bwrap, not just a hardcoded literal.

Both come through correctly, sandboxed, with the mechanism identical to
BUCKETSPIKE-1's already-documented python case.

## What this spike does NOT prove (explicitly out of scope, per task brief)

- No real marshaling protocol for `@javascript` or `@bash` — no free-variable
  analysis, no JSON-decode-from-env-var prologue, no "last-bound-name"
  epilogue generation. Node's sentinel line is a hardcoded literal in the
  test source, not derived from any actual Crush variable. Bash's sentinel
  line uses real env-var passthrough but still isn't a generated marshaling
  epilogue — building either is separate, much bigger, unscoped future work,
  same as `crates/crush-lang-sdk/README.md` already documents.
- Not wired into the real `EXEC_LANG` opcode handler
  (`crates/crush-vm/src/scheduler.rs`) — untouched, same as BUCKETSPIKE-1.
- No deno/bun attempted — `resolve_lang_binary` doesn't route to them today,
  out of scope per task brief.
- Node/openssl/unicode and bash bottles are bare buckets-resolved binaries,
  no additional runtime configuration (npm registry, locale data, etc.)
  beyond what buckets' own companion-package wiring provides.

## Files

- `crates/crush-bucketspike/src/main.rs` — extended in place: BUCKETSPIKE-1's
  python-specific resolve/sandbox/scan logic generalized into a
  `run_language` harness, reused for python (regression re-run) + node +
  bash. `Cargo.toml` unchanged from BUCKETSPIKE-1 (same deps already cover
  this — no new crates needed for node/bash).
- This file. `SPIKE_RESULTS.md` (BUCKETSPIKE-1, unchanged) holds the
  original python-only numbers this addendum compares against.

## Explicitly not done (per task scope)

- `crates/crush-vm/src/scheduler.rs`'s real `EXEC_LANG` handler: untouched.
- `crates/crush-pkg`: untouched.
- No marshaling protocol built for node or bash.
- No deno/bun.
- `Cargo.toml` `[package]` version lines: untouched.
