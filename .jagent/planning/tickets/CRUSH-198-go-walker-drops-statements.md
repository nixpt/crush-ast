# CRUSH-198 — Go walker silently drops `:=`, `var`, `for`, assignment, `switch`, `defer`

| Field | Value |
|-------|-------|
| **ID** | CRUSH-198 |
| **Priority** | P0 |
| **Status** | Backlog |
| **Phase** | M6 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

`crush-lang-go/src/lib.rs:~91` matches `"short_variable_declaration"`, but
tree-sitter-go calls the node `short_var_declaration`, so it never matches. The
catch-all `_ => Ok(None)` (~line 159) then silently drops `for`, `switch`, `var`,
assignment, `++` and `defer`; unknown expressions become `NullLiteral` (~line 256).
The Go README claims "For Loops ✅ all variants" and "Variables ✅".

- `func main(){ a := 7; fmt.Println(a) }` → `Error: load from uninitialised slot 0`.
- `switch x { case 2: fmt.Println("two") }` → no output; CAST body empty.

## Success criteria

- [ ] Fix the node name; implement or loudly reject every statement kind (no `_ => Ok(None)`).
- [ ] Unknown expressions are an error, not `NullLiteral`.
- [ ] README feature table matches reality; output tests against `go run`.
