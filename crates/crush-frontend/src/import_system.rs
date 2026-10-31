//! Source-level imports for Crush (CRUSH-110).
//!
//! ```crush
//! import facet                              // facet.facet_parse(spec)
//! import facet { facet_parse, facet_help }  // facet_parse(spec)
//! import util.strings as s                  // util/strings.crush → s.title(x)
//! ```
//!
//! Imports are resolved **at compile time**: loading a module parses another
//! `.crush` file and merges its definitions into the importing [`Program`]
//! before semantic analysis, so imported names type-check and no runtime
//! capability is involved. [`resolve_imports`] is the only entry point; the
//! CLIs reach it through [`crate::parse_file`] /
//! [`crate::parse_source_with_imports`].
//!
//! # Rules
//!
//! - **Search path.** `import a.b` looks for `a/b.crush` in, in order: the
//!   importing file's directory, each [`ImportOptions::search_paths`] entry
//!   (`-L DIR`, then `CRUSH_PATH`). The first hit wins. There is no
//!   `a/b/mod.crush` form. Not found is a compile error that lists every
//!   directory searched.
//! - **Names.** A module's functions are merged under a module-qualified name,
//!   `<module path with :: for .>::<name>` (`facet::facet_parse`,
//!   `util::strings::title`) — the separator the lifted methods already use
//!   (`Point::norm2`), so every backend and the debugger take it. `::` cannot
//!   appear in a Crush identifier, so a mangled name never equals a
//!   user-written one; a clash with a lifted method of the same spelling (a
//!   `struct facet` with a method `facet_parse`) is a compile error, not an
//!   overwrite. (`.` is not an option: the compiler reads a dotted call name as
//!   a capability call on a receiver.) A second file imported under an
//!   already-used module path gets a `#2`, `#3`, … suffix on its prefix.
//! - **Binding.** `import a.b` binds the qualifier `b` (the last segment);
//!   `import a.b as x` binds `x`. `q.f(…)` then calls the module's `f`; a name
//!   the module doesn't define is a compile error. A selective list
//!   `import a.b { f, g }` binds the bare names `f` and `g` (and the qualifier
//!   only when `as x` is also given). A selective name that collides with a
//!   function of the importing file or with another selective import is a
//!   compile error naming both.
//! - **Inside a module**, a call names the module's own functions first
//!   (`__facet_list(…)` inside facet resolves to `facet::__facet_list`), then
//!   the module's own imports, then everything global (builtins,
//!   capabilities). A local variable or parameter of the same name shadows all
//!   of them.
//! - **Module contents** are definitions only: `fn`, `struct` / `impl` /
//!   `trait` (with `@kind`), and `import`s of other Crush modules. A module's
//!   `main` is ignored. Any other top-level statement — a `let`, a call, a
//!   `use @…` — is a compile error naming file and line: a module has no init
//!   code, so there is no hidden init order. `struct` / `impl` / `trait` names
//!   are **not** mangled: Kinds stay one global namespace, and the same Kind
//!   name declared twice is the usual duplicate-declaration error.
//! - **Graph.** Each file is loaded once per compile (a diamond shares one
//!   copy); a cycle is a compile error that shows the cycle; nesting deeper
//!   than [`ImportOptions::max_depth`] is a compile error.
//! - **Positions.** Errors raised while loading carry the file they are in
//!   ([`ImportError`]); every merged function records its file in
//!   `meta["source_file"]`, which the semantic checker attaches to its errors
//!   ([`crate::SemanticError::file`]).
//!
//! The non-Crush import forms are not handled here: `use @lang` and `use @cap`
//! keep their existing lowering, and `import @http/@git/@file` / `use @mcp` are
//! refused at compile time ([`unsupported_import`]).

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::Result;
use crush_cast::{Function, ImportStatement, Program, Statement};
use serde_json::Value;

/// Default for [`ImportOptions::max_depth`].
pub const DEFAULT_MAX_IMPORT_DEPTH: usize = 64;

