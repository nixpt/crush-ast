# `capsule.toml` — the crush-pkg manifest

`crush-pkg` looks for `capsule.toml` (also `Capsule.toml`, `crush.toml`,
`Crush.toml`) in the current directory and its parents. The source of truth
is `src/manifest.rs` (`Manifest`, `CapsuleSection`); this page covers the
fields most packages use.

```toml
[capsule]
name        = "life"              # required
version     = "0.1.0"             # default "0.1.0"
description = "Conway's Game of Life in Crush"
entry       = "src/main.crush"    # default "src/main.crush"
language    = "crush"             # crush | python | node | bun | deno | sona | native …
                                  # empty = detected from the entry's extension
runtime_version = "3.11"          # script capsules only: buckets version constraint
category    = "game"              # optional, see below
platforms   = ["linux", "macos", "web"]  # optional, see below

[capabilities]
required = ["io.print"]
optional = []

[[dependencies]]
name = "util"
path = "../util"                  # path deps are compiled into the package
```

`crush-pkg show` prints the parsed manifest, including `category` and
`platforms` when they're set.

## `[capabilities]` and `crush-pkg check`

`required` (and `optional`) list the capabilities the program calls.
`crush-pkg check` compiles the package the way `build` does (entry plus path
deps, as one program), works out which capabilities that program can call,
and compares the two:

| Finding | Level |
|---|---|
| the program uses a capability `[capabilities]` doesn't declare | error (check fails) |
| a `required` entry covers nothing the program uses | warning |
| `platforms` includes `web`, and the program uses a capability the browser lacks | warning |

`--message-format=json` prints each finding as an `E-CAPS` NDJSON record.

An entry covers a capability when it's the same name (`"fs.cat"`), the same
name with a scope (`"fs.read:/var/log"`; the scope isn't checked), or a
family (`"fs"` or `"fs.*"` covers every `fs.<x>`). Capabilities the runtime
always provides don't need declaring: the VM built-ins (`io.print`, `str.*`,
`conv.chr/ord`, …), `caison.parse`, and the pure standard library.
Everything else does, including `polyglot.<lang>` for each `@lang` block and
any name check doesn't recognise (a host-specific capability, or
`module.load` from an `import`).

How "used" is worked out, and where it can be wrong:

- It reads the compiled bytecode, so it sees every lowering: a builtin that
  becomes a capability call counts, and capability names are always
  constants. There are no dynamic capability names.
