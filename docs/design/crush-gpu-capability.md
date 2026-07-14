# The `gpu` capability — interface sketch (draft, Vega s371)

Companion to `crush-ptx-backend.md`. That doc covers **compile-time** (crush →
PTX codegen). This one covers **runtime**: how a compiled crush kernel actually
runs, expressed as a *host-provided capability* — the same extension point
`fs.*`/`net.*` already use (crush defines the interface; the host registers the
impl). Ground truth: `crush-vm/src/host.rs` (`HostCap`) and
`crush-vm/src/fastvm/mod.rs` (`Capability` + `Hal`).

## The two-phase split (this is the whole design)

| | Compile-time | Runtime |
|---|---|---|
| **What** | `@kernel fn q6k_gemv(...)` → PTX text | `@gpu.launch(q6k_gemv, ...)` |
| **Where** | crush compiler (new CASM→PTX backend) | host-registered `gpu.*` capability |
| **Is it a capability?** | **No** — it's codegen. The kernel is a build artifact. | **Yes** — dispatched at VM runtime to the host. |
| **Who owns it** | crush | the host (zorro registers it, backed by its CUDA context) |

The kernel body never becomes a capability. Only the *host interactions*
(allocate, copy, launch, sync) cross the VM boundary — exactly like a crush
program isn't a capability but its `@fs.write` calls are.

## The `gpu.*` capability surface (crush-facing)

Minimal but complete. Each row is one registered `HostCap` (one name per
handler; `HostCapSpec.name` is a single string).

| Capability | argc | returns | Meaning |
|---|---|---|---|
| `gpu.alloc(nbytes)` | 1 | Buffer | device malloc → opaque handle |
| `gpu.upload(bytes)` | 1 | Buffer | H2D: alloc + copy from a crush byte array |
| `gpu.download(buf, nbytes)` | 2 | Bytes | D2H: copy device→host → crush bytes |
| `gpu.free(buf)` | 1 | — | release a handle |
| `gpu.launch(kernel, grid, block, args)` | 4 | — | load-if-needed + launch |
| `gpu.sync()` | 0 | — | device synchronize |

Crush usage (the host half of the Q6_K GEMV example):

```crush
fn run_q6k_gemv(w_bytes: Bytes, act_bytes: Bytes, rows: u32, K: u32) -> Bytes {
    let w   = @gpu.upload(w_bytes);            // Buffer handle
    let act = @gpu.upload(act_bytes);
    let out = @gpu.alloc(rows * 4);            // f32 per row
    @gpu.launch(q6k_gemv, grid: rows, block: 32, args: [w, act, out, rows, K]);
    let result = @gpu.download(out, rows * 4);
    @gpu.free(w); @gpu.free(act); @gpu.free(out);
    return result;
}
```

## The ABI: how crush `Value`s cross to the device

crush `Value`s are dynamic (Int/Float/String/Array/Struct). No VM changes
needed — represent a **device buffer as an opaque `u64` handle** (`Value::Int`);
the host keeps a `handle → CudaSlice` table. At the kernel-launch boundary:

| Kernel param type | crush `Value` | Marshalled as |
|---|---|---|
| `Ptr` (global mem) | `Value::Int` (handle) | device pointer (table lookup) |
| `u32` / `i32` | `Value::Int` | 4-byte scalar param |
| `f32` | `Value::Float` | 4-byte scalar param |

This keeps the interface inside the existing `Value` type — the buffer is just
an integer the host knows how to resolve. (A later `Value::Native(Arc<dyn Any>)`
would let handles carry RAII drop, but is not needed for v0.)

## The Rust impl the host registers (skeleton)

One handler per `gpu.*` name. Shared device state (context + module cache +
handle table) lives in an `Arc` the handlers close over.

