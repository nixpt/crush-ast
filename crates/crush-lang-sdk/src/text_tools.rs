//! `text.head` / `text.tail` / `text.wc` / `text.cut` / `text.grep` — file
//! text tools, registered with the other filesystem host capabilities.
//!
//! Ported from exosphere `core/base/stdlib/src/text.rs` (W10 / CRUSH-122).
//! nanovm put these in its stdlib with ambient access to any path; here they
//! read files, so they follow the `fs.*` rules instead: registered only when
//! the host grants filesystem access (`HostCapsBuilder::fs` / `--fs`), and
//! every path is resolved inside the sandbox root (`--fs-root`) by the same
//! `resolve_path` the `fs.*` caps use. `text.grep` needs the `regex`
//! dependency and so also needs the `stdlib` feature.

use crate::host_caps::resolve_path;
use crush_vm::vm::Value;
use crush_vm::{HostCap, HostCapSpec, HostCaps};
use std::collections::HashMap;
use std::path::Path;

pub(crate) fn register(caps: &mut HostCaps, root: &str) {
    let root = root.to_string();
    caps.register(Box::new(TextHeadCap { root: root.clone() }));
    caps.register(Box::new(TextTailCap { root: root.clone() }));
    caps.register(Box::new(TextWcCap { root: root.clone() }));
    caps.register(Box::new(TextCutCap { root: root.clone() }));
    #[cfg(feature = "stdlib")]
    caps.register(Box::new(TextGrepCap { root }));
}

fn read(cap: &str, path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{cap} {}: {e}", path.display()))
}

fn count_arg(cap: &str, v: &Value) -> Result<usize, String> {
    match v {
        Value::Int(n) if *n >= 0 => Ok(*n as usize),
        other => Err(format!("{cap}: expected a non-negative int, got {other}")),
    }
}

fn str_array<'a>(items: impl Iterator<Item = &'a str>) -> Value {
    Value::new_array(items.map(|s| Value::Str(s.to_string())).collect())
}

macro_rules! fs_text_cap {
    ($name:ident, $full:expr, $argc:expr, $body:expr) => {
        pub struct $name {
            root: String,
        }
        impl HostCap for $name {
            fn spec(&self) -> HostCapSpec {
                HostCapSpec {
                    name: $full.to_string(),
                    argc: $argc,
                    returns: true,
                }
            }
            fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
                #[allow(clippy::redundant_closure_call)]
                ($body)(self.root.as_str(), &args)
            }
        }
    };
}

// First `n` lines of a file.
fs_text_cap!(
    TextHeadCap,
    "text.head",
    Some(2),
    |root: &str, args: &[Value]| {
        let path = resolve_path(root, &args[0])?;
        let n = count_arg("text.head", &args[1])?;
        let content = read("text.head", &path)?;
        Ok(Some(str_array(content.lines().take(n))))
    }
);

// Last `n` lines of a file.
fs_text_cap!(
    TextTailCap,
    "text.tail",
    Some(2),
    |root: &str, args: &[Value]| {
        let path = resolve_path(root, &args[0])?;
        let n = count_arg("text.tail", &args[1])?;
        let content = read("text.tail", &path)?;
        let lines: Vec<&str> = content.lines().collect();
        Ok(Some(str_array(
            lines[lines.len().saturating_sub(n)..].iter().copied(),
        )))
    }
);

// `{lines, words, chars}` of a file, like `wc(1)` (chars are Unicode scalars).
fs_text_cap!(
    TextWcCap,
    "text.wc",
    Some(1),
    |root: &str, args: &[Value]| {
        let path = resolve_path(root, &args[0])?;
        let content = read("text.wc", &path)?;
        let mut m = HashMap::new();
        m.insert(
            "lines".to_string(),
            Value::Int(content.lines().count() as i64),
        );
        m.insert(
            "words".to_string(),
            Value::Int(content.split_whitespace().count() as i64),
        );
        m.insert(
            "chars".to_string(),
            Value::Int(content.chars().count() as i64),
        );
        Ok(Some(Value::new_map(m)))
    }
);

// Column `col` (1-based) of every line split on `delim`; "" where a line is
// too short, like `cut -d -f`.
fs_text_cap!(
    TextCutCap,
    "text.cut",
    Some(3),
    |root: &str, args: &[Value]| {
        let path = resolve_path(root, &args[0])?;
        let delim = args[1].to_string();
        let col = count_arg("text.cut", &args[2])?;
        if col == 0 || delim.is_empty() {
            return Err("text.cut: column is 1-based and the delimiter must be non-empty".into());
        }
        let content = read("text.cut", &path)?;
        Ok(Some(str_array(content.lines().map(|line| {
            line.split(delim.as_str()).nth(col - 1).unwrap_or("")
        }))))
    }
);

/// Path shown to the program: relative to the sandbox root, never the host's
/// absolute path.
#[cfg(feature = "stdlib")]
fn display_path(root: &str, path: &Path) -> String {
    let root = Path::new(root);
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    path.strip_prefix(&root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

/// Every regular file under `dir`, depth-first in name order, not following
/// symlinks (nanovm's `WalkDir::follow_links(false)`).
#[cfg(feature = "stdlib")]
fn walk_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        match entry.file_type() {
            Ok(t) if t.is_dir() => walk_files(&entry.path(), out),
            Ok(t) if t.is_file() => out.push(entry.path()),
            _ => {}
        }
    }
}

