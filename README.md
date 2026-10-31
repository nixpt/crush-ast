<p align="center">
  <img src="assets/hero.png" alt="Crush AST Banner" width="100%" />
</p>

<h1 align="center">
  <img src="assets/logo.png" alt="Crush Logo" width="40" height="40" style="vertical-align: middle;" />
  Crush AST
</h1>

<p align="center">
  <strong>Polyglot Intermediate Representation & Toolchain</strong>
</p>

<p align="center">
  <a href="LICENSE-MIT"><img src="https://img.shields.io/badge/license-MIT%20/%20Apache--2.0-blue.svg" alt="License"></a>
  <img src="https://img.shields.io/badge/rust-1.95%2B-orange.svg" alt="Rust Version">
</p>

<p align="center">
  <a href="https://crushlang.org">crushlang.org</a> ·
  <a href="https://docs.crushlang.org">Language Guide</a>
</p>

---
[![CI](https://github.com/nixpt/crush-ast/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/nixpt/crush-ast/actions/workflows/ci.yml)
**Crush** is a capability-based, polyglot programming language and virtual runtime. This repository hosts the **standalone Crush implementation**: providing the CAST intermediate representation, tree-sitter grammar, polyglot walkers, compiler frontend, VM runtime, package manager, and installer.

👉 **[crushlang.org](https://crushlang.org)** for the landing page and a live example, or **[docs.crushlang.org](https://docs.crushlang.org)** for the full language guide (syntax, capability system, polyglot programming, stdlib, CAST/CASM, worked examples).

## 📦 Install

```bash
cargo install crush-lang-sdk    # crush, crushc, crush-run, crush-repl, crush-compile, crush-diff, crush-debug
```

> **Not `cargo install crush`** — the `crush` crate on crates.io is an unrelated project. The
> toolchain is published as **`crush-lang-sdk`** (plus `crush-pkg`, `crush-installer`, and the
> library crates). There is no `crush-ast` crate; that's this repository's name.

**Prebuilt binaries.** Starting with v0.9.0, every `vX.Y.Z` tag has a GitHub **pre-release** with a
Linux x86_64 tarball (`crush`, `crushc`, `crush-run`, `crush-compile`, `crush-repl`, `crush-debug`,
`crush-pkg`, `crush-gui`), the browser runtime (`crush-web-<ver>-pkg.tar.gz`) and `SHA256SUMS`, so
`cargo binstall crush-lang-sdk` downloads instead of compiling. (Pre-releases until 1.0: see
[`docs/planning/RELEASE-1.0-CRITERIA.md`](docs/planning/RELEASE-1.0-CRITERIA.md).)

**Updating.** `crush update` (= `crush-installer update`; needs `cargo install crush-installer`)
replaces the binaries of a tarball/`crush-installer` install with the newest release: it checks the
download against the release's `SHA256SUMS` before extracting and swaps every binary in with an
atomic rename (a failure leaves the old set), so it can replace the running `crush`. Linux x86_64
only; a `cargo install` is refused — update that with `cargo install crush-lang-sdk`.

```bash
crush update --check            # exit 0 = up to date, 10 = update available; writes nothing
crush update                    # newest release; never downgrades
crush update --version 0.9.0    # pin (the only way to downgrade)
crush update --dir ~/opt/crush  # default: crush's own dir if crush-installer manages it, else ~/.crush/bin
```

**File types and editors.** So file managers, `file(1)` and editors recognise `.crush`, `.cast.json`,
`.casm`, `.cvm1`, `capsule.toml` and the rest: `crush-installer install --desktop-integration`
(opt-in) registers the MIME types and `magic(5)` rules, and [`contrib/`](contrib/README.md) has
Vim/Neovim, Helix, Emacs and Zed support plus the tree-sitter queries.

All crates are released together at the workspace version in `Cargo.toml`.

| Command | What it does |
|---|---|
| `crush` / `crush repl` | interactive REPL |
| `crush run FILE` | run a `.crush` / `.casm` / `.cvm1` program |
| `crush build FILE` | compile to `.cvm1` |
| `crush debug FILE --step --inspect` | breakpoints by source line, step, watch, print (grant-gated) |
| `crush gui run FILE --ui` | run a program that draws a window with `ui.*` (needs `crush-gui`) |
| `crush doctor` | check the host runtimes polyglot blocks need |
| `crush update [--check]` | update to the newest GitHub release (needs `crush-installer`) |

## 🏗️ Repository Structure

```text
crates/
├── crush-errors/         # Unified error types (leaf crate)
├── crush-cast/           # CAST — Crush Abstract Syntax Tree IR
├── crush-cosm/           # COSM — Crush Object Semantic Model: Kind declarations, C3, validation (structs/impl/trait/@kind)
├── crush-caison/         # CAISON (formerly CSON) format + `caison.parse` VM capability
├── crush-cson-shim/      # retired `crush-cson` name: re-exports crush-caison
├── casm/                 # CASM — Crush Assembly bytecode format
├── tree-sitter-crush/    # Tree-sitter grammar for the Crush language
├── crush-diagnostics/    # Shared diagnostics/wire-format types
├── crush-index/          # Cross-crate symbol/reference index
│
├── crush-walker-core/    # LanguageAdapter trait + shared walker utilities (renamed from walker-core)
├── cli/                  # crush-walker — CLI that auto-detects source language and dispatches a walker
├── crush-lang-python/    # Python → CAST (bin: python_walker)
├── crush-lang-rust/      # Rust → CAST (syn-based; refuses what it can't prove exact)
├── crush-lang-js/        # JavaScript → CAST
├── crush-lang-go/        # Go → CAST
├── crush-lang-c/         # C → CAST
├── crush-lang-zig/       # Zig → CAST
├── crush-lang-bash/      # Bash → CAST
├── crush-lang-zsh/       # Zsh → CAST
├── crush-lang-wasm/      # WebAssembly walker — parses Wasm modules into CAST IR
├── crush-lang-dart/      # Dart → CAST
├── crush-lang-nepali/    # Nepali (Sona) → CAST
├── crush-lang-java/      # Java → CAST (skeleton)
├── crush-lang-custom/    # Custom/user-defined language adapter scaffold
│
├── crush-frontend/       # Crush language parser, semantic analyzer, optimizer, and CASM compiler
├── crush-lang-sdk/       # Rust SDK for hosting the CVM1 runtime (bins: crush, crushc, crush-run, crush-compile, crush-repl, crush-diff, crush-debug)
├── crush-vm/             # CVM1 bytecode assembler, disassembler, and sandboxed interpreter (incl. polyglot exec + capability gates)
├── crush-vm-capi/        # C API shared library for embedding CrushVM in C/C++ programs
├── crush-vm-py/          # Python extension module (PyO3) exposing the canonical CVM1 VM (outside the workspace)
├── crush-jit/            # Cranelift-based JIT backend (native execution tier)
├── crush-aot/            # AOT: CASM → Rust (rustc) or C (gcc/clang) → native .so (bins: crush-aotc, crush-walk-run)
├── crush-aotc/           # Typed AOT compiler library: CASM → C source → shared library (.so)
├── crush-ptx/            # CASM → PTX (NVIDIA GPU assembly) emission backend
├── crush-web/            # Browser WebAssembly runtime (outside the workspace; its pkg ships with each tag's pre-release)
├── crush-ffi/            # C-ABI plugin interface
├── crush-plugin-example/ # Example native plugin built on crush-ffi
├── crush-python/         # PyO3 bindings exposing crush-cast to Python
├── crush-debugger/       # Debugger engine over crush-vm (breakpoints, step, watch, grant-gated inspection); `crush debug` / `crush-debug`
├── crush-lint/           # Static lint pass over CAST/CASM
├── crush-macros/         # Proc-macros shared across the workspace
├── crush-net/            # Raw TCP/TLS sockets as `tcp.*` capabilities (`--tcp`, `--tcp-listen`)
├── crush-tui/            # Terminal UI front-end
├── crush-gui/            # Desktop GUI: `ui.*` capabilities on arniko/bliss (`crush gui run`, `--ui`; not a default member, needs fontconfig)
├── crush-pkg/            # Package manager — cargo-like build tool for Crush (uses buckets for sandboxed script runtimes)
└── crush-installer/      # Toolchain installer and uninstaller
```

## 🚀 Quick Start

### Building from Source

You need Rust 1.95 or newer (the MSRV that CI checks; `rust-toolchain.toml` selects `stable`). `crush-pkg` and `crush-vm` path-depend on
[`nixpt/buckets`](https://github.com/nixpt/buckets), so clone it next to this
repo first (CI does the same):

```bash
git clone https://github.com/nixpt/buckets ../buckets
```

```bash
# Build the core SDK and package manager (`cargo build` alone builds every
# default member; `crush-gui` is left out because it needs `libfontconfig1-dev`)
cargo build -p crush-lang-sdk -p crush-pkg

# Install to ~/.crush using the included installer
cargo run --bin crush-installer -- install --bin-dir target/debug
```

### Running Directly

```bash
# Check compiler options
cargo run --bin crushc -- --help

# Run a Crush script directly
cargo run --bin crush-run -- run my-program.crush
```

A first program, with what it prints (verified against `faeb86e8`, 2026-10-31):

```crush
fn make_counter() {
    let c = 0
    return || {
        c = c + 1
        return c
    }
}
let next = make_counter()
next()
print(next())
```

```text
2
```

Capabilities are opt-in per run. The standard library (`str.*`, `math.*`,
`json.*`, …) is on by default; everything that touches the host needs a flag
(`--fs`, `--net`, `--tcp`, `--env`, `--time`, `--process`, `--polyglot`, …), and a program
that needs an ungranted capability is refused before it runs.
`crush-run caps FILE` shows what a program needs without running it.
Raw sockets (`tcp.*`, crate `crush-net`) have their own grants: `--tcp` for
outbound client sockets and a separate, stronger `--tcp-listen` for being a
server; `--net` is HTTP-client only and grants neither.

## ⚙️ Compilation Pipeline

The Crush pipeline seamlessly converts polyglot source code down to execution-ready bytecode through an interconnected set of stages.

```mermaid
graph TD
    A[Source File<br/>.crush, .py, .rs, .go, .c, .js, .sh, .zig, .wasm] --> B
    B(Walker<br/>Language-specific tree-sitter → CAST) --> C
    C[CAST<br/>Crush AST — JSON] --> D
    D(Crush Compiler<br/>crates/crush-frontend) --> E
    E[CASM<br/>Crush Assembly — JSON / binary .castb] --> F
    F((CVM1 VM<br/>Execution))
    
    classDef file fill:#2d3436,stroke:#74b9ff,stroke-width:2px,color:#fff
    classDef process fill:#0984e3,stroke:#74b9ff,stroke-width:2px,color:#fff
    
    class A,C,E file
    class B,D,F process
```

## 🎮 Examples

[`examples/crush/`](examples/crush/) holds 50+ `.crush` programs, from language-feature smoke
tests (`arithmetic.crush`, `control_flow.crush`, `exception_test.crush`, ...) to full self-playing
programs written by several different LLMs given the same "learn this language" prompt — two
hosted-language interpreters (`forth.crush`, `brainfuck.crush`), self-playing games
(`tictactoe.crush`, `lights_out.crush`, `pong.crush`, `breakout.crush`), an IDA*-based 15-puzzle
solver (`fifteen_puzzle.crush`), and Conway's Game of Life. The story behind that particular set —
which model wrote what, and the real language constraints/bugs each one hit — is documented in
[`nixpt/awesome-crush`](https://github.com/nixpt/awesome-crush).

## 📚 Documentation

- [The Crush Language Guide](https://github.com/nixpt/crush-language-guide) ([docs.crushlang.org](https://docs.crushlang.org)) — language reference, examples, CAST/CASM specifications. Every example in it is run against a pinned crush-ast commit, and its `examples/verified/` corpus keeps one small program per language behaviour with its exact output.
- Comprehensive crate-level documentation is available in the respective `crates/*/README.md` files.

## ⚖️ License

Licensed under either of [MIT](LICENSE-MIT) or [Apache License 2.0](LICENSE-APACHE) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this work shall be dual-licensed as above, without any additional terms or conditions.