/// Where imports are looked up, and how deep they may nest.
#[derive(Debug, Clone)]
pub struct ImportOptions {
    /// Directories searched after the importing file's own directory, in
    /// order: `-L DIR` flags first, then `CRUSH_PATH` (see
    /// [`ImportOptions::with_crush_path`]).
    pub search_paths: Vec<PathBuf>,
    /// The deepest import chain allowed (the root file imports at depth 1).
    pub max_depth: usize,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self { search_paths: Vec::new(), max_depth: DEFAULT_MAX_IMPORT_DEPTH }
    }
}

impl ImportOptions {
    /// Search `lib_paths` (the `-L` directories) and nothing else.
    pub fn new(lib_paths: impl IntoIterator<Item = PathBuf>) -> Self {
        Self { search_paths: lib_paths.into_iter().collect(), ..Self::default() }
    }

    /// Search `lib_paths`, then every directory of the `CRUSH_PATH`
    /// environment variable (colon-separated; empty entries are skipped). What
    /// the CLIs use.
    pub fn with_crush_path(lib_paths: impl IntoIterator<Item = PathBuf>) -> Self {
        let mut opts = Self::new(lib_paths);
        if let Some(var) = std::env::var_os("CRUSH_PATH") {
            opts.search_paths.extend(
                std::env::split_paths(&var).filter(|p| !p.as_os_str().is_empty()),
            );
        }
        opts
    }
}

/// An import that could not be resolved — not found, a cycle, a parse error
/// or a forbidden statement in an imported module, an unknown name. Carries
/// the file the problem is in (the importing file for a bad `import`, the
/// imported file for an error inside it) and the 1-based position.
#[derive(Debug, Clone)]
pub struct ImportError {
    /// Path of the file the error is in, as found on the search path.
    pub file: String,
    /// 1-based line and column, when known.
    pub pos: Option<(usize, usize)>,
    /// The error text, without file or position.
    pub message: String,
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.pos {
            Some((line, col)) => write!(f, "{}:{line}:{col}: {}", self.file, self.message),
            None => write!(f, "{}: {}", self.file, self.message),
        }
    }
}

impl std::error::Error for ImportError {}

/// Why an import form can't be compiled, if it can't: a Crush module import
/// that reached code generation unresolved, or an external / MCP import (no
/// host implements either). `None` for the forms that compile (`use @lang`,
/// `use @cap`, secure-env). Shared by the semantic checker and the compiler so
/// every compile path refuses with the same text.
pub fn unsupported_import(import: &ImportStatement) -> Option<String> {
    match import {
        ImportStatement::CrushModule { module_path, .. } => Some(format!(
            "`import {module_path}` was not resolved: Crush loads imported modules from \
             .crush files at compile time, and this compile has no file path to search from \
             (compile a file with `crushc FILE` / `crush-run run FILE`, or call \
             `crush_frontend::parse_file` / `parse_source_with_imports`; an `import` inside \
             a block or a function is not supported — imports go at the top level of a file)"
        )),
        ImportStatement::External { uri, resource_type, .. } => {
            let form = match resource_type {
                crush_cast::ExternalResourceType::Http => "@http",
                crush_cast::ExternalResourceType::Git => "@git",
                crush_cast::ExternalResourceType::File => "@file",
                _ => "@…",
            };
            Some(format!(
                "`import {form} \"{uri}\"` is not supported yet: no Crush host can load an \
                 external resource (it used to compile to an `external.load` call that failed \
                 at run time)"
            ))
        }
        ImportStatement::MCPImport { server_url, .. } => Some(format!(
            "`use @mcp \"{server_url}\"` is not supported yet: no Crush host implements MCP \
             client connections (it used to compile to an `mcp.connect` call that failed at \
             run time)"
        )),
        _ => None,
    }
}

/// Refuse a program that still holds an import no compile path can lower
/// ([`unsupported_import`]), at the import's position. Checks the top level of
/// every function body.
pub fn refuse_unsupported_imports(program: &Program) -> Result<()> {
    let mut names: Vec<&String> = program.functions.keys().collect();
    names.sort();
    for name in names {
        let func = &program.functions[name];
        for stmt in &func.body {
            if let Statement::Import { import, meta } = stmt
                && let Some(message) = unsupported_import(import)
            {
                return Err(anyhow::Error::new(crate::SemanticError {
                    pos: span(meta),
                    message,
                    file: source_file(func),
                }));
            }
        }
    }
    Ok(())
}

