//! `ticket-id` — allocate and check Crush ticket IDs (CRUSH-286).
//!
//! Usage:
//!
//! ```text
//! cargo run -p xtask --bin ticket-id -- next    # print the next free CRUSH-<N>
//! cargo run -p xtask --bin ticket-id -- check   # same uniqueness check as the CI test
//! ```
//!
//! `next` considers ticket files in the local working tree, on
//! `origin/main`, on every `refs/remotes/origin/*` branch and on every local
//! branch (`refs/heads/*`, which includes the branches of all this repo's
//! worktrees) — file names and branch names both (via `git ls-tree`) — and in
//! the titles of open PRs (via `gh pr list`). Local branches matter: a number
//! claimed only by an unpushed branch in another worktree is invisible to the
//! remote refs (CRUSH-298 was taken twice that way on 2026-10-30).
//! If `gh` is unavailable it says so on stderr and falls back to git
//! only. Only the allocated ID (`CRUSH-<N>`) goes to stdout; every
//! warning goes to stderr, so the output is safe to capture in scripts.
//!
//! `check` exits 0 when no two ticket files share an ID (modulo the
//! grandfathered allow-list) and 1 otherwise, listing each clash.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use xtask::ticket_ids::{
    check_repo, collect_ticket_entries, extract_ticket_ids, format_clashes, next_free_number,
    parse_allowlist, parse_ticket_id,
};

const USAGE: &str = "\
usage: ticket-id <next|check>

  next    print the next free CRUSH-<N> (considers the working tree,
          origin/main, every refs/remotes/origin/* branch, every local
          branch incl. other worktrees', and — when gh works — open PR
          titles)
  check   fail if two ticket files share an ID (same check as the
          xtask/tests/ticket_ids.rs CI test)";

fn main() -> ExitCode {
    let arg = std::env::args().nth(1);
    match arg.as_deref() {
        Some("next") => cmd_next(),
        Some("check") => cmd_check(),
        Some("-h" | "--help" | "help") => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

/// Repo root: `git rev-parse --show-toplevel` from the cwd, falling
/// back to the xtask manifest's parent, then the cwd itself.
fn repo_root() -> PathBuf {
    if let Ok(out) = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        && out.status.success()
    {
        let top = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        if !top.is_empty() {
            return PathBuf::from(top);
        }
    }
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
}

fn load_allowlist(root: &Path) -> BTreeSet<String> {
    let path = root.join("xtask/ticket-id-allowlist.txt");
    match std::fs::read_to_string(&path) {
        Ok(text) => parse_allowlist(&text),
        Err(e) => {
            eprintln!(
                "ticket-id: warning: cannot read {} ({e}); treating the allow-list as empty",
                path.display()
            );
            BTreeSet::new()
        }
    }
}

fn cmd_check() -> ExitCode {
    let root = repo_root();
    let allowlist = load_allowlist(&root);
    let clashes = check_repo(&root, &allowlist);
    if clashes.is_empty() {
        println!(
            "ticket-id check: ok — no duplicate ticket IDs under .jagent/planning/tickets/ or archive/ ({} grandfathered)",
            allowlist.len()
        );
        ExitCode::SUCCESS
    } else {
        eprintln!(
            "ticket-id check: FAILED — duplicate ticket IDs (renumber the newer file; get a free number with `ticket-id next`):\n{}",
            format_clashes(&clashes)
        );
        ExitCode::FAILURE
    }
}

fn cmd_next() -> ExitCode {
    let root = repo_root();
    let mut ids: BTreeSet<String> = BTreeSet::new();

    // 1. Local working tree (covers this branch's unpushed tickets).
    for (id, _) in collect_ticket_entries(&root) {
        ids.insert(id);
    }

    // 2. origin/main + every refs/remotes/origin/* branch + every local branch
    //    (refs/heads/*, shared by all of this repo's worktrees).
    collect_git_ids(&root, &mut ids);

    // 3. Open PR titles, when gh works.
    collect_pr_title_ids(&mut ids);

    println!("CRUSH-{}", next_free_number(ids.iter().map(String::as_str)));
    ExitCode::SUCCESS
}

fn collect_git_ids(root: &Path, ids: &mut BTreeSet<String>) {
    let refs = git_stdout(
        root,
        &[
            "for-each-ref",
            "--format=%(refname)",
            "refs/remotes/origin/",
            "refs/heads/",
        ],
    );
    let Some(refs) = refs else {
        eprintln!("ticket-id: warning: git refs unavailable; considering the working tree only");
        return;
    };
    let mut saw_ref = false;
    for line in refs.lines() {
        let refname = line.trim();
        if refname.is_empty() {
            continue;
        }
        saw_ref = true;
        // The branch name itself may claim a number before any ticket
        // file lands on it (e.g. `agent/kai/CRUSH-287-…`).
        ids.extend(extract_ticket_ids(refname));
        if let Some(tree) = git_stdout(root, &["ls-tree", "-r", "--name-only", refname]) {
            for path in tree.lines() {
                if let Some(id) = parse_ticket_id(path) {
                    ids.insert(id);
                }
            }
        }
    }
    if !saw_ref {
        eprintln!(
            "ticket-id: warning: no refs/remotes/origin/* found; considering the working tree only"
        );
    }
}

fn collect_pr_title_ids(ids: &mut BTreeSet<String>) {
    let output = Command::new("gh")
        .args(["pr", "list", "--state", "open", "--json", "title"])
        .output();
    let output = match output {
        Ok(out) if out.status.success() => out,
        Ok(out) => {
            let detail = String::from_utf8_lossy(&out.stderr).trim().to_owned();
            eprintln!(
                "ticket-id: warning: gh unavailable ({}); falling back to git only",
                if detail.is_empty() {
                    "gh pr list failed".to_owned()
                } else {
                    detail
                }
            );
            return;
        }
        Err(e) => {
            eprintln!("ticket-id: warning: gh unavailable ({e}); falling back to git only");
            return;
        }
    };
    let parsed: Result<serde_json::Value, _> = serde_json::from_slice(&output.stdout);
    match parsed {
        Ok(prs) => {
            if let Some(prs) = prs.as_array() {
                for pr in prs {
                    if let Some(title) = pr.get("title").and_then(serde_json::Value::as_str) {
                        ids.extend(extract_ticket_ids(title));
                    }
                }
            }
        }
        Err(e) => {
            eprintln!(
                "ticket-id: warning: cannot parse `gh pr list` output ({e}); falling back to git only"
            );
        }
    }
}

fn git_stdout(root: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        None
    }
}
