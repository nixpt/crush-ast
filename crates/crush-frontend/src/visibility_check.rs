//! `W-VIS02`: `pub fn __x` exports a name marked internal (V1, #519).
//!
//! The `__` prefix is Crush's convention for "internal, even inside the
//! package"; `pub` exports a function to other packages. Both on one function
//! contradict each other, so the compiler warns. The cross-package checks
//! (`W-VIS01`, `N-VIS01`) need the import graph and live in
//! [`crate::import_system`]; this pass needs only the program.
//!
//! [`module_surface`] and [`surfaces`] are the library author's view (V1.5,
//! #539): what each module file exports, with no foreign user needed. They
//! back `crushc --list-exports` and the package check of
//! `crush-pkg build --deny-legacy-visibility`.

use crate::diagnostics::{CompilerDiagnostic, DiagnosticSeverity};
use crush_cast::manifest::SourceLoc;
use crush_cast::{ImportStatement, Program, Statement};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The diagnostic code.
pub const W_VIS02: &str = "W-VIS02";

/// One `W-VIS02` per `pub` function whose own name (after any module
/// prefix) starts with `__`, sorted by file and line.
pub fn check_pub_internal(program: &Program) -> Vec<CompilerDiagnostic> {
    let mut out: Vec<CompilerDiagnostic> = program
        .functions
        .iter()
        .filter(|(_, f)| {
            f.meta
                .get("pub")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
        })
        .filter_map(|(mangled, f)| {
            let name = mangled.rsplit("::").next().unwrap_or(mangled);
            if !name.starts_with("__") {
                return None;
            }
            let num = |k: &str| {
                f.meta
                    .get(k)
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0)
            };
            Some(CompilerDiagnostic {
                code: W_VIS02,
                severity: DiagnosticSeverity::Warning,
                message: format!(
                    "`pub fn {name}` exports a name marked internal: the `__` prefix says \
                     internal, `pub` says exported to other packages"
                ),
                location: SourceLoc {
                    file: f
                        .meta
                        .get("source_file")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    line: num("line") as u32,
                    col: num("col") as u32,
                },
                hint: Some(format!(
                    "drop `pub` to keep `{name}` inside its package, or rename it without `__` \
                     to export it"
                )),
            })
        })
        .collect();
    out.sort_by(|a, b| {
        (&a.location.file, a.location.line, a.location.col).cmp(&(
            &b.location.file,
            b.location.line,
            b.location.col,
        ))
    });
    out
}

/// What one module file exports (V1.5, #539): its functions (an explicit
/// `fn main` excluded), which of them are `pub`, and its `struct` / `trait`
/// names with their `pub`. Read from the file alone, as the import resolver
/// reads a module (`Parser::parse_module`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModuleSurface {
    /// Every function name, `main` excluded.
    pub functions: BTreeSet<String>,
    /// The functions marked `pub`.
    pub pub_functions: BTreeSet<String>,
    /// Every `struct` / `trait` name, with whether it is marked `pub`.
    pub kinds: BTreeMap<String, bool>,
    /// It has an explicit `fn main`. The import resolver ignores a module's
    /// `main`, so this alone does not stop the file being imported.
    pub has_main: bool,
    /// It has a top-level statement other than a definition or a Crush
    /// module import (a `let`, a call, a `use @lang`, …). Such a file cannot
    /// be imported: the resolver refuses it.
    pub has_toplevel_code: bool,
}

impl ModuleSurface {
    /// It marks something `pub` (a `fn`, `struct` or `trait`). The import
    /// resolver's test for "not legacy".
    pub fn has_pub(&self) -> bool {
        !self.pub_functions.is_empty() || self.kinds.values().any(|p| *p)
    }

    /// It defines names besides `main` (functions or kinds).
    pub fn defines_names(&self) -> bool {
        !(self.functions.is_empty() && self.kinds.is_empty())
    }

    /// Another file can `import` it: it has no top-level statement that runs.
    pub fn is_importable(&self) -> bool {
        !self.has_toplevel_code
    }

