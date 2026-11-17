# Visibility and packages for Crush (CRUSH-509): design note

altair (crush lane), 2026-11-14, asked for by the captain through vega. **Design only.** The captain ruled on
all six decisions on 2026-11-14 (see **Rulings**). **Status: V1 (#519) implemented**: §1 stage 1 as
described in `docs/design/import-system.md` "Visibility"; where the code had to choose, that section says
what it does (notes only for a module another package uses; the first-`pub` listing rides on the first
`W-VIS01` per module). The 1.0 feature freeze is 2026-12-31. Every decision below has options and a recommendation. The ones the captain
must rule on are collected under **Decisions**.

## What exists today (checked on main 30d6d581)

*These facts are the state on 2026-11-14, before V3a (#529) merged. Since #529, `MemberDecl` rejects
unknown keys (`check_keys`), `cosm_version` is read on every load, and a versioned kind table is written as
`kt_format 2`. Where this note says "today" about those three (this table, §2 "Schema change", and the
advisor's NOT CHECKED list), read "before #529"; decision 7 below describes what was built.*

| Fact | Where | Checked how |
|---|---|---|
| `pub fn` and `pub async fn` parse, and `pub` is thrown away. `pub` is not a keyword: the parser matches the identifier `pub` before `fn`. | `crates/crush-frontend/src/parser/mod.rs:648-700` | read |
| `pub struct …` and a `pub x` field do not parse. `pub struct P {…}` gives `Undefined variable: pub`; `struct P { pub x: Int }` makes a field named `pub`. | prebuilt crush-run @7e6696db | probe |
| Every function of a module is importable, qualified (`a.f`) or selectively (`import a { f }`), including `__` names. | `docs/design/import-system.md` "Not yet"; probe: `a.__hidden()` and `import a { __hidden }` both run | probe |
| `__` is only a convention: such names are hidden from the "it defines: …" lists. facet has 38 `__facet_*` helpers. polish imports `__polish_clen` and `__polish_finding` from its own `lint` module. | `facet.crush`, polish `src/main.crush` | read |
| **Crush-pkg dependencies are already import roots.** CRUSH-367 (f17d6517, 2026-10-31): `import facet` loads the dependency's entry, `import facet.env` loads `env.crush` in its root, and a package name wins over the search path. `import-system.md` still lists this as "Not yet"; that line is stale. | `crush-frontend/src/import_system.rs:17-24, 397-420`; `crush-pkg/src/builder.rs:475-486, 595-625` | read |
| crush-pkg still has a legacy **concatenation mode** for an entry with no `import`. | `builder.rs:484-520` | read |
| Kind names (`struct`, `trait`) are **one global namespace**, and they are not mangled. | `import-system.md` "Module contents" | read |
| **A kind-name clash is not caught.** `a.crush` and `b.crush` each define `struct P` with different fields. Importing both compiles, and `a.make_a()` fails **at run time** with `` `P` is sealed: it has no field `x` (fields: y) ``. The later definition silently replaces the earlier one in the kind table. A clash in their `impl` methods *is* caught (`duplicate member`), because lifted method names collide. Filed as #515. | probe (`m2.crush`, `m4.crush`; the repro is in #515) | probe |
| A struct literal of an imported struct does not parse in the importer (`P { x: 5 }` gives `Undefined variable: P`). The parser recognises a literal only for structs declared in the same file. | probe `m1.crush`; `import-system.md` "Not yet" | probe |
| Fields are writable from anywhere: `p.x = 9` on an imported struct works. | probe `m3.crush` | probe |
| COSM `FieldDecl` is `{name, type, default, span}`, and `MemberDecl` is `{name, span, member}`. Neither has visibility. `CosmModule`, `KindDecl` and `FieldDecl` are `deny_unknown_fields`. **`MemberDecl` is not**: it is `#[serde(flatten)]` over a tagged enum, so by serde's rules an unknown key on a member is **ignored**, not rejected (from serde's documented behaviour; not yet tested here). `KindDecl` has no module or package field. | `crates/crush-cosm/src/schema.rs:88-152, 234-242` | read |
| The kind table is embedded in CVM1 v3 as `kt_format(1) \| len \| canonical COSM JSON`. An unknown `kt_format` is an error. `COSM_VERSION = "0.1.0"` exists, but **nothing reads it**: the embedded JSON carries no version. | `crush-vm/src/bytecode.rs:6-24`; `crush-cosm/src/lib.rs:48` (its only use) | read, grep |
| GET/SET_MEMBER carry only a member name (a u16 const-pool index). The VM has no notion of *who* is asking. FastVM, JIT and AOT run a **map-compatible** kind table as plain maps, and refuse others up front. | `bytecode.rs:36, 201-228`; `object-model.md` §9 (line 428) | read |
| `import_system.rs`'s own module docs say "the same Kind name declared twice is the usual duplicate-declaration error". The probe above shows that this is not true. | `import_system.rs:57-59` | read |
| A field may be named `priv` today (`struct P { priv: Int }` runs). tree-sitter-crush parses `Name { f: v }` by its **shape**, with no table of declared names; the frontend parser needs that table. | probe; `crates/tree-sitter-crush/grammar.js:550-565` | probe, read |
| No walker records privacy: there is no handling of JS `#x`, Rust `pub`/`visibility_modifier`, or Go exported names in `crates/crush-lang-{js,rust,go}`. K3 (walkers lower to COSM) has not started. | grep | grep |
| 1.0 rows this touches: **L3** (`pub` has no guide coverage), **L5** (imports and modules), **L6** (object model decided), **F7** (COSM evolution rule), **L4** (no silent-wrong results; the kind clash is near it). | `docs/planning/RELEASE-1.0-CRITERIA.md:78-81, 97` | read |

One constraint drives most of what follows: **Crush is dynamically typed.** In `fn f(o) { return o.x }` the
compiler does not know `o`'s kind. Module visibility (which *names* a file may call) can be checked fully
at compile time, because imports are resolved at compile time. Member visibility (which *fields* a value
exposes) cannot be checked at compile time in general. Only the VM sees the receiver's kind.

## 1. Module visibility

**The question.** What `pub` means for functions (and later kinds), whether private items are excluded from
selective imports, from qualified access, or both, what happens to `__`, and how existing code migrates.

| | Rule | Cost |
|---|---|---|
| **A** | Status quo: everything is importable, and `__` stays a convention. Document that `pub` is accepted and ignored. | Nothing to build. 1.0 ships with no encapsulation at all, and `pub` stays a lie in the grammar (L3 is "documented", but the doc says "does nothing"). |
| **B** | Per **module**, Python/ES-module style: without `pub`, a function is visible only inside its own file. | polish breaks at once (`main` imports `__polish_clen` from `lint`), and so does any multi-file program that shares helpers. Every such program needs `pub` on internal helpers, which then leak to every other package. |
| **C** | Per **package**, Go style with an explicit keyword: without `pub`, a function is visible to every module of the **same package**; `pub` exports it to other packages. | Needs a definition of "package" for plain `crush-run -L` compiles (below). No new keyword (no `pub(pkg)`). polish keeps working unchanged. facet must add `pub` to the names it exports (`facet_run`, `facet_fail`, `facet_parse`, …). |
| **D** | Rust style, both `pub` and `pub(pkg)`, with module-private as the default. | The most expressive option, and the most to learn and to migrate (every shared helper needs `pub(pkg)`). Crush has no nested modules, which are what makes Rust's default pay off. |

**What "package" means (needed by C).** A package is a property of the **file**, not of the import edge
that reached it. The file's canonical path is matched against the known roots, and the longest root that
contains it wins:
- the **root package** is the entry file's directory and everything under it;
- each crush-pkg package root (its entry's directory) is one package;
- each `-L` / `CRUSH_PATH` directory is one package.

So polish's `src/main.crush`, `src/lint.crush` and `src/lexer.crush` share a package however they import
each other, even when someone adds `-L .`. facet, found under a `-L` directory, is a different package. A
diamond import gives one answer, because the file decides and not the route.

*Amended by #552.* The canonical path decides only when it is inside a known root. A file that really is
under **no** root (reached through `lib/vendor -> ../outside` under `-L lib`, with `outside/` in no root)
used to fall into the root package: its private names were callable from the root package with no
`W-VIS01`, and the stage-2 gate passed a legacy module there. Such a file now takes the package of its
**found path**: the directory that resolved the import, made canonical (a search directory, a package root,
or the importing file's own found directory), then the module's segments as written, with no symlink below
that directory followed. It belongs to `lib`, like the same file in a real directory under `lib`.

A file that is inside a root is not affected: the file still decides, so a library file that another
package reaches through its own symlink stays the library's on every route and in every import order (the
first version of #552 let the route decide there, which made a library's own modules foreign to each other
once the app had imported one of them through a link; fixed on review). The route can matter in one case
only: two packages each hold a link to the same file under no root. The file is loaded once, keyed by its
canonical path, so it gets one package, that of the **first** import that loads it (source order, depth
first), and a use from the other package, through its own link too, is cross-package. That includes the
file's own neighbours: a sibling in the linked directory that is loaded through the other package's link
lands in that other package, so its use of the first file's private names gets `W-VIS01` (an error under
the stage-2 gate) although the two files sit side by side. The remedy is to make the directory a root of
its own (`-L outside`), which gives it one package on every route.

One rule has to be written down: several unrelated libraries in a single `-L` directory form one package and share their privates. The
remedy is a directory each, which is what crush-pkg already does.

Inputs with no path (the REPL, `parse_source` string entry points, crush-web) are the root package. The
package is recorded per merged function next to the existing `meta["source_file"]`. (The first draft keyed
this on the import edge, which made a program's own modules foreign to each other when reached through
`-L`. Fixed on the advisor's review.)

**Enforcement covers both access paths.** Qualified `a.f(…)`, the function value `a.f`, and selective
`import a { f }` all resolve through the same lookup, so a private name is refused on every path, with the
error naming the package and the rule. Allowing `a.__x()` while refusing `import a { __x }` would be a
loophole with no use.

**`__`.** It stays a naming convention for "internal, even inside the package". It keeps being hidden from
"it defines: …" lists, and it gets no rule of its own. Once C lands, `__` and the absence of `pub` say the same
thing to other packages, and `pub fn __x` gets a lint warning (exporting something you named internal).

**Migration.** There are three stages, and each is one ticket.
1. `pub` is recorded on functions (CAST `meta["pub"]`), and the parser accepts `pub struct` and `pub trait`. A
   cross-package use of a non-`pub` name is a **warning** `W-VIS01` naming the name, the package, and the fix.
   A module **with no `pub` anywhere** is treated as all-public (legacy) and gets one note per module, so
   nothing that builds today stops building. Adding the **first** `pub` to such a module makes every other
   name in it private to the package, so that one edit gets its own warning listing the names it just hid.
2. facet, crush-http, polish and the examples add `pub`. The awesome-crush and guide corpora are swept
   (the crush lane owns those).
3. At a pre-1.0 minor bump, `W-VIS01` becomes an error, and "no `pub` anywhere" stops meaning all-public.

**Recommendation: C**, staged as above. Its default fits what Crush programs do today (packages share
helpers freely), it adds no second keyword, and the "package" boundary falls out of resolution data the
compiler already has. If finer control is ever needed, D's `pub(pkg)` can be added later as a strict subset.

## 2. COSM visibility: fields and methods

**The question.** Whether fields and methods can be private or read-only; whether that is enforced at compile
time only or also by the VM (GET/SET_MEMBER); how walker languages map onto it; what it does to `FieldDecl`
and to F7; and when it lands relative to K2.

**Levels.** `pub` (read and write from anywhere), `readonly` (read from anywhere; write only from the owning
scope), `private` (read and write only from the owning scope). The **owning scope** is a property of the
*declaring language*, which COSM already models (home semantics):

| Home language | Owning scope | Who enforces it at its home |
|---|---|---|
| Crush | the package that declares the kind (as §1 C) | this design |
| Rust (`pub`, `pub(crate)`, private) | crate or module | rustc, statically, before the walker ever runs |
| Go (capitalised or not) | package | the Go compiler, statically |
| Python `_x` | nobody: convention only | not enforced (keep public) |
| Python `__x` | nobody: **name-mangled** to `_C__x`, not private | lower as a public member named `_C__x` (Python semantics require the mangling, not privacy) |
| JS `#x` | the **class body** (lexical) | the JS engine, **at run time** (`TypeError`) |
| Java/Dart `private`/`_x` | class / library | statically |

So for programs that came through a walker, static privacy was already checked by the source compiler. Only
**JS `#x`** needs run-time enforcement for fidelity, and only **Crush** needs a new rule.

| | Enforcement | Cost |
|---|---|---|
| **A** | None: visibility is metadata only (tools, docs, LSP). | Cheap, and honest about dynamic typing. Crush gets no real encapsulation, and JS `#x` cannot be faithful. |
| **B** | Compile time only: the compiler checks accesses where it knows the receiver's kind (`self` inside an `impl`, a local bound by a literal or constructor) and lets the rest through. | Catches the common mistakes inside a package, but it is **not a guarantee**: `fn f(o) { o.x }` bypasses it. A rule that holds "sometimes" is the kind of semantics L4 forbids. |
| **C** | B, plus the VM: GET/SET_MEMBER (and HAS/DEL/CALL_MEMBER, GET/SET_SLOT) check a member's visibility against the **owner package of the executing function**, a static fact stored per function. A private member costs one flag test plus one id compare; public members cost a flag test. | It is real, and it is the only option that makes JS `#x` faithful. It needs an **owner id** on every kind and every function in CASM/CVM1 (below), which is a format change. Every engine must implement it or refuse loudly. A kind with any non-public member must count as **not map-compatible**, or FastVM/JIT/AOT would run it as a plain map and silently drop the privacy. It must also close the **side doors** (next paragraph). |

**Side doors** that C must close or document as deliberate escapes:
- map-style indexing on a record (`p["x"]`) and key iteration (`collections.keys(p)`);
- printing and JSON of a record, and `clone`, structural `==` and hash across a private field;
- struct patterns that bind fields;
- a struct literal outside the package (a private field with no default makes the kind unconstructable
  there, which is the point, and needs a clear error);
- host capabilities that receive a record, and the debugger, which is a documented escape.

`readonly` is **shallow**: `p.z.push(1)` still mutates the array `z` holds. Say so wherever `readonly` is
documented.

**Default.** Fields and methods stay **public by default**. This matches every Crush program today
(`p.x = 9` works), Python and JS, and the Crush KindDecl's "today's behaviour: no existing program changes
meaning". It also means nothing migrates. Private and read-only are opt-in: `struct P { x: Int, priv y: Int,
readonly z: Int }`. The modifiers are **contextual**: a field may already be named `priv` (`struct P {
priv: Int }` runs today). `priv y:` is an identifier followed by an identifier, so the two never meet. The
spelling (`priv` or `private`) is the captain's call. This is deliberately the opposite default from §1: a
function you don't export is an implementation detail, while a field of a struct you hand out is data the
caller already holds.

**Schema change.**
- An optional `visibility` on `FieldDecl` and on `MemberDecl`: `"readonly"` or `{"private": "package" |
  "kind"}`, where `kind` is JS's class-body scope. It is omitted when public (`skip_serializing_if`), so
  every kind table written today stays the same byte for byte.
- An **owner** on `KindDecl`: `module` and `package` fields beside `name`. The same goes for each function
  in CASM/CVM1 (its package id). Without these, `{"private": "package"}` does not say *which* package, and
  the VM has nothing to compare. This is a CVM1/CASM format change that old runtimes must refuse by name
  (§4.5 of the 1.0 criteria: "new sections that old runtimes refuse by name" is a minor bump).

**F7, the evolution rule this forces (it comes first).** Today nothing says whether a field may be added,
or how a reader knows what it is reading. The strictness is also **not uniform**:
- `CosmModule`, `KindDecl` and `FieldDecl` reject unknown keys;
- `MemberDecl` (flattened) silently **ignores** them.

So a `visibility` on a *method* would be dropped by today's runtime without an error. Strict parsing alone
does not protect members. Options:
- **(i)** Drop `deny_unknown_fields`, so readers ignore unknown fields. This is wrong for visibility: an old
  runtime would silently drop a `private` and run the member as public. That is a security downgrade with
  no error, and it is what `MemberDecl` already does.
- **(ii)** Add `cosm_version` to `CosmModule`. A reader accepts the same major and a minor no greater than
  its own. An additive field bumps the minor, and a writer stamps the **lowest version that can express the
  module**, so a program with no new features still loads on an old runtime. Because `CosmModule` is strict,
  a runtime from before this change rejects any blob carrying `cosm_version`. To be safe, the same change
  also bumps **`kt_format` to 2**, which older readers already refuse by name. For 0.x the rule must compare
  the minor too (a major-only gate accepts every 0.x; see the F3 row).
- **(iii)** A `requires: ["visibility", …]` feature list instead of a linear minor, which gives a by-name
  refusal per feature. Slightly more schema; it reads better in an error message.

**Ruled (decision 7, 2026-11-14): (ii)**, with the one-time `kt_format 2` bump and with (iii)'s by-name
message as the error text. A purely additive or cosmetic field may be marked ignore-unknown, still gated by
the version or a `requires` list. Security-relevant fields (visibility first) are must-understand. Add a test that today's runtime refuses a blob with a private method, which pins down
`MemberDecl`'s lenience. F7 closes with the rule written into `crush-cosm`'s docs plus that test. **It must
land before any other schema-touching ticket**, including the owner fields and V3.

**Timing against K2.** The owner package is a static property of each function, so "the executing
function" is just the current frame. K2's re-entrancy API is not what blocks VM enforcement. **K2's
protocols** (str, eq, hash, iterate) are, because they are the side doors. The order stays the same, for
that reason:
- the F7 rule, the schema field and the owner ids go in **before K2**;
- VM enforcement (C) lands **with or after K2**, and **before K3's JS lowering**, which needs it for `#x`.

**What 1.0 can promise.** The 1.0 criteria (§4.5) make a change in a stable program's observable result a
**major** bump. Accepting `priv` without enforcing it in 1.0 and enforcing it in 1.x would be exactly that.
It would also repeat the "`pub` is a lie" problem §1 rejects. So the honest choices are:
- **C1:** enforce on the scheduler and portable VMs (the exact VMs) **before** the 1.0 freeze. The other
  engines are exact-or-loud.
- **C2:** ship **no** `priv`/`readonly` syntax in 1.0. Reserve the schema field and the contextual keywords,
  and list member visibility under the experimental COSM carve-out (§1.2 already lists `@kind`/COSM as not
  covered by 1.0). C follows in 1.x as a new feature.

**Recommendation: C**, public by default, delivered as **C2 unless K2 lands before the freeze**, in which
case C1. Metadata-only privacy (A), or compile-time-only checks presented as a rule (B), are the choices to
avoid. B's static checks can still run as **warnings** inside a package.

## 3. Kind namespacing

**The question.** Kind names (`struct`, `trait`, `@kind`) are one global namespace. Should they become
per-module?

| | Rule | Cost |
|---|---|---|
| **A** | Keep the global namespace, but make a clash a **compile error** that names both files. This is what `import_system.rs:57-59` already says happens. | Small: it fixes #515. Two packages can then never both have a `Config` or a `Point`, and that gets worse with every package. Programs with identical copy-pasted structs in two files stop compiling, so sweep the example, guide and awesome-crush corpora first. |
| **B** | Per-module kinds: `KindDecl` gets `module`/`package` fields (§2's owner), and `name` stays the display name. Resolution works like functions: `a.P { … }` / `import a { P }`, the bare `P` inside its own module; `IS_KIND` and struct patterns compare (package, module, name). Error messages print `a.P`. | Medium: frontend, kind table and debugger display. It is a schema change, so it follows F7. **Not** done by mangling the name string: `a::P::norm2` would collide with function `norm2` of a module `a.P`, and the `#2` suffix of a re-imported file would leak into kind identity. |

**Foreign `impl`.** Once kinds belong to a package, decide whether another package may write
`impl a.P { … }`. **Recommendation:** refuse inherent impls outside the declaring package, and allow trait
impls (`impl Shape for a.P`), which get no access to private members.

**Recommendation: A now, as a bug fix (after the corpus sweep), then B with the package work (§4).** A
needs no decision: it makes the code match its own documented rule. B is what makes `pub struct` mean
anything and what cross-file struct literals need. Without B, two packages' private `Node` kinds would
still collide.

## 4. Packages as the crate unit

- **Dependencies as import roots: already done** (CRUSH-367). Fix the stale "Not yet" line in
  `import-system.md`.
- **Retire concatenation mode, narrowly.** The trigger today is "the entry file has no top-level `import`"
  (`crush-pkg/src/builder.rs` `uses_imports`). That also describes a legitimate single-file package, which
  must keep working. Retire only the case that needs concatenation: a package with **more than one source
  file, or with dependencies**, whose entry has no `import`. It gets a deprecation warning, then an error at
  the same minor bump as §1 stage 3, since visibility can't be enforced across a concatenated program.
- **Directory modules.** Options:
  - (a) None: the package entry *is* the directory module (`import facet` is the entry, `import facet.env` is
    `env.crush`). This is today's behaviour.
  - (b) `a/b/mod.crush` (Rust 2015, Python `__init__`).
  - (c) `a/b.crush` beside an `a/b/` directory (Rust 2018).

  **Recommendation: (a).** A package already gives a directory a single entry point. Adding (b) or (c) makes
  two ways to say the same thing, and both need a rule for when `a/b.crush` and `a/b/mod.crush` both exist.
  Revisit only if a real program needs nested sub-packages.
- **`pub` per package or per module:** per package (§1 C). No `pub(pkg)`.
- **Cross-file struct literals.** Parse by **shape**, as tree-sitter-crush already does
  (`grammar.js:550-565`): `Name {` followed by `}` or by `ident :` is a struct literal, whether or not
  `Name` is declared in this file. Resolve the name after the imports are merged, and add the qualified
  form `a.P { … }` to **both** grammars. Parsing then never depends on another file, so the two parsers can
  keep agreeing (L2). The rejected alternative is pre-loading the imports so the parser knows their kind
  names; that makes the frontend's parse depend on other files, which tree-sitter cannot match. Literals
  work for **`pub`** kinds; a non-`pub` kind can be built only through its package's functions. This needs
  §3 B, and it closes the "struct literals of an imported struct" item in `import-system.md`.

## 5. Phasing

| Ticket | Delivers | Closes / moves |
|---|---|---|
| **V0** #518 (S) | Corpus sweep for duplicate kind names, then a kind-name clash is a compile error (§3 A). `import-system.md`: deps-as-roots marked done; `pub` documented as "accepted, not yet enforced". The guide gets a `pub` section. | #515; L3's `pub` item (the row stays 🟡: `capability`, `mcp`/external imports and `let mut` remain) |
| **V1** #519 (M) | §1 stage 1: `pub` recorded on `fn`/`struct`/`trait`, the per-file package rule, `W-VIS01`, the legacy all-public note and the first-`pub` warning | L5 (modules have an interface) |
| **V2** #520 (S, other repos) | `pub` added in facet, crush-http, polish, examples, the guide and awesome-crush | — |
| **V3a** #521 (S-M) | F7: `cosm_version` in `CosmModule`, `kt_format 2`, the reader gate and its by-name error, the lowest-version stamp, docs, and the private-method refusal test. **Before every schema-touching ticket.** | **F7** ✅ |
| **V3b** #522 (M) | Owner ids: `module`/`package` on `KindDecl`, and a package id per function in CASM/CVM1 (refused by name on old runtimes) | F4/F9 inputs (the format change is documented) ✅ (see "V3b as built") |
| **V4** #523 (M) | §3 B per-module kinds, the foreign-`impl` rule, and shape-parsed cross-file struct literals with `a.P { … }` in both grammars | L5 (the "Not yet" items); keeps L2 |
| **V5** #524 (M) | §1 stage 3, plus the narrow concatenation retirement: `W-VIS01` becomes an error at a minor bump | — |
| **V6** #525 (L, with or after K2) | §2: the `visibility` schema field and contextual `priv`/`readonly`. The VM enforces it on the scheduler and portable VMs. Kinds with private members are not map-compatible. The side doors are closed or documented. FastVM/JIT/AOT are exact-or-loud. Under C2, the syntax stays experimental until this ships. | L6 ✅ when combined with K2 |
| **K3** (existing) | Walker mapping per the §2 table: JS `#x` uses `private: kind`, Python `__x` is mangled, Rust/Go visibility is recorded as metadata | — |

V0 needs no decision and can start now. V1, V2, V4 and V5 need decisions 1, 3 and 4. V3a needs decision 5
(the F7 rule). V6 needs decision 2 and K2.

## Decisions (for the captain)

1. **Module visibility:** C (per package, with the package taken from the file's path; `pub` exports), with
   the staged migration (warning first, then an error at a pre-1.0 minor bump)?
2. **Member visibility:** C (VM enforcement, public by default, opt-in contextual `priv`/`readonly`),
   shipped as C2 (no syntax in 1.0, listed as experimental) unless K2 lands before the freeze, in which case
   C1 (enforced on the exact VMs before 1.0)? And the spelling: `priv` or `private`.
3. **Kinds:** per-module (§3 B, owner fields rather than name mangling) after the #515 fix? Inherent impls
   only inside the declaring package?
4. **Directory modules:** none (the package entry is the directory module)? And retire concatenation only
   for multi-file packages, or packages with dependencies, whose entry has no `import`?
5. **F7 rule:** `cosm_version` with the lowest-version stamp plus a one-time `kt_format 2`, landed before any
   other schema change?
6. **Freeze date:** the 1.0 criteria's §4.1 says "no new syntax after the feature freeze", but its release
   numbering is stale. `pub struct`, the visibility error flip and (under C1) `priv`/`readonly` all have to
   land before the freeze, so the freeze needs a date.

## V3b as built (#522)

- **Where function ownership lives.** Inside the COSM module, as `fn_packages` (function name →
  package id), not as a per-function CASM field. CASM's `check_version` takes any `1.x`, so an old
  reader would have dropped a per-function field and ownership with it; COSM is must-understand
  since V3a and both load paths (CVM1's kind table, CASM's `cosm`) go through
  `CosmModule::from_json`. So an old runtime refuses ownership by name, never runs without it.
- **Schema** (COSM `0.2.0`, `COSM_OWNERS_VERSION`): `KindDecl.module` (the declaring module's import
  path), `KindDecl.package`, `CosmModule.fn_packages`. All are omitted for the root package, so a
  one-package program is written exactly as before (COSM `0.1.0`, `kt_format 1`), however many
  files it has; any of them stamps `0.2.0` and `kt_format 2`. `module` is written only together
  with a non-root `package`. None of them is ignorable.
- **One owner-id rule** (`crush_cosm::owner_id_problem`): a package id is `pkg:<name>` or
  `dir:<label>`, with no `/` or `\` and never empty; the root package is never spelled out; a module
  id is a non-empty import path without separators. The reader refuses, the writer refuses to write
  (CASM returns the error, CVM1's infallible writer panics) and `validate` reports
  (`cosm::invalid_owner_id`) the same ids; the frontend checks what it compiles.
- **`module` is the import path as first written** (`geo`, `lib.geo`), not a canonical identity:
  two import paths can reach one file. V4 must not treat it as unique; the package plus the file is
  what identifies a module.
- **Owners of synthesised code.** A function a pass creates from another takes its owner
  (`cosm_lower::inherit_owner`): lifted methods take their `impl`/`trait`'s, lowered polyglot blocks
  and their helpers take the enclosing function's, and what the compiler lifts (nested `fn`s,
  lambdas, async bodies) is recorded under the function it came from. A merged `struct`/`impl`/
  `trait` carries its module's merge prefix and takes the package of that module's functions
  (`inherit_declaration_owners`); a module whose functions disagree, or a declaration that disagrees
  with them, is an internal compiler error. A `"root"` stamp (V1 puts one on every merged function)
  is removed in the same lowering step, before anything copies meta, so it changes no output.
- **A module with types and no function** has nothing to derive an owner from; it is owned
  because the resolver stamps its merged `struct` / `impl` / `trait` with the module's package, the
  same id it stamps on functions (#537 stamped functions only; V3b adds the declarations, next to
  `module`). Before V6 (#525) any other producer of merged declarations (a walker's CAST, CAST JSON)
  must stamp them too: an unstamped declaration is root-owned.
- **A program with no Kinds** still gets a kind-table section when it has owners
  (`Program::cosm_section`); `kind_table()` keeps meaning "declares Kinds or interfaces", so the
  engines' checks are unchanged.
- **Package ids in artifacts.** `root` (omitted), `pkg:<name>` (a crush-pkg package, kept), and
  `dir:<label>` for a library directory: the directory's last path component, numbered `#2`, `#3`, …
  when two directories of one program share it. A user's absolute directory never reaches an
  artifact. The label comes from the spelling alone, never the filesystem or the working
  directory, so the same source compiles to the same owners in any checkout: a directory written
  `.` or `..` has no real component and is labelled `dir` (`#N` on a clash). The resolver (V1) is
  the place to pass a meaningful directory spelling. The frontend reads `meta["package"]`, which the V1 resolver
  (#519, #537) records on merged functions and declarations.
- **Run time.** The scheduler VM and the portable VM read `fn_packages` at load
  (`KindTable::function_package`, `KindTable::code_owners` (a sorted entry index, binary-searched),
  `PortableVm::current_function_package`) and a
  Kind's owner (`Kind::module` / `package`). FastVM, the JIT and AOT run such programs as before:
  owners are inert until V6 (#525) enforces them.
- **Compatibility.** A compiled artifact with owners needs a runtime that reads COSM `0.2.0`;
  released 0.11.x refuses it (CVM1 by name, `unknown kind-table format 2`; CASM JSON with a serde
  unknown-field error). That happens only when a program uses code from another package. Source
  programs are unaffected.
- **Not as the note had it:** §2 says "each function in CASM/CVM1 (its package id)"; it is in COSM, as
  above.
- **Ruling (altair, 2026-11-16, on independent review):** a Kind of the root package records no
  owner, wherever it is declared. The first build recorded `module` for every Kind of an imported
  module, which stamped ordinary multi-file programs `0.2.0` and made old runtimes refuse them
  before any program had a second package. Now nothing is stamped until a program has one, and the
  CASM of a one-package program is unchanged (tested against the same program with the stamps
  removed).

## Rulings (captain, 2026-11-14, via vega)

| # | Ruling |
|---|---|
| 1 | **Approved: C.** Module visibility is per package, with the package taken from the file's path; `pub` exports. Staged: the `W-VIS01` warning first, then an error at a pre-1.0 minor bump. |
| 2 | **Approved: C.** Member visibility is VM-enforced, public by default, with opt-in contextual modifiers. **Spelling: `priv` and `readonly`.** Shipped as **C2** (no syntax in 1.0, listed as experimental) unless K2 lands before the freeze, in which case **C1**. |
| 3 | **Approved.** Kinds become per-module (§3 B, owner fields) after the #515 fix. Inherent impls are allowed only inside the declaring package. |
| 4 | **Approved.** No directory modules: the package entry is the directory module. Concatenation is retired only for multi-file or with-deps packages whose entry has no `import`. |
| 5 | **Approved.** F7: `cosm_version`, each blob stamped with the lowest version that can express it, and a one-time `kt_format 2` bump. This lands **before any other schema change**. |
| 6 | **The Crush 1.0 feature freeze is 2026-12-31** (ruled 2026-11-14). "No new syntax after the freeze" applies, so `pub struct`/`pub trait` (V1), the qualified literal `a.P { … }` (V4), the F7 rule and the owner ids (V3a, V3b), and the `W-VIS01` error flip (V5, after V2) must land before it. Decision 2 resolves on this date: **C1 only if K2 lands by 2026-12-31**, otherwise C2. |

| 7 | **CRUSH-458 vs decision 5: decision 5 wins, with the middle path** (captain, 2026-11-14). COSM stays **strict by default**: `cosm_version` with the lowest-version stamp, plus the one-time `kt_format 2`. A field that is **purely additive or cosmetic** may be ignore-unknown, gated by the version or a `requires` list. **Visibility, and anything else security-relevant, is must-understand**: an old runtime refuses the blob rather than running a `priv` member as public. CRUSH-458 ("ruling 13": ignore unknown everywhere, major-only check) is superseded. V3a #521 is written against this ruling. |

Order: V0 (#515) first, then V3a (F7) before any schema-touching ticket.

## V1.5 as built (#539): the gate for stage 2

Stage 1 alone could not show that a library is ready for stage 3: a legacy module (no `pub`) gets
only a note, only when another package uses it, and `--deny-warnings` ignores notes. V1.5 adds one
opt-in flag, `--deny-legacy-visibility`, that fails (exit 1) while any exported name still relies on
the legacy rule. Nothing changes without it. There are two gates, and a library needs both.

- **The compile gate, `crushc --check --deny-legacy-visibility FILE`** (with the usual `-L`): a
  `W-VIS01` becomes an error (same code and text, `level: "error"`), and a legacy module whose
  **functions** another package uses becomes one `E-VIS01` error per module, naming its file, the
  using package(s) and every function used, in place of its `N-VIS01` note. It follows the whole
  import graph, so a legacy module reached through a `pub` one in a third package is caught. It sees
  **function uses only**: kinds are one global namespace until #523, so a legacy module another
  package uses only for its `struct` / `impl` is not reported here. A legacy module used only inside
  its own package is fine (stage 3 keeps it usable there). `--emit ast` resolves no imports, so it
  is refused with the flag.
- **The directory gate, `crushc --list-exports --deny-legacy-visibility PATH`** (a `.crush` file or a
  directory, walked recursively in path order): the library author's view, with no foreign user needed.
  The walk follows what an import can follow (it may still list a file no import can name, such
  as `my-lib/x.crush`). It **follows a symlinked directory**, as
  `import a.b` does when `a/` is a link; each directory is walked once by its canonical path, so a
  link loop ends and a directory two paths reach is listed once (under the real path, when the
  link sits beside its target). It skips a **hidden directory** (`.name`), which no import can
  name (a module path is made of identifiers); pass one as `PATH` to list it. It also skips
  **`target/`** as build output, although `import target.x` resolves: a module kept in a directory
  of that name is not gated here. Without the
  flag it lists, per module, `pub: …` and `private: …`, or `legacy: no pub, everything public` with
  the names. With it, every **importable** legacy module fails (`E-VIS01`), kinds included. Exempt:
  a file with top-level statements other than definitions and Crush module imports (a `let`, a
  call, a `use @lang`: the resolver refuses to import it) and a file that defines nothing besides
  `main`; this listing prints them as `program (exempt from the gate: …)`. A file with `fn main` **and** other
  definitions is importable (the resolver drops a module's `main`), so a library with a demo `main`
  fails. A file that does not parse, or a file argument without the `.crush` extension, exits 1; a
  directory with no `.crush` file prints "no .crush files" and exits 0, with or without the flag.
- **`crush-pkg build --deny-legacy-visibility`**: both, for a package. It is a **library gate**. The
  package's sources are the entry's directory; when that is the package root, its `tests/` and
  `examples/` are left out. The walk is the directory gate's (symlinked directories followed,
  hidden directories and `target/` skipped). `tests/` and `examples/` are not entered at all, so
  a link inside them cannot take a real source directory out of the gate, and a link from
  outside them into them is gated under the link's path. Every importable legacy module there fails, the entry included, even
  when nobody imports it; the same two exemptions apply (top-level statements, or nothing besides
  `main`). An application does not need this gate: legacy inside one package stays fine at every
  stage. An application that runs it anyway must mark its importable modules `pub`, entry included
  (an entry with helpers beside `main` is importable). The failing modules are printed with their
  names; the compile then runs under the compile gate. A concatenation-mode package (entry without `import`) that has
  dependencies fails: concatenation reaches every name of a dependency by the legacy rule.
- **crush-pkg prints the findings** (`W-VIS01`, `N-VIS01`, `W-VIS02`) on stderr for every import-mode
  compile: `build`, `check`, `run`, the bare build-then-run, and `PackageBuilder::build_file`
  (what `squeeze test` compiles each test with), also when the builder is quiet; they are also on
  `BuildOutput::visibility`. They are warnings and notes: without the flag nothing new fails, and
  `--message-format strict` (which lifts its own `note` records) does not touch them. A
  concatenation-mode package has no packages to compare and prints none.

**The gate V2 (#520) uses**, per repository of the sweep:
- a crush-pkg package (facet, crush-http, polish): `crush-pkg build --deny-legacy-visibility` in the
  package directory;
- a library directory that is not a crush-pkg package: the directory gate on the library,
  `crushc --list-exports --deny-legacy-visibility <dir>`, and the compile gate on each of its users;
- a corpus of example **programs** (the examples, the guide, awesome-crush): the compile gate per
  program, `crushc --check --deny-legacy-visibility [-L <lib dir>] <program>`, not the directory
  gate (a program that shares helpers through a sibling module is one package, and that is fine).

A repository passes when these exit 0. The scaffolded library of `crush-pkg new --lib` has no `pub`
yet; it is part of V2.

**Known limits.** No gate flag on `crush-pkg run`, `check` or the bare build-then-run (only
`build`). The REPL, crush-debug and CAST input print no visibility findings. In crush-pkg's JSON
message mode the findings are text on stderr, not NDJSON records (its stdout also carries the
existing `building` / `compiled` status lines, as before). `BuildOutput` and `ImportOptions` gained
a public field, so struct literals of either need `..`.

## Advisor

chanakya (claude-fable-5-2, quick ask, 2026-11-14, logged in `consults.jsonl`). Confidence: medium. It
read this draft, import-system.md, object-model.md, schema.rs, the parser, import_system.rs and the 1.0
criteria; it ran nothing.

**Its advice, in short.** Keep the direction of all six recommendations, but:
- define "package" by the file, not the import edge;
- put the F7 rule first, and drop the claim that strict parsing protects members;
- give kinds and functions an owner id in the schema and the artifact;
- don't ship `priv`/`readonly` in 1.0 unless the exact VMs enforce it.

It also raised:
- shape-parsing struct literals, for L2;
- narrowing the concatenation retirement;
- the missing side doors;
- the map-compatible gate;
- the foreign-`impl` rule;
- the corpus sweep before the clash error;
- that V0 cannot close L3 alone.

**What I verified before taking it**, and what I did with each claim:

| Claim | Check | Result |
|---|---|---|
| The edge-based package rule makes a program's own modules foreign | `import_system.rs:420-428`: importer's directory, then search paths | **Taken.** §1 now keys the package on the file's path |
| `MemberDecl` ignores unknown keys | `schema.rs:234-242`: no `deny_unknown_fields`, `#[serde(flatten)]` | **Taken.** The serde behaviour is from its docs and is pinned by a test in V3a |
| The VM has no owner to compare | `KindDecl` has no module/package field; GET_MEMBER carries a name only | **Taken.** Owner ids are V3b (built: see "V3b as built") |
| Unenforced `priv` in 1.0 breaks the promise later | RELEASE-1.0-CRITERIA §4.5 (semantics row) and §1.2 (COSM experimental) | **Taken.** C1/C2 replace the old "A for 1.0" fallback |
| The kind clash contradicts documented behaviour | `import_system.rs:57-59` | **Taken.** V0 is a bug fix; #515 |
| Map-compatible kinds run as plain maps on FastVM/JIT/AOT | `object-model.md` §9 (line 428) | **Taken.** Private members make a kind non-map-compatible |
| Struct-literal pre-scan would break L2 | `grammar.js:550-565`: tree-sitter parses literals by shape | **Taken, with a different fix:** parse by shape in the frontend too, instead of granting an L2 exception |
| Concatenation trigger also hits single-file packages | `builder.rs` `uses_imports`: any entry without a top-level `import` | **Taken.** Retirement is narrowed |
| K2 re-entrancy is not the real blocker | an owner is static per function | **Taken.** The reason changes to "K2's protocols are the side doors"; the order is unchanged |

**Its NOT CHECKED list, which is my to-verify list:**
- **Serde `flatten` with an unknown key: not run.** V3a's test settles it.
- **`kt_format` gate:** verified in `bytecode.rs:11-24` ("an unknown one is an error").
- **casm's map-compatible rule and version gate:** read after the consult.
  - The Maps-level rule is the single function `kind_unsupported_strategies` (`casm/src/lib.rs:902-946`), which lists what a plain map cannot honour (value semantics, identity equality, constructors, hooks, accessors, field defaults). V6 adds "a non-public member" to that list, and that is the whole gate.
  - CASM carries the same `crush_cosm::CosmModule` (`Program.cosm`, through `cosm_serde`, `lib.rs:1163-1187`), so a `cosm_version` gate in `CosmModule::from_json` covers CASM and CVM1 together.
  - CASM's own `check_version` (`lib.rs:796-813`) accepts any `1.x`. So V3b's per-function package id on CASM must be checked: a new field that an old CASM reader ignores would drop ownership silently. Either the field goes inside the COSM module (gated by `cosm_version`), or it is a must-understand CASM change.
- **Whether `struct P { priv y: Int }` parses, and the `priv` field-name clash:** probed. A field named `priv` is legal today, so the modifiers are contextual.
- **crush-pkg test layouts that need package-private access:** not checked. Under the per-file rule, a package's `tests/` sit under the root package only when the test is the entry.
- **Reads on 30d6d581 versus probes on 7e6696db:** the only difference between them is #509 (`net.*` options), which touches none of the import, parser or COSM code read here.
