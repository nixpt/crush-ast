# CRUSH-67 — crush-aot cache TOCTOU race: concurrent identical compiles collide

| Field | Value |
|-------|-------|
| **ID** | CRUSH-67 |
| **Priority** | P1 — deterministic failure from `cargo test` with two identical AOT tests |
| **Status** | Ready |
| **Phase** | Correctness |
| **Assignee** | unassigned |
| **Depends on** | none |
| **Estimated effort** | S |

## Origin

Found during bozo development (2026-07-25). Two AOT tests compiling the *identical* source (`fn main(){return 42;}`) concurrently both miss `AotCompiler::compile_casm`'s cache check (`so_path.exists()` → false) and race on the same `rustc` output path + work dir. The loser's thin-LTO temp objects get clobbered by the winner's `rustc`, producing `rust-lld: error: cannot open ...rcgu.o: No such file or directory` → `collect2: error: ld returned 1 exit status`.

## Reproduction

Two concurrent `rustc` cdylib compiles to the same `-o`:
```bash
cd /tmp && echo 'pub extern "C" fn crush_run() -> i64 { 42 }' > lib.rs
(rustc --edition 2024 --crate-type cdylib --crate-name r -o /tmp/race.so -C lto=thin lib.rs; echo p1=$?) &
(rustc --edition 2024 --crate-type cdylib --crate-name r -o /tmp/race.so -C lto=thin lib.rs; echo p2=$?) &
wait
# One succeeds (p1=0), one fails (p2=1) with the .rcgu.o linker error
```

## Root cause

`compile_casm` line 61: `if so_path.exists() { return Ok(so_path); }` — two callers see `false` concurrently, both proceed to invoke `rustc -o <same-path>`. The thin-LTO temporary object files in the output directory collide.

## Fix

**Compile to a temporary output name, then atomic rename to the cache path.** This is the standard content-addressed cache pattern (ccache, sccache):

```rust
let tmp_path = so_path.with_extension(format!("tmp.{}", std::process::id()));
cmd.arg("-o").arg(&tmp_path);
// ... rustc ...
std::fs::rename(&tmp_path, &so_path)?; // atomic on same filesystem
```

If a concurrent compile already created `so_path`, the rename replaces it atomically — both compiles succeed, one result wins (they produced the same bit-identical .so anyway). No lock needed.

## Files to modify

- `crates/crush-aot/src/compiler.rs` — `compile_casm` and `compile_c` methods

## Non-goals

- General concurrent-safety for `AotCompiler` (the struct itself is `&self`, not `&mut self` — fine)
- Lock-file per-module (renames are atomic, no lock needed)