    /// It defines names and marks none `pub`, so every name is public by the
    /// legacy rule: what stage 3 (#524) turns private.
    pub fn is_legacy(&self) -> bool {
        !self.has_pub() && self.defines_names()
    }

    /// What the stage-2 gate refuses in a directory or package (V1.5, #539):
    /// an importable legacy module, including one with a demo `fn main` (the
    /// resolver drops the `main` and exports the rest).
    pub fn fails_gate(&self) -> bool {
        self.is_legacy() && self.is_importable()
    }

    /// The names it exports to other packages: the `pub` ones, or every
    /// name of a legacy module.
    pub fn exported(&self) -> Vec<&str> {
        let legacy = !self.has_pub();
        self.functions
            .iter()
            .filter(|f| legacy || self.pub_functions.contains(*f))
            .map(String::as_str)
            .chain(
                self.kinds
                    .iter()
                    .filter(|(_, p)| legacy || **p)
                    .map(|(k, _)| k.as_str()),
            )
            .collect()
    }

    /// The names it keeps private to its package (none for a legacy module).
    pub fn private(&self) -> Vec<&str> {
        if !self.has_pub() {
            return Vec::new();
        }
        self.functions
            .iter()
            .filter(|f| !self.pub_functions.contains(*f))
            .map(String::as_str)
            .chain(
                self.kinds
                    .iter()
                    .filter(|(_, p)| !**p)
                    .map(|(k, _)| k.as_str()),
            )
            .collect()
    }
}

/// The [`ModuleSurface`] of Crush `source`. A parse error is returned as
/// the parser's message.
pub fn module_surface(source: &str) -> Result<ModuleSurface, String> {
    let (program, toplevel) = crate::parser::Parser::parse_module(source)
        .map_err(|errs| crate::ParseErrors(errs).to_string())?;
    let is_pub = |meta: &std::collections::HashMap<String, serde_json::Value>| {
        meta.get("pub")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
    };
    let mut surface = ModuleSurface::default();
    for (name, f) in &program.functions {
        if name == "main" {
            surface.has_main = true;
            continue;
        }
        surface.functions.insert(name.clone());
        if is_pub(&f.meta) {
            surface.pub_functions.insert(name.clone());
        }
    }
    for stmt in &toplevel {
        match stmt {
            Statement::StructDef { name, meta, .. } | Statement::TraitDef { name, meta, .. } => {
                surface.kinds.insert(name.clone(), is_pub(meta));
            }
            // Only a Crush module import is allowed in a module; any other
            // top-level statement (a `use @python`, `import @http`, a `let`,
            // a call) makes the resolver refuse to import the file.
            Statement::Import {
                import: ImportStatement::CrushModule { .. },
                ..
            }
            | Statement::ImplBlock { .. } => {}
            _ => surface.has_toplevel_code = true,
        }
    }
    Ok(surface)
}

/// The surface of every `.crush` file at `path`: the file itself (it must
/// have the `.crush` extension), or every `.crush` file under the directory
/// (recursively, sorted by path). A file that cannot be read or parsed comes
/// back as its error. An empty result means there is no `.crush` file.
///
/// The walk sees what the import resolver sees. The resolver looks a module
/// up by path (`import a.b` is `<root>/a/b.crush`) and so goes through a
/// symlinked directory; the walk follows one too. Each directory is walked
/// once, by its canonical path: a link loop ends there, and a directory
/// that two paths reach is listed under one of them (the real path, when
/// the link and its target sit in the same directory). The two skipped
/// names are not symmetrical:
///
/// - a **hidden** directory (`.name`) is skipped because no import can name
///   it: a module path is made of identifiers, and none starts with `.`. To
///   list one, pass it as `path`: the root is never filtered.
/// - **`target/`** is skipped as build output, although `import target.x`
///   does resolve; a module kept in a directory of that name is not seen.
pub fn surfaces(path: &Path) -> std::io::Result<Vec<(PathBuf, Result<ModuleSurface, String>)>> {
    let mut files = Vec::new();
    if path.is_dir() {
        collect_crush_files(path, &mut BTreeSet::new(), &mut files)?;
        files.sort();
    } else if path.extension().is_some_and(|e| e == "crush") {
        files.push(path.to_path_buf());
    } else if path.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{} is not a .crush file or a directory", path.display()),
        ));
    } else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("{} does not exist", path.display()),
        ));
    }
    Ok(files
        .into_iter()
        .map(|f| {
            let surface = std::fs::read_to_string(&f)
                .map_err(|e| format!("cannot read {}: {e}", f.display()))
                .and_then(|src| module_surface(&src));
            (f, surface)
        })
        .collect())
}

