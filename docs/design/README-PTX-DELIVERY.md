# crush→PTX design — delivered to this tree 2026-07-14 (foreman/Kai)

Origin: **Vega [zorro]**, cross-box delivery. Upstream is branch
`design/crush-ptx-backend` on `nixpt/crush-ast` (commit `a257533`) — a **parked,
design-complete** turn from s373. **Not merged to `main`** anywhere; these are docs.

## What arrived

| file | what |
|---|---|
| `crush-ptx-backend.md` | **Way 3**: crush → PTX *text* → `ptxas`. `ptxas` IS the optimizer — structurally simpler than crush→LLVM→nvvm. Tiered opcode vocab (~20 ops; Tier 5 deferred). |
| `crush-gpu-capability.md` | `@` = capability-call. GPU is a **host-provided capability** (`@gpu.launch`); buffer = a `u64` handle, so **no VM changes**. v0 = cap-owns-context: zorro registers handlers over its device-0 primary context, so crush kernels and zorro kernels **share context + buffers**. |
| `../../examples/gpu/q6k_gemv.crush` | the target-shaped example. |

## READ THIS BEFORE YOU PLAN ANY TESTING

**This box `[main]` has NO CUDA GPU.** Vega is explicit about the split:

- **Testable here (CPU-side):** the *language/backend* half — crush → PTX text emission,
  the opcode tiers, the capability wiring. Validate emitted PTX with **`ptxas --verify`**;
  it does not need a device.
- **NOT testable here:** the launch path. That stays a `[zorro]` job — they have the GPU.

So if you touch this: build and verify **PTX text**, don't try to run kernels.

## Known-related landmines in this repo (found s380, already in `.jagent/planning/TASKS.md`)

- **`crush-jit` silently miscompiles.** `crates/crush-jit/src/compiler.rs:421` ends its opcode
  match with `_ => { push(TAG_NULL) }`. ~55 of ~86 `FastOp`s compile to a **silent null** and
  execution *continues* — JIT and interpreter can return **different answers with no error**.
  Directly relevant: a new backend that adds opcodes inherits this fallthrough.
- **Lambdas cannot be parsed** (lexer maps bare `|` to `Ident`; parser wants `Token::Pipe`).

## Blueprint Vega offers

**pyptx** (Apache-2.0, sm_120-validated, MoE grouped-GEMM 10× torch) — sitting in `[zorro]`
scratch. Say the word and she'll bundle it across.

## Open question for the captain

Vega proposes a **`crush-qsim`** capability that composes with this: a quantum simulator is
just another host capability — `@qsim.kernel(...)` exactly parallel to `@gpu.launch`,
buffer = `u64` handle either way, no VM changes. She'll spec it as
`docs/design/crush-qsim-capability.md` **if it reads right from our side.** Captain's call.
