//! `fs.ls` / `fs.cat` / `fs.pwd` / `fs.cd` / `fs.mkdir` / `fs.rm` / `fs.cp` /
//! `fs.mv` / `fs.touch` / `fs.find` — the filesystem coreutils (CRUSH-151).
//!
//! Registered next to `fs.read` / `fs.write` / `fs.exists` / `fs.list`, and
//! only when the host grants filesystem access (`HostCapsBuilder::fs` /
//! `--fs`). Every path goes through [`FsSandbox`], so it is resolved against
//! the working directory and can never leave the sandbox root (`--fs-root`):
//! not with `..`, not through a symlink, not for a target that does not exist
//! yet.
//!
//! `fs.cd` moves that working directory (decision C-5). It is state of the
//! capability registry — shared by every `fs.*` and `text.*` file cap built
//! with it, i.e. local to the VM that registry serves — and it never
//! `chdir`s the host process. `fs.pwd` reports it relative to the sandbox
//! root (`"."` at the root), never as a host path.
//!
//! Re-implemented over `std::fs`; the names and argument orders follow the
//! older exosphere / `nixpt/crush` coreutils.

use crate::host_caps::FsSandbox;
use crush_vm::vm::Value;
use crush_vm::{HostCap, HostCapSpec, HostCaps};
use std::path::Path;

/// Most entries `fs.find` returns before it gives up with an error, so a
/// walk over a huge tree cannot exhaust host memory.
pub const FIND_MAX_RESULTS: usize = 10_000;

pub(crate) fn register(caps: &mut HostCaps, fs: &FsSandbox) {
    let names = [
        "fs.ls", "fs.cat", "fs.pwd", "fs.cd", "fs.mkdir", "fs.rm", "fs.cp", "fs.mv", "fs.touch",
        "fs.find",
    ];
    for name in names {
        caps.register(Box::new(FsToolCap {
            name,
            fs: fs.clone(),
        }));
    }
}

/// One coreutil. A single struct keyed by name keeps the arity table and the
/// dispatch next to each other.
struct FsToolCap {
    name: &'static str,
    fs: FsSandbox,
}

