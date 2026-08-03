# Crush-Notebook — AI-Native Jupyter Alternative

## Strategic Context

Two parallel explorations converged:

1. **Crush-ast ecosystem** — 30-crate Rust workspace with a full language (parser, semantic analyzer, optimizer, bytecode compiler, sandboxed VM), an existing REPL, **AI-native constructs built into the AST** (AIExpression, AIStatement), and a **three-tier execution model**:
   - **CVM1 (PortableVm)** — interpreted, supports debugger, async yields, step-by-step
   - **FastVM** — lowered bytecode, enum-dispatch hot path
   - **crush-jit** — Cranelift JIT compiler, native x86 code from the same `LoweredProgram`
2. **sona** — a parallel C-like frontend language that compiles to the same CASM bytecode, also targeting FastVM + JIT. Demonstrates the multi-language frontend pattern.

2. **Surfer-browser capsule system** — runtime-registered capsule dispatch (PolicyGate �� CapsuleRouter → Handlers), 8 built-in services (ai, gui, wallet, memory, identity, vfs, system, hub), per-origin isolation with quota enforcement, and first-class Crush app support (`TabContent::CrushApp`).

The intersection is a **capability-gated, AI-native notebook** running as a surfer capsule with crush as its native language.

## What Makes This Different From Jupyter

| Dimension | Jupyter | Crush-Notebook |
|-----------|---------|----------------|
| Native language | Python | Crush (Rust VM, ~70 opcodes) |
| AI integration | None (add-on magics) | **Built into the AST** — `synthesize`, `query`, `agent_delegate`, `semantic_match`, `goal_declaration`, `knowledge_sharing` |
| Polyglot cells | IPython kernel magics | Native `@python`, `@javascript` blocks via WASM/WASI sandbox |
| Capability model | None (full filesystem access) | PolicyGate + CapsuleRouter — per-cell capability gating |
| Quotas | None | ResourceQuotas: CPU fuel, memory (256MB), storage (50MB), network bandwidth |
| Output types | MIME bundles | HostCap display pipeline + DOM rendering |
| Execution model | Per-kernel process | **Three-tier**: PortableVm (step debug) → FastVM (hot) → crush-jit (native Cranelift) |
| Multi-language | Python only | Crush + Sona + `@python`/`@javascript` polyglot blocks |
| Frontend | Browser-based | Surfer capsule (HTML/JS + Crush co-runtime) |
| Persistence | .ipynb JSON | .crush-nb (CAST-based format) |
| Multi-agent | None | AIExpression::AgentDelegation — dispatch sub-agents as cells |

## Architecture

```
┌─────────────────────────────────────────────────────┐
│              Surfer-Browser Capsule                  │
│  ┌──────────────────────────────────────────┐       │
│  │         Notebook Frontend                 │       │
│  │  (HTML/JS rendered via surfer's Boa+DOM)  │       │
│  │  ┌─────┐ ┌─────┐ ┌─────┐ ┌──────────┐   │       │
│  │  │Cell │ │Cell │ │Cell │ │New Cell  │   │       │
│  │  │  1  │ │  2  │ │  3  │ │  [add]   │   │       │
│  │  ├─────┤ ├─────┤ ├─────┤ ├──────────┤   │       │
│  │  │crush│ │crush│ │@py  │ │  crush   │   │       │
│  │  │code │ │AI●  │ │code │ │  +AI     │   │       │
│  │  │ Out │ │ Out │ │ Out │ │  options │   │       │
│  │  └─────┘ └─────┘ └─────┘ └──────────┘   │       │
│  └──────────────┬───────────────────────────┘       │
│                 │ PolicyGate::route()                │
│                 ▼                                    │
│  ┌──────────────────────────────────────────┐       │
│  │         CapsuleRouter                     │       │
│  │  notebook.eval_cell      → NotebookKernel│       │
│  │  notebook.complete_cell  → NotebookKernel│       │
│  │  notebook.interrupt      → NotebookKernel│       │
│  │  notebook.list_vars      → NotebookKernel│       │
│  │  ai.inference            → AI service    │       │
│  │  gui.create_panel        → GUI service   │       │
│  └──────────────┬───────────────────────────┘       │
└─────────────────��───────────────────────────────────┘
                  │
                  ▼
┌─────────────────────────────────────────────────────┐
│           Notebook Kernel (MCP server)              │
│  ┌──────────────────────────────────────────┐       │
│  │  Cell Manager                             │       │
│  │  Vec<Cell { id, source, status, outputs }>│       │
│  └──────────────┬───────────────────────────┘       │
│                 │                                    │
│  ┌──────────────▼───────────────────────────┐       │
│  │  Crush Compiler Pipeline                 │       │
│  │  tree-sitter → crush-frontend → crusm   │       │
│  └──────────────┬───────────────────────────┘       │
│                 │                                    │
│  ┌──────────────▼───────────────────────────��       │
│  │  PortableVm (CVM1)                       │       │
│  │  ┌─────────┐ ┌──────────┐ ┌───────────┐ │       │
│  │  │Stack    │ │Registers │ │HostCaps   │ │       │
│  │  │(values) │ │(ip,sp)   │ │display()  │ │       │
│  │  └─────────┘ └──────────┘ │mcp_tool() │ │       │
│  │                           │ai_gen()   │ │       │
│  │                           └───────────┘ │       │
│  ��──────────────────────────────────────────┘       │
│                                                     │
│  Host Capabilities:                                 │
│  • cell.display(type, data) → rich output            │
│  • cell.stream_output(chunk) → streaming text        │
│  • cell.mcp_tool(name, args) → MCP tool call        │
│  • cell.ai_gen(prompt) → @ai.synthesize              │
└─────────────────────────────────────────────────────┘
```

