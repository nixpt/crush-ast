# crush-python-rt — Multi-Lane Python Execution for CrushVM

Three backends for running Python code under Crush's capability model.

## Architecture

```
Python Source
     │
     ▼
┌─────────────────────────────────────────────┐
│            PythonRouter                      │
│  analyze_python() → choose_backend()        │
└─────────────────────────────────────────────┘
     │                    │                  │
     ▼                    ▼                  ▼
┌──────────┐    ┌──────────────┐    ┌──────────────┐
│ CAST     │    │ RustPython   │    │ Subprocess   │
│ Transpile│    │ Embedded     │    │ External     │
├──────────┤    ├──────────────┤    ├──────────────┤
│ py_walker│    │ RustPython   │    │ python3 -c   │
│ → CAST   │    │ VM in-process│    │ JSON bridge  │
│ → CASM   │    │ cap-gated    │    │ full CPython │
│ → CrushVM│    │ exo.* modules│    │ numpy/pandas │
│          │    │ AST filter    │    │              │
│ native   │    │ no C-ext     │    │ heavy IPC    │
│ fastest  │    │ medium speed │    │ slowest      │
│ safest   │    │ safe         │    │ process-iso  │
└──────────┘    └──────────────┘    └──────────────┘
```

## RustPython Embedded Backend (Lane 2)

### Design

RustPython (v0.4.0+, crates.io) is used as an embeddable library — no source fork needed. Sandboxing is achieved through configuration:

**Settings:**
```rust
Settings {
    isolated: true,                 // no site.py, no user site
    allow_external_library: false,  // no filesystem imports
    install_signal_handlers: false, // CrushVM retains control
    import_site: false,             // no startup filesystem access
    path_list: vec![],              // no import search paths
    ignore_environment: true,       // no PYTHONPATH/HOME leaks
    ..
}
```

**Builtin replacement (before executing user code):**
- `builtins.open` → Crush-gated version (requires `fs.read`/`fs.write` capability)
- `builtins.eval` / `builtins.exec` → Crush-gated (requires `sys.eval` cap)
- `builtins.__import__` → Crush-gated (requires `sys.import` cap)
- `sys.path` → empty list
- `sys.argv` → Crush-controlled

**exo.* module injection:**
- `exo.fs` — file operations through Crush's capability bridge
- `exo.net` — network through Crush's capability bridge
- `exo.env` — environment variable access (gated)
- `exo.log` — logging through Crush's output
- `exo.clock` — time access
- `exo.cap` — capability inspection

**AST filtering (parse → filter → compile → execute):**
- Parse Python source to AST via `rustpython-parser`
- Walk AST nodes, reject dangerous constructs:
  - `import os`, `import socket` → denied unless capability granted
  - `eval()`, `exec()` → denied unless `sys.eval` capability granted
  - `open()` → denied unless `fs.read`/`fs.write` capability granted
  - `__import__()` → denied unless `sys.import` capability granted
- Compile filtered AST to bytecode
- Execute in sandboxed RustPython VM

### Implementation Plan

```
Milestone 0: Spike — eval_source("print('hello')") works
  - Add rustpython-vm + rustpython-compiler deps
  - Create Interpreter with Crush settings
  - Run simple Python, capture stdout

Milestone 1: Output capture
  - Replace Python's stdout with Crush buffer
  - No direct host stdout

Milestone 2: Value passing
  - GuestValue ↔ PyObject conversion
  - Crush → Python → Crush round-trip

Milestone 3: exo.fs module
  - Custom Python module that calls through capability bridge
  - exo.fs.read("/path") → Crush cap check → ScopedHal → data

Milestone 4: Deny host FS
  - open("/etc/passwd") → CapabilityDenied
  - No capability bypass

Milestone 5: AST inspection
  - Parse → inspect → reject dangerous imports
  - import os → Denied: requires cap sys.import.os

Milestone 6: Python function calls from Crush
  - python.call("add", [2, 3]) → 5
  - Cross-lane function delegation
```

### Key Files

| Path | Purpose |
|------|---------|
| `src/lib.rs` | Entry point, execute_python() dispatcher |
| `src/router.rs` | PythonRouter, analyze_python(), backend selection |
| `src/rustpython_backend.rs` | Lane 2: RustPython VM wrapper |
| `src/backends/cast.rs` | Lane 1: CAST transpile (future) |
| `src/backends/subprocess.rs` | Lane 3: subprocess dispatch (future) |
| `src/ast_filter.rs` | AST node allow/deny policy (future) |
| `src/exo/` | exo.* capability modules (future) |
| `src/value.rs` | GuestValue ↔ PyObject conversion |

## Dependency

`rustpython-vm` from crates.io, enabled via `--features rustpython`.