/// The file an imported function came from (`meta["source_file"]`).
pub(crate) fn source_file(func: &Function) -> Option<String> {
    func.meta.get("source_file").and_then(Value::as_str).map(str::to_string)
}

/// Resolve every `import a.b [as x] [{ f, … }]` at the top level of `program`
/// (the parse of the file at `origin`): load each module from disk, merge its
/// definitions under module-qualified names, and rewrite the program's calls
/// to them. The import statements are removed. See the module docs for the
/// rules. `origin` need not exist (the REPL passes a name in its working
/// directory); only its parent directory is searched.
pub fn resolve_imports(program: &mut Program, origin: &Path, opts: &ImportOptions) -> Result<()> {
    let origin_display = origin.display().to_string();
    // Imports anywhere but the top level of the file are not resolved.
    for (name, func) in &program.functions {
        if name == "main" {
            continue;
        }
        if let Some(meta) = func.body.iter().find_map(|s| match s {
            Statement::Import { import: ImportStatement::CrushModule { .. }, meta } => Some(meta),
            _ => None,
        }) {
            return Err(import_error(
                &origin_display,
                span(meta),
                format!("`import` inside `fn {name}`: imports go at the top level of a file"),
            ));
        }
    }
    let Some(main) = program.functions.get_mut("main") else { return Ok(()) };
    let mut imports = Vec::new();
    main.body.retain(|stmt| match stmt {
        Statement::Import { import: ImportStatement::CrushModule { .. }, .. } => {
            imports.push(stmt.clone());
            false
        }
        _ => true,
    });
    if imports.is_empty() {
        return Ok(());
    }

    let mut resolver = Resolver {
        opts,
        modules: Vec::new(),
        by_file: HashMap::new(),
        prefixes: HashSet::new(),
        stack: Vec::new(),
        functions: Vec::new(),
        defs: Vec::new(),
    };
    let root_dir = parent_dir(origin);
    let root_names: BTreeSet<String> = program.functions.keys().cloned().collect();
    let root_fn_lines: HashMap<String, Option<(usize, usize)>> =
        program.functions.iter().map(|(n, f)| (n.clone(), span(&f.meta))).collect();
    let scope = resolver.bind_imports(&root_dir, &origin_display, &imports, None, &|name| {
        root_names.contains(name).then(|| {
            let at = root_fn_lines.get(name).copied().flatten();
            format!("`fn {name}`{}", at.map(|(l, _)| format!(" (line {l})")).unwrap_or_default())
        })
    })?;

    let mut names: Vec<String> = program.functions.keys().cloned().collect();
    names.sort();
    for name in names {
        let func = program.functions.get_mut(&name).expect("listed above");
        rewrite_function(func, &scope, &resolver.modules, &origin_display)?;
    }
    for (name, func) in resolver.functions {
        program.functions.insert(name, func);
    }
    if !resolver.defs.is_empty() {
        let main = program.functions.get_mut("main").expect("main held the imports");
        let mut body = resolver.defs;
        body.append(&mut main.body);
        main.body = body;
    }
    Ok(())
}

/// One loaded module.
struct Module {
    /// Its import path as first written (`util.strings`).
    path: String,
    /// The prefix its functions are merged under (`util::strings`, or
    /// `util::strings#2` for a second file imported under the same path).
    prefix: String,
    /// The file it was loaded from.
    file: String,
    /// Its function names, unqualified (`main` excluded).
    functions: BTreeSet<String>,
}

impl Module {
    fn mangled(&self, name: &str) -> String {
        format!("{}::{name}", self.prefix)
    }
}

/// The names visible in one file.
#[derive(Default)]
struct Scope {
    /// The module this file is (`None` for the root file, whose own
    /// functions keep their names).
    own: Option<usize>,
    /// Selectively imported bare names → their mangled names.
    selective: HashMap<String, String>,
    /// Qualifiers (`facet`, `s`) → module index.
    qualifiers: HashMap<String, usize>,
}

impl Scope {
    /// What a bare call `name(…)` refers to, if it is a module function.
    fn bare(&self, name: &str, modules: &[Module]) -> Option<String> {
        if let Some(own) = self.own
            && modules[own].functions.contains(name)
        {
            return Some(modules[own].mangled(name));
        }
        self.selective.get(name).cloned()
    }
}

