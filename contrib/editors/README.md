# Editor support

All text-editor kits use the grammar in `crates/tree-sitter-crush` and its `queries/`.
Defaults everywhere: **4 spaces** (matches the polish formatter), `//` line comments (the lexer
also accepts `#`), `/* … */` block comments.

| Editor | Files | Covers |
|---|---|---|
| Vim / Neovim | [`vim/`](vim) | filetype detection for every text format, `ftplugin` (comment string, 4-space indent) |
| Neovim + tree-sitter | below | highlighting, indents, injections via nvim-treesitter |
| Helix | [`helix/languages.toml`](helix/languages.toml) | language + grammar entry |
| Emacs | [`emacs/crush-mode.el`](emacs/crush-mode.el) | `crush-mode`, `crush-asm-mode` (.cvms), `crush-ts-mode` (Emacs 29+) |
| Zed | [`zed/`](zed) | extension: language config, grammar, queries |
| VS Code | [`nixpt/crush-vscode`](https://github.com/nixpt/crush-vscode) | TextMate grammar, `.cvms`, JSON schemas |

Sublime Text / TextMate: the TextMate grammar is `syntaxes/crush.tmLanguage.json` in crush-vscode
(MIT OR Apache-2.0, same licensing as this repo). Sublime reads `.tmLanguage` plists, not JSON, so it
needs a conversion step that is not shipped here.

## Vim / Neovim

```bash
# Vim
cp -r contrib/editors/vim/* ~/.vim/                # or ~/.config/nvim/ for Neovim
```

Neovim tree-sitter (nvim-treesitter `main`/`master`):

```lua
vim.api.nvim_create_autocmd("User", {
  pattern = "TSUpdate",
  callback = function()
    require("nvim-treesitter.parsers").crush = {
      install_info = {
        url = "https://github.com/nixpt/crush-ast",
        location = "crates/tree-sitter-crush",
        files = { "src/parser.c" },
        queries = "queries",            -- picks up highlights/indents/locals/injections
      },
    }
  end,
})
vim.treesitter.language.register("crush", "crush")
vim.api.nvim_create_autocmd("FileType", { pattern = "crush", callback = function() vim.treesitter.start() end })
```

then `:TSInstall crush`. (On the older `master` branch use `require("nvim-treesitter.parsers").get_parser_configs().crush = { install_info = {...}, filetype = "crush" }` and copy `queries/*.scm` to `~/.config/nvim/queries/crush/`.)

## Helix, Emacs, Zed

See the header comment of each file for the install steps. For Zed, `zed/sync-queries.sh`
re-copies the queries into the extension (Zed packages extensions standalone); install with
"zed: install dev extension" pointing at `contrib/editors/zed`.

## Verification status

Checked: Vim filetype/ftplugin results (real `vim`); the queries (real `tree-sitter query` 0.25 over
every `examples/crush/*.crush`, plus a `cargo test`); `languages.toml` parses as TOML; the elisp is
paren-balanced. **Not run** (editors not installed on the authoring box): Neovim, Helix, Emacs, Zed.

## .editorconfig

```ini
[*.{crush,cvms}]
indent_style = space
indent_size = 4
```
