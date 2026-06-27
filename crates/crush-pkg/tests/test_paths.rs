//! `tests/test_paths.rs` — shared test-path helper module.
//!
//! Single anchor for the runtime fixture-path idiom. Callers do
//! `mod test_paths;` at the top of their integration-test file, then use
//! either of:
//!
//!   `test_paths::FIXTURES_BASE`     compile-time `&'static str`, anchors
//!                                   on `<crate-root>/tests/fixtures/`.
//!   `test_paths::fixture_root(N)`   runtime, returns `PathBuf` joining
//!                                   `FIXTURES_BASE + N`.
//!
//! ## Why the return type is `PathBuf` (not `&'static Path`)
//!
//! The user-requested signature was `fn fixture_root(name: &str) -> &'static Path`,
//! which is structurally impossible in Rust: `&'static Path` requires the
//! underlying path storage to live forever, but `name: &str` is a runtime
//! borrow from the call site — no runtime borrow lifetime can be
//! promoted to `'static`. Returning `PathBuf` (owned, heap-allocated) is
//! the canonical Rust idiom (resembles `tempfile::tempdir() -> TempDir`).
//!
//! If a caller actually needs a `&'static Path` (rare for runtime use),
//! reach for `Path::new(test_paths::FIXTURES_BASE)` or
//! `test_paths::FIXTURES_BASE` directly.
//!
//! ## The `include_str!` constraint is separate (NOT covered here)
//!
//! Callers that need compile-time embed (e.g. feeding `main.crush` text
//! to `compile_crush_source`) MUST inline the fixture-dir literal in
//! their `include_str!` call site. The `include_str!` macro requires a
//! LITERAL path at compile time — a runtime `&str` cannot be threaded
//! through. The convention is therefore:
//!
//! ```ignore
//! // Compile-time embed (source-file-relative literal — required):
//! const PROGRAM_SRC: &str = include_str!("fixtures/<name>/main.crush");
//! // Runtime path (uses this helper, single anchor):
//! let fixture_root = test_paths::fixture_root("<name>");
//! ```
//!
//! **Both literals must change together** if the fixture directory is
//! renamed. The compile-time `include_str!` side cannot be abstracted
//! via this helper — the runtime side (this file) IS the abstract.
//!
//! ## File-name convention
//!
//! The user-requested filename `tests/test_paths.rs` is preserved
//! exactly. Cargo compiles EVERY `.rs` file directly under `tests/` as
//! its own integration-test binary; this helper has no `#[test]`
//! functions, so the binary compiles as a 0-test artifact. Standard
//! harmless Cargo pattern (the same applies to `tests/common.rs` or
//! `tests/common/mod.rs`). The build cost is a one-time ~few-second
// link of an empty test binary per `cargo test -p crush-pkg` invocation;
// documented here so a future reviewer isn't surprised by the lack of
//! `#[test]` functions.

/// Compile-time-anchored base path: `<crate-root>/tests/fixtures/`.
///
/// `concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/")` is
/// const-evaluable (both `env!` and string literals are valid in
/// const context, even in edition 2015). The result is a `'static`
/// `&str` usable wherever a string literal is accepted.
pub const FIXTURES_BASE: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/");

/// Return `<crate-root>/tests/fixtures/<name>` as a `PathBuf`.
///
/// Single-string idiom matching the integration-test call sites'
/// ergonomic intent. Allocates a `PathBuf` (heap) per call — fine for
/// test setup, not hot-path. Uses `PathBuf::push` (mutates in place)
/// rather than `Path::join` (returns new `PathBuf`) because the
/// allocation pattern is identical but `push` is slightly clearer
/// here.
pub fn fixture_root(name: &str) -> std::path::PathBuf {
    let mut p = std::path::PathBuf::from(FIXTURES_BASE);
    p.push(name);
    p
}

/// Borrow `<crate-root>/tests/fixtures/` as a `&'static Path`.
///
/// Zero-alloc counterpart to `fixture_root(name)` — same anchor, but
/// returns a borrow over the const base rather than an owned `PathBuf`.
/// Use this when the caller already has the fixture's own file name
/// (e.g. `fixtures_base().join("sdk-matrix-locator").join("main.crush")`)
/// or when chaining without allocation. Pair with `.join(...)` for the
/// typical "I want to anchor on the fixtures base" intent.
pub fn fixtures_base() -> &'static std::path::Path {
    std::path::Path::new(FIXTURES_BASE)
}
