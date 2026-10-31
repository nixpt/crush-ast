//! Git dependencies (CRUSH-351): fetch into a cache, pin in `capsule.lock`,
//! then behave like a path dependency. Every repo here is a local `file://`
//! repository created by the test — no network.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false", "-c", "tag.gpgsign=false"])
        .args(args)
        .current_dir(dir)
        .output()
        .expect("spawn git");
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// A library repo defining `gd_greet()` that returns `greeting`, tagged `v1`.
fn lib_repo(root: &Path, greeting: &str) -> PathBuf {
    let repo = root.join("gdlib");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    write_lib(&repo, greeting);
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "v1"]);
    git(&repo, &["tag", "-a", "v1", "-m", "v1"]);
    repo
}

fn write_lib(repo: &Path, greeting: &str) {
    std::fs::write(
        repo.join("capsule.toml"),
        "[capsule]\nname = \"gdlib\"\nversion = \"0.1.0\"\nentry = \"gd.crush\"\nlanguage = \"crush\"\ncategory = \"library\"\n",
    )
    .unwrap();
    std::fs::write(repo.join("gd.crush"), format!("fn gd_greet() {{\n    return \"{greeting}\"\n}}\n")).unwrap();
}

fn url(repo: &Path) -> String {
    format!("file://{}", repo.display())
}

/// A consumer calling `gd_greet()` with the given `[[dependencies]]` body.
fn consumer(root: &Path, dep_body: &str) -> PathBuf {
    let dir = root.join("consumer");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("capsule.toml"),
        format!(
            "[capsule]\nname = \"consumer\"\nversion = \"0.1.0\"\nentry = \"main.crush\"\nlanguage = \"crush\"\ncategory = \"cli\"\n\n[[dependencies]]\nname = \"gdlib\"\n{dep_body}\n"
        ),
    )
    .unwrap();
    std::fs::write(dir.join("main.crush"), "fn main() {\n    print(gd_greet())\n}\n").unwrap();
    dir
}

fn crush_pkg(dir: &Path, cache: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_crush-pkg"))
        .args(args)
        .current_dir(dir)
        .env("CRUSH_PKG_CACHE", cache)
        .env_remove("CRUSH_PKG_OFFLINE")
        .stdin(Stdio::null())
        .output()
        .expect("spawn crush-pkg")
}

fn out_text(o: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

struct Fx {
    _tmp: tempfile::TempDir,
    repo: PathBuf,
    app: PathBuf,
    cache: PathBuf,
}

fn fixture(dep_body: impl FnOnce(&str) -> String) -> Fx {
    let tmp = tempfile::tempdir().unwrap();
    let repo = lib_repo(tmp.path(), "hello from gitdep");
    let app = consumer(tmp.path(), &dep_body(&url(&repo)));
    let cache = tmp.path().join("cache");
    Fx { _tmp: tmp, repo, app, cache }
}

#[test]
fn tag_dependency_builds_runs_and_is_pinned_in_the_lockfile() {
    let fx = fixture(|u| format!("git = \"{u}\"\ntag = \"v1\""));
    let sha = git(&fx.repo, &["rev-parse", "v1^{commit}"]);

    let out = crush_pkg(&fx.app, &fx.cache, &["run"]);
    assert!(out.status.success(), "{}", out_text(&out));
    assert!(String::from_utf8_lossy(&out.stdout).contains("hello from gitdep"), "{}", out_text(&out));
    // The lint knows `gd_greet` belongs to `gdlib`: no false dead-dep note.
    assert!(!out_text(&out).contains("dead-code"), "{}", out_text(&out));

    let lock = std::fs::read_to_string(fx.app.join("capsule.lock")).unwrap();
    assert!(lock.contains("name = \"gdlib\""), "{lock}");
    assert!(lock.contains("ref = \"tag:v1\""), "{lock}");
    assert!(lock.contains(&format!("rev = \"{sha}\"")), "{lock}");

    // <cache>/git/local/gdlib-<hash>/<sha>/ holds the tree, without .git.
    let found = walk_for(&fx.cache, &sha).expect("checkout dir named by the commit");
    assert!(found.join("gd.crush").is_file());
    assert!(!found.join(".git").exists());
}

fn walk_for(root: &Path, name: &str) -> Option<PathBuf> {
    walkdir_lite(root).into_iter().find(|e| e.file_name().is_some_and(|n| n == name))
}

fn walkdir_lite(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.push(p.clone());
                stack.push(p);
            }
        }
    }
    out
}

#[test]
fn lockfile_keeps_builds_reproducible_when_the_tag_moves() {
    let fx = fixture(|u| format!("git = \"{u}\"\ntag = \"v1\""));
    let first = crush_pkg(&fx.app, &fx.cache, &["run"]);
    assert!(String::from_utf8_lossy(&first.stdout).contains("hello from gitdep"));

    // Upstream retags v1 onto a new commit that prints something else.
    write_lib(&fx.repo, "changed upstream");
    git(&fx.repo, &["commit", "-qam", "change"]);
    git(&fx.repo, &["tag", "-f", "-a", "v1", "-m", "moved"]);

    let again = crush_pkg(&fx.app, &fx.cache, &["run"]);
    assert!(String::from_utf8_lossy(&again.stdout).contains("hello from gitdep"), "{}", out_text(&again));

    // Dropping the lock re-resolves the tag.
    std::fs::remove_file(fx.app.join("capsule.lock")).unwrap();
    let fresh = crush_pkg(&fx.app, &fx.cache, &["run"]);
    assert!(String::from_utf8_lossy(&fresh.stdout).contains("changed upstream"), "{}", out_text(&fresh));
}

