//! `use @lang <lang> "<module>" [as alias] [{ "name", ... }]` → the guest's
//! own import line, spliced into every later `@lang` block (CRUSH-224).
//!
//! Each `@lang` block runs in a fresh interpreter, so an import cannot be
//! loaded once and reused: it has to be written into every block that needs
//! it. `crush_lang_sdk::compile::prepare_polyglot_blocks` decides which `use`
//! reaches which block and records it in `LangBlock.imports`. This module
//! turns that list into source text, and the compiler splices it in when it
//! lowers the block.
//!
//! What a `use` binds inside the guest:
//! - the module under its own name (Python `import os.path` → `os`; JS
//!   `"node:fs"` → `fs`), unless the `use` is selective-only;
//! - the alias, if there is one, in addition;
//! - each selected name.
//!
//! The import text goes on the guest's first line, never on a line of its
//! own when that can be avoided, so guest line K stays `.crush` line
//! `block_line + K - 1` (the `(at .crush line N)` mapping in
//! `VmError::LangRuntimeError` names the block line). See [`splice_header`]
//! for the one case that shifts.

use crush_cast::ImportStatement;

/// Canonical language for a `@lang` / `use @lang` tag. The same table as
/// `crush_vm::scheduler::canonical_lang` (crush-frontend sits below crush-vm
/// in the dependency DAG, so it can't call it).
pub fn canonical_lang(lang: &str) -> Option<&'static str> {
    match lang {
        "python" | "python3" | "py" => Some("python"),
        "javascript" | "js" | "es6" | "ecmascript" | "node" => Some("javascript"),
        "bash" | "sh" => Some("bash"),
        _ => None,
    }
}

/// Reject a `use @lang` this compiler can't carry into a block.
pub fn check_polyglot_use(language: &str) -> Result<(), String> {
    match canonical_lang(language) {
        Some("python") | Some("javascript") => Ok(()),
        Some("bash") => Err(format!(
            "use @lang {language}: bash has no module imports to carry into @{language} blocks; \
             `source` the file inside the block instead"
        )),
        _ => Err(format!(
            "use @lang {language}: unsupported language (supported: python, javascript)"
        )),
    }
}

fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

/// The import statements for every `PolyglotModule` in `imports`, joined into
/// one line of `lang` source. `None` when there is nothing to import.
pub fn import_header(lang: &str, imports: &[ImportStatement]) -> Result<Option<String>, String> {
    let lang = canonical_lang(lang).unwrap_or(lang);
    let mut parts: Vec<String> = Vec::new();
    for import in imports {
        let ImportStatement::PolyglotModule {
            language,
            module_path,
            alias,
            selective,
        } = import
        else {
            continue;
        };
        check_polyglot_use(language)?;
        if let Some(a) = alias
            && !is_ident(a)
        {
            return Err(format!(
                "use @lang {language} \"{module_path}\": alias `{a}` is not an identifier"
            ));
        }
        if let Some(bad) = selective.iter().find(|n| !is_ident(n)) {
            return Err(format!(
                "use @lang {language} \"{module_path}\": `{bad}` is not an identifier"
            ));
        }
        match lang {
            "python" => parts.extend(python_imports(module_path, alias.as_deref(), selective)?),
            "javascript" => parts.extend(javascript_imports(
                module_path,
                alias.as_deref(),
                selective,
            )?),
            other => return Err(format!("use @lang: cannot import into @{other} blocks")),
        }
    }
    Ok(if parts.is_empty() {
        None
    } else {
        Some(parts.join(if lang == "python" { "; " } else { " " }))
    })
}

fn python_imports(
    path: &str,
    alias: Option<&str>,
    selective: &[String],
) -> Result<Vec<String>, String> {
    // The path is spliced into source, so it must be a dotted module name and nothing more.
    if path.is_empty() || !path.split('.').all(is_ident) || path.contains('$') {
        return Err(format!(
            "use @lang python \"{path}\": not a Python module name"
        ));
    }
    let mut out = Vec::new();
    if selective.is_empty() || alias.is_some() {
        out.push(format!("import {path}"));
    }
    if let Some(a) = alias {
        out.push(format!("import {path} as {a}"));
    }
    if !selective.is_empty() {
        out.push(format!("from {path} import {}", selective.join(", ")));
    }
    Ok(out)
}

fn javascript_imports(
    path: &str,
    alias: Option<&str>,
    selective: &[String],
) -> Result<Vec<String>, String> {
    let literal = serde_json::to_string(path).expect("a string serializes");
    // `node:fs` → `fs`, `lodash/fp` → `fp`, `is-number` → `is_number`.
    let base = path
        .rsplit(['/', ':'])
        .next()
        .unwrap_or(path)
        .replace(['-', '.'], "_");
    let own_name = is_ident(&base).then_some(base);
    let mut names: Vec<String> = Vec::new();
    if selective.is_empty() || alias.is_some() {
        match &own_name {
            Some(n) => names.push(n.clone()),
            None if alias.is_none() => {
                return Err(format!(
                    "use @lang javascript \"{path}\": no identifier to bind it to; add `as <name>`"
                ));
            }
            None => {}
        }
    }
    if let Some(a) = alias
        && !names.iter().any(|n| n == a)
    {
        names.push(a.to_string());
    }
    let mut out: Vec<String> = names
        .iter()
        .map(|n| format!("const {n} = require({literal});"))
        .collect();
    if !selective.is_empty() {
        out.push(format!(
            "const {{ {} }} = require({literal});",
            selective.join(", ")
        ));
    }
    Ok(out)
}

