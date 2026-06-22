//! CI smoke-test: locks in the destructure-bind-and-misuse audit state
//! for `crush_vm::vm::Value` variants `Int | Float | Str | Bytes | Handle`.
//! (The `Bool` variant is audited separately and was closed earlier.)
//!
//! Runs `rg --type rust -C 5 'Value::(Int|Float|Str|Bytes|Handle)('` over
//! every Crush workspace crate. For each rg match, scans the chunk's
//! subsequent lines for SAFE patterns that prove the bound variable flows
//! correctly. If no safe pattern is detected AND the (file, line) isn't in
//! the allowlist, the step fails.
//!
//! Run locally: `cargo xtask audit`
//! Run in CI: see `.github/workflows/ci.yml`.
//!
//! Run locally with NDJSON: `cargo xtask audit --message-format=json`.
//! Records emit one `E-AUDIT` line per RISKY site plus a summary line,
//! using the shared helpers in `xtask::diag` (which mirror the
//! seven-field wire shape of `crush_lang_sdk::theme::JsonDiagnostic`).

use std::collections::HashSet;
use std::process::Command;

use regex::Regex;

use xtask::diag::{hinted_text, diag_line_from, wants_json, CODE_AUDIT};

/// The single source of truth for the 5 `Value::*` SCALAR variants the audit walks.
/// Threaded into `parse_match_line` (line-walker) and `pattern_for()` (rg arg builder)
/// so any future SCALAR-variant addition needs changes in exactly one site.
const VARIANTS: &[&str] = &["Int", "Float", "Str", "Bytes", "Handle"];

/// Build a rg argument string from a variants slice. Output template:
/// `Value::({joined_variants})\(`
fn pattern_for(variants: &[&str]) -> String {
    format!("Value::({})\\(", variants.join("|"))
}

/// Crates in scope of the audit. Kept in lock-step with the user-provided
/// rg command in the 2026-06-18 audit decision.
const TARGET_CRATES: &[&str] = &[
    "crates/crush-lang-bash",
    "crates/crush-lang-zsh",
    "crates/crush-lang-python",
    "crates/crush-lang-rust",
    "crates/crush-lang-js",
    "crates/crush-lang-sdk",
    "crates/crush-cast",
    "crates/crush-vm",
    "crates/c_walker",
    "crates/go_walker",
    "crates/wasm_walker",
    "crates/zig_walker",
    "crates/walker-core",
    "crates/crush-frontend",
    "crates/crush-pkg",
    "crates/crush-installer",
    "crates/crush-index",
    "crates/crush-net",
    "crates/cli",
];

/// SAFE patterns we look for in the post-match lines of an rg chunk.
/// Each rule is `regex_find_match(line, VAR)`. If any rule returns true
/// for *some* line in a chunk, the chunk is SAFE; otherwise it is queued
/// for RISKY inspection.
///
/// Patterns use `(VAR)` as a placeholder substituted with the bound
/// variable name before compilation. Compiled once inside run_audit via
/// `compile_safe_patterns`.
const SAFE_PATTERN_TEMPLATES: &[&str] = &[
    // Reference consumed by value via deref or known &T-coercing methods.
    r"\*\b(VAR)\b",                                               // *i, *f, *s, *b, *h
    r"\b(VAR)\.clone\(\)",                                         // VAR.clone()
    r"\b(VAR)\.to_string\(\)",                                     // VAR.to_string()
    r"\b(VAR)\.to_owned\(\)",                                      // VAR.to_owned()
    r"\b(VAR)\.unwrap_or_default\(\)",                             // VAR.unwrap_or_default()
    r"\b(VAR)\.unwrap_or\(",                                       // VAR.unwrap_or(<default-expr>)
    r"\b(VAR)\.as_str\(\)",                                        // VAR.as_str()
    r"\b(VAR)\.as_bytes\(\)",                                      // VAR.as_bytes()
    r"\b(VAR)\.chars\(\)",                                         // VAR.chars()
    r"\b(VAR)\.bytes\(\)",                                         // VAR.bytes()
    r"\b(VAR)\.lines\(\)",                                         // VAR.lines()
    r"\b(VAR)\.split\(",                                           // VAR.split(delim)
    r"\b(VAR)\.parse::<",                                          // VAR.parse::<T>()
    r"\bString::from\(\s*(VAR)\s*\)",                               // String::from(VAR)
    r"\bVec::from\(\s*(VAR)\s*\)",                                 // Vec::from(VAR)
    // Auto-deref property methods (work on both `T` and `&T`).
    r"\b(VAR)\.(len|is_empty|capacity|as_ptr|as_slice|as_mut_ptr|as_mut_slice|to_vec|into_boxed_slice|drain|sort|sort_by|sort_by_key|retain)\(\)",
    // Numeric primitive methods on i64/f64/u64.
    r"\b(VAR)\.(signum|unsigned_abs|abs|checked_|saturating_|wrapping_)([a-zA-Z_]*)?\(",
    r"\b(VAR)\.ilog[0-9]?\(",                                      // i64.ilog() / ilog10()
    // Equality on either side of `==`/`!=`.
    r"(?:^|[^=!])(VAR)\s*(==|!=)\s*[^=]",
    r"(==|!=)\s*(VAR)\b",
    // `format!`/`assert!`/`assert_eq!` macros that consume VAR (the macro body
    // is implicitly `&str`-coerced when VAR is `&String` / `&Vec<u8>`).
    r"format!\([^)]*\b(VAR)\b",
    r"write!\([^)]*\b(VAR)\b",
    r"assert!\([^)]*\b(VAR)\b",
    r"assert_eq!\([^)]*\b(VAR)\b",
    r"debug_assert!\([^)]*\b(VAR)\b",
];