impl HostCap for FsToolCap {
    fn spec(&self) -> HostCapSpec {
        let argc = match self.name {
            "fs.pwd" => Some(0),
            "fs.cat" | "fs.cd" | "fs.touch" => Some(1),
            "fs.mv" => Some(2),
            // optional trailing argument: ls [dir], mkdir/rm path [flag],
            // cp src dst [recursive], find dir [pattern]
            _ => None,
        };
        HostCapSpec {
            name: self.name.to_string(),
            argc,
            returns: true,
        }
    }

    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        let fs = &self.fs;
        let name = self.name;
        let value = match name {
            "fs.ls" => {
                arity(name, &args, 0, 1)?;
                let dir = match args.first() {
                    Some(v) => fs.resolve(v)?,
                    None => fs.resolve(&Value::Str(".".into()))?,
                };
                list_dir(name, &dir, fs)?
            }
            "fs.cat" => {
                let path = fs.resolve(&args[0])?;
                Value::Str(
                    std::fs::read_to_string(&path)
                        .map_err(|e| format!("{name} {}: {e}", fs.display(&path)))?,
                )
            }
            "fs.pwd" => Value::Str(fs.pwd()),
            "fs.cd" => {
                fs.cd(&args[0])?;
                Value::Str(fs.pwd())
            }
            "fs.mkdir" => {
                arity(name, &args, 1, 2)?;
                let path = fs.resolve(&args[0])?;
                let result = if flag(&args, 1) {
                    std::fs::create_dir_all(&path)
                } else {
                    std::fs::create_dir(&path)
                };
                result.map_err(|e| format!("{name} {}: {e}", fs.display(&path)))?;
                Value::Null
            }
            "fs.rm" => {
                arity(name, &args, 1, 2)?;
                let path = fs.resolve_nofollow(&args[0])?;
                fs.refuse_working_dir(name, &path)?;
                let meta = std::fs::symlink_metadata(&path)
                    .map_err(|e| format!("{name} {}: {e}", fs.display(&path)))?;
                let result = if meta.is_dir() {
                    if !flag(&args, 1) {
                        return Err(format!(
                            "{name} {}: is a directory (pass true as the second argument to remove it recursively)",
                            fs.display(&path)
                        ));
                    }
                    std::fs::remove_dir_all(&path)
                } else {
                    std::fs::remove_file(&path)
                };
                result.map_err(|e| format!("{name} {}: {e}", fs.display(&path)))?;
                Value::Null
            }
            "fs.cp" => {
                arity(name, &args, 2, 3)?;
                let src = fs.resolve(&args[0])?;
                let dst = fs.resolve(&args[1])?;
                if src.is_dir() {
                    if !flag(&args, 2) {
                        return Err(format!(
                            "{name} {}: is a directory (pass true as the third argument to copy it recursively)",
                            fs.display(&src)
                        ));
                    }
                    if dst.starts_with(&src) {
                        return Err(format!(
                            "{name}: cannot copy {} into itself",
                            fs.display(&src)
                        ));
                    }
                    copy_tree(fs, &src, &dst)?;
                } else {
                    std::fs::copy(&src, &dst).map_err(|e| {
                        format!("{name} {} -> {}: {e}", fs.display(&src), fs.display(&dst))
                    })?;
                }
                Value::Null
            }
            "fs.mv" => {
                let src = fs.resolve_nofollow(&args[0])?;
                let dst = fs.resolve_nofollow(&args[1])?;
                fs.refuse_working_dir(name, &src)?;
                std::fs::rename(&src, &dst).map_err(|e| {
                    format!("{name} {} -> {}: {e}", fs.display(&src), fs.display(&dst))
                })?;
                Value::Null
            }
            "fs.touch" => {
                let path = fs.resolve(&args[0])?;
                let file = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                    .map_err(|e| format!("{name} {}: {e}", fs.display(&path)))?;
                file.set_modified(std::time::SystemTime::now())
                    .map_err(|e| format!("{name} {}: {e}", fs.display(&path)))?;
                Value::Null
            }
            "fs.find" => {
                arity(name, &args, 1, 2)?;
                let base = crate::caps::value_as_text(&args[0]);
                let dir = fs.resolve(&args[0])?;
                let pattern = args.get(1).map(crate::caps::value_as_text);
                let mut found = Vec::new();
                find(&dir, Path::new(&base), pattern.as_deref(), &mut found)
                    .map_err(|e| format!("{name} {base}: {e}"))?;
                found.sort();
                Value::new_array(found.into_iter().map(Value::Str).collect())
            }
            other => return Err(format!("{other}: not an fs coreutil")),
        };
        Ok(Some(value))
    }
}

fn arity(name: &str, args: &[Value], min: usize, max: usize) -> Result<(), String> {
    if args.len() < min || args.len() > max {
        return Err(format!(
            "{name}: expected {min}..={max} arguments, got {}",
            args.len()
        ));
    }
    Ok(())
}

/// An optional flag argument (`parents`, `recursive`): `true` or a non-zero
/// int turns it on; absent or anything else leaves it off.
fn flag(args: &[Value], index: usize) -> bool {
    matches!(args.get(index), Some(Value::Bool(true)))
        || matches!(args.get(index), Some(Value::Int(n)) if *n != 0)
}

/// Directory entry names, sorted so output does not depend on the host
/// filesystem's iteration order. Shared with `fs.list`.
pub(crate) fn list_dir(cap: &str, dir: &Path, fs: &FsSandbox) -> Result<Value, String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map_err(|e| format!("{cap} {}: {e}", fs.display(dir)))?
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    names.sort();
    Ok(Value::new_array(
        names.into_iter().map(Value::Str).collect(),
    ))
}

