# CRUSH-480 — Pinned @lang deps that cannot be honoured become a loud error on every Tier-1 engine

| Field | Value |
|-------|-------|
| **ID** | CRUSH-480 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#480](https://github.com/nixpt/crush-ast/issues/480) |
| **Size** | M |
| **Dependencies** | CRUSH-459, CRUSH-454 |

## Problem

Without `sandboxed-polyglot`, run_exec_lang_stdout discards a block's `@python[pypi:...]`/npm pins (`let _ = deps;`, scheduler.rs:407-419), so the block runs against whatever the host has installed. The scheduler VM and PortableVm both reach that path through SubprocessExecutor (lang_exec.rs:174-189; portable_vm.rs:1954, 2053). FastVM drops deps by construction: ExecLangSite has only lang/code/vars (fastvm/instructions.rs:210-215) and HostRequest::ExecLang yields lang, code and variables only (fastvm/execution.rs:1274-1301). The JIT refuses ExecLang (crush-jit/src/compiler.rs:1669) and falls back to FastVM, inheriting the drop. Ruling 22: silently dropped pinned deps become a loud error (CRUSH-236 criterion 2).

## Evidence (at origin/main 7ecd2515)

- crates/crush-vm/src/scheduler.rs:407-419
- crates/crush-vm/src/lang_exec.rs:174-189
- crates/crush-vm/src/portable_vm.rs:1954-1960,2047-2060
- crates/crush-vm/src/fastvm/instructions.rs:210-215
- crates/crush-vm/src/fastvm/execution.rs:1297
- crates/crush-jit/src/compiler.rs:1669
- ruling 22; CRUSH-236 criterion 2

## Goal

A block with a non-empty deps list fails with a VmError naming the lang and the spec strings whenever the executing path cannot provision them. On scheduler and PortableVm that is the unsandboxed SubprocessExecutor branch (reached only with the explicit unsandboxed opt-in). On FastVM, either lowering refuses an EXEC_LANG whose spec has deps (LowerError naming them), or deps travel in ExecLangSite/HostRequest::ExecLang and the host-side handler refuses them; pick one and document it. Blocks without deps behave as before.

## Non-goals

- Provisioning deps on the unsandboxed path
- New dep-spec syntax
- Backend selection (zandbox vs buckets)
- The sandbox-default flag itself

## Files

- crates/crush-vm/src/scheduler.rs
- crates/crush-vm/src/fastvm/instructions.rs
- crates/crush-vm/src/fastvm/execution.rs
- crates/crush-vm/src/fastvm/types.rs

## Done when

- scheduler VM test, built without sandboxed-polyglot and with the unsandboxed opt-in: a granted `@python[pypi:requests]` block returns Err containing 'pypi:requests' and nothing is spawned
- PortableVm test: the same program returns Err containing 'pypi:requests'
- FastVM test: lowering or running the same program fails loudly naming 'pypi:requests'; it must not yield an ExecLang request without the deps
- JIT test: the program is not JIT-compiled (compilation_count() == 0, fallback reported) and the fallback fails the same way as FastVM
- a granted block without deps still returns its output on all three engines
- cargo +1.100.0 clippy -p crush-vm -p crush-jit --all-targets -- -D warnings clean, with and without sandboxed-polyglot

## Depends on

- CRUSH-459
- CRUSH-454

## Refs

- CRUSH-236
- CRUSH-271
- CRUSH-140
- ruling 22

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
