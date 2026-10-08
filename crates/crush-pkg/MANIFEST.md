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

`platforms` is a claim made by the author. crush-pkg doesn't check it
against the capabilities the program uses. For example, a `web` capsule that
calls `fs.read` will still be refused at run time in the browser, which only
grants `io.print` and the pure string/conversion built-ins.