// `text.grep(pattern, path [, recursive [, ignore_case]])` → array of
// `{file, line, content}` maps (line is 1-based). A directory needs
// `recursive = true`; unreadable / non-UTF-8 files are skipped while walking.
#[cfg(feature = "stdlib")]
fs_text_cap!(
    TextGrepCap,
    "text.grep",
    None,
    |root: &str, args: &[Value]| {
        if !(2..=4).contains(&args.len()) {
            return Err(format!(
                "text.grep: expected 2 to 4 arguments (pattern, path, recursive, ignore_case), got {}",
                args.len()
            ));
        }
        let flag = |i: usize| matches!(args.get(i), Some(Value::Bool(true)));
        let (recursive, ignore_case) = (flag(2), flag(3));
        let pattern = args[0].to_string();
        let re = regex::RegexBuilder::new(&pattern)
            .case_insensitive(ignore_case)
            .build()
            .map_err(|e| format!("text.grep: invalid pattern: {e}"))?;
        let path = resolve_path(root, &args[1])?;

        let files = if path.is_file() {
            vec![path]
        } else if path.is_dir() && recursive {
            let mut files = Vec::new();
            walk_files(&path, &mut files);
            files
        } else {
            return Err(
                "text.grep: path must be a file, or a directory with recursive = true".into(),
            );
        };

        let single = files.len() == 1;
        let mut hits = Vec::new();
        for file in files {
            let content = match std::fs::read_to_string(&file) {
                Ok(c) => c,
                Err(e) if single => return Err(format!("text.grep {}: {e}", file.display())),
                Err(_) => continue,
            };
            let shown = display_path(root, &file);
            for (i, line) in content.lines().enumerate() {
                if re.is_match(line) {
                    let mut m = HashMap::new();
                    m.insert("file".to_string(), Value::Str(shown.clone()));
                    m.insert("line".to_string(), Value::Int(i as i64 + 1));
                    m.insert("content".to_string(), Value::Str(line.to_string()));
                    hits.push(Value::new_map(m));
                }
            }
        }
        Ok(Some(Value::new_array(hits)))
    }
);

#[cfg(test)]
mod tests {
    use super::*;

    fn sandbox() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "one two\nthree\nfour five six\n").unwrap();
        std::fs::write(dir.path().join("t.csv"), "x,1\ny,2\nz\n").unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub/b.txt"), "Three\n").unwrap();
        dir
    }

    fn call(
        dir: &tempfile::TempDir,
        name: &str,
        args: Vec<Value>,
    ) -> Result<Option<Value>, String> {
        let mut caps = HostCaps::new();
        register(&mut caps, dir.path().to_str().unwrap());
        caps.get(name).expect(name).call(args)
    }

    fn s(v: &str) -> Value {
        Value::Str(v.to_string())
    }

    fn strs(items: &[&str]) -> Value {
        str_array(items.iter().copied())
    }

    #[test]
    fn head_tail_cut() {
        let d = sandbox();
        assert_eq!(
            call(&d, "text.head", vec![s("a.txt"), Value::Int(2)]),
            Ok(Some(strs(&["one two", "three"])))
        );
        assert_eq!(
            call(&d, "text.tail", vec![s("a.txt"), Value::Int(1)]),
            Ok(Some(strs(&["four five six"])))
        );
        assert_eq!(
            call(&d, "text.tail", vec![s("a.txt"), Value::Int(10)]),
            call(&d, "text.head", vec![s("a.txt"), Value::Int(10)]),
        );
        assert_eq!(
            call(&d, "text.cut", vec![s("t.csv"), s(","), Value::Int(2)]),
            Ok(Some(strs(&["1", "2", ""])))
        );
        assert!(call(&d, "text.cut", vec![s("t.csv"), s(","), Value::Int(0)]).is_err());
    }

    #[test]
    fn wc_counts_lines_words_chars() {
        let d = sandbox();
        let Some(Value::Map(m)) = call(&d, "text.wc", vec![s("a.txt")]).unwrap() else {
            panic!("expected map");
        };
        let m = m.borrow();
        assert_eq!(m["lines"], Value::Int(3));
        assert_eq!(m["words"], Value::Int(6));
        assert_eq!(m["chars"], Value::Int(28));
    }

    #[test]
    fn paths_stay_inside_the_sandbox() {
        let d = sandbox();
        let err = call(&d, "text.head", vec![s("../../etc/passwd"), Value::Int(1)]).unwrap_err();
        assert!(err.contains("escapes sandbox"), "{err}");
        assert!(call(&d, "text.wc", vec![s("/etc/passwd")]).is_err());
    }

    #[cfg(feature = "stdlib")]
    #[test]
    fn grep_file_and_recursive_directory() {
        let d = sandbox();
        let Some(Value::Array(hits)) =
            call(&d, "text.grep", vec![s("^three"), s("a.txt")]).unwrap()
        else {
            panic!("expected array");
        };
        assert_eq!(hits.borrow().len(), 1);

        assert!(call(&d, "text.grep", vec![s("three"), s(".")]).is_err());
        let Some(Value::Array(hits)) = call(
            &d,
            "text.grep",
            vec![s("three"), s("."), Value::Bool(true), Value::Bool(true)],
        )
        .unwrap() else {
            panic!("expected array");
        };
        let files: Vec<String> = hits
            .borrow()
            .iter()
            .map(|h| match h {
                Value::Map(m) => m.borrow()["file"].to_string(),
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(files, vec!["a.txt", "sub/b.txt"]);
    }
}
