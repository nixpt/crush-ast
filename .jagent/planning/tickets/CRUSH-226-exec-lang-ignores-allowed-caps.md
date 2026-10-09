# CRUSH-226 — `EXEC_LANG` skips the declared-caps and `Quotas::allowed_caps` checks

| Field | Value |
|-------|-------|
| **ID** | CRUSH-226 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M7 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | XS |
| **Filed by** | kai (foreman) — s476, 2026-10-09; read on `5755262` |

## Problem

Every other capability call goes through `dispatch_cap`, which checks two things:

- the program's declared caps (`CapNotDeclared`), and
- the embedder's `Quotas::allowed_caps` allowlist (`CapDenied`).

These checks are at `scheduler.rs:1432-1439` and `portable_vm.rs:1536-1545`.

The `EXEC_LANG` arms check neither. They only ask whether a `polyglot.<lang>`
handler is registered in `host_caps`:

- `crates/crush-vm/src/scheduler.rs:1104-1109`
- `crates/crush-vm/src/portable_vm.rs:~1380` (same check)

The consequence: an embedder that registers the polyglot gate on a shared `HostCaps`
and restricts one program with `allowed_caps = ["io.print"]` still lets that program
spawn `python3`/`node`/`bash` with the host's authority. `cap_inference.rs:83`
reports the op as `polyglot.<lang>`, so the cap is named. It is just not enforced
like other caps.

## Fix

In both `EXEC_LANG` arms, before spawning, apply the same two checks `dispatch_cap`
uses to `gate`:

- declared caps, unless the manifest model deliberately exempts polyglot; if so,
  document that;
- `allowed_caps`.

Preferably factor them into one helper that both `dispatch_cap` and `EXEC_LANG` call.

## Success criteria

- [ ] `allowed_caps = Some(vec!["io.print"])` + polyglot gate registered → `@python`
      fails with `CapDenied("polyglot.python")` on both engines.
- [ ] Declared-caps behaviour decided and tested (enforced, or exemption documented).
- [ ] Existing `--polyglot` paths in `crush-run` unchanged.

## Related

- CRUSH-155 (effects metadata), CRUSH-164 (allowlist wildcards), CRUSH-179 (ambient
  caps in the `crush-vm` binary).