/// Recursive copy that never follows a symlink: a link inside the tree could
/// point outside the sandbox, so it is refused rather than copied through.
fn copy_tree(fs: &FsSandbox, src: &Path, dst: &Path) -> Result<(), String> {
    let err = |p: &Path, e: std::io::Error| format!("fs.cp {}: {e}", fs.display(p));
    std::fs::create_dir_all(dst).map_err(|e| err(dst, e))?;
    for entry in std::fs::read_dir(src).map_err(|e| err(src, e))? {
        let entry = entry.map_err(|e| err(src, e))?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        let kind = entry.file_type().map_err(|e| err(&from, e))?;
        if kind.is_symlink() {
            return Err(format!(
                "fs.cp {}: refusing to copy a symlink in a recursive copy",
                fs.display(&from)
            ));
        } else if kind.is_dir() {
            copy_tree(fs, &from, &to)?;
        } else {
            std::fs::copy(&from, &to).map_err(|e| err(&from, e))?;
        }
    }
    Ok(())
}

/// Walk `dir` (not following symlinks), collecting every entry below it as
/// `base/<relative path>` — the form the caller can pass straight back to
/// another `fs.*` call — optionally filtered by a `*`/`?` name glob.
fn find(
    dir: &Path,
    base: &Path,
    pattern: Option<&str>,
    found: &mut Vec<String>,
) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let shown = base.join(&name);
        if pattern.is_none_or(|p| glob_match(p, &name.to_string_lossy())) {
            if found.len() >= FIND_MAX_RESULTS {
                return Err(std::io::Error::other(format!(
                    "more than {FIND_MAX_RESULTS} results"
                )));
            }
            found.push(clean(&shown));
        }
        if entry.file_type()?.is_dir() {
            find(&entry.path(), &shown, pattern, found)?;
        }
    }
    Ok(())
}

/// `./a/b` → `a/b`, so a search from the working directory reads naturally.
fn clean(p: &Path) -> String {
    let s = p.to_string_lossy();
    s.strip_prefix("./").unwrap_or(&s).to_string()
}

