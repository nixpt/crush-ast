# EXEC_LANG callbacks — letting guest code call host capabilities mid-run

> Status: design note (CRUSH-166, 2026-10-31). **Nothing here is built**, and nothing should be
> until a consumer needs it. It records the one working prior design — exosphere's Python worker
> bridge, in a crate exosphere retired (EXO-216) — before that code goes, and says how the idea
> would fit crush-ast's grant model.

## 1. Today: one-shot, no way back in

A granted `@lang { … }` block runs once, start to finish, with no channel back to the VM:

- **in:** the block's input variables as environment pairs (JSON for Python/JS, text for bash) —
  `PolyglotRequest::vars` (`crates/crush-vm/src/lang_exec.rs`);
- **out:** everything it prints on stdout; the VM reads the declared outputs from the line starting
  with `CRUSH_RESULT_SENTINEL` (`"\0CRUSH_RESULT\0"`, `scheduler.rs`), and stderr becomes
  `VmError::LangRuntimeError` on failure;
- **who runs it:** the embedder's `PolyglotExecutor`, else `SubprocessExecutor` (a local interpreter,
  or a buckets/bwrap sandbox under `sandboxed-polyglot`, `bucket_exec.rs`);
- **budget:** `PolyglotRequest::deadline_ms` (`Quotas::max_wall_time_ms`), enforced by the executor.

So a Python block can compute with what it was given, but it can't read a file through `fs.read`,
fetch through `net.http_get`, or ask an AI op. It has to use its own interpreter's libraries, which
by default run with the host process's authority (CRUSH-236) — the cap model doesn't see them at all.

Callbacks would close that gap the other way round: the guest asks the **VM's** `HostCaps` to do the
I/O, and the program's grants decide.

## 2. The prior design: exosphere's Python worker

Source (reference only): `exosphere:crates/platform/runtimes/python/src/{worker.rs,bin/python_worker.rs,native.rs,lib.rs}`
@ `0c35ddec`.

**Transport.** A long-lived `python_worker` process (PyO3-embedded CPython) speaking NDJSON over its
stdin/stdout, one tagged object per line:

| host → worker | worker → host |
|---|---|
| `{"type":"execute","code":…,"policy":…,"globals":{…}}` | `{"type":"bridge_request","request":…}` |
| `{"type":"bridge_response","response":…}` | `{"type":"result","result":…}` |
| `{"type":"ping"}` | `{"type":"error","message":…}` / `{"type":"pong"}` |

**Requests** the guest could make (`BridgeRequest`): `Export {name, value}`, `Call {service, method,
args}`, `RequestCapability {name}`, and three DOM ones (`ApplyMutations`, `QueryDom`, `RegisterEvent`).
**Responses** (`BridgeResponse`): `Ok(value?)`, `Error(msg)`, `CapabilityResult {granted, reason}`.

**Guest side.** Injected functions `__crush_export__`, `__crush_call__(service, method, *args)` and a
capability request. Each serialises its arguments to JSON, sends a request on an mpsc channel, and
**blocks** for the response, so calls are strictly lock-step: at most one outstanding request, and
the worker's loop reads exactly one `bridge_response` line per `bridge_request` it emits.

**Sandbox.** A `SandboxPolicy` of booleans (`allow_fs`, `allow_net`, `allow_subprocess`, `allow_env`)
plus `timeout_ms` (default 5000) and `max_memory`.

### What to keep, what not to

- **Keep:** NDJSON, tagged messages, one request in flight, the guest blocks on each call, `ping`
  for liveness. Simple, debuggable with `tee`, language-neutral.
- **Don't keep:**
  - **The bridge was dead in its default configuration.** The bridge context lived in a
    `thread_local!` (`native.rs:51`) set on the calling thread, but `timeout_ms > 0` — the default —
    ran the guest on a *new* thread (`execute_with_timeout`, `native.rs:143`), where the context is
    `None`. With no context the injected functions fall into "local-only mode": `__crush_export__`
    returns `true` and nothing reaches the host. Lesson: whatever carries the channel must be passed
    explicitly to the thread or process running the guest, and *no channel* must be an error, never
    a silent success.
  - **Requests and program output shared one stream.** Results, bridge requests and anything the
    framing didn't own all went over the worker's stdout; a guest that writes to the real fd 1 can
    corrupt or forge frames.
  - **`RequestCapability`** — granting at run time from inside the guest. crush-ast's grants are
    fixed before the program runs and checked before anything executes; a guest must not be able
    to widen them.
  - **`service`/`method` addressing and the boolean policy** — crush-ast already has one namespace
    of named capabilities with specs (`HostCapSpec`) and one grant check; callbacks should reuse it.
  - **DOM requests** — UI hosts are out of scope (MIGRATION-INVENTORY §2.2); `ui.*` is crush-gui's.
  - **`Export`** — already covered by declared outputs and the sentinel line.

