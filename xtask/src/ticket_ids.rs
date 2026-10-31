//! Unique ticket-ID checking and allocation (CRUSH-286).
//!
//! Ticket files live in `.jagent/planning/tickets/` (and, historically,
//! `.jagent/planning/archive/`). Parallel agents and boxes used to pick
//! numbers with nothing checking them, so IDs collided repeatedly.
//! This module is the one shared implementation behind both enforcement
//! points:
//!
//! - the CI test (`xtask/tests/ticket_ids.rs`, run by the existing
//!   `cargo test --tests -p xtask` CI job — no workflow change needed), and
//! - the `ticket-id` binary (`xtask/src/ticket_id.rs`), which allocates
//!   the next free number (`next`) and re-runs the same check locally
//!   (`check`).
//!
//! An ID is the whole filename prefix before the slug:
//! `CRUSH-<digits>[<lowercase letter>]` (e.g. `CRUSH-42`, `CRUSH-30a`)
//! plus namespaced series of the same shape, e.g. `CRUSH-zorro-10`,
//! `CRUSH-xps-3`. A letter suffix makes a *distinct* ID (`CRUSH-266` and
//! `CRUSH-266a` do not clash); namespaced IDs are their own series and
//! only clash with themselves. Non-ticket names (`CRUSH-INDEX-…`,
//! `CRUSHSDK-1`, `TASKS-…`) have no ID.
//!
//! Duplicates that already existed on `main` when the check landed are
//! grandfathered through `xtask/ticket-id-allowlist.txt` (one ID per
//! line); every new duplicate fails. Never renumber old tickets just to
//! shrink the allow-list, and never add a new entry to silence a fresh
//! clash — pick a free number with `ticket-id next` instead.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Directories (relative to the repo root) scanned for ticket files.
pub const TICKET_DIRS: [&str; 2] = [".jagent/planning/tickets", ".jagent/planning/archive"];

/// Parse an ID at the start of `text`, returning the ID and its byte
/// length. The character after the ID must not be alphanumeric, so
/// `CRUSH-12ab` is rejected while `CRUSH-12a-…`, `CRUSH-12: …`, and a
/// trailing `CRUSH-12` are accepted.
fn parse_id_at(text: &str) -> Option<(&str, usize)> {
    let rest = text.strip_prefix("CRUSH-")?;
    let bytes = rest.as_bytes();
    let mut i = 0;

    // Optional `<lowercase namespace>-` segment (`zorro`, `xps`, …).
    // Reset if it is not followed by `-<digit>`, so a plain slug word
    // can never swallow the number.
    while i < bytes.len() && bytes[i].is_ascii_lowercase() {
        i += 1;
    }
    if i > 0
        && i < bytes.len()
        && bytes[i] == b'-'
        && bytes.get(i + 1).is_some_and(u8::is_ascii_digit)
    {
        i += 1; // consume the '-' after the namespace
    } else {
        i = 0;
    }

    let digit_start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == digit_start {
        return None;
    }
    // Optional single-letter suffix (`CRUSH-30a`).
    if i < bytes.len() && bytes[i].is_ascii_lowercase() {
        i += 1;
    }
    if i < bytes.len() && bytes[i].is_ascii_alphanumeric() {
        return None;
    }
    let end = "CRUSH-".len() + i;
    Some((&text[..end], end))
}

/// Extract the ticket ID from a file name (or a path ending in one).
///
/// ```
/// # use xtask::ticket_ids::parse_ticket_id;
/// assert_eq!(parse_ticket_id("CRUSH-42-thing.md").as_deref(), Some("CRUSH-42"));
/// assert_eq!(parse_ticket_id("CRUSH-zorro-7-x.md").as_deref(), Some("CRUSH-zorro-7"));
/// assert_eq!(parse_ticket_id("CRUSH-121.md").as_deref(), Some("CRUSH-121"));
/// assert_eq!(parse_ticket_id("CRUSH-INDEX-INVARIANT-DEDUP.md"), None);
/// ```
pub fn parse_ticket_id(file_name: &str) -> Option<String> {
    let base = file_name.rsplit(['/', '\\']).next().unwrap_or(file_name);
    parse_id_at(base).map(|(id, _)| id.to_owned())
}

