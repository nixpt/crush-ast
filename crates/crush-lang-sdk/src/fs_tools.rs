//! `fs.ls` / `fs.cat` / `fs.pwd` / `fs.cd` / `fs.mkdir` / `fs.rm` / `fs.cp` /
//! `fs.mv` / `fs.touch` / `fs.find` — the filesystem coreutils (CRUSH-151).
//! `fs.find dir [pattern] [type]` takes a `find -type` filter too (CRUSH-240).
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
//! Every file operation goes through the sandbox's [`FsBackend`](crate::fs_backend::FsBackend)
//! (the disk, or memory); the names and argument orders follow the older
//! exosphere / `nixpt/crush` coreutils.

use crate::fs_backend::Kind;
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
                    fs.backend()
                        .read_to_string(&path)
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
                    fs.backend().create_dir_all(&path)
                } else {
                    fs.backend().create_dir(&path)
                };
                result.map_err(|e| format!("{name} {}: {e}", fs.display(&path)))?;
                Value::Null
            }
            "fs.rm" => {
                arity(name, &args, 1, 2)?;
                let path = fs.resolve_nofollow(&args[0])?;
                fs.refuse_working_dir(name, &path)?;
                let kind = fs.backend().symlink_kind(&path)
                    .map_err(|e| format!("{name} {}: {e}", fs.display(&path)))?;
                let result = if kind == Kind::Dir {
                    if !flag(&args, 1) {
                        return Err(format!(
                            "{name} {}: is a directory (pass true as the second argument to remove it recursively)",
                            fs.display(&path)
                        ));
                    }
                    fs.backend().remove_dir_all(&path)
                } else {
                    fs.backend().remove_file(&path)
                };
                result.map_err(|e| format!("{name} {}: {e}", fs.display(&path)))?;
                Value::Null
            }
            "fs.cp" => {
                arity(name, &args, 2, 3)?;
                let src = fs.resolve(&args[0])?;
                let dst = fs.resolve(&args[1])?;
                if fs.is_dir(&src) {
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
                    fs.backend().copy_file(&src, &dst).map_err(|e| {
                        format!("{name} {} -> {}: {e}", fs.display(&src), fs.display(&dst))
                    })?;
                }
                Value::Null
            }
            "fs.mv" => {
                let src = fs.resolve_nofollow(&args[0])?;
                let dst = fs.resolve_nofollow(&args[1])?;
                fs.refuse_working_dir(name, &src)?;
                fs.backend().rename(&src, &dst).map_err(|e| {
                    format!("{name} {} -> {}: {e}", fs.display(&src), fs.display(&dst))
                })?;
                Value::Null
            }
            "fs.touch" => {
                let path = fs.resolve(&args[0])?;
                fs.backend()
                    .touch(&path)
                    .map_err(|e| format!("{name} {}: {e}", fs.display(&path)))?;
                Value::Null
            }
            "fs.find" => {
                arity(name, &args, 1, 3)?;
                let base = crate::caps::value_as_text(&args[0]);
                let dir = fs.resolve(&args[0])?;
                let pattern = args.get(1).map(crate::caps::value_as_text);
                let want = args.get(2).map(|v| find_type(name, v)).transpose()?;
                let mut found = Vec::new();
                find(fs, &dir, Path::new(&base), pattern.as_deref(), want, &mut found)
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
    let mut names: Vec<String> = fs
        .backend()
        .read_dir(dir)
        .map_err(|e| format!("{cap} {}: {e}", fs.display(dir)))?
        .into_iter()
        .filter_map(|e| e.ok())
        .filter_map(|e| e.name.into_string().ok())
        .collect();
    names.sort();
    Ok(Value::new_array(
        names.into_iter().map(Value::Str).collect(),
    ))
}