## Implementation Plan

### Phase 1 — Notebook Kernel (2-3 sessions)

| Step | What | Crates involved |
|------|------|-----------------|
| 1 | Create `crush-notebook-kernel` crate | New |
| 2 | Implement `CellManager` — Vec of cells with id, source, status, outputs | New |
| 3 | Extend `ReplState` to support cell lifecycle (create, eval, re-eval, delete) | crush-lang-sdk |
| 4 | Implement `HostCap` for `cell.display()` and `cell.stream_output()` | crush-vm |
| 5 | Add MCP tool bridge — `@cell.mcp_tool(name, args)` → call through MCP | crush-vm |
| 6 | Expose `PortableVm::current_state()` for variable inspection | crush-vm |
| 7 | Build MCP server wrapping the kernel (tools: `eval_cell`, `list_vars`, `interrupt`) | New |

### Phase 2 — Surfer Capsule (1-2 sessions)

| Step | What | Crates involved |
|------|------|-----------------|
| 1 | Register `notebook` service in `init_builtin_services()` | surfer |
| 2 | Add `CapsuleCapability::Notebook` variant | surfer-core |
| 3 | Create notebook frontend as HTML/JS capsule | New |
| 4 | Wire frontend to Notebook Kernel MCP server via CapsuleRouter | surfer |
| 5 | Add cell toolbar (run, stop, add cell above/below, toggle AI mode) | New |

### Phase 3 — AI Integration (1 session)

| Step | What | Crates involved |
|------|------|-----------------|
| 1 | AI cell type — `@ai(prompt)` generates crush code → insert as next cell | crush-cast |
| 2 | Cell completion — `@cell.complete(prefix)` → AI suggests next expression | crush-vm |
| 3 | Agent delegation — `delegate <task> to <agent>` → spawns agent cell | crush-cast/ai.rs |
| 4 | Knowledge sharing — `@cell.share(output)` → pushes to capsule's memory service | crush-cast/ai.rs |

---

## Execution Tiers (Corrected)

The crush ecosystem has **three execution tiers**, all sharing the same `LoweredProgram` representation:

```
Source (.crush) �� crush-frontend → CASM
Source (.sn)    → SonaCompiler   → CASM
                                     │
                                     ▼
                              lower_program()
                                     │
                                     ▼
                              LoweredProgram
                              ┌────┼────┐
                              ▼    ▼    ▼
                          CVM1  FastVM JIT
                       (debug)(hot) (native)
```

| Tier | Crate | Input | Dispatch | Notebook Use |
|------|-------|-------|----------|-------------|
| **CVM1** (PortableVm) | `crush-vm/src/portable_vm.rs` | String opcodes | String-match | Step-by-step debug, async yields, breakpoints |
| **FastVM** | `crush-vm/src/fastvm/` | `LoweredProgram` (FastInstr enums) | Enum-match | Hot-path execution, full opcode coverage |
| **crush-jit** | `crush-jit/` | Same `LoweredProgram` | Cranelift → native x86 | Maximum throughput, Phase 1 (core ops only) |

**There is no "CVM2".** The naming convention is: PortableVm = CVM1, FastVM = the lowered interpreter, crush-jit = native JIT.

### crush-jit Details

- **Uses Cranelift** (not LLVM, not hand-rolled x86)
- Compiles `LoweredProgram` → Cranelift IR → native code → RWX allocation
- Fixed-layout `JitContext` struct (8768 bytes, `#[repr(C)]`) with nan-boxed 64-bit values
- Tests cross-validate JIT output against FastVM interpreter for every supported instruction
- Phase 1: stack ops, arithmetic, comparisons, logic, jumps, locals. Not yet: cap calls, function calls, ref/arena, host calls.

### sona Details

- Independent C-like frontend language, crate `crush-lang-sona`
- 300-line hand-written recursive descent parser → `SonaExpr` AST → CASM
- **Bypasses the crush-frontend pipeline entirely** — no CAST, no semantic analyzer, no optimizer
- Targets FastVM (`run_fastvm_with_caps`) exclusively
- Demonstrates the **multi-frontend pattern**: any language that compiles to CASM runs on the same VM ecosystem

### Multi-Language Notebook Cells

Each cell can declare its frontend:

```
Cell 1: crush   → tree-sitter-crush → crush-frontend → CASM
Cell 2: sona    → SonaParser        → CASM (direct)
Cell 3: @python → WASM/WASI sandbox (polyglot block)
Cell 4: crush   → + @ai.synthesize("generate a sort function")
```

All converge on the same CASM → `lower_program()` → FastVM/JIT execution path.