#[test]
fn rev_and_branch_dependencies_resolve() {
    let fx = fixture(|u| format!("git = \"{u}\"\nbranch = \"main\""));
    let out = crush_pkg(&fx.app, &fx.cache, &["run"]);
    assert!(String::from_utf8_lossy(&out.stdout).contains("hello from gitdep"), "{}", out_text(&out));
    assert!(std::fs::read_to_string(fx.app.join("capsule.lock")).unwrap().contains("ref = \"branch:main\""));

    let sha = git(&fx.repo, &["rev-parse", "HEAD"]);
    let app = consumer(fx.app.parent().unwrap().join("second").as_path(), &format!("git = \"{}\"\nrev = \"{sha}\"", url(&fx.repo)));
    let out = crush_pkg(&app, &fx.cache, &["run"]);
    assert!(String::from_utf8_lossy(&out.stdout).contains("hello from gitdep"), "{}", out_text(&out));
}

#[test]
fn offline_uses_only_the_cache_and_says_so_when_it_cannot() {
    let fx = fixture(|u| format!("git = \"{u}\"\ntag = \"v1\""));

    // Cold cache + no lock: refuse, naming the fix.
    let cold = crush_pkg(&fx.app, &fx.cache, &["--offline", "run"]);
    assert!(!cold.status.success());
    assert!(out_text(&cold).contains("--offline"), "{}", out_text(&cold));
    assert!(!fx.app.join("capsule.lock").exists());

    // Warm the cache, then take the remote away: --offline still builds.
    assert!(crush_pkg(&fx.app, &fx.cache, &["build"]).status.success());
    let hidden = fx.repo.with_file_name("gdlib-gone");
    std::fs::rename(&fx.repo, &hidden).unwrap();
    let warm = crush_pkg(&fx.app, &fx.cache, &["--offline", "run"]);
    assert!(warm.status.success(), "{}", out_text(&warm));
    assert!(String::from_utf8_lossy(&warm.stdout).contains("hello from gitdep"));

    // A cold cache with the remote gone is a network failure that points at --offline.
    let cold_cache = fx.cache.with_file_name("cache2");
    std::fs::remove_file(fx.app.join("capsule.lock")).unwrap();
    let net = crush_pkg(&fx.app, &cold_cache, &["build"]);
    assert!(!net.status.success());
    let t = out_text(&net);
    assert!(t.contains("could not reach") && t.contains("--offline"), "{t}");
}

#[test]
fn unknown_tag_is_a_clear_error() {
    let fx = fixture(|u| format!("git = \"{u}\"\ntag = \"nope\""));
    let out = crush_pkg(&fx.app, &fx.cache, &["build"]);
    assert!(!out.status.success());
    assert!(out_text(&out).contains("tag `nope` not found"), "{}", out_text(&out));
}

#[test]
fn dependency_without_a_usable_entry_is_a_clear_error() {
    let fx = fixture(|u| format!("git = \"{u}\"\ntag = \"v1\""));
    std::fs::remove_file(fx.repo.join("gd.crush")).unwrap();
    git(&fx.repo, &["add", "-A"]);
    git(&fx.repo, &["commit", "-qm", "drop entry"]);
    git(&fx.repo, &["tag", "-f", "-a", "v1", "-m", "no entry"]);
    let out = crush_pkg(&fx.app, &fx.cache, &["build"]);
    assert!(!out.status.success());
    let t = out_text(&out);
    assert!(t.contains("entry file `gd.crush`") && t.contains("git dependency"), "{t}");
}

#[test]
fn exactly_one_source_and_one_ref_are_required() {
    let fx = fixture(|u| format!("git = \"{u}\"\ntag = \"v1\""));
    for (body, needle) in [
        ("path = \"../gdlib\"\ngit = \"file:///x\"\ntag = \"v1\"", "not both"),
        ("", "needs a source"),
        ("git = \"file:///x\"", "exactly one of `tag`, `rev` or `branch`"),
        ("git = \"file:///x\"\ntag = \"a\"\nbranch = \"b\"", "exactly one of `tag`, `rev` or `branch`"),
        ("path = \"../gdlib\"\ntag = \"v1\"", "only apply to a `git` dependency"),
    ] {
        let app = consumer(fx.app.parent().unwrap().join(format!("bad{}", needle.len())).as_path(), body);
        let out = crush_pkg(&app, &fx.cache, &["build"]);
        assert!(!out.status.success(), "{body}");
        assert!(out_text(&out).contains(needle), "{body}: {}", out_text(&out));
    }
}
