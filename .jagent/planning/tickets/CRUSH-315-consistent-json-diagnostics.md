# CRUSH-315 — One JSON diagnostic schema, one record per error, across the CLIs

| Field | Value |
|-------|-------|
| **ID** | CRUSH-315 |
| **Priority** | P1 |
| **Status** | Done (pending review) |
| **Assignee** | nova |
| **Dependencies** | CRUSH-74 (source locations in AST meta) |
| **Estimated effort** | M |
| **Filed by** | nova — 2026-10-30, branch `agent/nova/CRUSH-315` |

## Problem

`crush-diagnostics` documents "one schema across the entire CLI surface"
(`code, level, file, line, col, message, hint`), but the same broken program
produced different records from different tools (debug builds, main `dcd5c823`):

| program | `crushc --message-format json` | `crush-run run --message-format json` |
|---|---|---|
| syntax error, line 2 | `E-PP01`, file, line 2, col 13 | `E-IO`, file/line/col `null`, text "Parse errors: …" |
| `print(undefined_name)` | `E-TP01`, **line/col `null`** | `E-IO`, everything `null` |
| `print(xs[5])` out of range | compiles | `E-RT05`, **`message` empty**, text in `hint` |
| `print(fs.read("x"))` ungranted | compiles | `E-IO`, "needs capabilities … --fs: fs.read" |

and the wire struct was defined twice (`crush_diagnostics::DiagRecord` and
`crush_lang_sdk::theme::JsonDiagnostic`).

## Root causes

1. `crush-run` flattened every compile failure to an `anyhow` string
   (`parse_source` joined the parse errors into `"Parse errors: …"`), so it
   could only ever emit the generic `E-IO`.
2. The semantic analyzer returned plain `anyhow` errors and never read the
   `meta` CRUSH-74 put on the AST — and the parser stamped `meta` from the
   *current token after* the node was parsed (the token that follows it), so
   even if read, the position was that of the next token, not the construct.
3. Two copies of the wire shape.

## Change

- `crush-diagnostics`: owned `Diagnostic` (the single wire definition beside
  `DiagRecord`; byte-identical, pinned by a test); `CODE_*` constants.
- `crush-lang-sdk`: `JsonDiagnostic` is now `pub type … = crush_diagnostics::Diagnostic`;
  constructors needing sdk/parser types are on `JsonDiagnosticExt`;
  `theme::diagnostics_for_error` maps any compile/run failure to its records,
  used by both `crushc` and `crush-run` (and the REPL).
- `crush-frontend`: `ParseErrors` and `SemanticError` (position optional) are
  typed `anyhow` payloads with unchanged `Display`; the analyzer attaches the
  position of the innermost construct that has one; the parser stamps `Var`,
  `Call`, `BinaryOp` (operator) and statement `meta` at the construct itself.
- `crush-run`: syntax → `E-PP0n`, semantic → `E-TP01`, runtime → `E-RT0n` with
  the text in `message`, ungranted capability → `E-CAPS` (existing code, used by
  `crush-pkg check` for capability findings; hint names the grant flag). The
  program path is attached to every record.

## Success criteria

- [x] One wire definition; sdk's `JsonDiagnostic` aliases it.
- [x] `crushc` and `crush-run` emit byte-identical records for programs that
      fail to compile (golden corpus `crates/crush-lang-sdk/tests/diagnostics_corpus`).
- [x] Runtime errors carry their text in `message` (hint empty); capability
      refusals have a dedicated code.
- [x] Undefined variable / function and type errors carry line/col.

## Not done (precisely)

- **Runtime errors have no line/col.** The VM raises `VmError` without a
  program counter→source mapping; the compiler records lines only for some
  instructions (`compiler.rs` ~1195/1251). Needs a pc→line table surfaced in
  `RuntimeError`. `file` is filled.
- **Semantic errors raised from an expression kind the parser does not stamp
  at its own start** (field access, index, method call, lambda, literals…) are
  located by the nearest enclosing statement/expression that has a position,
  not their own. Statement-level and the kinds above are exact.
- `crush-pkg` / `crush-installer` compile whole capsules under their own domain
  codes (`E-BUILDER`, `E-RUN`) and a concatenation of sources, so a position
  has no single file to name; left as is and excluded from the golden matrix.
