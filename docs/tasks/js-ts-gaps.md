# JS/TS Frontend Gap Map

Status as of s289 (2026-06-17). All gaps identified via audit of
`crates/crush-lang-js/`. 14+27 = 41 tests pass on both backends.

---

## HIGH — produces wrong CAST output

- ~~**Switch statements silently dropped** (Boa) — `lower_boa.rs:168-171`~~ ✅ FIXED — lowered as if-else chain
- ~~**For-loop `update` discarded** (swc) — loops lose increment at `lower_swc.rs:278`~~ ✅ FIXED — appended as ExprStmt
- ~~**`new X()` → `NullLiteral`** (Boa) — `lower_boa.rs:335-338`~~ ✅ FIXED — lowered as `new_<name>` Call
- ~~**Optional chaining `a?.b` → `NullLiteral`** (Boa) — `lower_boa.rs:462-465`~~ ✅ FIXED — lowers the inner expression
- ~~**Named exports / `export *` silently dropped** (swc) — `lower_swc.rs:223`~~ ✅ FIXED — ExportNamed/ExportAll/Ts* handled
- ~~**Class default exports → LangBlock** (swc) — `lower_swc.rs:187-213`~~ ✅ FIXED — DefaultDecl::Class lowered

## MEDIUM — lossy or wrong for important patterns

- ~~**Tagged templates hard-error** (swc) — `lower_swc.rs:762-767`~~ ✅ FIXED — lowered as `Call { function: tag_name, args: [quasis..., exprs...] }`
- ~~**JSX fully errors** (swc) — `lower_swc.rs:879-882`~~ ✅ FIXED — lowered as `__crush_jsx__` call; JSXText as string literal
- ~~**`with` inconsistent** — hard-error (swc) vs silent drop (Boa)~~ ✅ FIXED — swc now returns `Ok(None)` to match Boa
- ~~**Class exports → LangBlock** (swc) — `ExportDecl` path~~ ✅ FIXED (was in HIGH batch)
- ~~**TS decls (interface/type/enum/module) silently skipped** — `lower_swc.rs:246`~~ ✅ FIXED — return `Ok(None)` gracefully
- ~~**`using` decls silently skipped** (stage 3) — `lower_swc.rs:246`~~ ✅ FIXED — lowered as `VarDecl`
- ~~**Dynamic `import()` hard-errors** — `lower_swc.rs:957`~~ ✅ FIXED — lowered as `Call { function: "import" }`
- ~~**Boa exports not lowered** — `ExportDeclaration` type private~~ ✅ SKIPPED — known limitation, already handled gracefully

## LOW — graceful degradation / missing analysis

- No `uses_typescript` / `uses_await` / `uses_generators` / `uses_arrow_functions` / `uses_destructuring` / `uses_optional_chaining` / `uses_spread` in `FeatureReport`
- Boa object literal drops spreads and method definitions (`lower_boa.rs:390-406`)
- Boa `ClassExpression` returns `NullLiteral` (`lower_boa.rs:454`)
- Sequence expressions drop intermediate values (swc side-effect-only, `lower_swc.rs:720-726`)
- `swc-backend` feature flag is a no-op (`Cargo.toml:15`)
- Complex call expressions hard-error (swc, `lower_swc.rs:916`)
- Walker binary has no stdin mode, panics on missing args (`src/bin/walker.rs:7`)