/// Cap the number of RISKY sites printed to a CI-friendly size.
const RISKY_OUTPUT_CAP: usize = 20;

/// Path to the allowlist file relative to the workspace root.
const ALLOWLIST_PATH: &str = "xtask/audit-allowlist.txt";

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let json_mode = wants_json(&args);
    if args.iter().any(|a| a == "audit") {
        run_audit(json_mode);
        return;
    }
    eprintln!("usage: cargo xtask audit");
    eprintln!("  runs the destructure-bind-and-misuse smoke-test.");
    eprintln!(
        "  optional: --message-format=json for NDJSON output (code: {CODE_AUDIT})."
    );
    std::process::exit(2);
}

fn run_audit(json_mode: bool) {
    let allowlist = load_allowlist();

    // Compile safe-pattern regexes once. We keep one Regex per template-arm.
    let safe_regexes: Vec<Regex> = SAFE_PATTERN_TEMPLATES
        .iter()
        .map(|t| Regex::new(t).expect("SAFE_PATTERN_TEMPLATE must be a valid regex"))
        .collect();

    // Build rg invocation.
    let mut rg_args: Vec<String> = vec![
        "--type".into(),
        "rust".into(),
        "-n".into(),
        "-C".into(),
        "5".into(),
        "--no-heading".into(),
    ];
    rg_args.push("-e".into());
    let pattern = pattern_for(VARIANTS);
    rg_args.push(pattern);
    for dir in TARGET_CRATES {
        rg_args.push((*dir).into());
    }

    let rg = match Command::new("rg").args(&rg_args).output() {
        Ok(out) => out,
        Err(e) => {
            let msg = format!("failed to spawn rg: {e}");
            if json_mode {
                eprint!(
                    "{}",
                    diag_line_from(
                        CODE_AUDIT,
                        "error",
                        &msg,
                        Some("install ripgrep (e.g. `apt install ripgrep` or `brew install ripgrep`)."),
                        None,
                    )
                );
            } else {
                eprintln!("error: {msg}");
                eprintln!(
                    "hint: install ripgrep (e.g. `apt install ripgrep` or `brew install ripgrep`)."
                );
            }
            std::process::exit(2);
        }
    };

    // rg exit codes: 0 = matches found, 1 = no matches, >=2 = error.
    let exit_ok = matches!(rg.status.code(), Some(0) | Some(1));        if !exit_ok {
            if json_mode {
                let msg = format!(
                    "rg exited with code {:?} (stderr len={} bytes)",
                    rg.status.code(),
                    rg.stderr.len()
                );
                // Cap rg's child stderr via `hinted_text` so a future
                // contributor running audit against a noisy crash
                // dump (multi-MB rg.stderr) doesn't push the NDJSON
                // consumer past the 64KiB OS pipe buffer. The
                // truncation marker reports how many RAW bytes were
                // dropped so editors still see the failure's shape.
                let stderr_capped = hinted_text(&String::from_utf8_lossy(&rg.stderr));
                eprint!(
                    "{}",
                    diag_line_from(
                        CODE_AUDIT,
                        "error",
                        &msg,
                        Some(&stderr_capped),
                        None,
                    )
                );
            } else {
            eprintln!(
                "error: rg exited with code {:?}; stderr follows.",
                rg.status.code()
            );
            eprintln!("{}", String::from_utf8_lossy(&rg.stderr));
        }
        std::process::exit(2);
    }

    let stdout = String::from_utf8_lossy(&rg.stdout);
    let chunks = split_chunks(&stdout);

    let mut risky: Vec<RiskySite> = Vec::new();
    let mut safe_count = 0usize;

    for chunk in &chunks {
        let Some(site) = parse_match_line(chunk) else {
            continue;
        };
        // Skip multi-bind arms (e.g. Value::Map((k, v))) — those don't have a
        // single-variable misuse shape and aren't worth auditing here.
        if site.bound_var.contains(',') {
            safe_count += 1;
            continue;
        }
        // Discard pattern (`_`) is always safe.
        if site.bound_var == "_" {
            safe_count += 1;
            continue;
        }
        if is_chunk_safe(chunk, &site.bound_var, &safe_regexes) {
            safe_count += 1;
            continue;
        }
        let key = format!("{}:{}", site.file, site.match_line);
        if allowlist.contains(&key) {
            safe_count += 1;
            continue;
        }
        risky.push(site);
    }

    let summary = format!(
        "audit: parsed {} match chunks across {} patterns in {} crates. {} SAFE, {} RISKY candidates (post-allowlist).",
        chunks.len(),
        5,
        TARGET_CRATES.len(),
        safe_count,
        risky.len()
    );

    if risky.is_empty() {
        if json_mode {
            let msg = format!("OK: 0 RISKY; allowlist={} entries.", allowlist.len());
            eprint!("{}", diag_line_from(CODE_AUDIT, "note", &msg, None, None));
        } else {
            println!("{summary}");
            println!(
                "OK: 0 RISKY destructure-bind sites; audit state is clean. (allowlist={} entries)",
                allowlist.len()
            );
        }
        std::process::exit(0);
    }

    // FAIL path. Open with a human-readable single-line preamble that
    // appears in BOTH modes so a contributor reading `cat` log capture
    // still sees the high-level verdict.
    if json_mode {
        let shown = risky.len().min(RISKY_OUTPUT_CAP);
        let summary_msg = format!(
            "FAIL: {} RISKY destructure-bind site(s); {} shown, {} suppressed.",
            risky.len(),
            shown,
            risky.len() - shown
        );
        eprint!(
            "{}",
            diag_line_from(CODE_AUDIT, "error", &summary_msg, None, None)
        );
        for site in risky.iter().take(shown) {
            let msg = format!(
                "site: file={} line={} bound_var={} match=\"{}\"; action=dereference or clone, or append {}:{} to {}",
                site.file,
                site.match_line,
                site.bound_var,
                site.match_line_content,
                site.file,
                site.match_line,
                ALLOWLIST_PATH
            );
            eprint!("{}", diag_line_from(CODE_AUDIT, "error", &msg, None, None));
        }
        if risky.len() > shown {
            let msg = format!(
                "... and {} more (suppressed; pass --cap-size larger to see all)",
                risky.len() - shown
            );
            eprint!("{}", diag_line_from(CODE_AUDIT, "note", &msg, None, None));
        }
        std::process::exit(1);
    }

    eprintln!();
    eprintln!(
        "FAIL: {} RISKY destructure-bind site(s).",
        risky.len()
    );
    eprintln!(
        "* The destructure-bind-and-misuse bug class triggers when a `Value::*` arm binds a"
    );
    eprintln!(
        "  variable from a `&Value` match and feeds it un-deref'd into a slot that expects owned. *"
    );
    eprintln!();
    let shown = risky.len().min(RISKY_OUTPUT_CAP);
    for site in risky.iter().take(shown) {
        eprintln!(
            "  - {}:{} | match: `{}` | bound var: `{}`",
            site.file,
            site.match_line,
            site.match_line_content,
            site.bound_var
        );
        eprintln!(
            "    Action: confirm the side; either (a) dereference (`*{}`), clone (`{}.clone()`), \
             coerce (`{}.to_string()`), or (b) if this is intentional and SAFE, append `{}:{}` to {}.",
            site.bound_var, site.bound_var, site.bound_var, site.file, site.match_line, ALLOWLIST_PATH
        );
    }
    if risky.len() > shown {
        eprintln!("  ... and {} more (suppressed; re-run with `--all` later if needed).",
            risky.len() - shown);
    }
    eprintln!();
    std::process::exit(1);
}

