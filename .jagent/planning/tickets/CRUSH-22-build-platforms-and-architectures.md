# CRUSH-22 — Build platforms & architectures (capture only — not scoped, not started)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-22 |
| **Priority** | Aspirational (not scheduled) |
| **Status** | Captured |
| **Phase** | none — post-M1 idea |
| **Assignee** | unassigned |
| **Dependencies** | none blocking capture |
| **Estimated effort** | unscoped |

## Origin

Captain, s388: capture a ticket for crush's build targets across OSes and CPU architectures —
"windows, macos, android, intel, amd, riscv, pi etc." Explicit capture-only request, same as
CRUSH-21 — no code, no CI changes, this session only documents what's real today and what's open.

## Current state, verified (not assumed)

- **CI tests exactly one target**: `.github/workflows/ci.yml` runs every job on `ubuntu-latest`
  only (`x86_64-unknown-linux-gnu`). No macOS, Windows, ARM, or RISC-V runner exists anywhere in
  the workflow.
- **Two separate AOT-to-native-code backends, inconsistently platform-aware**:
  - `crush-aot` (older) already branches on `cfg!(target_os = "linux" | "macos" | "windows")` in
    both `compiler.rs` and `bin/aotc.rs` — picking `.so`/`.dylib`/`.dll` per OS — but since CI never
    runs on macOS or Windows, **this branching has never been exercised outside Linux**. It could
    be broken on the other two branches and nothing would catch it.
  - `crush-aotc` (newer — just added to the workspace `members` this session, s388) has **no OS
    branching at all**: `codegen.rs` hardcodes `Command::new("cc")` unconditionally. This is the
    same "two backends silently disagree" bug class already on file for numeric correctness
    (CRUSH-13, five arithmetic implementations) and for the JS/C codegen builtin-name mismatch
    (Math.floor case bug, filed s388) — here it shows up as platform coverage instead of semantics.
  - `crush-installer` has its own separate `#[cfg(target_os = "windows")]` branch (`main.rs:466`),
    a third place OS-specific logic lives independently of the two AOT backends above.
- **No CPU-architecture-specific code exists at all**: zero occurrences of `aarch64`, `riscv`,
  `arm64`, or even `x86_64` anywhere in `crates/` — everything just shells out to whatever `cc` the
  host provides and trusts its default target. This means "does crush cross-compile to ARM/RISC-V"
  is currently an untested, unanswered question, not a known-broken or known-working one.