/// Recursive copy that never follows a symlink: a link inside the tree could
/// point outside the sandbox, so it is refused rather than copied through.
/// The same goes for the destination: only its top was confined, so a link
/// already sitting at any path below it is refused too (CRUSH-180).
fn copy_tree(fs: &FsSandbox, src: &Path, dst: &Path) -> Result<(), String> {
    let err = |p: &Path, e: std::io::Error| format!("fs.cp {}: {e}", fs.display(p));
    if matches!(fs.backend().symlink_kind(dst), Ok(Kind::Symlink)) {
        return Err(format!(
            "fs.cp {}: refusing to copy onto a symlink in a recursive copy",
            fs.display(dst)
        ));
    }
    fs.backend().create_dir_all(dst).map_err(|e| err(dst, e))?;
    for entry in fs.backend().read_dir(src).map_err(|e| err(src, e))? {
        let entry = entry.map_err(|e| err(src, e))?;
        let from = src.join(&entry.name);
        let to = dst.join(&entry.name);
        let kind = entry.kind.map_err(|e| err(&from, e))?;
        if kind == Kind::Symlink {
            return Err(format!(
                "fs.cp {}: refusing to copy a symlink in a recursive copy",
                fs.display(&from)
            ));
        } else if kind == Kind::Dir {
            copy_tree(fs, &from, &to)?;
        } else {
            fs.backend().copy_file(&from, &to).map_err(|e| err(&to, e))?;
        }
    }
    Ok(())
}

/// Walk `dir` (not following symlinks), collecting every entry below it as
/// `base/<relative path>` — the form the caller can pass straight back to
/// another `fs.*` call — optionally filtered by a `*`/`?` name glob and by
/// type (CRUSH-240: `find -type f`), and always descending into directories.
fn find(
    fs: &FsSandbox,
    dir: &Path,
    base: &Path,
    pattern: Option<&str>,
    want: Option<Kind>,
    found: &mut Vec<String>,
) -> std::io::Result<()> {
    for entry in fs.backend().read_dir(dir)? {
        let entry = entry?;
        let name = entry.name;
        let kind = entry.kind?;
        let shown = base.join(&name);
        if want.is_none_or(|w| w == kind) && pattern.is_none_or(|p| glob_match(p, &name.to_string_lossy())) {
            if found.len() >= FIND_MAX_RESULTS {
                return Err(std::io::Error::other(format!(
                    "more than {FIND_MAX_RESULTS} results"
                )));
            }
            found.push(clean(&shown));
        }
        if kind == Kind::Dir {
            find(fs, &dir.join(&name), &shown, pattern, want, found)?;
        }
    }
    Ok(())
}