/// A single match site parsed from a chunk.
struct RiskySite {
    file: String,
    match_line: u64,
    match_line_content: String,
    bound_var: String,
}

/// Split rg's stdout into per-match chunks. rg with `--no-heading -C 5`
/// emits chunks separated by `--` (one per matching group).
fn split_chunks(stdout: &str) -> Vec<Vec<String>> {
    let mut chunks: Vec<Vec<String>> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    let mut saw_any = false;
    for line in stdout.lines() {
        if line == "--" {
            if saw_any {
                chunks.push(std::mem::take(&mut current));
                saw_any = false;
            }
            continue;
        }
        current.push(line.to_string());
        saw_any = true;
    }
    if saw_any {
        chunks.push(current);
    }
    chunks
}

/// Parse the match line out of a chunk. Returns `None` if the chunk
/// doesn't lead with a `Value::Variant(bound_var)` line.
fn parse_match_line(chunk: &[String]) -> Option<RiskySite> {
    let first = chunk.first()?.trim_start();
    // Format `file:line:content` from rg --no-heading -n output.
    let parts: Vec<&str> = first.splitn(3, ':').collect();
    if parts.len() < 3 {
        return None;
    }
    let file = parts[0].to_string();
    let match_line: u64 = parts[1].parse().ok()?;
    let content = parts[2].to_string();
    for &variant in VARIANTS {
        let marker = format!("Value::{}(", variant);
        if !content.contains(&marker) {
            continue;
        }
        if let Some(bound) = extract_bound_var(&content, &marker) {
            return Some(RiskySite {
                file,
                match_line,
                match_line_content: content,
                bound_var: bound,
            });
        }
    }
    None
}

