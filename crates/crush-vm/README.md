# crush-vm

The CVM1 runtime for the Crush toolchain — a sandboxed stack virtual machine plus
a binary bytecode assembler and disassembler.

## Purpose

crush-vm executes CASM bytecode. It is the standalone Crush runtime (extracted
from exosphere): a stack VM with **resource quotas** and **capability gates**, so
untrusted programs run under explicit limits and a controlled host surface.

```
CASM bytecode → crush-vm (CVM1) → execution
```

## What's here

- **VM** — `run` / `run_with_caps` execute a program under `Quotas`; `VmError` /
  `VmResult` report faults.
- **Assembler** — `assemble` / `disassemble` between CVM1 binary bytecode and a
  textual form (`AssemblyError`); `Program` is the loaded bytecode.
- **Capabilities** — `HostCap` / `HostCaps` / `CapabilitySpec` describe and gate
  the host functions a program may call; `capabilities()` enumerates them.
- **Portable VM** — `PortableVm` with `Frame` / `VmYield` for embedding and
  step-wise execution.

## Example

```rust
use crush_vm::{assemble, run, Quotas};

let program = assemble(source_text)?;
let result = run(&program, Quotas::default())?;
```

## License

Licensed under either of MIT or Apache-2.0 at your option.
