# CRUSHSDK-1: crush-lang-sdk P0 — `--all-features` Value::Bool deref-fix in db + stdlib

**Status**: open
**Branch**: `agent/buffy/CRUSHSDK-1` — created off `ac1ca0f` (= PR #4's tip /
`agent/buffy/CRUSHVM-1-PORTABLE-PARITY`'s head, merged / open via PR #4).
**Priority**: high (the only 🔴 P0 left in `TASKS.md` after CRUSHVM-1
landed; blast radius is the workspace's `--all-features` CI lane).
**Goal**: close the `--all-features` build break that surfaces on the
`db` and `stdlib` feature arms of `crush-lang-sdk` after the s298 VM
type expansion added `Value::Bool(bool)` (and `Map` / `Error` / `Bytes`)
variants.

## Identity

- Crate/feature: `crush-ast/crates/crush-lang-sdk` + features `db` and
  `stdlib` (each is a single-feature gate, declared in `Cargo.toml`
  as `[features] db = ["dep:rusqlite"]` and `stdlib = ["dep:regex"]`).
- Files in scope:
  - `crates/crush-lang-sdk/src/db.rs` — 2 sites (lines 29, 52); only
    built when the `db` feature is on.
  - `crates/crush-lang-sdk/src/stdlib.rs` — 1 site (line 658); only
    built when the `stdlib` feature is on.
  - `crates/crush-lang-sdk/Cargo.toml` — sanity: features still wire
    up after the patch (no edit).
- Symptom (verified today on `ac1ca0f`, fresh `cargo check`): 3
  `error[E0308]` mismatched-type sites, all `Value::Bool(b) => ...
  b: &bool` (borrow-mode binds inside a `match &Value { ... }`) where
  the consuming API expects an owned `bool`.

## Goal

Restore the following so `cargo check --workspace --all-features`
exits 0:

```bash
cargo check -p crush-lang-sdk --features=db,stdlib        # currently 101
cargo check --workspace --all-features                    # currently 101
cargo test  -p crush-lang-sdk --features=db,stdlib        # green after
cargo test  --workspace --all-features                    # green after
```

Default-feature build was already green at PR #4 (the CRUSHVM-1
parity test suite locks `Value::Bool` roundtrip identity); this
ticket only covers the feature-gated arms that fall out.

## Scope

- **3 sites, 3-line fix.** Prepend `*` to `b` in each:
  - `crates/crush-lang-sdk/src/db.rs:29` —
    `serde_json::Value::Bool(b)` → `serde_json::Value::Bool(*b)`
  - `crates/crush-lang-sdk/src/db.rs:52` —
    `if b { 1 } else { 0 }` → `if *b { 1 } else { 0 }`
  - `crates/crush-lang-sdk/src/stdlib.rs:658` —
    `serde_json::Value::Bool(b)` → `serde_json::Value::Bool(*b)`
- **Adjacent site audit (in same commit, recommended)**. Grep all
  of `crates/crush-lang-sdk/src/{*.rs,bin/*.rs}` for the
  `Value::Bool(b) =>` pattern and patch anything else that surfaces
  under `--all-features`. Today's `db,stdlib` surface is exactly the
  three sites above; other feature combos may surface more — the
  audit prevents a followup ticket per lane.
- **TASKS.md docs flip** (companion docs change, same commit):
  flip the `[ ] **Fix `--all-features` build**` 🔴 P0 bullet to
  `[x]`, rewrite the description body to point at this patch, and
  prepend a Done log entry.

## Out of Scope

- Refactoring the matching style to owned-mode `match value {
  Value::Bool(b) => ... }`. The current `match &value { ... }`
  idiom is correct for the matcher that follows; we'd be churning
  style for zero functional gain.
- `Value::Bool`'s runtime representation. It's already specified as
  `bool` (no `Box`) per the s298 VM contract.
- Refactors to `db.rs` / `stdlib.rs` semantics; only type
  correctness is the issue here.
- New `Value::Bool` arms on non-VM bridges (e.g., a future
  `crush-frontend` roundtrip). They land with future features.
- Adding `bind mode = by-value` to Value's `#[derive(PartialEq)]` —
  orthogonal, defer to a future s299 if relevant.

## Done conditions

- [ ] `cargo check -p crush-lang-sdk --features=db,stdlib` exits 0
      (`currently 101; expected diff: ≤ 10 lines / ≤ 3 sites`).
- [ ] `cargo check --workspace --all-features` exits 0
      (currently propagates 101 from crush-lang-sdk).
- [ ] Adjacent `Value::Bool(b) =>` audit conducted; if any further
      sites surfaced they're patched in the same commit; if none,
      the audit script's output is captured in the commit message.
- [ ] `cargo test -p crush-lang-sdk --features=db,stdlib` exits 0
      with the same test count it had pre-fix on the default
      features (no test count delta expected; the existing 191 test
      markers in this crate should all still run).
- [ ] Public API of `crush-lang-sdk` unchanged (`pub use` set in
      `lib.rs` diff is empty under `git diff`).
- [ ] TASKS.md 🔴 P0 *Fix `--all-features` build* bullet flipped to
      `[x]`; new Done log entry prepended at the top of `TASKS.md`'s
      Done log section.

## Investigation findings (verified today on `ac1ca0f`)

```text
error[E0308]: mismatched types
   --> crates/crush-lang-sdk/src/db.rs:29:51
    |
 29 |         Value::Bool(b) => serde_json::Value::Bool(b),
    |                           ----------------------- ^ expected `bool`, found `&bool`
    |
note: tuple variant defined here
   --> /home/nixp/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/serde_json-1.0.150/src/value/mod.rs:133:5
    |
133 |     Bool(bool),
    |     ^^^^

error[E0308]: mismatched types
  --> crates/crush-lang-sdk/src/db.rs:52:62
   |
52 |         Value::Bool(b) => rusqlite::types::Value::Integer(if b { 1 } else { 0 }),
   |                                                              ^ expected `bool`, found `&bool`

error[E0308]: mismatched types
   --> crates/crush-lang-sdk/src/stdlib.rs:658:51
    |
658 |         Value::Bool(b) => serde_json::Value::Bool(b),
    |                           ----------------------- ^ expected `bool`, found `&bool`

error: could not compile `crush-lang-sdk` (lib) due to 3 previous errors
```

**Root cause** — the surrounding `match` is on `&Value`
(by-reference). Each arm therefore binds `b: &bool` (the field
borrowed by-ref). `serde_json::Value::Bool(bool)` and
`rusqlite::types::Value::Integer(bool-derived)` both expect an
owned `bool`. The borrow checker — correctly — refuses to coerce
`&bool` into the consuming enum constructor's `bool` argument
without an explicit deref. This is fallout from the s298 VM type
expansion: prior to s298 `Value` had no `Bool` variant, so no
match arm would have bound `b: &bool` and forwarded into those
constructors.

**Fix path** — `bool: Copy` makes `*b` the canonical fix (free,
unobservable, preserves the symmetric match). `b.clone()` is
equivalent but costs noise — no need.

**Adjacent audit command** (snap during change):

```bash
cd /workspace/projects/cr-ast-crushsdk-1-worktree
grep -nE 'Value::Bool\(' crates/crush-lang-sdk/src/*.rs \
    crates/crush-lang-sdk/src/bin/*.rs 2>&1
```

Expect only the 3 sites above to appear under `--all-features`. If
more shows up, fix them in the same commit.

## The exact patch (≤ 6 lines)

```diff
--- a/crates/crush-lang-sdk/src/db.rs
+++ b/crates/crush-lang-sdk/src/db.rs
@@ -27,7 +27,7 @@
     fn to_sqlite(v: &Value) -> rusqlite::types::Value {
         match v {
             Value::Null => rusqlite::types::Value::Null,
-            Value::Bool(b) => serde_json::Value::Bool(b),
+            Value::Bool(b) => serde_json::Value::Bool(*b),
             Value::Int(i) => rusqlite::types::Value::Integer(i),
             Value::Float(f) => rusqlite::types::Value::Real(f),
             Value::Str(s) => rusqlite::types::Value::Text(s.clone()),
@@ -50,7 +50,7 @@
     fn to_sqlite(v: &Value) -> rusqlite::types::Value {
         match v {
             Value::Null => rusqlite::types::Value::Null,
-            Value::Bool(b) => rusqlite::types::Value::Integer(if b { 1 } else { 0 }),
+            Value::Bool(b) => rusqlite::types::Value::Integer(if *b { 1 } else { 0 }),
             ...

--- a/crates/crush-lang-sdk/src/stdlib.rs
+++ b/crates/crush-lang-sdk/src/stdlib.rs
@@ -656,7 +656,7 @@
     fn to_json(v: &Value) -> serde_json::Value {
         match v {
             Value::Null => serde_json::Value::Null,
-            Value::Bool(b) => serde_json::Value::Bool(b),
+            Value::Bool(b) => serde_json::Value::Bool(*b),
             ...
```

## Reference files

- `crates/crush-lang-sdk/src/db.rs` lines 21-58 — `to_sqlite` /
  `to_json` adapters (the two error sites here).
- `crates/crush-lang-sdk/src/stdlib.rs` lines 650-680 — adjacent
  mirror adapter block (the third error site).
- `crates/crush-lang-sdk/Cargo.toml` lines 28-36 — `[features]` block.
- `crates/crush-vm/src/vm.rs` lines ~110-130 — the canonical
  `Value::Bool(bool)` definition locked by CRUSHVM-1 (parity test
  suite).
- `TASKS.md` 🔴 P0 *Fix `--all-features` build* — bullet to mark
  complete.
- `TICKETS/CRUSHVM-2-EXEC-LANG-POP-NAMED.md` — sibling-format
  reference; same convention.

## Skills needed

- cargo feature-flag ergonomics (`-p`, `--features=…`,
  `--all-features`).
- Rubust on `match &Value` (borrow-mode) pattern matching in Rust.
- Familiarity with the s298 VM type-expansion contract
  (`Value::Bool(bool)`, `Value::Map(...)`, `Value::Error(...)`,
  `Value::Bytes(...)`).
- `dejavue` decisions capture (post-commit).

## Bridge dispatch

- Post on `#general` after filing — flag that this is the first
  post-s298 cleanup ticket in `crush-lang-sdk`.
- Coordinate with the `crush-pkg` watchtower: their integration
  tests currently rely on `--features=db,stdlib`, so this fix
  unblocks their CI lane as a side effect.

## Migration risk

- **Programs persisted on disk via `Program::to_blob()` with
  `Value::Bool` round-trips.** Both serializers' owned-bool
  contracts are unchanged at the wire (serde_json still emits a
  JSON boolean `true|false`, rusqlite still emits INTEGER 1|0).
  No `.rzm` file break; no persisted-program compatibility break.
- **Test diff deltas — zero.** The 191 test markers in
  `crush-lang-sdk` should run unchanged; the `*b` deref is
  type-correct and runtime-equivalent.
- **`--all-features` exposure beyond `db,stdlib`.** Adjacent site
  audit is REQUIRED in the same commit to prevent a second
  `--all-features` failure on a different feature combo showing up
  in a followup ticket.

## Post-merge follow-up

- Capture the by-ref-match pattern into `.dejavue/decisions.md`:
  *"All `match &Value { Value::Bool(b) => ... }` arms must
  explicitly deref (`*b`) Copy-extracted fields when forwarding
  into consumer APIs that expect owned types. `bool`, `i64`,
  `f64` are Copy; `String`, `Bytes`, `Rc<...>` are not."* A future
  agent reading `dejavue context` will see this convention.
- Sweep `crush-frontend`, `crush-pkg`, `crush-installer`,
  `crush-cli`, `crush-python` for the same `Value::Bool(b) =>`
  pattern under their respective `--all-features` runs. Each
  location that surfaces gets a target-narrow ticket
  (e.g., `CRUSHFRONTEND-1`, `CRUSHPKG-3`).
- Verify `clippy::needless_borrow` already flags this class of
  pattern (it should). If it does NOT flag the Value-arm case,
  file a `clippy.toml` config PR.
- After the next VM-type expansion (`Error(Arc<…>)`?),
  re-run `cargo check --workspace --all-features` end-to-end to
  confirm the convention caught the new variant's arms before they
  land as P0 again.
- Once CRUSHSDK-1 is closed, the only remaining 🔴 P0 in TASKS.md
  (if any) gets a fresh audit. Run `grep -n '^## 🔴' TASKS.md` to
  enumerate after this lands.