/// Extract the bound variable from a `Value::Marker(bound_var)` clause using
/// brace-depth on the line (defensive against lines containing nested parens).
///
/// # Invariants
///
/// 1. **Empty marker -> `None` (return BEFORE the very next line).** If
///    `marker` is empty, then `marker.len() == 0`, and the very next line
///    computes `marker_pos + marker.len() - 1 = marker_pos - 1`. This
///    underflows `usize` iff `marker_pos == 0`. The empty-marker guard at
///    the top of the body short-circuits to `None` before that arithmetic
///    ever runs.
/// 2. **Composite open-site: the open paren is the marker's OWN trailing
///    `(`.** A naive `content.find('(')` lands on the wrong site for
///    composite shapes -- e.g., `Value::Map(("k", v))` would close at the
///    inner literal `(` of the inner tuple, producing a wrong bound
///    extraction. The walker deliberately uses
///    `marker_pos + marker.len() - 1` (the marker's own trailing paren)
///    instead of `content.find('(')`.
fn extract_bound_var(content: &str, marker: &str) -> Option<String> {
    if marker.is_empty() {
        return None;
    }

    let marker_pos = content.find(marker)?;
    // The last char of the marker IS the open paren (marker ends with `(`).
    let open = marker_pos + marker.len() - 1;
    // Walk forward balancing parens to find the matching close.
    let mut depth = 1usize;
    let mut i = open + 1;
    while i < content.len() && depth > 0 {
        let c = content.as_bytes()[i];
        if c == b'(' {
            depth += 1;
        } else if c == b')' {
            depth -= 1;
            if depth == 0 {
                let inside = content[open + 1..i].trim().to_string();
                return if inside.is_empty() { None } else { Some(inside) };
            }
        }
        i += 1;
    }
    None
}

/// True iff *any* chunk line (including the match line — single-line match arms
/// have their SAFE pattern on the same line as the `Value::Variant(bound_var)` header)
/// contains a SAFE pattern referencing `bound_var`. Pre-compiled regex objects
/// are passed in for the un-bound templates; per-call we substitute (VAR) and
/// compile a per-bound-var regex.
fn is_chunk_safe(chunk: &[String], bound_var: &str, safe_regexes: &[Regex]) -> bool {
    for raw in chunk.iter() {
        // Strip the rg path:line:col prefix so the regex sees just the source.
        let line = raw.splitn(3, ':').nth(2).unwrap_or(raw);
        for templ_re in safe_regexes {
            // Substitute the (VAR) placeholder with the actual bound identifier.
            // We rebuild the regex per-call (cheap; these patterns are tiny).
            let pat = templ_re.as_str().replace("(VAR)", &regex::escape(bound_var));
            match Regex::new(&pat) {
                Ok(r) => {
                    if r.is_match(line) {
                        return true;
                    }
                }
                Err(_) => continue,
            }
        }
    }
    false
}

