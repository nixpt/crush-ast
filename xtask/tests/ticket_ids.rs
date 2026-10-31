//! Ticket-ID uniqueness gate (CRUSH-286).
//!
//! Runs in CI with no workflow change: the existing test job already
//! runs `cargo test --tests -p xtask`. Fails when two files under
//! `.jagent/planning/tickets/` (or `.jagent/planning/archive/`) share
//! an ID, except the pre-existing duplicates grandfathered in
//! `xtask/ticket-id-allowlist.txt`. Get a free number before minting a
//! ticket: `cargo run -p xtask --bin ticket-id -- next`.

use std::collections::BTreeSet;
use std::path::PathBuf;

use xtask::ticket_ids::{
    check_repo, collect_ticket_entries, extract_ticket_ids, find_clashes, format_clashes,
    next_free_number, parse_allowlist, parse_ticket_id, plain_number,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask manifest dir has a parent (the repo root)")
        .to_path_buf()
}

#[test]
fn repo_ticket_ids_are_unique_modulo_allowlist() {
    let root = repo_root();
    let allowlist_path = root.join("xtask/ticket-id-allowlist.txt");
    let allowlist_text = std::fs::read_to_string(&allowlist_path).unwrap_or_else(|e| {
        panic!(
            "allow-list {} must exist and be readable: {e}",
            allowlist_path.display()
        )
    });
    let allowlist = parse_allowlist(&allowlist_text);
    let clashes = check_repo(&root, &allowlist);
    assert!(
        clashes.is_empty(),
        "duplicate ticket IDs found (renumber the newer file — `cargo run -p xtask --bin ticket-id -- next` prints a free one; do not extend xtask/ticket-id-allowlist.txt for new clashes):\n{}",
        format_clashes(&clashes)
    );
}

#[test]
fn parses_plain_suffixed_and_bare_ids() {
    assert_eq!(
        parse_ticket_id("CRUSH-42-thing.md").as_deref(),
        Some("CRUSH-42")
    );
    assert_eq!(
        parse_ticket_id("CRUSH-121.md").as_deref(),
        Some("CRUSH-121")
    );
    assert_eq!(
        parse_ticket_id("CRUSH-30a-exhaustive-lint-residue.md").as_deref(),
        Some("CRUSH-30a")
    );
    assert_eq!(
        parse_ticket_id(".jagent/planning/tickets/CRUSH-9-x.md").as_deref(),
        Some("CRUSH-9")
    );
}

#[test]
fn parses_namespaced_ids_as_whole_prefix() {
    assert_eq!(
        parse_ticket_id("CRUSH-zorro-10-js-thing.md").as_deref(),
        Some("CRUSH-zorro-10")
    );
    assert_eq!(
        parse_ticket_id("CRUSH-xps-3-sdk-api.md").as_deref(),
        Some("CRUSH-xps-3")
    );
}

#[test]
fn rejects_non_ticket_names() {
    assert_eq!(parse_ticket_id("CRUSH-INDEX-INVARIANT-DEDUP.md"), None);
    assert_eq!(parse_ticket_id("CRUSHRUNNERS-1.md"), None);
    assert_eq!(parse_ticket_id("CRUSHSDK-1.md"), None);
    assert_eq!(parse_ticket_id("TASKS-2026-10-14.md"), None);
    assert_eq!(parse_ticket_id("CRUSH-12ab-two-letters.md"), None);
    assert_eq!(parse_ticket_id("README.md"), None);
}

#[test]
fn letter_suffix_is_a_distinct_id() {
    let entries = vec![
        ("CRUSH-266".to_owned(), "CRUSH-266-js-walker.md".to_owned()),
        (
            "CRUSH-266a".to_owned(),
            "CRUSH-266a-load-crashes.md".to_owned(),
        ),
    ];
    assert!(find_clashes(&entries, &BTreeSet::new()).is_empty());
}

#[test]
fn duplicate_detection_lists_both_files_and_allowlist_grandfathers() {
    let entries = vec![
        (
            "CRUSH-31".to_owned(),
            "tickets/CRUSH-31-dejavue-integration.md".to_owned(),
        ),
        (
            "CRUSH-31".to_owned(),
            "tickets/CRUSH-31-dejavue-crush-index-integration.md".to_owned(),
        ),
        ("CRUSH-32".to_owned(), "tickets/CRUSH-32-fine.md".to_owned()),
    ];
    let clashes = find_clashes(&entries, &BTreeSet::new());
    assert_eq!(clashes.len(), 1);
    assert_eq!(clashes[0].id, "CRUSH-31");
    let msg = format_clashes(&clashes);
    assert!(msg.contains("CRUSH-31"), "message names the ID: {msg}");
    assert!(
        msg.contains("CRUSH-31-dejavue-integration.md"),
        "message lists first file: {msg}"
    );
    assert!(
        msg.contains("CRUSH-31-dejavue-crush-index-integration.md"),
        "message lists second file: {msg}"
    );

    let allowlist: BTreeSet<String> = ["CRUSH-31".to_owned()].into_iter().collect();
    assert!(find_clashes(&entries, &allowlist).is_empty());
}