/// The `fs.find` type filter, the names `find -type` uses: `"file"`, `"dir"`,
/// `"symlink"` or `"other"`. The entry's own type, so a symlink is
/// `"symlink"` even when it points at a file (that is what `find -type l`
/// means; `fs.stat` follows the link instead).
fn find_type(cap: &str, value: &Value) -> Result<Kind, String> {
    let text = crate::caps::value_as_text(value);
    match text.as_str() {
        "file" => Ok(Kind::File),
        "dir" => Ok(Kind::Dir),
        "symlink" => Ok(Kind::Symlink),
        "other" => Ok(Kind::Other),
        other => Err(format!(
            "{cap}: unknown type {other:?} (expected \"file\", \"dir\", \"symlink\" or \"other\")"
        )),
    }
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
        let builder = HostCapsBuilder::new();
        #[cfg(feature = "stdlib")]
        let builder = builder.stdlib(true);
        let caps = builder.build();
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
            assert_escape(sb.call("fs.stat", &[path]), &format!("stat {path}"));
            assert_escape(sb.call("fs.is_file", &[path]), &format!("is_file {path}"));
            assert_escape(sb.call("fs.append", &[path, "x"]), &format!("append {path}"));
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
            assert_escape(sb.call("fs.stat", &[path]), &format!("stat {path}"));
            assert_escape(sb.call("fs.is_file", &[path]), &format!("is_file {path}"));
            assert_escape(sb.call("fs.append", &[path, "x"]), &format!("append {path}"));
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

    /// CRUSH-180: a dangling symlink cannot be canonicalized, so it used to
    /// be treated as a new file inside the root, and the write then followed
    /// it out. Every create/write/copy/move/mkdir is checked against links
    /// that point outside the root, dangling or not, at any component.
    #[test]
    fn dangling_symlinks_never_lead_out_of_the_root() {
        use std::os::unix::fs::symlink;
        let sb = Sandbox::new();
        let (root, outside) = (sb.root(), sb.outside());
        symlink("../outside/new.txt", root.join("dangle")).unwrap();
        symlink(outside.join("abs.txt"), root.join("dangle-abs")).unwrap();
        symlink("dangle", root.join("chain")).unwrap();
        symlink("../outside/deep", root.join("dmid")).unwrap();
        symlink("../outside", root.join("odir")).unwrap();
        symlink("loop-b", root.join("loop-a")).unwrap();
        symlink("loop-a", root.join("loop-b")).unwrap();

        for link in ["dangle", "dangle-abs", "chain"] {
            assert_escape(
                sb.call_values("fs.write", vec![Value::Str(link.into()), Value::Str("x".into())]),
                &format!("write {link}"),
            );
            assert_escape(sb.call("fs.touch", &[link]), &format!("touch {link}"));
            assert_escape(sb.call("fs.cp", &["a.txt", link]), &format!("cp -> {link}"));
            assert_escape(sb.call("fs.mkdir", &[link]), &format!("mkdir {link}"));
            assert_escape(sb.call_values("fs.mkdir", vec![Value::Str(link.into()), Value::Bool(true)]), &format!("mkdir -p {link}"));
        }
        // A dangling or outside-pointing directory link as a middle component.
        for path in ["dmid/x", "odir/x", "dmid/a/b", "odir/a/b"] {
            assert_escape(
                sb.call_values("fs.write", vec![Value::Str(path.into()), Value::Str("x".into())]),
                &format!("write {path}"),
            );
            assert_escape(sb.call("fs.touch", &[path]), &format!("touch {path}"));
            assert_escape(sb.call("fs.cp", &["a.txt", path]), &format!("cp -> {path}"));
            assert_escape(sb.call("fs.mv", &["a.txt", path]), &format!("mv -> {path}"));
            assert_escape(sb.call_values("fs.mkdir", vec![Value::Str(path.into()), Value::Bool(true)]), &format!("mkdir -p {path}"));
        }
        let copy = vec![Value::Str("sub".into()), Value::Str("dmid".into()), Value::Bool(true)];
        assert_escape(sb.call_values("fs.cp", copy), "cp -r -> dangling dir link");
        // A loop is an error, not a hang and not an escape.
        assert!(sb.call("fs.touch", &["loop-a"]).is_err());

        let mut entries: Vec<_> = std::fs::read_dir(&outside)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        entries.sort();
        assert_eq!(entries, ["secret.txt"], "something was created outside the root");
        assert!(root.join("a.txt").exists(), "mv moved a.txt somewhere");
    }

    /// `fs.cp -r` into an existing directory: only the destination's top is
    /// resolved, so links already below it must not be written through.
    #[test]
    fn recursive_cp_never_writes_through_a_link_in_the_destination() {
        use std::os::unix::fs::symlink;
        let sb = Sandbox::new();
        let root = sb.root();
        std::fs::create_dir_all(root.join("tree/sub")).unwrap();
        std::fs::write(root.join("tree/a.txt"), "A").unwrap();
        std::fs::write(root.join("tree/sub/b.txt"), "B").unwrap();
        for (dest, link, target) in [
            ("d1", "a.txt", "../../outside/pwn"), // dangling file link
            ("d2", "sub", "../../outside"),       // existing outside dir
            ("d3", "sub", "../../outside/deep"),  // dangling dir link
        ] {
            std::fs::create_dir(root.join(dest)).unwrap();
            symlink(target, root.join(dest).join(link)).unwrap();
            let args = vec![Value::Str("tree".into()), Value::Str(dest.into()), Value::Bool(true)];
            let err = sb.call_values("fs.cp", args).expect_err(dest);
            assert!(err.contains("symlink"), "{dest}: {err}");
        }
        let mut entries: Vec<_> = std::fs::read_dir(sb.outside())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        entries.sort();
        assert_eq!(entries, ["secret.txt"], "something was created outside the root");
    }

    /// Links that stay inside the root keep working, dangling ones included:
    /// writing through them creates their in-root target.
    #[test]
    fn symlinks_inside_the_root_still_work() {
        use std::os::unix::fs::symlink;
        let sb = Sandbox::new();
        let root = sb.root();
        symlink("a.txt", root.join("alink")).unwrap();
        symlink("sub", root.join("sublink")).unwrap();
        symlink("sub/fresh.txt", root.join("fresh")).unwrap();
        symlink("../root/sub/../made", root.join("made-link")).unwrap();

        sb.call_values("fs.write", vec![Value::Str("alink".into()), Value::Str("via".into())])
            .unwrap();
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "via");
        assert!(root.join("alink").is_symlink(), "the link itself was replaced");

        sb.call_values("fs.write", vec![Value::Str("sublink/x.txt".into()), Value::Str("x".into())])
            .unwrap();
        assert!(root.join("sub/x.txt").is_file());

        sb.call("fs.touch", &["fresh"]).unwrap();
        assert!(root.join("sub/fresh.txt").is_file());

        sb.call("fs.cp", &["a.txt", "made-link"]).unwrap();
        assert_eq!(std::fs::read_to_string(root.join("made")).unwrap(), "via");
        sb.call_values("fs.mkdir", vec![Value::Str("sublink/d/e".into()), Value::Bool(true)]).unwrap();
        assert!(root.join("sub/d/e").is_dir());
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
        // CRUSH-240: the `find -type` filter.
        assert_eq!(
            strings(sb.call("fs.find", &["sub", "*", "file"]).unwrap()),
            ["sub/deep/three.txt", "sub/one.txt", "sub/two.log"]
        );
        assert_eq!(
            strings(sb.call("fs.find", &["sub", "*", "dir"]).unwrap()),
            ["sub/deep"]
        );
        assert_eq!(
            strings(sb.call("fs.find", &["sub", "*.txt", "dir"]).unwrap()),
            Vec::<String>::new()
        );
        assert_eq!(
            strings(sb.call("fs.find", &[".", "*", "symlink"]).unwrap()),
            ["out"]
        );
        assert!(
            sb.call("fs.find", &["sub", "*", "bogus"])
                .unwrap_err()
                .contains("unknown type \"bogus\""),
            "an unknown type is an error, not an empty list"
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

    /// CRUSH-240: `fs.is_file`, `fs.stat` and `fs.append` — the type, size and
    /// time a script needs to tell files from directories, and to log a line at
    /// a time.
    #[test]
    fn file_type_size_time_and_append() {
        let sb = Sandbox::new();
        let map = |v: Option<Value>| match v {
            Some(Value::Map(m)) => m.borrow().clone(),
            other => panic!("expected a map, got {other:?}"),
        };
        let stat = |p: &str| map(sb.call("fs.stat", &[p]).unwrap());

        let st = stat("a.txt");
        assert_eq!(st["type"], Value::Str("file".into()));
        assert_eq!(st["size"], Value::Int(5));
        let Value::Int(modified) = &st["modified"] else {
            panic!("modified is not an int: {:?}", st["modified"]);
        };
        let modified = *modified;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        assert!((now - modified).abs() < 300, "{modified} vs {now}");

        // A directory's size is 0: what the platform reports for one differs.
        assert_eq!(stat("sub")["type"], Value::Str("dir".into()));
        assert_eq!(stat("sub")["size"], Value::Int(0));

        // `fs.stat` follows a symlink inside the root, like stat(2).
        std::os::unix::fs::symlink("a.txt", sb.root().join("alink")).unwrap();
        assert_eq!(stat("alink")["type"], Value::Str("file".into()));
        assert_eq!(stat("alink")["size"], Value::Int(5));

        // The two predicates agree with it.
        assert_eq!(sb.call("fs.is_file", &["a.txt"]).unwrap(), Some(Value::Int(1)));
        assert_eq!(sb.call("fs.is_file", &["sub"]).unwrap(), Some(Value::Int(0)));
        assert_eq!(sb.call("fs.is_file", &["nope"]).unwrap(), Some(Value::Int(0)));

        // A missing path is an error, naming the sandbox path only.
        let err = sb.call("fs.stat", &["nope"]).unwrap_err();
        assert!(err.starts_with("fs.stat nope:"), "{err}");
        assert!(!err.contains(sb.dir.path().to_str().unwrap()), "{err}");

        // `fs.append` creates the file, then adds to it instead of replacing.
        sb.call("fs.append", &["log.txt", "one\n"]).unwrap();
        sb.call("fs.append", &["log.txt", "two\n"]).unwrap();
        sb.call("fs.append", &["a.txt", "!"]).unwrap();
        assert_eq!(text(sb.call("fs.cat", &["log.txt"]).unwrap()), "one\ntwo\n");
        assert_eq!(text(sb.call("fs.cat", &["a.txt"]).unwrap()), "alpha!");
        assert_eq!(stat("log.txt")["size"], Value::Int(8));
        // A directory is refused, not replaced by a file.
        assert!(
            sb.call("fs.append", &["sub", "x"])
                .unwrap_err()
                .starts_with("fs.append sub:")
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
