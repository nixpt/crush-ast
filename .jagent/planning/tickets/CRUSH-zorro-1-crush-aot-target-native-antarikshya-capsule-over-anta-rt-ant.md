# CRUSH-zorro-1 — crush AOT target: native Antarikshya capsule over anta-rt / antarikshya-abi

| Field | Value |
|-------|-------|
| **ID** | CRUSH-zorro-1 |
| **Priority** | P3 |
| **Status** | Backlog |
| **Phase** | M8 (multi-target) |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-258 (structured C codegen, for size); CRUSH-121 (memory) for long-running capsules |
| **Estimated effort** | L |
| **Filed by** | claude (vega) — 2026-10-14, after the asm-bridge review; facts read from antarikshya `main` |

## Problem

crush can't be compiled into a **native Antarikshya capsule**. Two routes exist today, but neither runs AOT-compiled crush natively:

- `antarikshya/capsules/crush-tictactoe` embeds pre-compiled CVM1 and runs **crush-ast's `crush-vm`** (path dep, `default-features = false`) inside the **linux-compat** gate (`x86_64-unknown-linux-musl`). It works, but it is an interpreter in a compat layer, and it makes `crush-vm` a cross-repo API consumer.
- `antarikshya/capsules/crush-demo` is native: it is written against the `crush_*` **capsule C surface** that `anta-rt`'s `crush-abi` feature serves directly over the kernel ABI (`anta-rt/src/crush.rs`). That surface is exosphere's `abi-c` `crush/capsule.h`, vendored in `anta-rt`. But nothing in crush-ast emits code for it. The C AOT backend calls libc (`printf`, `malloc`) and its own helpers.

## Facts the design has to respect (antarikshya `main`)

- **Capsule memory:** image at `0x400000` in a **64 KiB** demand-faulted region (`.bss` included), stack `0x800000..0x810000`, heap `0xA00000..0xB00000` (1 MiB), and printed bytes staged through a 4 KiB message page (`anta-rt` lib docs).
- **Served `crush_*` calls today:**
  - `crush_io_print` / `crush_io_println` (`LogWrite` 0x60);
  - `crush_sys_time` (`ClockNow` 0x70), `crush_sys_sleep`;
  - `crush_capsule_init` / `run` / `yield` / `exit`;
  - `crush_mem_alloc` / `free` / `realloc`.
- **Not served yet:** `crush_fs_*`, `crush_net_*`, `crush_crypto_*` and `crush_storage_*` answer an honest status (grants ahead of implementation).
- **Size:** the C backend's `.so` is 25–45 KiB per program today (`docs/benchmarks/js_vs_node`, `py_vs_cpython`), so it can fit in 64 KiB. The Rust backend (about 2.6 MiB) cannot.
- **Syscall path:** `anta-rt` makes the syscalls itself, with inline `asm!` and the `syscall` instruction (an `int 0x7F` gate is an optional feature). The kernel-side `asm-bridge` crate is **not** needed here (reviewed 2026-10-14).

## Technical approach

- **A freestanding C AOT mode** (`--target antarikshya`) in `crates/crush-aot/src/codegen_c.rs`. Its prelude includes `crush/capsule.h` instead of libc:
  - `io.print` → `crush_io_println`;
  - `io.write` → `crush_io_print`;
  - allocation → `crush_mem_*`;
  - `time.now` / `time.now_ms` → `crush_sys_time`;
  - `main` → `crush_capsule_run`.
- **Text formatting stays in the prelude.** `_to_text_buf` already writes into a caller buffer, so drop the `printf` / `snprintf` family. Ints are formatted by hand; floats can be hand-written, or a loud "unsupported" error at first.
- **Capability gate at compile time:** a program using a capability with no served `crush_*` call **fails to compile with a named error**. It is never stubbed to null (the CRUSH-196 lesson). The mapping table is the single source of truth.
- **Link against `anta-rt` `crush-abi`.** Build an object with `-ffreestanding -nostdlib -fno-pic`; the link step and target spec follow `capsules/crush-demo`. Output: a capsule image plus a size report that fails if it exceeds 64 KiB.
- **Test it with the boot smoke** (`kernel/limine/boot-smoke.sh --capsule …`, as `crush-demo` does). A crush `hello` and a small loop must print under QEMU. Add a crush-ast-side test for the generated C: it must compile freestanding, with no libc symbols (`nm -u`).

## Success criteria

- [ ] `crush`/`aotc` builds a `.crush` program into an Antarikshya capsule image (C backend, freestanding, ≤ 64 KiB).
- [ ] `hello` + a loop + `time.now` print correctly in Antarikshya's QEMU boot smoke, natively. No linux-compat gate, no VM.
- [ ] An unserved capability (e.g. `fs.read`) is a compile-time error naming the capability and its missing `crush_*` call.
- [ ] Generated C has no undefined libc symbols (checked in crush-ast CI with `nm -u` on the object).
- [ ] Documented: the capability → `crush_*` → `CallClass` table, and the size budget.

## Files to modify

- `crates/crush-aot/src/codegen_c.rs`: the freestanding prelude, and target-specific lowering of `io.*`, memory and time.
- `crates/crush-aot/src/lib.rs` (or a new `target/antarikshya.rs`): the target enum, capability table and size check.
- The CLI entry (`crush` / `aotc`): `--target antarikshya`.
- antarikshya, cross-repo, coordinate (don't land unilaterally): a `capsules/crush-hello` built from crush output, and the boot-smoke case.

## Non-goals

- Running the JIT or FastVM in a capsule; and polyglot `EXEC_LANG` (Antarikshya hosts that set via buckets, per CRUSH-55 D-2).
- Changing `asm-bridge` or the kernel; the syscall side is `anta-rt`'s.
- Removing the linux-compat route (`crush-tictactoe` keeps working).

## Notes

- `crush-tictactoe` path-depends on `crush-ast/crates/crush-vm`. Treat `crush-vm`'s public API as having an Antarikshya consumer (e.g. when changing `PortableVm` / `run_with_caps` signatures).
- `capsule.h` came from exosphere `abi-c`, which is being retired (CRUSH-55 D-2). The vendored copy in `anta-rt` is now the reference, so pin its version in this target.