```rust
struct GpuState {
    ctx:     Arc<CudaContext>,                 // zorro's device-0 primary context
    modules: Mutex<HashMap<String, CudaModule>>, // kernel-name → loaded PTX
    buffers: Mutex<HandleTable<CudaSlice<u8>>>,  // u64 → device alloc
}

struct GpuLaunch(Arc<GpuState>);

impl HostCap for GpuLaunch {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec { name: "gpu.launch".into(), argc: Some(4), returns: false }
    }
    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        let kernel = args[0].as_str()?;                 // compiler lowered symbol→name
        let grid   = args[1].as_u32()?;
        let block  = args[2].as_u32()?;
        let launch_args = marshal(&args[3], &self.0.buffers)?;  // handles→ptrs, scalars
        let module = self.0.load_or_get(kernel)?;       // cuModuleLoadData once, cached
        module.launch(kernel, grid, block, &launch_args)?;
        Ok(None)
    }
}

struct GpuAlloc(Arc<GpuState>);
impl HostCap for GpuAlloc {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec { name: "gpu.alloc".into(), argc: Some(1), returns: true }
    }
    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        let n = args[0].as_usize()?;
        let slice = self.0.ctx.alloc_zeros::<u8>(n).map_err(|e| e.to_string())?;
        let handle = self.0.buffers.lock().insert(slice);
        Ok(Some(Value::Int(handle as i64)))
    }
}
// … GpuUpload / GpuDownload / GpuFree / GpuSync likewise.

// Registration (zorro's host bootstrap):
let state = Arc::new(GpuState::new(cuda_ctx));
host_caps
    .register(Box::new(GpuAlloc(state.clone())))
    .register(Box::new(GpuUpload(state.clone())))
    .register(Box::new(GpuDownload(state.clone())))
    .register(Box::new(GpuLaunch(state.clone())))
    .register(Box::new(GpuFree(state.clone())))
    .register(Box::new(GpuSync(state)));
```

For the **FastVM** path, the same logic implements `fastvm::Capability`
(`call(arena, args, hal)`); the `Hal` argument is the alternative home for the
device (see Open Questions).

## Where the PTX modules come from

The crush compiler emits one PTX per `@kernel fn` into the program artifact
bundle (alongside the CASM). At load time the host registers those PTX blobs
keyed by kernel symbol name. `@gpu.launch(q6k_gemv, …)` — the compiler lowers
the `q6k_gemv` symbol to its module/entry-point name string, so the runtime
lookup is a plain `HashMap` hit. `load_or_get` runs `cuModuleLoadData` once and
caches (ptxas JITs at that point — the "ptxas is the optimizer" step).

## Permissions (deny-by-default, like every capability)

```json
{ "manifest": { "permissions": {
    "gpu.alloc": true, "gpu.upload": true, "gpu.download": true,
    "gpu.free": true, "gpu.sync": true,
    "gpu.launch": ["q6k_gemv"]          // scope to named kernels, or true for all
} } }
```

Scoping `gpu.launch` to a kernel allow-list gives a capsule "may run *these*
kernels" — a real security property, free from the existing model.

## Open questions (next turns)

1. **HAL vs cap-owns-context. — DECIDED (captain, s371): cap-owns-context for
   v0.** The `gpu` capability holds the CUDA context / module cache / handle
   table directly in its `Arc<GpuState>`; no `GpuHal` trait for now. Fewest
   moving parts, and it lets zorro register the handlers over its *existing*
   device-0 primary context immediately. Refactor to a `Hal` impl only if a
   second host (exo-light) needs GPU capsules — the capability's `call()` body
   is the same either way, so the migration is mechanical.
2. **Buffer lifetime.** Explicit `gpu.free` (drafted) vs RAII via
   `Value::Native` handles that drop the device alloc. Explicit is simpler and
   matches the C/CUDA mental model; revisit if leaks bite.
3. **Sync model.** Implicit sync inside `gpu.download` (drafted — download
   blocks) vs an explicit `gpu.sync()` for pipelining multiple launches. Both
   offered; download-syncs is the safe default.
4. **Who registers it first.** zorro (has the CUDA context + kernels + PTX
   tooling already) is the natural first host. exo-light registers the same
   handlers to run GPU capsules. The *interface* (this doc) is shared; the impl
   is per-host.
```