#[test]
fn namespaced_duplicates_clash_only_with_themselves() {
    let entries = vec![
        (
            "CRUSH-zorro-10".to_owned(),
            "a/CRUSH-zorro-10-one.md".to_owned(),
        ),
        (
            "CRUSH-zorro-10".to_owned(),
            "b/CRUSH-zorro-10-two.md".to_owned(),
        ),
        (
            "CRUSH-xps-10".to_owned(),
            "a/CRUSH-xps-10-other-series.md".to_owned(),
        ),
    ];
    let clashes = find_clashes(&entries, &BTreeSet::new());
    assert_eq!(clashes.len(), 1);
    assert_eq!(clashes[0].id, "CRUSH-zorro-10");
}

#[test]
fn allowlist_parsing_strips_comments_and_blank_lines() {
    let text = "# header\n\nCRUSH-31  # grandfathered\n  CRUSH-266 # claimed twice\n";
    let parsed = parse_allowlist(text);
    assert_eq!(
        parsed,
        BTreeSet::from(["CRUSH-31".to_owned(), "CRUSH-266".to_owned()])
    );
}

#[test]
fn extracts_ids_from_branch_names_and_pr_titles() {
    assert_eq!(
        extract_ticket_ids("agent/kai/CRUSH-287-bench"),
        ["CRUSH-287"]
    );
    assert_eq!(
        extract_ticket_ids("feat: CRUSH-xps-9 shared debug (CRUSH-42)"),
        ["CRUSH-xps-9", "CRUSH-42"]
    );
    assert!(extract_ticket_ids("no ticket here").is_empty());
}

#[test]
fn next_free_number_uses_plain_series_only() {
    assert_eq!(plain_number("CRUSH-30a"), Some(30));
    assert_eq!(plain_number("CRUSH-zorro-99"), None);
    assert_eq!(
        next_free_number(["CRUSH-1", "CRUSH-285", "CRUSH-zorro-99", "CRUSH-xps-13"]),
        286
    );
    assert_eq!(next_free_number(Vec::<&str>::new()), 1);
}

#[test]
fn collect_scans_tickets_and_archive_recursively() {
    let root = std::env::temp_dir().join(format!("ticket-ids-test-{}", std::process::id()));
    let tickets = root.join(".jagent/planning/tickets");
    let archive = root.join(".jagent/planning/archive/old");
    std::fs::create_dir_all(&tickets).unwrap();
    std::fs::create_dir_all(&archive).unwrap();
    std::fs::write(tickets.join("CRUSH-5-one.md"), "").unwrap();
    std::fs::write(archive.join("CRUSH-5-two.md"), "").unwrap();
    std::fs::write(tickets.join("not-a-ticket.md"), "").unwrap();

    let entries = collect_ticket_entries(&root);
    assert_eq!(
        entries.len(),
        2,
        "both CRUSH-5 files collected: {entries:?}"
    );
    let clashes = check_repo(&root, &BTreeSet::new());
    assert_eq!(clashes.len(), 1);
    assert!(
        format_clashes(&clashes).contains("archive"),
        "archive file listed: {clashes:?}"
    );

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn committed_allowlist_covers_exactly_the_known_clashes() {
    // Guard against allow-list drift: every allow-listed ID must still
    // be a real duplicate on disk, so entries get removed when the old
    // tickets are ever cleaned up.
    let root = repo_root();
    let text = std::fs::read_to_string(root.join("xtask/ticket-id-allowlist.txt")).unwrap();
    let allowlist = parse_allowlist(&text);
    let raw_clashes = check_repo(&root, &BTreeSet::new());
    let raw_ids: BTreeSet<&str> = raw_clashes.iter().map(|c| c.id.as_str()).collect();
    for id in &allowlist {
        assert!(
            raw_ids.contains(id.as_str()),
            "allow-list entry {id} is no longer a duplicate — remove it"
        );
    }
    // And the helper used by `check` agrees the repo is clean.
    assert!(check_repo(&root, &allowlist).is_empty());
}