## 3. Proposed shape for crush-ast

### Channel

A **separate pipe pair**, not stdout: the executor passes the guest two extra file descriptors
(e.g. fd 3 = guest→host requests, fd 4 = host→guest responses) and names them in the environment
(`CRUSH_CALL_FD=3`, `CRUSH_REPLY_FD=4`). Guest stdout stays ordinary program output plus the
sentinel line, exactly as today, so a block that never calls back is unchanged and a block that
prints can't forge a request. bwrap passes inherited fds through to the sandbox, but the buckets
path must be checked (and tested) to keep these two open.

### Messages

Only one request kind:

```json
{"call": "fs.read", "args": ["notes.md"], "id": 1}
{"id": 1, "ok": "…file contents…"}
{"id": 1, "err": {"kind": "denied", "message": "fs.read is not granted to this @python block"}}
```

`id` is for log correlation; the protocol stays lock-step (one call in flight), so a guest library
needs no async machinery: write a line, read a line. Values travel as JSON in the shape
`Value ↔ JSON` already uses for polyglot variables; `Bytes` as base64 with a tag, or refused in a
first version.

### Grants — the part that matters

1. **The `polyglot.<lang>` grant never implies a callback grant.** A block that may run Python may
   not, by that alone, read files.
2. **Callback capabilities must be declared on the block,** e.g. `@python uses fs.read, net.http_get
   { … }`, so the static check (`crush_lang_sdk::effects::missing_grants`) can still refuse an
   ungranted program *before it runs* — today's promise. The checker can't see inside guest code,
   so an undeclared callback is refused at run time as `denied`, never quietly allowed.
3. **Each call goes through the same check as a Crush-side capability call:** granted in `HostCaps`,
   inside `Quotas::allowed_caps`, declared by the block. A refusal goes back to the guest as an
   error result (the guest decides whether that's fatal); it is not a VM error.
4. **No reentry into the VM:** callbacks reach host capabilities only — never Crush functions or VM
   state — so a guest can't re-enter the interpreter that is waiting on it.

### Time

`deadline_ms` keeps covering the **whole** block, host time spent servicing callbacks included; the
executor still enforces it and returns `PolyglotError::Timeout`. A host capability's own limits
(`--net-max-response-bytes`, …) apply per call as they do on the Crush side.

### API

`PolyglotRequest` is `#[non_exhaustive]`, so this is additive: a `callbacks: Option<&dyn CapCallbacks>`
field (call a named cap with JSON args under this block's declared set) that an executor uses to
service the channel. `SubprocessExecutor` would service it on the subprocess's fds; an embedder's
executor (browser worker, capsule) bridges it however its transport works, or ignores it — in which
case every callback fails with `unavailable`, explicitly.

## 4. Failure modes to design for

| Failure | Required behaviour |
|---|---|
| no callback channel (executor doesn't support it, or fd not passed) | guest-side call fails with `unavailable`; never a fake success (exosphere's bug) |
| undeclared or ungranted cap | `denied` to the guest; the VM records it like any refused call |
| guest exits / crashes mid-call | the host stops waiting on fd 3 at EOF; the block fails with the guest's stderr |
| host capability errors | `err` with the capability's message; the guest decides |
| malformed frame | the block fails with a protocol error naming the line; no attempt to resync |
| deadline passes while a call is in flight | the executor kills the guest and returns `Timeout`; the in-flight call's result is dropped |
| very large results | capped by the capability's own limits; frames are line-delimited JSON, so no length prefix needed |

## 5. Recommendation

Don't build it yet: no consumer asks for it, and the safer fix for "polyglot code has ambient
authority" is the sandbox (CRUSH-236), not a bridge. When a consumer does need it (a notebook cell
or a capsule that wants `@python` to use the program's grants), build exactly §3: fd pair, one
request kind, declared-per-block capabilities, the existing grant check, whole-block deadline — and
start with Python, the one language with a prior implementation to test against.
