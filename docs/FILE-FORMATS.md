# Crush file formats

Every file the Crush toolchain reads or writes, one extension per format
(#365). The CLIs decide what a path holds in one place,
`crush_lang_sdk::formats` (`crates/crush-lang-sdk/src/formats.rs`).

```text
.crush ──walker/parser──▶ CAST (.cast.json | .castb)
                              │ crush-frontend
                              ▼
                         CASM (.casm JSON | .casmb MessagePack)
                              │ casm_to_vm
                              ▼
            CVM1 assembly (.cvms) ◀─disassemble / assemble─▶ CVM1 bytecode (.cvm1)
```

## The table

| Extension | Content | Encoding | Magic / version constant and gate | Written by | Read by |
|---|---|---|---|---|---|
| `.crush` | Crush source | UTF-8 text | none | you | `crushc`, `crush-run`, `crush-debug`, `crush-repl`, `crush-pkg` |
| `.cast.json` | CAST IR (`crush_cast::Program`) | JSON, pretty-printed | `"cast_version"` field; writers write `crush_cast::CAST_VERSION` = **`"0.1.0"`**; load fails unless the **major** matches (0) (`crates/crush-cast/src/pack.rs`) | `crushc --emit cast`, `crushc cast unpack`, the walkers' CAST, `examples/cast/build_*.py` | `crushc`, `crush-run`, `crushc cast …`, `crush_cast::Program::load` |
| `.cast` | CAST IR | JSON (same as `.cast.json`; exosphere's name) | as `.cast.json` | exosphere | `crushc`, `crush-run`, `crushc cast …` |
| `.castb` (`.cbor`) | CAST IR | CBOR (`cbor4ii`), no header | as `.cast.json` (the field is inside the CBOR) | `crushc --emit cast -o x.castb`, `crushc cast pack` | `crushc`, `crush-run`, `crushc cast …` |
| `.casm` | CASM IR (`casm::Program`) | JSON, pretty-printed | `"version"` field = `casm::CASM_VERSION` = **`"1.0"`**; load fails unless the major matches (1) (`crates/casm/src/lib.rs`) | `crushc --emit casm`, `crush-pkg build` (`target/<name>.casm`) | `crush-run`, `crush-compile`, `crush-pkg` payloads, `casm::Program::load` |
| `.sno` | CASM IR | JSON (as `.casm`) | as `.casm` | Sona (`nixpt/sona`) compiling a `.sn` program | `crush-run`, `crush-pkg` payloads — an alias of `.casm` |
| `.casmb` | CASM IR | MessagePack (`rmp_serde`, positional), first line `#!/usr/bin/env crush run` | as `.casm` | `casm::Program::save` | `crush-run`, `crush-compile`, `crush-pkg` payloads. **Can't hold a program with functions yet** (#372); `crushc` refuses to write one |
| `.cvms` | CVM1 assembly (`.func`, `PUSH 40`, `CAP_CALL "io.print" 1`, …) | UTF-8 text; line comments start with `;` or `#` (not `//`) | none; `.cap NAME` lines declare the manifest permissions, `.cosm <json>` the kind table | `crushc --emit cvms`, `crush_vm::disassemble` | `crush-compile`, `crush-run`, `crush_vm::assemble` |
| `.cvm1` | CVM1 bytecode (`crush_vm::Program`) | binary: `CVM1` · version byte · manifest JSON · consts · code (· kind table) | magic **`CVM1`** (bytes 0–3), then the version byte (byte 4): writers emit **`2`** (`VERSION_NO_KIND_TABLE`) for a program without a kind table — most programs — and **`3`** (`VERSION_KIND_TABLE`) for one with a kind table (structs/`impl`/`trait`/`@kind`); readers accept `MIN_VERSION`..=`VERSION` = **1..=3** and refuse anything else (`crates/crush-vm/src/bytecode.rs`). `VERSION` (3) is the newest the runtime *reads*, not what every write carries | `crushc` (default), `crush-compile`, `crush-pkg build` (`target/<name>.cvm1`) | `crush-run`, `crush-gui`, `crush-pkg` payloads and bundles |
| `.cvm` | CVM1 bytecode | the same blob as `.cvm1` | as `.cvm1` | `crush-pkg build` before #365 | every `.cvm1` reader (kept as an alias, no note) |

Compiling the same source twice gives byte-identical `.cvm1`, `.casm` and
`.cvms`: the CVM1 manifest's function table and every permission list are
written in name order (#365; before, `HashMap` order made two compiles of one
file differ, which is why the committed `examples/crush/fibonacci.cvm1` didn't
match a fresh compile). The two committed example blobs were regenerated —
the old `arrays_and_loops.cvm1` looped back to its start until the output
quota tripped.

### Deprecated, accepted for one release with a `note:` on stderr

| Name | Was | Now |
|---|---|---|
| `.casm.json` | CASM JSON (and crush-pkg's unloadable summary dump) | `.casm` |
| a `.casm` whose first non-blank byte isn't `{` | CVM1 assembly text (`crushc --emit casm` before #365; exo-light's `examples/golden.casm`, nixpt/exo-light#12) | `.cvms` |

The note is printed in text mode only; `--message-format json` keeps stderr pure NDJSON.

## Output files: one rule

For `crushc`, `crush build` (which is `crushc`) and `crush-compile`:

1. An **artifact** emit writes a file by default, `<input stem>.<ext>` beside
   the input, and prints the path and the bytes written:
   `--emit vm` → `.cvm1` (the default), `--emit casm` → `.casm`,
   `--emit cvms` → `.cvms`, `--emit cast` → `.cast.json`;
   `crush-compile x.cvms` → `x.cvm1`. The stem drops a compound extension:
   `crushc x.cast.json` writes `x.cvm1`.
2. A **view** goes to stdout by default: `--emit ast` (the parsed program
   rendered back as Crush source — not CAST; use `--emit cast` for CAST).
3. `-o FILE` overrides; **`-o -` is stdout** for every emit
   (`crushc x.crush --emit casm -o - | jq .manifest`). With `-o`, the
   extension picks the encoding where there are two: `-o x.castb` writes CBOR
   CAST.
4. `--emit types` is not implemented (#371) and says so.

## Capabilities in the text form

CVM1 assembly carries its manifest permissions as `.cap` lines, written by
the disassembler and read back by the assembler (merged with any `--cap`):

```text
; capabilities (manifest permissions)
.cap io.print
.func main
    PUSH 40
    PUSH 2
    ADD
    CAP_CALL "io.print" 1
    PUSH_NULL
    RET
```

So `crushc x.crush --emit cvms`, then `crush-compile x.cvms`, then
`crush-run run x.cvm1` runs a program that prints, as compiling it directly
does. (Chosen over having `crush-run` infer capabilities from the text: the
grant then travels with the file, as it does in `.cvm1` and `.casm`, and
`crush-compile` produces the same manifest `crushc` does.) A `.cvms` written
by hand declares what it uses the same way, or is granted with `--cap`.

## CAST version

`crush_cast::CAST_VERSION` is `"0.1.0"`. Every producer in this repository
writes the constant: the Crush parser (`crates/crush-frontend/src/parser/mod.rs`),
`mutation_check`, the language lowerers (`crush-lang-{c,custom,go,js,python,rust,wasm,zig}`,
`crush-walker-core`), the SDK's codebase walker, REPL and `compile` module,
and `crushc --emit cast`. The gate is unchanged: any `0.x` loads, so CAST
written as `"0.1"` or `"0.2"` by older tools still does. Two tests hold this:
`crates/crush-cast/tests/repo_cast_files.rs` loads every `.cast.json` in the
repository through `Program::load` and checks its version, and rejects a
literal `cast_version: "…"` in any crate's `src/`.

## Tools

```text
crushc cast validate FILE            schema + version check (.cast.json, .cast, .castb)
crushc cast pack FILE [-o OUT]       CAST JSON → .castb
crushc cast unpack FILE [-o OUT]     .castb → CAST JSON (stdout without -o)
crushc cast format FILE [--in-place] canonical JSON
crushc cast diff LEFT RIGHT          semantic diff; exit 1 when different
```

`crush cast …` forwards to the same tool. Ported from exosphere's
`ant crush cast` handler.

## Not here

ECASM, exosphere's `CachedProgram` / `LoweredProgram` codecs, CBV and ICBF
belong to exosphere (or are dead) and aren't read by this toolchain. The
CASM JSON schema is `crates/casm/SCHEMA.md`; the CAST JSON schema is
`docs/cast/schema-reference.md`.
