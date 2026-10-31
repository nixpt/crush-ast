# CRUSH-366 — File-type identifiers and editor support

| Field | Value |
|-------|-------|
| **ID** | CRUSH-366 |
| **Priority** | P2 |
| **Status** | In review |
| **Assignee** | nova |
| **Dependencies** | file-extension names from #365 (satya, in flight) |

## Problem

Crush's file formats (`.crush`, `.cast.json`, `.castb`, `.casm`, `.casmb`,
`.cvms`, `.cvm1`, `capsule.toml`, `.cap`/`.crush-pack`/`.ecap`) are not
recognised by file managers, `file(1)`, editors or IDEs.

## Success criteria

- [x] freedesktop shared-mime-info package `contrib/mime/crush.xml` (globs, sub-classes, magic for `.cvm1` / `.casmb`).
- [x] `magic(5)` file `contrib/magic/crush.magic`.
- [x] tree-sitter queries (highlights, indents, locals, injections) validated against every `examples/crush/*.crush`.
- [x] Editor kits: Vim/Neovim, Helix, Emacs, Zed.
- [x] `crush-installer install --desktop-integration` / `uninstall` handle MIME + magic.
- [x] Linguist path documented (no faked language).
- [x] `contrib/README.md`.

## Notes

Format code is not touched here; mismatches seen while writing the magic are
listed in the PR body for #365.