/// Shell-style name glob: `*` matches any run of characters, `?` exactly one.
fn glob_match(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let n: Vec<char> = name.chars().collect();
    let (mut pi, mut ni) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while ni < n.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == n[ni]) {
            pi += 1;
            ni += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some((pi, ni));
            pi += 1;
        } else if let Some((sp, sn)) = star {
            pi = sp + 1;
            ni = sn + 1;
            star = Some((sp, sn + 1));
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|&c| c == '*')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HostCapsBuilder;

    struct Sandbox {
        dir: tempfile::TempDir,
        caps: HostCaps,
    }

    impl Sandbox {
        /// `<tmp>/root` is the sandbox; `<tmp>/outside` sits next to it and
        /// `root/out` is a symlink to it.
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().join("root");
            std::fs::create_dir_all(root.join("sub")).unwrap();
            std::fs::create_dir(dir.path().join("outside")).unwrap();
            std::fs::write(dir.path().join("outside/secret.txt"), "secret").unwrap();
            std::os::unix::fs::symlink(dir.path().join("outside"), root.join("out")).unwrap();
            std::fs::write(root.join("a.txt"), "alpha").unwrap();
            let caps = HostCapsBuilder::new()
                .fs(true)
                .fs_root(root.to_str().unwrap())
                .build();
            Self { dir, caps }
        }

        fn root(&self) -> std::path::PathBuf {
            self.dir.path().join("root")
        }

        fn outside(&self) -> std::path::PathBuf {
            self.dir.path().join("outside")
        }

        fn call(&self, cap: &str, args: &[&str]) -> Result<Option<Value>, String> {
            let args = args.iter().map(|a| Value::Str(a.to_string())).collect();
            self.call_values(cap, args)
        }

        fn call_values(&self, cap: &str, args: Vec<Value>) -> Result<Option<Value>, String> {
            self.caps
                .get(cap)
                .unwrap_or_else(|| panic!("{cap} not registered"))
                .call(args)
        }
    }

    fn text(v: Option<Value>) -> String {
        match v {
            Some(Value::Str(s)) => s,
            other => panic!("expected a string, got {other:?}"),
        }
    }

    fn strings(v: Option<Value>) -> Vec<String> {
        match v {
            Some(Value::Array(a)) => a
                .borrow()
                .iter()
                .map(|v| match v {
                    Value::Str(s) => s.clone(),
                    other => panic!("expected strings, got {other:?}"),
                })
                .collect(),
            other => panic!("expected an array, got {other:?}"),
        }
    }

    fn assert_escape(result: Result<Option<Value>, String>, what: &str) {
        let err = result.expect_err(what);
        assert!(
            err.contains("escapes sandbox") || err.contains("absolute paths"),
            "{what}: {err}"
        );
    }

    #[test]
    fn every_coreutil_needs_the_fs_grant() {
        let caps = HostCapsBuilder::new().stdlib(true).build();
        for cap in [
            "fs.ls", "fs.cat", "fs.pwd", "fs.cd", "fs.mkdir", "fs.rm", "fs.cp", "fs.mv",
            "fs.touch", "fs.find",
        ] {
            assert!(caps.get(cap).is_none(), "{cap} registered without --fs");
        }
    }

    #[test]
    fn every_path_argument_is_confined_to_the_root() {
        let sb = Sandbox::new();
        // `..` escapes, for an existing target (outside/secret.txt) and a new one.
        for path in [
            "../outside/secret.txt",
            "../outside/new",
            "sub/../../x",
            "/etc/passwd",
        ] {
            assert_escape(sb.call("fs.cat", &[path]), &format!("cat {path}"));
            assert_escape(sb.call("fs.touch", &[path]), &format!("touch {path}"));
            assert_escape(
                sb.call("fs.mkdir", &[path, "true"]),
                &format!("mkdir {path}"),
            );
            assert_escape(sb.call("fs.rm", &[path]), &format!("rm {path}"));
            assert_escape(sb.call("fs.cp", &["a.txt", path]), &format!("cp -> {path}"));
            assert_escape(
                sb.call("fs.cp", &[path, "copy.txt"]),
                &format!("cp {path} ->"),
            );
            assert_escape(sb.call("fs.mv", &["a.txt", path]), &format!("mv -> {path}"));
            assert_escape(sb.call("fs.mv", &[path, "moved"]), &format!("mv {path} ->"));
            assert_escape(sb.call("fs.cd", &[path]), &format!("cd {path}"));
            assert_escape(sb.call("fs.ls", &[path]), &format!("ls {path}"));
            assert_escape(sb.call("fs.find", &[path]), &format!("find {path}"));
        }
        // Through the symlink `out` -> outside: existing and not-yet-existing.
        for path in ["out/secret.txt", "out/new.txt", "out"] {
            assert_escape(sb.call("fs.cat", &[path]), &format!("cat {path}"));
            assert_escape(sb.call("fs.touch", &[path]), &format!("touch {path}"));
            assert_escape(sb.call("fs.cp", &["a.txt", path]), &format!("cp -> {path}"));
            assert_escape(sb.call("fs.cd", &[path]), &format!("cd {path}"));
            assert_escape(sb.call("fs.ls", &[path]), &format!("ls {path}"));
            assert_escape(sb.call("fs.find", &[path]), &format!("find {path}"));
        }
        assert_escape(
            sb.call("fs.mkdir", &["out/d", "true"]),
            "mkdir through link",
        );
        assert_escape(sb.call("fs.rm", &["out/secret.txt"]), "rm through link");
        assert_escape(
            sb.call("fs.mv", &["out/secret.txt", "stolen"]),
            "mv through link",
        );

        // Nothing outside changed.
        let mut outside: Vec<_> = std::fs::read_dir(sb.outside())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        outside.sort();
        assert_eq!(outside, ["secret.txt"]);
        assert_eq!(
            std::fs::read_to_string(sb.outside().join("secret.txt")).unwrap(),
            "secret"
        );
    }

    #[test]
    fn rm_and_mv_act_on_a_symlink_not_its_target() {
        let sb = Sandbox::new();
        sb.call("fs.mv", &["out", "renamed-link"]).unwrap();
        assert!(sb.root().join("renamed-link").is_symlink());
        sb.call("fs.rm", &["renamed-link"]).unwrap();
        assert!(!sb.root().join("renamed-link").exists());
        assert!(
            sb.outside().join("secret.txt").exists(),
            "rm followed the link"
        );
    }

    #[test]
    fn cd_moves_a_vm_local_working_directory() {
        let host_cwd = std::env::current_dir().unwrap();
        let sb = Sandbox::new();
        assert_eq!(text(sb.call("fs.pwd", &[]).unwrap()), ".");
        assert_eq!(text(sb.call("fs.cd", &["sub"]).unwrap()), "sub");
        assert_eq!(text(sb.call("fs.pwd", &[]).unwrap()), "sub");

        // Every fs.* (and text.*) call now resolves against `sub`.
        sb.call("fs.touch", &["here.txt"]).unwrap();
        assert!(sb.root().join("sub/here.txt").exists());
        sb.call_values(
            "fs.write",
            vec![Value::Str("w.txt".into()), Value::Str("w".into())],
        )
        .unwrap();
        assert!(sb.root().join("sub/w.txt").exists());
        assert_eq!(text(sb.call("fs.cat", &["../a.txt"]).unwrap()), "alpha");
        assert_eq!(
            strings(sb.call("fs.ls", &[]).unwrap()),
            ["here.txt", "w.txt"]
        );
        assert!(
            sb.caps
                .get("text.wc")
                .unwrap()
                .call(vec![Value::Str("w.txt".into())])
                .is_ok()
        );

        // `..` back to the root is fine; one more is an escape.
        assert_eq!(text(sb.call("fs.cd", &[".."]).unwrap()), ".");
        assert_escape(sb.call("fs.cd", &[".."]), "cd above the root");
        assert!(
            sb.call("fs.cd", &["a.txt"])
                .unwrap_err()
                .contains("not a directory")
        );
        assert!(sb.call("fs.cd", &["missing"]).is_err());
        assert_eq!(text(sb.call("fs.pwd", &[]).unwrap()), ".");

        // The host process never moved.
        assert_eq!(std::env::current_dir().unwrap(), host_cwd);

        // A second registry has its own working directory.
        sb.call("fs.cd", &["sub"]).unwrap();
        let other = HostCapsBuilder::new()
            .fs(true)
            .fs_root(sb.root().to_str().unwrap())
            .build();
        assert_eq!(
            text(other.get("fs.pwd").unwrap().call(vec![]).unwrap()),
            "."
        );
    }

    #[test]
    fn rm_refuses_the_root_and_the_working_directory() {
        let sb = Sandbox::new();
        for path in [".", "sub/.."] {
            let err = sb.call("fs.rm", &[path, "true"]).unwrap_err();
            assert!(err.contains("refusing"), "{path}: {err}");
        }
        sb.call("fs.cd", &["sub"]).unwrap();
        assert!(
            sb.call("fs.rm", &["../sub", "true"])
                .unwrap_err()
                .contains("refusing")
        );
        assert!(
            sb.call("fs.mv", &["../sub", "../moved"])
                .unwrap_err()
                .contains("refusing")
        );
        assert!(sb.root().join("sub").is_dir());
    }

    #[test]
    fn mkdir_rm_cp_mv_touch() {
        let sb = Sandbox::new();
        let root = sb.root();

        assert!(sb.call("fs.mkdir", &["x/y"]).is_err(), "no parents flag");
        sb.call_values(
            "fs.mkdir",
            vec![Value::Str("x/y".into()), Value::Bool(true)],
        )
        .unwrap();
        assert!(root.join("x/y").is_dir());

        sb.call("fs.touch", &["x/y/t.txt"]).unwrap();
        assert_eq!(std::fs::read_to_string(root.join("x/y/t.txt")).unwrap(), "");
        std::fs::write(root.join("x/y/t.txt"), "kept").unwrap();
        sb.call("fs.touch", &["x/y/t.txt"]).unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("x/y/t.txt")).unwrap(),
            "kept"
        );

        sb.call("fs.cp", &["a.txt", "b.txt"]).unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("b.txt")).unwrap(),
            "alpha"
        );
        assert!(
            sb.call("fs.cp", &["x", "x2"])
                .unwrap_err()
                .contains("recursively")
        );
        sb.call_values(
            "fs.cp",
            vec![
                Value::Str("x".into()),
                Value::Str("x2".into()),
                Value::Bool(true),
            ],
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("x2/y/t.txt")).unwrap(),
            "kept"
        );
        let into_itself = vec![
            Value::Str("x".into()),
            Value::Str("x/inner".into()),
            Value::Bool(true),
        ];
        assert!(
            sb.call_values("fs.cp", into_itself)
                .unwrap_err()
                .contains("into itself")
        );

        sb.call("fs.mv", &["b.txt", "c.txt"]).unwrap();
        assert!(!root.join("b.txt").exists() && root.join("c.txt").exists());

        assert!(
            sb.call("fs.rm", &["x"])
                .unwrap_err()
                .contains("recursively")
        );
        sb.call("fs.rm", &["c.txt"]).unwrap();
        sb.call_values("fs.rm", vec![Value::Str("x".into()), Value::Bool(true)])
            .unwrap();
        assert!(!root.join("c.txt").exists() && !root.join("x").exists());
    }

    #[test]
    fn cp_refuses_symlinks_inside_a_recursive_copy() {
        let sb = Sandbox::new();
        std::fs::create_dir(sb.root().join("tree")).unwrap();
        std::os::unix::fs::symlink(sb.outside(), sb.root().join("tree/link")).unwrap();
        let args = vec![
            Value::Str("tree".into()),
            Value::Str("copy".into()),
            Value::Bool(true),
        ];
        assert!(
            sb.call_values("fs.cp", args)
                .unwrap_err()
                .contains("symlink")
        );
        assert!(!sb.root().join("copy/link/secret.txt").exists());
    }

    #[test]
    fn ls_and_find() {
        let sb = Sandbox::new();
        for f in ["sub/one.txt", "sub/two.log", "sub/deep/three.txt"] {
            sb.call("fs.mkdir", &["sub/deep"]).ok();
            sb.call("fs.touch", &[f]).unwrap();
        }
        assert_eq!(
            strings(sb.call("fs.ls", &[]).unwrap()),
            ["a.txt", "out", "sub"]
        );
        assert_eq!(
            strings(sb.call("fs.ls", &["sub"]).unwrap()),
            strings(sb.call("fs.list", &["sub"]).unwrap())
        );
        assert_eq!(
            strings(sb.call("fs.find", &["sub"]).unwrap()),
            [
                "sub/deep",
                "sub/deep/three.txt",
                "sub/one.txt",
                "sub/two.log"
            ]
        );
        assert_eq!(
            strings(sb.call("fs.find", &["sub", "*.txt"]).unwrap()),
            ["sub/deep/three.txt", "sub/one.txt"]
        );
        // From inside `sub`, results are relative to the working directory
        // and can be passed straight back.
        sb.call("fs.cd", &["sub"]).unwrap();
        let found = strings(sb.call("fs.find", &[".", "*.log"]).unwrap());
        assert_eq!(found, ["two.log"]);
        assert_eq!(text(sb.call("fs.cat", &[&found[0]]).unwrap()), "");
        // `find` does not descend through the `out` symlink.
        sb.call("fs.cd", &[".."]).unwrap();
        assert!(
            !strings(sb.call("fs.find", &["."]).unwrap())
                .iter()
                .any(|p| p.contains("secret"))
        );
    }

    #[test]
    fn errors_show_sandbox_paths_not_host_paths() {
        let sb = Sandbox::new();
        let err = sb.call("fs.cat", &["missing.txt"]).unwrap_err();
        assert!(err.starts_with("fs.cat missing.txt:"), "{err}");
        assert!(!err.contains(sb.dir.path().to_str().unwrap()), "{err}");
    }

    #[test]
    fn glob() {
        assert!(glob_match("*.txt", "a.txt"));
        assert!(!glob_match("*.txt", "a.log"));
        assert!(glob_match("file?.*", "file1.txt"));
        assert!(glob_match("*", ""));
        assert!(!glob_match("a*b", "acd"));
        assert!(glob_match("a*b*c", "aXbYbc"));
    }
}