- Only code reachable from the entry function counts, so a dependency's
  functions that are never called don't add anything. A function counts as
  reachable if it's called, or if its name appears as a string constant
  (that's how `spawn` names its target). A spawn target whose name is built
  at run time is missed.
- It's per program, not per path: a capability behind an `if` that never
  runs still counts as used.

## `category` (optional)

Catalogue metadata, for listing and grouping capsules. It has no effect on
how a capsule is built or run. Unset means uncategorised. One of:

| Value | For |
|---|---|
| `cli` | command-line tool |
| `library` | reusable package, mainly consumed as a dependency |
| `app` | end-user application (TUI, GUI or web UI) |
| `service` | long-running or background capsule |
| `game` | game |
| `dev-tool` | tool for writing, building or debugging code |
| `language` | language support: a walker, front end or plugin |
| `example` | demo, tutorial or teaching material |

Any other value fails to load with
`unknown [capsule] category "…"; expected one of: cli, library, …`.

Manifests written against the older crush-capsules category sketch keep
loading. Its names are mapped on load: `core-utility` and `system-utility`
become `cli`, `development-tool` becomes `dev-tool`, `user-app` becomes
`app`, `system-service` becomes `service`, and `language-walker` becomes
`language`.

## `platforms` (optional)

The platforms the capsule claims to run on. An empty or missing list means
"no claim", not "runs nowhere". Each entry is one of:

| Value | Meaning |
|---|---|
| `linux`, `macos`, `windows` | a native host running `crush-pkg` / `crush-run` |
| `web` | the browser, via crush-web's wasm VM |

Rules, checked when the manifest loads:

- unknown values are an error (`unknown [capsule] platform "…"`);
- a value may appear only once;
- `web` is only allowed for Crush capsules. The browser runs Crush programs,
  not Python, JavaScript or native binaries, so `language = "python"` with
  `platforms = ["web"]` is rejected.

`platforms` is a claim made by the author. `crush-pkg check` warns when a
`web` capsule uses a capability the browser doesn't provide: crush-web runs
the VM with no host capabilities, so only the VM built-ins (`io.print`,
`str.*`, `conv.chr/ord`, the array built-ins) are there. A `web` capsule
that calls `fs.read` is refused at run time in the browser.

## `[capabilities]` and running a capsule

`required` lists the capabilities the program calls (`io.print`, `fs.cat`,
`env.get`, …). Declaring a capability does not grant it. Whoever runs the
capsule grants authority on the command line, with the same flags as
`crush run`:

| Flag | Grants |
|---|---|
| `--fs` | `fs.*` and the `text.*` file tools, confined to `--fs-root` |
| `--fs-root DIR` | the directory `--fs` is confined to (default `.`) |
| `--env` | `env.get`, `env.all`, `env.home_dir` |
| `--time` | `time.*`, `async.sleep` |

The VM built-ins (`io.print`, `str.*`, `conv.chr/ord`, …) and the pure
standard library are always available. Grants work on `crush-pkg run` and
on bare `crush-pkg` (build, then run). A call to a capability that wasn't
granted stops the program with `unknown capability: <name>`.

```sh
cd examples/crush/capsules/squad-bridge-peek
crush-pkg run --fs --fs-root fixtures --env
```

## Host-approved Rust plugins

`crush-pkg` can load a manually built Rust `cdylib` for a Crush package.
The host supplies both the library path and each export to grant; `capsule.toml`
cannot select a library or grant native code. Plugins are trusted native code
and execute in the host process, so only load libraries you trust. Cargo is not
run by `crush-pkg` to build them.

Build a small wrapper crate against `crush-ffi`:

```toml
# rust-math/Cargo.toml
[package]
name = "rust-math"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
crush-ffi = "0.5"
num-integer = "0.1"
```

```rust
// rust-math/src/lib.rs
use crush_ffi::{CrushPlugin, CrushPluginExport, FfiType, FfiValue};
use num_integer::Integer;

extern "C" fn gcd(args: *const FfiValue, count: usize, out: *mut FfiValue) -> bool {
    if count != 2 || args.is_null() || out.is_null() {
        return false;
    }
    let args = unsafe { std::slice::from_raw_parts(args, count) };
    if args[0].tag != FfiType::Int || args[1].tag != FfiType::Int {
        return false;
    }
    let left = unsafe { args[0].data.integer };
    let right = unsafe { args[1].data.integer };
    unsafe { *out = FfiValue::from_int(left.gcd(&right)) };
    true
}

static PLUGIN_NAME: &[u8] = b"rust-math\0";
static EXPORT_NAME: &[u8] = b"rust.gcd\0";
static EXPORTS: [CrushPluginExport; 1] = [CrushPluginExport {
    name: EXPORT_NAME.as_ptr().cast(),
    func: gcd,
}];
static PLUGIN: CrushPlugin = CrushPlugin {
    plugin_name: PLUGIN_NAME.as_ptr().cast(),
    exports: EXPORTS.as_ptr(),
    export_count: EXPORTS.len(),
};

#[unsafe(no_mangle)]
pub extern "C" fn crush_plugin_init() -> *const CrushPlugin {
    &PLUGIN
}
```

Build the wrapper separately, then run the package with explicit host grants
(use `.so`, `.dylib`, or `.dll` as appropriate for the host):

```sh
cargo build --manifest-path rust-math/Cargo.toml --release
crush-pkg run --rust-plugin rust-math/target/release/librust_math.so \
  --plugin-export rust.gcd
```

Declare the export in the package and call it from its Crush entry file:

```toml
# capsule.toml
[capabilities]
required = ["rust.gcd"]
```

```crush
# src/main.crush
fn main() {
    io.print(rust.gcd(84, 30))
}
```

The wrapper calls `num-integer`'s `gcd` API, so the package prints `6`. The MVP supports
null, booleans, integers, floats, and strings across the FFI boundary; arrays
and objects are rejected until their ownership rules are defined. The plugin
export still has to be declared in the package's `[capabilities].required`
list for `crush-pkg check` to report a consistent package contract.
