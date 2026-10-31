# contrib/ — file types and editor support (CRUSH-366)

Everything that makes the OS and your editor recognise Crush's file formats.

## The formats

| Extension | Content | MIME type | Identified by |
|---|---|---|---|
| `.crush` | Crush source (UTF-8) | `text/x-crush` | glob |
| `.cast.json` (legacy `.cast`) | CAST syntax tree, JSON (`"cast_version"` key) | `application/x-crush-cast+json` | glob |
| `.castb` | CAST, CBOR | `application/x-crush-cast+cbor` | glob (CBOR has no header) |
| `.casm`, `.sno` (legacy `.casm.json`) | CASM IR, JSON | `application/x-crush-casm+json` | glob |
| `.casmb` | CASM, MessagePack behind `#!/usr/bin/env crush run\n` | `application/x-crush-casm+msgpack` | glob + magic |
| `.cvms` | CVM1 assembly text | `text/x-crush-cvms` | glob |
| `.cvm1` (legacy `.cvm`) | CVM1 bytecode, `CVM1` + version byte | `application/x-crush-cvm1` | glob + magic |
| `capsule.toml` | crush-pkg manifest | `application/x-crush-capsule+toml` | exact filename |
| `.cap`, `.crush-pack`, `.ecap` | capsule packages (zip; `.ecap` may be bincode) | `application/x-crush-package` | glob |

Sona source (`.sn`) is a separate language and not covered.

## What is here

| Path | What |
|---|---|
| [`mime/crush.xml`](mime/crush.xml) | freedesktop **shared-mime-info** package: globs, `sub-class-of`, magic, icons. Used by GNOME Files, Nautilus, KDE (Dolphin), Thunar, `xdg-mime`, `gio`. |
| [`magic/crush.magic`](magic/crush.magic) | **`magic(5)`** rules for `file(1)`: CVM1 (prints the container version), CASM MessagePack, CAST/CASM JSON. |
| [`editors/`](editors/README.md) | Vim/Neovim, Helix, Emacs, Zed. |
| [`../crates/tree-sitter-crush/queries/`](../crates/tree-sitter-crush/queries) | tree-sitter `highlights`, `indents`, `locals`, `injections` (used by the editors above and by GitHub-style highlighters). |

VS Code lives in [`nixpt/crush-vscode`](https://github.com/nixpt/crush-vscode) (languages for `.cvms`,
`capsule.toml`, JSON schemas for CAST/CASM).

## Install

**Linux, one command (opt-in):**

```bash
crush-installer install --desktop-integration
```

copies `crush.xml` to `$XDG_DATA_HOME/mime/packages/` (default `~/.local/share/mime/packages/`),
runs `update-mime-database` if it is on `PATH`, and puts the magic file at
`$XDG_DATA_HOME/crush/crush.magic`. It prints how to hook that into `file(1)`; it does **not**
edit `~/.magic` or any system directory. `crush-installer uninstall` removes exactly what was written.

**By hand:**

```bash
mkdir -p ~/.local/share/mime/packages
cp contrib/mime/crush.xml ~/.local/share/mime/packages/
update-mime-database ~/.local/share/mime        # system-wide: /usr/share/mime, as root
xdg-mime query filetype hello.crush             # -> text/x-crush
# file(1):
cat contrib/magic/crush.magic >> ~/.magic       # or: file -m contrib/magic/crush.magic FILE
```

Verify: `gio info -a standard::content-type FILE`. A `.cvm1` renamed to no extension is still
`application/x-crush-cvm1` (magic). Note `xdg-mime query filetype` only consults the MIME database
when a desktop (`XDG_CURRENT_DESKTOP`, e.g. `GNOME`) is detected; on a bare shell it falls back to
`file --mime-type`, which knows nothing about Crush — use `gio` there.

**`file(1)` and JSON.** file 5.35+ detects JSON before it consults magic rules, so plain
`file x.cast.json` still says "JSON text data"; the CAST/CASM JSON rules fire with `file -e json`.
The binary formats (`.cvm1`, `.casmb`) work with plain `file`.

**macOS / Windows.** The XML and magic are Linux/freedesktop and `file(1)` concepts. macOS UTIs and
Windows registry associations are not provided yet.

## GitHub Linguist

Crush is **not** a Linguist language and this repository does not pretend it is. Linguist
([contributing guide](https://github.com/github-linguist/linguist/blob/main/CONTRIBUTING.md))
requires a language to have a unique, in-use extension and **use in at least 200 distinct public
repositories** (and a TextMate/tree-sitter grammar under a compatible licence, plus samples) before
it will be added. When the ecosystem reaches that, the PR needs: `lib/linguist/languages.yml`
entry (`.crush`, `tm_scope: source.crush`), the grammar from
[`crush-vscode/syntaxes`](https://github.com/nixpt/crush-vscode) (MIT OR Apache-2.0), and a handful of
samples from `examples/crush/`. Until then `.gitattributes` only marks compiled artifacts
(`*.cvm1`, `*.castb`, `*.casmb`) as `binary linguist-generated`.

## Maintaining

`crates/crush-installer/assets/` embeds copies of `crush.xml` and `crush.magic`; a unit test fails if
they drift from `contrib/`. After editing: `cp contrib/mime/crush.xml contrib/magic/crush.magic crates/crush-installer/assets/`.
The queries are checked by `cargo test -p tree-sitter-crush --test queries`.
