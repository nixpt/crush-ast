# Visibility and packages for Crush (CRUSH-509): design note

altair (crush lane), 2026-11-14, asked for by the captain through vega. **Design only. Nothing here is built
until the captain picks.** Every decision below has options and a recommendation. The ones the captain
must rule on are collected under **Decisions**.

## What exists today (checked on main 30d6d581)

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
diamond import gives one answer, because the file decides and not the route. One rule has to be written
down: several unrelated libraries in a single `-L` directory form one package and share their privates. The
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

**Recommendation: (ii)**, with the one-time `kt_format 2` bump and with (iii)'s by-name message as the
error text. Add a test that today's runtime refuses a blob with a private method, which pins down
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
| **V0** (S) | Corpus sweep for duplicate kind names, then a kind-name clash is a compile error (§3 A). `import-system.md`: deps-as-roots marked done; `pub` documented as "accepted, not yet enforced". The guide gets a `pub` section. | #515; L3's `pub` item (the row stays 🟡: `capability`, `mcp`/external imports and `let mut` remain) |
| **V1** (M) | §1 stage 1: `pub` recorded on `fn`/`struct`/`trait`, the per-file package rule, `W-VIS01`, the legacy all-public note and the first-`pub` warning | L5 (modules have an interface) |
| **V2** (S, other repos) | `pub` added in facet, crush-http, polish, examples, the guide and awesome-crush | — |
| **V3a** (S-M) | F7: `cosm_version` in `CosmModule`, `kt_format 2`, the reader gate and its by-name error, the lowest-version stamp, docs, and the private-method refusal test. **Before every schema-touching ticket.** | **F7** ✅ |
| **V3b** (M) | Owner ids: `module`/`package` on `KindDecl`, and a package id per function in CASM/CVM1 (refused by name on old runtimes) | F4/F9 inputs (the format change is documented) |
| **V4** (M) | §3 B per-module kinds, the foreign-`impl` rule, and shape-parsed cross-file struct literals with `a.P { … }` in both grammars | L5 (the "Not yet" items); keeps L2 |
| **V5** (M) | §1 stage 3, plus the narrow concatenation retirement: `W-VIS01` becomes an error at a minor bump | — |
| **V6** (L, with or after K2) | §2: the `visibility` schema field and contextual `priv`/`readonly`. The VM enforces it on the scheduler and portable VMs. Kinds with private members are not map-compatible. The side doors are closed or documented. FastVM/JIT/AOT are exact-or-loud. Under C2, the syntax stays experimental until this ships. | L6 ✅ when combined with K2 |
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
| The VM has no owner to compare | `KindDecl` has no module/package field; GET_MEMBER carries a name only | **Taken.** Owner ids are V3b |
| Unenforced `priv` in 1.0 breaks the promise later | RELEASE-1.0-CRITERIA §4.5 (semantics row) and §1.2 (COSM experimental) | **Taken.** C1/C2 replace the old "A for 1.0" fallback |
| The kind clash contradicts documented behaviour | `import_system.rs:57-59` | **Taken.** V0 is a bug fix; #515 |
| Map-compatible kinds run as plain maps on FastVM/JIT/AOT | `object-model.md` §9 (line 428) | **Taken.** Private members make a kind non-map-compatible |
| Struct-literal pre-scan would break L2 | `grammar.js:550-565`: tree-sitter parses literals by shape | **Taken, with a different fix:** parse by shape in the frontend too, instead of granting an L2 exception |
| Concatenation trigger also hits single-file packages | `builder.rs` `uses_imports`: any entry without a top-level `import` | **Taken.** Retirement is narrowed |
| K2 re-entrancy is not the real blocker | an owner is static per function | **Taken.** The reason changes to "K2's protocols are the side doors"; the order is unchanged |

**Its NOT CHECKED list, which is my to-verify list:**
- **Serde `flatten` with an unknown key: not run.** V3a's test settles it.
- **`kt_format` gate:** verified in `bytecode.rs:11-24` ("an unknown one is an error").
- **casm's map-compatible rule and its COSM version gate** (`casm/src/lib.rs:870-916, 1046-1085`): **not read yet.** It must be read before V3a/V6 are written as tickets.
- **Whether `struct P { priv y: Int }` parses, and the `priv` field-name clash:** probed. A field named `priv` is legal today, so the modifiers are contextual.
- **crush-pkg test layouts that need package-private access:** not checked. Under the per-file rule, a package's `tests/` sit under the root package only when the test is the entry.
- **Reads on 30d6d581 versus probes on 7e6696db:** the only difference between them is #509 (`net.*` options), which touches none of the import, parser or COSM code read here.