fn collect_crush_files(
    dir: &Path,
    visited: &mut BTreeSet<PathBuf>,
    out: &mut Vec<PathBuf>,
) -> std::io::Result<()> {
    // One walk per real directory: this is what ends a symlink loop, and
    // what keeps a directory reached by two paths from being listed twice.
    if !visited.insert(std::fs::canonicalize(dir)?) {
        return Ok(());
    }
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<_>>()?;
    entries.sort();
    let mut linked_dirs = Vec::new();
    for path in entries {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        // `symlink_metadata` is the entry itself; `metadata` follows a link,
        // as the resolver's `is_file` does. A dangling link, or a link that
        // loops on itself, has no `metadata`: the resolver cannot load it
        // either, so it is skipped.
        let is_link = std::fs::symlink_metadata(&path)?.file_type().is_symlink();
        let Ok(kind) = std::fs::metadata(&path).map(|m| m.file_type()) else {
            continue;
        };
        if kind.is_dir() {
            if name.starts_with('.') || name == "target" {
                continue;
            }
            if is_link {
                linked_dirs.push(path);
            } else {
                collect_crush_files(&path, visited, out)?;
            }
        } else if kind.is_file() && path.extension().is_some_and(|e| e == "crush") {
            out.push(path);
        }
    }
    // Real directories first, so a directory and a link to it that sit side
    // by side are listed under the real path.
    for path in linked_dirs {
        collect_crush_files(&path, visited, out)?;
    }
    Ok(())
}

/// One module's listing for `crushc --list-exports` (V1.5, #539): a head
/// line `FILE: …` and indented name lines.
pub fn render_surface(file: &str, surface: &ModuleSurface) -> String {
    let names = |v: Vec<&str>| {
        if v.is_empty() {
            "(none)".to_string()
        } else {
            v.join(", ")
        }
    };
    if !surface.is_importable() {
        return format!(
            "{file}: program (exempt from the gate: top-level statements, so it cannot be \
             imported)\n"
        );
    }
    if !surface.defines_names() {
        return if surface.has_main {
            format!("{file}: program (exempt from the gate: nothing to export besides `main`)\n")
        } else {
            format!("{file}: empty, exports nothing\n")
        };
    }
    let main = if surface.has_main {
        " (its `fn main` is ignored when it is imported)"
    } else {
        ""
    };
    if surface.is_legacy() {
        return format!(
            "{file}: legacy: no pub, everything public{main}\n  public: {}\n",
            names(surface.exported())
        );
    }
    format!(
        "{file}: pub{main}\n  pub: {}\n  private: {}\n",
        names(surface.exported()),
        names(surface.private())
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pub_dunder_warns_and_plain_does_not() {
        let program = crate::parse_source(
            "pub fn __leak() { return 1 }\npub fn ok() { return 2 }\nfn __quiet() { return 3 }\n",
        )
        .unwrap();
        let d = check_pub_internal(&program);
        assert_eq!(d.len(), 1, "{d:?}");
        assert_eq!(d[0].code, W_VIS02);
        assert!(d[0].message.contains("`pub fn __leak`"), "{}", d[0].message);
        assert_eq!(d[0].location.line, 1);
    }
}