/// Find every ticket ID mentioned in arbitrary text (branch names, PR
/// titles, file listings). IDs are returned in order of appearance,
/// duplicates included.
pub fn extract_ticket_ids(text: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let mut search = text;
    while let Some(pos) = search.find("CRUSH-") {
        let candidate = &search[pos..];
        match parse_id_at(candidate) {
            Some((id, len)) => {
                ids.push(id.to_owned());
                search = &candidate[len..];
            }
            None => search = &candidate["CRUSH-".len()..],
        }
    }
    ids
}

/// The number of a plain-series ID (`CRUSH-42`, `CRUSH-30a` → `30`),
/// or `None` for namespaced IDs (`CRUSH-zorro-10`), which allocate in
/// their own series and do not consume plain numbers.
pub fn plain_number(id: &str) -> Option<u64> {
    let rest = id.strip_prefix("CRUSH-")?;
    if rest.contains('-') {
        return None;
    }
    let bytes = rest.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == 0 {
        return None;
    }
    // After the digits: end of string, or exactly one lowercase suffix.
    if i < bytes.len() && !(i + 1 == bytes.len() && bytes[i].is_ascii_lowercase()) {
        return None;
    }
    rest[..i].parse().ok()
}

/// Next free plain-series number: one past the highest plain number in
/// `ids`, or 1 if there are none.
pub fn next_free_number<'a>(ids: impl IntoIterator<Item = &'a str>) -> u64 {
    ids.into_iter()
        .filter_map(plain_number)
        .max()
        .map_or(1, |max| max.saturating_add(1))
}

/// Parse an allow-list: one ID per line, `#` starts a comment, blank
/// lines are ignored, and anything after the ID on a line is treated as
/// the justification note and dropped.
pub fn parse_allowlist(text: &str) -> BTreeSet<String> {
    text.lines()
        .filter_map(|line| {
            let line = line.split('#').next().unwrap_or("").trim();
            line.split_whitespace().next().map(str::to_owned)
        })
        .collect()
}

/// One duplicated ID and every file carrying it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clash {
    /// The duplicated ticket ID (e.g. `CRUSH-31`).
    pub id: String,
    /// All files sharing `id`, sorted.
    pub files: Vec<String>,
}

/// Group `(id, file)` entries by ID and return every ID carried by
/// more than one distinct file, minus `allowlist`ed grandfathered IDs.
/// Sorted by ID; files within a clash are sorted and deduplicated.
pub fn find_clashes(entries: &[(String, String)], allowlist: &BTreeSet<String>) -> Vec<Clash> {
    let mut by_id: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for (id, file) in entries {
        by_id.entry(id).or_default().insert(file);
    }
    by_id
        .into_iter()
        .filter(|(id, files)| files.len() > 1 && !allowlist.contains(*id))
        .map(|(id, files)| Clash {
            id: id.to_owned(),
            files: files.into_iter().map(str::to_owned).collect(),
        })
        .collect()
}

/// Render clashes for a failure message — each clash names its ID and
/// *every* file carrying it, so the offender can be found without a
/// second search.
pub fn format_clashes(clashes: &[Clash]) -> String {
    let mut out = String::new();
    for clash in clashes {
        out.push_str(&format!("duplicate ticket ID {}:\n", clash.id));
        for file in &clash.files {
            out.push_str(&format!("  - {file}\n"));
        }
    }
    out
}

/// Collect `(id, path)` for every ticket file under [`TICKET_DIRS`]
/// below `root`. Paths are displayed relative to `root`. Missing
/// directories are skipped (the archive may not exist everywhere).
pub fn collect_ticket_entries(root: &Path) -> Vec<(String, String)> {
    let mut entries = Vec::new();
    for rel in TICKET_DIRS {
        collect_from_dir(&root.join(rel), root, &mut entries);
    }
    entries.sort();
    entries
}

fn collect_from_dir(dir: &Path, root: &Path, out: &mut Vec<(String, String)>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<_> = read.filter_map(Result::ok).map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            collect_from_dir(&path, root, out);
        } else if let Some(name) = path.file_name().and_then(|n| n.to_str())
            && let Some(id) = parse_ticket_id(name)
        {
            let display = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            out.push((id, display));
        }
    }
}

/// Run the uniqueness check against the working tree at `root`:
/// every duplicate ticket ID not in `allowlist` is a clash.
pub fn check_repo(root: &Path, allowlist: &BTreeSet<String>) -> Vec<Clash> {
    find_clashes(&collect_ticket_entries(root), allowlist)
}