struct Resolver<'a> {
    opts: &'a ImportOptions,
    modules: Vec<Module>,
    /// Canonical file → module index, for files fully loaded.
    by_file: HashMap<PathBuf, usize>,
    prefixes: HashSet<String>,
    /// The chain being loaded: canonical file and display path.
    stack: Vec<(PathBuf, String)>,
    /// Merged module functions, mangled.
    functions: Vec<(String, Function)>,
    /// Merged module `struct` / `impl` / `trait` declarations.
    defs: Vec<Statement>,
}

impl Resolver<'_> {
    /// Resolve the `imports` of one file (in `dir`, shown as `file`) and build
    /// its scope. `own` is the module the file is; `local` names a definition
    /// of the importing file that a selective name would collide with.
    fn bind_imports(
        &mut self,
        dir: &Path,
        file: &str,
        imports: &[Statement],
        own: Option<usize>,
        local: &dyn Fn(&str) -> Option<String>,
    ) -> Result<Scope> {
        let mut scope = Scope { own, ..Scope::default() };
        // Where each binding came from, for collision messages.
        let mut selective_from: HashMap<String, (String, usize)> = HashMap::new();
        let mut qualifier_from: HashMap<String, (String, usize)> = HashMap::new();
        for stmt in imports {
            let Statement::Import {
                import: ImportStatement::CrushModule { module_path, alias, selective },
                meta,
            } = stmt
            else {
                continue;
            };
            let at = span(meta);
            let line = at.map_or(0, |(l, _)| l);
            let idx = self.load(dir, file, module_path, at)?;
            let qualifier = match (alias, selective.is_empty()) {
                (Some(a), _) => Some(a.clone()),
                (None, true) => module_path.rsplit('.').next().map(str::to_string),
                (None, false) => None,
            };
            if let Some(q) = qualifier {
                if let Some((prev, prev_line)) = qualifier_from.get(&q)
                    && self.modules[scope.qualifiers[&q]].file != self.modules[idx].file
                {
                    return Err(import_error(
                        file,
                        at,
                        format!(
                            "`import {module_path}` binds `{q}`, which `import {prev}` (line \
                             {prev_line}) already binds; give one of them a different name with \
                             `as`"
                        ),
                    ));
                }
                qualifier_from.insert(q.clone(), (module_path.clone(), line));
                scope.qualifiers.insert(q, idx);
            }
            for name in selective {
                let module = &self.modules[idx];
                if !module.functions.contains(name) {
                    return Err(import_error(
                        file,
                        at,
                        format!(
                            "`import {module_path} {{ {name} }}`: {} defines no function \
                             `{name}`{}",
                            module.file,
                            available(&module.functions)
                        ),
                    ));
                }
                let mangled = module.mangled(name);
                if let Some(what) = local(name) {
                    return Err(import_error(
                        file,
                        at,
                        format!(
                            "`import {module_path} {{ {name} }}` collides with {what} of the \
                             same name in this file"
                        ),
                    ));
                }
                if let Some((prev, prev_line)) = selective_from.get(name)
                    && scope.selective.get(name) != Some(&mangled)
                {
                    return Err(import_error(
                        file,
                        at,
                        format!(
                            "`import {module_path} {{ {name} }}` collides with \
                             `import {prev} {{ {name} }}` (line {prev_line}): both bind `{name}`"
                        ),
                    ));
                }
                selective_from.insert(name.clone(), (module_path.clone(), line));
                scope.selective.insert(name.clone(), mangled);
            }
        }
        Ok(scope)
    }

    /// Find, load and merge the module `module_path` imported from `dir`
    /// (file `file`, at `at`); its index. A file already loaded is reused.
    fn load(
        &mut self,
        dir: &Path,
        file: &str,
        module_path: &str,
        at: Option<(usize, usize)>,
    ) -> Result<usize> {
        let mut rel = PathBuf::new();
        for seg in module_path.split('.') {
            rel.push(seg);
        }
        rel.set_extension("crush");
        let mut searched = vec![dir.to_path_buf()];
        searched.extend(self.opts.search_paths.iter().cloned());
        let Some(found) = searched.iter().map(|d| d.join(&rel)).find(|p| p.is_file()) else {
            let dirs: Vec<String> =
                searched.iter().map(|d| format!("  {}", display_dir(d))).collect();
            return Err(import_error(
                file,
                at,
                format!(
                    "`import {module_path}`: module not found — no {} in any of:\n{}",
                    rel.display(),
                    dirs.join("\n")
                ),
            ));
        };
        let canonical = std::fs::canonicalize(&found).unwrap_or_else(|_| found.clone());
        if let Some(&idx) = self.by_file.get(&canonical) {
            return Ok(idx);
        }
        let display = found.display().to_string();
        if let Some(pos) = self.stack.iter().position(|(c, _)| *c == canonical) {
            let mut chain: Vec<&str> = self.stack[pos..].iter().map(|(_, d)| d.as_str()).collect();
            chain.push(&display);
            return Err(import_error(
                file,
                at,
                format!("`import {module_path}`: import cycle: {}", chain.join(" → ")),
            ));
        }
        if self.stack.len() >= self.opts.max_depth {
            return Err(import_error(
                file,
                at,
                format!(
                    "`import {module_path}`: imports nest deeper than {} levels",
                    self.opts.max_depth
                ),
            ));
        }

        let source = std::fs::read_to_string(&found).map_err(|e| {
            import_error(file, at, format!("`import {module_path}`: cannot read {display}: {e}"))
        })?;
        let (mut parsed, toplevel) = crate::parser::Parser::parse_module(&source).map_err(|errs| {
            let pos = errs.first().and_then(parse_error_pos);
            import_error(&display, pos, crate::ParseErrors(errs).to_string())
        })?;
        parsed.functions.remove("main");

        // `.` is not allowed in a function name: the compiler reads
        // `a.b(…)` as a call of capability `b` on `a`.
        let base = module_path.replace('.', "::");
        let mut prefix = base.clone();
        let mut n = 2;
        while self.prefixes.contains(&prefix) {
            prefix = format!("{base}#{n}");
            n += 1;
        }
        self.prefixes.insert(prefix.clone());
        let idx = self.modules.len();
        self.modules.push(Module {
            path: module_path.to_string(),
            prefix,
            file: display.clone(),
            functions: parsed.functions.keys().cloned().collect(),
        });

        let mut imports = Vec::new();
        let mut defs = Vec::new();
        for stmt in toplevel {
            match &stmt {
                Statement::Import { import: ImportStatement::CrushModule { .. }, .. } => {
                    imports.push(stmt)
                }
                Statement::StructDef { .. }
                | Statement::ImplBlock { .. }
                | Statement::TraitDef { .. } => defs.push(stmt),
                other => {
                    let what = match other {
                        Statement::Import { .. } => "a `use @…` / `import @…` declaration",
                        Statement::VarDecl { .. } => "a top-level `let`",
                        _ => "a top-level statement",
                    };
                    return Err(import_error(
                        &display,
                        statement_span(other),
                        format!(
                            "{what} in imported module `{module_path}`: a module may only define \
                             functions, structs, impls and traits, and import other Crush \
                             modules — it has no init code (move the statement into a function)"
                        ),
                    ));
                }
            }
        }

        self.stack.push((canonical.clone(), display.clone()));
        let module_dir = parent_dir(&found);
        let own_names = self.modules[idx].functions.clone();
        let scope = self.bind_imports(&module_dir, &display, &imports, Some(idx), &|name| {
            own_names.contains(name).then(|| format!("`fn {name}`"))
        })?;
        self.stack.pop();

        let mut names: Vec<String> = parsed.functions.keys().cloned().collect();
        names.sort();
        for name in names {
            let mut func = parsed.functions.remove(&name).expect("listed above");
            rewrite_function(&mut func, &scope, &self.modules, &display)?;
            func.meta.insert("source_file".to_string(), Value::String(display.clone()));
            func.meta.insert("module".to_string(), Value::String(module_path.to_string()));
            self.functions.push((self.modules[idx].mangled(&name), func));
        }
        for mut def in defs {
            rewrite_statement(&mut def, &scope, &self.modules, &display)?;
            self.defs.push(def);
        }
        self.by_file.insert(canonical, idx);
        Ok(idx)
    }
}