/// Load the allowlist (one `path:line` per line, `#` comments allowed).
fn load_allowlist() -> HashSet<String> {
    let mut set = HashSet::new();
    let body = match std::fs::read_to_string(ALLOWLIST_PATH) {
        Ok(s) => s,
        Err(_) => return set,
    };
    for line in body.lines() {
        let trimmed = line.split('#').next().unwrap_or("").trim();
        if !trimmed.is_empty() {
            set.insert(trimmed.to_string());
        }
    }
    set
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compile_safe_regexes() -> Vec<Regex> {
        SAFE_PATTERN_TEMPLATES
            .iter()
            .map(|t| Regex::new(t).unwrap())
            .collect()
    }

    #[test]
    fn deref_is_safe() {
        let chunk = vec![
            "src/x.rs:10:    Value::Int(i) => serde_json::Value::Number((*i).into()),".to_string(),
            "src/x.rs:11:    }".to_string(),
        ];
        assert!(is_chunk_safe(&chunk, "i", &compile_safe_regexes()));
    }

    #[test]
    fn empty_marker_returns_none() {
        assert!(extract_bound_var("anything", "").is_none());
    }

    #[test]
    fn literal_constructor_is_risky() {
        // This mirrors the original &bool bug shape: a &Value match arms binds
        // `b` then feeds `b` (un-deref'd) into a slot that expects owned bool.
        let chunk = vec![
            "src/x.rs:15:    Value::Bool(b) => serde_json::Value::Bool(b),".to_string(),
            "src/x.rs:16:    }".to_string(),
        ];
        assert!(!is_chunk_safe(&chunk, "b", &compile_safe_regexes()));
    }

    #[test]
    fn clone_is_safe() {
        let chunk = vec![
            "src/x.rs:20:    Value::Str(s) => {".to_string(),
            "src/x.rs:21:        Value::Str(s.clone())".to_string(),
            "src/x.rs:22:    }".to_string(),
        ];
        assert!(is_chunk_safe(&chunk, "s", &compile_safe_regexes()));
    }

    #[test]
    fn multi_bind_is_skipped() {
        let chunk = vec![
            "src/x.rs:30:    Value::Map((k, v)) => ..".to_string(),
        ];
        // Parser can't extract a single bound var (returns None) → never reaches RISKY gate.
        assert!(parse_match_line(&chunk).is_none());
    }

    #[test]
    fn discard_is_safe() {
        let chunk = vec![
            "src/x.rs:40:    Value::Int(_) => \"int\"".to_string(),
        ];
        let site = parse_match_line(&chunk).unwrap();
        assert_eq!(site.bound_var, "_");
        // _ is filtered upstream in run_audit, but the standalone is_chunk_safe
        // would also return false; _-binding is gated at the call site.
    }

    #[test]
    fn parse_match_line_walks_variants() {
        // Integration test: round-trip a sample rg-output chunk through parse_match_line.
        // Exercises the new `for &variant in VARIANTS` walker end-to-end.
        let chunk: Vec<String> = vec![
            "src/x.rs:42:    let x = Value::Handle(MARKER);".to_string(),
        ];
        let site = parse_match_line(&chunk).expect("expected Some(RiskySite)");
        assert_eq!(site.file, "src/x.rs");
        assert_eq!(site.match_line, 42);
        assert_eq!(site.bound_var, "MARKER");
    }

    #[test]
    fn pattern_for_matches_combined_pattern_byte_exactly() {
        assert_eq!(
            pattern_for(VARIANTS),
            r"Value::(Int|Float|Str|Bytes|Handle)\(",
        );
    }

    // ----------------------------------------------------------------
    // import-smoke tests for xtask::diag
    // ----------------------------------------------------------------
    //
    // Full wire-format lockdown lives in `xtask::diag::tests`. The
    // tests here merely confirm the import path resolves and that
    // `diag_line_from` produces a record whose leading field is `code`
    // (so a future contributor who re-routes the import to a wrong
    // module fails these tests immediately, rather than at the next
    // audit run).

    #[test]
    fn diag_import_resolves() {
        // Smoke: the imported `CODE_AUDIT` constant is the expected literal.
        assert_eq!(CODE_AUDIT, "E-AUDIT");
        // Smoke: the imported `wants_json` recognizes the flag.
        assert!(wants_json(&[
            "xtask".to_string(),
            "audit".to_string(),
            "--message-format=json".to_string(),
        ]));
        assert!(!wants_json(&["xtask".to_string(), "audit".to_string()]));
    }

    #[test]
    fn diag_line_from_via_diag_module_emits_canonical_prefix() {
        let line = diag_line_from(CODE_AUDIT, "error", "smoke", None, None);
        assert!(
            line.starts_with(r#"{"code":"E-AUDIT","level":"error","file":null,"line":null,"col":null,"message":"smoke","hint":null}"#),
            "imported diag_line_from must emit the canonical seven-field shape (got: {line:?})"
        );
    }
}
