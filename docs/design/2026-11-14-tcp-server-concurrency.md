# TCP server concurrency for Crush (#506): design note

altair (crush lane), 2026-11-14, CRUSH-NET-GAPS G2. **Design only. Nothing here is built
until vega or the captain picks a scope.** Advisor consult: chanakya (claude-fable-5-2,
confidence medium). Its load-bearing claims were checked against the code (marked *verified*).

## The problem

A Crush server cannot serve one client while waiting for another, and cannot do any other
work while it waits.

- **Observed** (arisu, prebuilt crush-run @7e6696db): `spawn acceptor()` (which calls
  `tcp.listen` then `tcp.accept`), plus `spawn ticker()`, plus a main loop. After
  `acceptor: listening, calling accept` nothing runs. The ticker never prints and `main: done`
  never prints, until an external `timeout 10` kills the run.
- **Why** (*verified*):
  - The CVM1 scheduler is one OS thread running green threads round-robin, 50 instructions
    per slice (`crates/crush-vm/src/scheduler.rs`). A thread switches when its slice runs out
    or at SPAWN, AWAIT and YIELD. All of these happen between instructions, never inside a
    capability call.
  - Host capabilities are called synchronously from the step loop
    (`dispatch_cap` → `call_with_deadline`). So any blocking cap holds the only OS thread:
    `tcp.accept`, `tcp.recv`, and also **`time.sleep`**.
  - Probe: with `spawn sleeper(); spawn ticker(); print("main"); time.sleep(800)` only `main`
    prints, because main blocks inside its first slice and its return ends the program.
- **Two more facts that shape any fix** (*verified*):
  - When no thread is runnable, the scheduler returns a normal halt
    (`if !any_runnable { … halted: true }`). Main returning also ends the program.
  - crush-net's polling reactor (`crates/crush-net/src/reactor.rs`) probes readiness with
    `Source::try_read`. For `TcpConnection`, that is a **consuming** `read`. Used as is, it
    would eat bytes meant for `tcp.recv`. The same poller calls `try_accept` on listeners and
    drops the accepted stream (`Ok(Some(_))` only wakes the waker), so it would also eat
    connections meant for `tcp.accept`.
  - `Value` is `Rc<RefCell<…>>`, so it cannot be sent to another thread, and `parallel_map`
    workers run in a fresh VM with no host caps.

## Options

| | What | Cost | Fixes #506? |
|---|---|---|---|
| **A** | One connection at a time, documented as v1. Liveness comes from #512's `tcp.accept`/`tcp.recv` `{timeout_ms}` and #510's `--max-wall-ms unlimited`. | No new code. A slow client stalls everything for up to its timeout. An acceptor plus a background task only work with short accept timeouts in a loop. | No, but it is livable |
| **B** | Wire crush-net's reactor (poller thread, `Waker`s) into the scheduler. | Adds a second thread and waker plumbing to a single-threaded scheduler. The reactor's consuming `try_read` **and** its `try_accept` (which drops the accepted stream) must both change to non-consuming readiness checks (`peek`, or `poll(2)`) first. | Yes |
| **B'** | `HostCap::try_call(args) -> Ready(v) \| Pending`, with a default that falls back to the blocking call, so every other cap and engine is unchanged. Only `tcp.accept`/`tcp.recv`/`tcp.send`/`time.sleep` implement it. A green thread that gets `Pending` is parked; the **scheduler** retries parked threads each round, and sleeps 1-5 ms when every live thread is parked. | One trait method on the public SDK (crates.io surface), one park state, the all-parked idle path, timeout/wall checks while parked. A few ms of polling latency. CVM1 scheduler only; FastVM/JIT/AOT/portable stay blocking (the same gap as #161). | Yes |
| **C** | A worker VM per connection: an OS thread per VM, with its own caps, and only plain data (the conn id) crosses. | True parallelism and isolation, but the largest semantic surface: a grant decision for workers (today's `parallel_map` workers deliberately have no host caps), a thread per connection, and no shared mutable state between handlers. | Yes |

## Recommendation

1. **v1 = A, now.** It ships with #509, #510 and #512. Document it in the language guide with
   the exact acceptor-plus-ticker example and the workaround (`tcp.accept(l, {timeout_ms: 50})`
   in a loop that also does the other work). The Rust avalanche oracle drives requests
   sequentially, so the v1 server port needs nothing more.
2. **v2 = B'.** It is the smallest change that actually fixes #506, and it builds on what
   exists: `TcpListener::try_accept` and the non-blocking sockets in crush-net, and the
   scheduler's AWAIT park shape (`waiting_for`, skipped by the round-robin scan). It needs
   no second thread and no `Send` values. Its rules:
   - **The all-parked path must wait and retry, not halt.** Halt only when every thread is
     done, or when main has returned.
   - **Retry must be idempotent.** A cap returns `Pending` only before it has consumed or sent
     anything. `tcp.recv` keeps its split-UTF-8 tail in `NetState.pending` (#512) across a
     `Pending`. A partial `tcp.send` returns the count it wrote (today's contract) rather than
     `Pending`.
   - **Timeouts and the wall budget keep working while parked.** A parked call stores its
     `Bound` (deadline, own-vs-wall); the retry loop fails it with the same catchable or
     uncatchable error as today.
   - **In scope first:** `tcp.accept`, `tcp.recv`, `time.sleep`. Then `tcp.send` (a slow
     reader). `tcp.tls_wrap` handshakes and `net.*` (ureq) stay blocking for now; say so in
     the docs.
3. **B only** if B' polling latency or CPU turns out to matter. Fix the reactor's consuming
   readiness check first, whatever happens.
4. **C is a separate question** (parallel handlers, isolation). It is not needed for a
   loopback server, and its grant model is the captain's call.

## Tests a B' PR must carry

- arisu's g2 repro: the ticker prints while the acceptor waits, a client connecting later is
  served, and the run ends when main returns.
- The sleeper/ticker probe above prints `tick` ×3 before `slept`.
- Idle CPU is near zero while every thread is parked (the 1-5 ms sleep, no busy spin).
- A client sending one request in two slow fragments is reassembled by a parked `tcp.recv`
  (idempotent retry).
- `{timeout_ms}` on a parked accept/recv fires on time and is catchable; the wall budget still
  kills.
- A program on the FastVM/JIT/AOT backends still behaves as today (blocking), as documented.

## Decision

- The content of this note: altair.
- Whether B' gets built, and when: vega/captain. It adds a public `HostCap` method, and it
  widens the CVM1-vs-other-engines gap.
- C's grant model: the captain.