/// Statement keywords that open a Python block: `import m; for x in y:` is a
/// syntax error, so a header can't share a line with one of them.
fn python_line_opens_block(line: &str) -> bool {
    const OPENERS: &[&str] = &[
        "if", "elif", "else", "for", "while", "def", "class", "with", "try", "except", "finally",
        "async", "match", "case",
    ];
    let t = line.trim_start();
    t.starts_with('@')
        || OPENERS.iter().any(|kw| {
            t.strip_prefix(kw)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', ':', '(', '\t']))
        })
}

/// Put `header` on line 1 of `code` without shifting the lines below it.
///
/// - A blank first line (the usual `@python {⏎`) is replaced by the header.
/// - Otherwise the header is prefixed onto line 1 (`import m; <line 1>`).
/// - Exception: a Python line 1 that opens a block (`for …:`) can't take a
///   prefix, so the header gets its own line and guest lines shift by one.
///
/// The rest of the block is dedented first so a column-0 header never sits
/// above an indented body; dedenting keeps every line, so the count holds.
pub fn splice_header(lang: &str, code: &str, header: &str) -> String {
    let python = canonical_lang(lang) == Some("python");
    let lines: Vec<&str> = code.split('\n').collect();
    let indent = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    let mut lines: Vec<String> = lines
        .iter()
        .map(|l| l.get(indent..).unwrap_or("").to_string())
        .collect();
    if lines[0].trim().is_empty() {
        lines[0] = header.to_string();
    } else if python && python_line_opens_block(&lines[0]) {
        lines.insert(0, header.to_string());
    } else {
        let sep = if python { "; " } else { " " };
        lines[0] = format!("{header}{sep}{}", lines[0]);
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn py(path: &str, alias: Option<&str>, selective: &[&str]) -> ImportStatement {
        ImportStatement::PolyglotModule {
            language: "python".into(),
            module_path: path.into(),
            alias: alias.map(str::to_string),
            selective: selective.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn js(path: &str, alias: Option<&str>, selective: &[&str]) -> ImportStatement {
        ImportStatement::PolyglotModule {
            language: "javascript".into(),
            module_path: path.into(),
            alias: alias.map(str::to_string),
            selective: selective.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn python_forms() {
        let h = import_header(
            "python",
            &[py("math", Some("m"), &[]), py("os.path", None, &["join"])],
        );
        assert_eq!(
            h.unwrap().unwrap(),
            "import math; import math as m; from os.path import join"
        );
        assert_eq!(
            import_header("py", &[py("json", None, &[])])
                .unwrap()
                .unwrap(),
            "import json"
        );
    }

    #[test]
    fn python_rejects_non_module_paths() {
        assert!(import_header("python", &[py("math; import os", None, &[])]).is_err());
        assert!(import_header("python", &[py("os.path", None, &["a.b"])]).is_err());
        assert!(import_header("python", &[py("math", Some("x y"), &[])]).is_err());
    }

    #[test]
    fn javascript_forms() {
        let h = import_header(
            "js",
            &[js("fs", Some("f"), &[]), js("path", None, &["join", "sep"])],
        );
        assert_eq!(
            h.unwrap().unwrap(),
            "const fs = require(\"fs\"); const f = require(\"fs\"); const { join, sep } = require(\"path\");"
        );
        assert_eq!(
            import_header("javascript", &[js("node:fs", None, &[])])
                .unwrap()
                .unwrap(),
            "const fs = require(\"node:fs\");"
        );
        assert!(import_header("javascript", &[js("@scope/", None, &[])]).is_err());
    }

    #[test]
    fn bash_and_unknown_are_rejected() {
        let bash = ImportStatement::PolyglotModule {
            language: "bash".into(),
            module_path: "lib.sh".into(),
            alias: None,
            selective: vec![],
        };
        assert!(import_header("bash", &[bash]).unwrap_err().contains("bash"));
        assert!(check_polyglot_use("ruby").is_err());
    }

    #[test]
    fn splice_keeps_line_numbers() {
        // Blank first line (the usual `@python {⏎`): replaced, nothing shifts.
        assert_eq!(
            splice_header("python", "\n    x = 1\n    y = 2\n", "import m"),
            "import m\nx = 1\ny = 2\n"
        );
        // One-liner: prefixed onto line 1.
        assert_eq!(
            splice_header("python", " print(1) ", "import m"),
            "import m; print(1) "
        );
        // Marshaling prologue already on line 1.
        assert_eq!(
            splice_header("python", "import json as j\nx = 1", "import m"),
            "import m; import json as j\nx = 1"
        );
        assert_eq!(
            splice_header("js", " f(1) ", "const a = require(\"a\");"),
            "const a = require(\"a\"); f(1) "
        );
    }

    #[test]
    fn splice_gives_a_python_block_opener_its_own_line() {
        assert_eq!(
            splice_header("python", "for i in r: print(i)", "import m"),
            "import m\nfor i in r: print(i)"
        );
        assert!(!python_line_opens_block("format(x)"));
        assert!(python_line_opens_block("if x:"));
    }
}