/// Rewrite the calls in `func` that name module functions (see
/// [`rewrite_value`]).
fn rewrite_function(func: &mut Function, scope: &Scope, modules: &[Module], file: &str) -> Result<()> {
    if scope.qualifiers.is_empty() && scope.selective.is_empty() && scope.own.is_none() {
        return Ok(());
    }
    let mut body = serde_json::to_value(&func.body)?;
    let mut locals: HashSet<String> = func.params.iter().map(|(p, _)| p.clone()).collect();
    collect_locals(&body, &mut locals);
    rewrite_value(&mut body, scope, modules, &locals, file)?;
    func.body = serde_json::from_value(body)?;
    Ok(())
}

/// [`rewrite_function`] for one top-level declaration (an `impl` / `trait`'s
/// method bodies).
fn rewrite_statement(stmt: &mut Statement, scope: &Scope, modules: &[Module], file: &str) -> Result<()> {
    let mut v = serde_json::to_value(&*stmt)?;
    let mut locals = HashSet::new();
    collect_locals(&v, &mut locals);
    rewrite_value(&mut v, scope, modules, &locals, file)?;
    *stmt = serde_json::from_value(v)?;
    Ok(())
}

/// Names a function binds locally — parameters, `let`s, loop and catch
/// variables, lambda parameters, assignment targets. A local shadows a module
/// function of the same name everywhere in the function (an approximation of
/// block scoping that can only leave a call unrewritten, never misdirect one).
fn collect_locals(v: &Value, out: &mut HashSet<String>) {
    match v {
        Value::Object(obj) => {
            let field = |k: &str| obj.get(k).and_then(Value::as_str).map(str::to_string);
            match obj.get("type").and_then(Value::as_str) {
                Some("VarDecl") => out.extend(field("name")),
                Some("Assign") => out.extend(field("target")),
                Some("For") => out.extend(field("variable")),
                Some("TryCatch") => out.extend(field("error_var")),
                Some("Lambda") | Some("FunctionDef") => {
                    if let Some(ps) = obj.get("params").and_then(Value::as_array) {
                        out.extend(ps.iter().filter_map(|p| {
                            p.get(0).and_then(Value::as_str).map(str::to_string)
                        }));
                    }
                }
                _ => {}
            }
            for (k, child) in obj {
                if k != "meta" {
                    collect_locals(child, out);
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|i| collect_locals(i, out)),
        _ => {}
    }
}

/// Point every reference to a module function at its mangled name:
/// `f(…)` / `spawn f(…)` / `f` as a value, when `f` is the file's own module
/// function or a selective import; `q.f(…)` / `q.f` when `q` is an import
/// qualifier (a `q.f` naming no function of `q` is an error).
fn rewrite_value(
    v: &mut Value,
    scope: &Scope,
    modules: &[Module],
    locals: &HashSet<String>,
    file: &str,
) -> Result<()> {
    match v {
        Value::Object(obj) => {
            let ty = obj.get("type").and_then(Value::as_str).map(str::to_string);
            match ty.as_deref() {
                Some("Call") | Some("Spawn") => {
                    if let Some(name) = obj.get("function").and_then(Value::as_str)
                        && !locals.contains(name)
                        && let Some(m) = scope.bare(name, modules)
                    {
                        obj.insert("function".to_string(), Value::String(m));
                    }
                }
                Some("Var") => {
                    if let Some(name) = obj.get("name").and_then(Value::as_str)
                        && !locals.contains(name)
                        && let Some(m) = scope.bare(name, modules)
                    {
                        obj.insert("name".to_string(), Value::String(m));
                    }
                }
                Some("CapabilityCall") => {
                    let resolved = obj
                        .get("meta")
                        .and_then(|m| m.get("resolved_capability"))
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    let name = obj.get("name").and_then(Value::as_str).unwrap_or("").to_string();
                    if !resolved
                        && let Some((q, f)) = name.split_once('.')
                        && !f.contains('.')
                        && !locals.contains(q)
                        && let Some(&idx) = scope.qualifiers.get(q)
                    {
                        let target = qualified(&modules[idx], q, f, obj.get("meta"), file)?;
                        obj.insert("type".to_string(), Value::String("Call".to_string()));
                        obj.remove("name");
                        obj.insert("function".to_string(), Value::String(target));
                    }
                }
                Some("GetField") => {
                    let target_var = obj
                        .get("target")
                        .filter(|t| t.get("type").and_then(Value::as_str) == Some("Var"))
                        .and_then(|t| t.get("name"))
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    if let Some(q) = target_var
                        && !locals.contains(&q)
                        && let Some(&idx) = scope.qualifiers.get(&q)
                    {
                        let f = obj.get("field").and_then(Value::as_str).unwrap_or("").to_string();
                        let target = qualified(&modules[idx], &q, &f, obj.get("meta"), file)?;
                        let meta = obj.get("meta").cloned().unwrap_or_else(|| Value::Object(Default::default()));
                        *v = serde_json::json!({"type": "Var", "name": target, "meta": meta});
                        return Ok(());
                    }
                }
                _ => {}
            }
            for (k, child) in obj.iter_mut() {
                if k != "meta" {
                    rewrite_value(child, scope, modules, locals, file)?;
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                rewrite_value(item, scope, modules, locals, file)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// The mangled name of `q.f`, or the error for a name `q`'s module lacks.
fn qualified(module: &Module, q: &str, f: &str, meta: Option<&Value>, file: &str) -> Result<String> {
    if module.functions.contains(f) {
        return Ok(module.mangled(f));
    }
    let pos = meta.and_then(|m| {
        let line = m.get("line")?.as_u64()? as usize;
        let col = m.get("col").and_then(Value::as_u64).unwrap_or(0) as usize;
        (line > 0).then_some((line, col))
    });
    Err(import_error(
        file,
        pos,
        format!(
            "`{q}.{f}`: module `{}` ({}) defines no function `{f}`{}",
            module.path,
            module.file,
            available(&module.functions)
        ),
    ))
}

/// `" (it defines: a, b, c)"`, for not-found messages.
fn available(functions: &BTreeSet<String>) -> String {
    let public: Vec<&str> =
        functions.iter().map(String::as_str).filter(|n| !n.starts_with("__")).collect();
    if public.is_empty() {
        String::new()
    } else {
        format!(" (it defines: {})", public.join(", "))
    }
}

fn import_error(file: &str, pos: Option<(usize, usize)>, message: String) -> anyhow::Error {
    anyhow::Error::new(ImportError { file: file.to_string(), pos, message })
}

fn span(meta: &HashMap<String, Value>) -> Option<(usize, usize)> {
    crate::cosm_lower::span_of(meta)
}

fn statement_span(stmt: &Statement) -> Option<(usize, usize)> {
    let v = serde_json::to_value(stmt).ok()?;
    let meta = v.get("meta")?;
    let line = meta.get("line")?.as_u64()? as usize;
    let col = meta.get("col").and_then(Value::as_u64).unwrap_or(0) as usize;
    (line > 0).then_some((line, col))
}

fn parse_error_pos(e: &crate::parser::ParseError) -> Option<(usize, usize)> {
    use crate::parser::ParseError as E;
    let (line, col) = match e {
        E::UnexpectedToken { line, col, .. }
        | E::Expected { line, col, .. }
        | E::UnexpectedEOF { line, col }
        | E::InvalidNumber { line, col, .. }
        | E::UnterminatedString { line, col }
        | E::UnknownAnnotation { line, col, .. } => (*line, *col),
    };
    (line > 0).then_some((line, col))
}

/// The directory of `path` — empty for a bare file name, so files found
/// there display as `m.crush`, not `./m.crush`.
fn parent_dir(path: &Path) -> PathBuf {
    path.parent().map(Path::to_path_buf).unwrap_or_default()
}

fn display_dir(dir: &Path) -> String {
    let dir = if dir.as_os_str().is_empty() { Path::new(".") } else { dir };
    let shown = dir.display().to_string();
    match std::fs::canonicalize(dir) {
        Ok(abs) if abs != dir => format!("{shown} ({})", abs.display()),
        Ok(_) => shown,
        Err(_) => format!("{shown} (does not exist)"),
    }
}
