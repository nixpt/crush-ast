# CRUSH-21 — Java/Kotlin language family (capture only — not scoped, not started)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-21 |
| **Priority** | Aspirational (not scheduled) |
| **Status** | Captured |
| **Phase** | none — post-M1 idea |
| **Assignee** | unassigned |
| **Dependencies** | none blocking capture; would sit alongside the existing `crush-lang-*` family |
| **Estimated effort** | unscoped |

## Origin

Captain, s388, while reviewing the fabric→ExoLightRuntime wiring in `mycelium-mobile` (a real
mobile fleet-node project): *"i wanted to look into adding java/kotlin family next to crush and
was thinking maybe that way we can run crush in mobile and still access various java/kotlin apis
for services and even capsules better."* Explicitly requested as a **capture, not a build** — no
code should exist for this yet, this ticket exists so the idea isn't lost.

## The idea, as two distinct (but connected) pieces

**1. A `crush-lang-java` / `crush-lang-kotlin` walker, matching the existing family's shape.**
Today's `crates/crush-lang-{bash,c,custom,dart,go,js,nepali,python,rust,wasm,zig,zsh}` each parse
their source language (mostly via `tree-sitter-<lang>`, see `crush-lang-go`'s `Cargo.toml` as the
simplest reference: `tree-sitter-go = "0.23"` + `crush-walker-core`/`crush-cast` to lower into CAST,
implementing the shared `LanguageWalker` trait in `crush-frontend/src/language_walkers.rs`
— `parse()` → language AST, `walk()` → `crush_cast::Program`). `tree-sitter-java` and
`tree-sitter-kotlin` both exist upstream, so the walker itself would follow a well-established
pattern, not a novel one.

**2. The actual motivation: JVM/Android API access from CRUSH running on mobile.**
A source-to-CAST walker (piece 1) lets crush *analyze or transpile* Java/Kotlin source — it does
NOT, by itself, let a running crush program *call* Android SDK services (notifications, sensors,
`ContentResolver`, etc.) or JVM libraries at runtime. That's a different, capability-shaped
problem: some `HostCap`/`CAP_CALL` bridge from crush-vm into a JVM (Android's ART runtime, or a
JNI/JNA bridge) analogous to how `EXEC_LANG`'s `@python`/`@javascript`/`@bash` blocks already give
crush programs a polyglot execution path (see CRUSH-2/CRUSH-18/CRUSH-20 — the existing polyglot
capability-gate + sandboxing arc). Captain's framing connects both: a Java/Kotlin walker in the
CAST family *and* a way for capsules/services on Android to reach Java/Kotlin APIs "better" —
likely meaning less awkwardly than routing everything through JNI by hand.

## Why this matters now (context, not urgency)

This surfaced directly out of the `mycelium-mobile`/`exo-light`/Android-fleet-node arc this
session (see `workspace-meta/plans/2026-07-16-exosphere-android-proot-hal.md` and the
`mycelium-mobile` fabric-wiring work, `agent/kai/FABRIC-EXOLIGHT-WIRE`). The Android app side of
that arc (`mycelium-light`'s JNA-based UniFFI binding, `android/app/build.gradle.kts`) is already
Kotlin — so "crush capsules reaching Kotlin/Java APIs" isn't a hypothetical mobile use case, it's
adjacent to code that already exists in this workspace today.

## Open questions (deliberately unanswered — this is a capture, not a design)

- Walker-first or bridge-first? A walker without the runtime bridge lets crush *read* Java/Kotlin
  but not *call into* a live JVM — captain's own framing ("access... apis for services and even
  capsules better") suggests the bridge is the actually-wanted capability, with the walker being
  either a means to that end or a separate, smaller-value deliverable on its own.
- What hosts the JVM? Options span from "shell out to a JVM subprocess" (heaviest, matches
  `EXEC_LANG`'s existing external-interpreter shape) to "JNI-embed directly in the same process as
  `ExoLightRuntime`" (lightest latency, much bigger integration surface) to "expose Android's own
  ART/JNI surface directly since the guest is already running inside the Android app process" (the
  mobile-specific option, doesn't generalize to desktop Linux).
- Kotlin vs Java as the primary target — Kotlin is what this workspace's existing Android code
  already uses (mycelium-mobile's Android app); Java has the more mature tree-sitter grammar and
  is JVM-baseline. Doesn't need resolving now.
- Relationship to CRUSH-20's buckets-sandboxed polyglot path — does a JVM bridge want the same
  sandboxing tier, or is "JVM code running inside the same app process on Android" a fundamentally
  different trust boundary than "spawn a python subprocess on a Linux desktop"?

## Non-goals (for this ticket, right now)

- No walker code, no bridge code, no tree-sitter grammar wiring — this is strictly a captured idea.
- Not claiming this is scheduled or prioritized against the real M1 correctness backlog
  (CRUSH-7 through CRUSH-20) — those remain buffy's active campaign; this is filed alongside them
  for visibility, not queued ahead of them.

## Cross-references

- `crates/crush-frontend/src/language_walkers.rs` — the `LanguageWalker` trait every existing
  language family implements; whatever walker piece gets built here would implement the same trait.
- `crates/crush-lang-go/Cargo.toml` — simplest existing reference for the tree-sitter-based walker
  shape a `crush-lang-java`/`crush-lang-kotlin` would likely follow.
- `.jagent/planning/tickets/CRUSH-2-polyglot-capability-bypass.md`,
  `CRUSH-18-polyglot-runtime-errors-not-mapped-to-diagnostics.md`,
  `CRUSH-20-wire-buckets-sandboxed-polyglot-execution.md` — the existing polyglot
  capability/sandboxing arc a JVM bridge would most naturally extend rather than reinvent.
- `workspace-meta/plans/2026-07-16-exosphere-android-proot-hal.md` — the Android/mobile
  architecture context this idea surfaced from.
- `projects/openko-network/mycelium-mobile` — the real, working Android project (Kotlin app side)
  this idea's motivating use case points at.
