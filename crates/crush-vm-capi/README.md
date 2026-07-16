# crush-vm-capi

C API shared library for embedding the CrushVM in C/C++ programs.

This crate builds as a `cdylib` (`libcrush_vm_capi.so` / `libcrush_vm_capi.dylib` / `libcrush_vm_capi.dll`) so that the main `crush-vm` crate can remain a plain Rust `lib`, avoiding duplicate compilation of `casm` in workspace builds.

## What it provides

A small, stable C ABI for:

- Initializing the CrushVM runtime.
- Running programs from CASM JSON.
- Running programs from CASM text assembly.
- Retrieving the last error message.
- Querying the library version.

## Building

From the workspace root:

```bash
cargo build -p crush-vm-capi
```

The shared library is produced in `target/debug/` (or `target/release/` with `--release`).

To build only the cdylib and skip tests:

```bash
cargo build -p crush-vm-capi --lib
```

### Features

- `native-plugins` (default): Enables the native C-API symbols. Disabling it (`--no-default-features`) removes the C-API exports so the crate can be linked as a normal Rust library without the cdylib symbols.

## C/C++ API

Include the header (`crates/crush-vm-capi/include/crush_vm.h`):

```c
#include "crush_vm.h"
```

Link against:

```text
-lcrush_vm_capi
```

On Linux the shared library is `libcrush_vm_capi.so`, on macOS `libcrush_vm_capi.dylib`, and on Windows `crush_vm_capi.dll` (import library `crush_vm_capi.dll.lib`).

### Functions

```c
int crush_vm_init(void);
```

Initialize the CrushVM runtime. Call once before any other function. Returns `0` on success.

```c
int crush_vm_run_casm(const char *casm_json);
```

Load and execute a CASM JSON program (`casm::Program`). Returns `0` on success, `-1` on parse error, `-2` on execution error.

```c
int crush_vm_run_asm(const char *asm_source);
```

Assemble and execute a CASM text source (the text format accepted by `crush_vm::assemble`). Returns `0` on success, `-1` on assembly error, `-2` on execution error.

```c
const char *crush_vm_last_error(void);
```

Get the last error message. Returns a pointer to a null-terminated string, or `NULL` if no error occurred. The pointer is valid until the next API call. Do not free it.

```c
const char *crush_vm_version(void);
```

Get the CrushVM library version string. The version returned matches the crate release (currently kept in sync with the workspace version).

## Example

See [`examples/embed.c`](examples/embed.c) for a complete working example.

```c
#include "crush_vm.h"
#include <stdio.h>

int main(void) {
    int rc;

    rc = crush_vm_init();
    if (rc != 0) {
        fprintf(stderr, "Failed to initialize CrushVM (rc=%d)\n", rc);
        return 1;
    }

    /* Run a CASM JSON program that prints 42. */
    const char *casm =
        "{"
        "  \"version\": \"1.0\","
        "  \"manifest\": {\"permissions\": [\"io.print\"]},"
        "  \"functions\": {"
        "    \"main\": {"
        "      \"params\": [],"
        "      \"locals\": [],"
        "      \"body\": ["
        "        {\"op\": \"push_int\", \"value\": 42},"
        "        {\"op\": \"cap_call\", \"name\": \"io.print\", \"argc\": 1},"
        "        {\"op\": \"ret\"}"
        "      ]"
        "    }"
        "  }"
        "}";

    rc = crush_vm_run_casm(casm);
    if (rc != 0) {
        fprintf(stderr, "crush_vm_run_casm failed (rc=%d): %s\n",
                rc, crush_vm_last_error());
        return rc;
    }

    /* Re-initialize before running a second program so the runtime starts fresh.
     * Text assembly currently has no way to declare capabilities, so this
     * example just pushes a value and halts. */
    crush_vm_init();
    const char *asm_src =
        "PUSH 42\n"
        "HALT\n";

    rc = crush_vm_run_asm(asm_src);
    if (rc != 0) {
        fprintf(stderr, "crush_vm_run_asm failed (rc=%d): %s\n",
                rc, crush_vm_last_error());
        return rc;
    }

    printf("CrushVM version: %s\n", crush_vm_version());
    return 0;
}
```

Compile and run on Linux from the `crates/crush-vm-capi/` directory (replace `target/debug` with your actual Cargo target directory if it differs; on macOS/Windows use the equivalent compiler/linker flags for that platform):

```bash
gcc -o embed examples/embed.c -I include -L target/debug -lcrush_vm_capi -Wl,-rpath,target/debug
LD_LIBRARY_PATH=target/debug ./embed
```

## Capabilities

Any `cap_call` instruction requires the capability to be declared in the program manifest. For CASM JSON, add a `manifest.permissions` array:

```json
{
  "version": "1.0",
  "manifest": {"permissions": ["io.print"]},
  "functions": { ... }
}
```

`crush_vm_run_asm` assembles text source with no manifest, so programs that use capabilities must be supplied as CASM JSON via `crush_vm_run_casm`.

## Testing

The crate includes a Rust integration test that compiles and runs `tests/test_embed.c` against the produced shared library:

```bash
cargo test -p crush-vm-capi
```

## License

Licensed under the Apache-2.0 or MIT licenses, the same terms as the rest of the Crush project.