- **GPU backend is NVIDIA-only today**: `crush-ptx` compiles to PTX (NVIDIA's own GPU ISA) — there
  is no AMD (ROCm/HIP) or Intel GPU backend. If "intel, amd" in the request meant GPU compute
  rather than CPU, that's a materially different, much larger gap than the CPU-arch story below.
- **`wasm32` is the one non-native target that's real and exercised**: `crush-vm`'s
  `scheduler.rs`/`portable_vm.rs` already gate real code behind
  `#[cfg(target_arch = "wasm32")]`/`#[cfg(not(...))]` (from this session's WEB-1 merge), and
  `crush-lang-wasm` exists as its own crate. This is the only place in the codebase today where
  "does this actually build for a different target" has been checked rather than assumed.

## The specific platforms/archs named, and what each actually implies

- **Windows / macOS** — `crush-aot`'s OS-cfg branches already gesture at this; the real gap is CI
  coverage, not code. Same reasoning as `feedback_workspace_bin_durable_cache`-adjacent research
  this workspace already did for a sibling project (`buckets`' `SQ-RESEARCH-VMLANE` decision,
  s380): local QEMU VM lanes don't buy anything a Rust project doesn't already get from GH Actions'
  free `macos-latest`/`windows-latest` hosted runners — and macOS licensing specifically means
  Apple hardware only, ruling out a local/self-hosted macOS lane entirely. That decision's reasoning
  transfers here directly; re-verify rather than re-derive it if this ticket is ever picked up.
- **Android** — not a CPU-arch distinction, a whole different libc/ABI (Bionic, no glibc) and
  typically `aarch64`/`armv7` target triples via `cargo-ndk`, same toolchain shape already proven
  working in this workspace for `mycelium-light` (see `project_mycelium_mobile` memory, and this
  session's `mycelium-mobile`/`exo-light` Android-fleet-node arc). Crush has never been built for
  Android at all yet — this is the platform this session's other threads (the Android/proot HAL
  design, the Java/Kotlin capture in CRUSH-21) most directly motivate, since a crush capsule
  running on an Android fleet node needs crush-vm itself to actually cross-compile there first.
- **Intel / AMD (as CPU vendors)** — both are `x86_64`; there is no *build-target* distinction
  between them at the triple level (`x86_64-unknown-linux-gnu` covers both). The real distinction,
  if this is what was meant, is instruction-set-feature tuning (AVX2/AVX-512 availability differs
  by generation and vendor) for anything performance-sensitive in the AOT/JIT backends — currently
  nothing in `crush-aot`/`crush-aotc`/`crush-jit` selects or is even aware of target CPU features;
  everything is whatever `cc`'s default `-march` picks. **Ambiguous — could also mean GPU compute**
  (NVIDIA vs AMD vs Intel GPUs), see the PTX note above. Needs clarifying before scoping, not
  assumed either way.
- **RISC-V** — genuinely untested, zero code, zero CI. The one architecture on this list with no
  existing partial coverage anywhere in the codebase.
- **Pi (Raspberry Pi)** — not its own architecture; it's `aarch64` (Pi 3+) or `armv7`
  (older/32-bit) Linux, glibc, so it's really "does crush build for ARM Linux" wearing a specific
  hostname. Would share a cross-compilation story with the Android `aarch64` case above at the
  toolchain level (different libc, same CPU ISA family), though Pi's a full Linux distro (glibc,
  systemd, normal cargo cross-compilation) versus Android's Bionic+NDK path — worth noting these
  are NOT the same problem despite both being "arm-shaped."

## Open questions (deliberately unanswered — this is a capture, not a design)

- Does "intel, amd" mean CPU (near-zero incremental work beyond the RISC-V/ARM cross-compile
  story) or GPU (an entirely separate ROCm/HIP backend alongside `crush-ptx`)? Materially different
  scope depending on the answer.
- Priority ordering — Android is the one with a live, in-workspace motivating use case (the
  mobile-fleet-node arc); Windows/macOS/RISC-V/Pi don't have an equivalent pull today. Worth
  deciding whether this is "make crush portable in general" or "unblock the Android story
  specifically" before scoping further.
- Should CI cross-platform coverage (the GH Actions matrix expansion) be scoped as its own,
  smaller, closer-to-actionable ticket separate from actual cross-*compilation* work (RISC-V/ARM/
  Android toolchain wiring)? The former is nearly mechanical (add runners, see what breaks); the
  latter is real engineering.
- Does `crush-aotc` need to inherit `crush-aot`'s OS-cfg branching before either backend's
  platform story can be trusted, independent of any new-platform work? (This is really a
  consistency bug, not new scope — flagged here since it surfaced during this research, but it
  may deserve its own non-aspirational ticket rather than living only inside this one.)

## Non-goals (for this ticket, right now)

- No CI workflow changes, no cross-compilation toolchain setup, no code — capture only.
- Not claiming any platform above is prioritized against the active M1 correctness backlog
  (CRUSH-7 through CRUSH-21) — filed alongside them for visibility, not queued ahead.

## Cross-references

- `.github/workflows/ci.yml` — current single-target (`ubuntu-latest`) CI.
- `crates/crush-aot/src/compiler.rs`, `crates/crush-aot/src/bin/aotc.rs` — existing OS-cfg
  branches (Linux/macOS/Windows), untested outside Linux.
- `crates/crush-aotc/src/codegen.rs` — the newer AOT-C backend with no OS branching yet.
- `crates/crush-installer/src/main.rs` — third, independent place OS-specific logic lives.
- `crates/crush-ptx` — NVIDIA-only GPU backend; relevant if "intel/amd" meant GPU compute.
- `crates/crush-vm/src/scheduler.rs`, `portable_vm.rs`, `crates/crush-lang-wasm` — the one
  already-real, already-tested non-native target (`wasm32`), useful as a template for how a new
  target should be introduced (feature-gated `cfg`, not a silent assumption).
- Memory: `feedback_cells_release_build_jobs`-adjacent `SQ-RESEARCH-VMLANE` decision (buckets,
  s380) — GH Actions over local VM lanes for cross-platform testing; reasoning transfers directly.
- `.jagent/planning/tickets/CRUSH-21-java-kotlin-language-family.md` — the Android/mobile
  motivating context this ticket's Android section leans on.
- `workspace-meta/plans/2026-07-16-exosphere-android-proot-hal.md`,
  `projects/openko-network/mycelium-mobile` — the real Android-fleet-node arc this session, the
  clearest existing pull toward actually doing the Android half of this ticket first.
