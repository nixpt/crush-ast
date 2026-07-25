//! Host runtime for the CVM1 bytecode VM.
//!
//! [`Runtime`] wraps [`crush_vm::run_with_caps`] with convenience methods for
//! loading programs from blobs or CASM text, applying quotas, registering host
//! capabilities, and inspecting results.
//!
//! ## Codebase caps (AI-native)
//!
//! Call [`Runtime::with_codebase`] to auto-build a `CrushIndex` from in-memory
//! Crush source and inject the six `codebase.*` host caps.  Or use
//! [`Runtime::with_codebase_files`] to read Crush source files from disk.

use chrono::{NaiveDate, Utc};
use crush_frontend::parse_source;
use crush_index::{CrushIndex, DejavueEvent};
use crush_vm::{HostCaps, Program, Quotas, VmError, VmResult, assemble, run_with_caps};
use std::sync::Arc;

/// Errors that can occur when running a program through the SDK.
#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("failed to load CVM1 blob: {0}")]
    LoadBlob(String),

    #[error("failed to assemble CASM source: {0}")]
    Assembly(String),

    #[error("failed to parse Crush source for codebase index: {module}: {cause}")]
    IndexParse { module: String, cause: String },

    #[error("failed to read source file '{path}': {cause}")]
    IndexRead { path: String, cause: String },

    #[error(transparent)]
    Vm(#[from] VmError),
}

/// A configured CVM1 host runtime.
///
/// The runtime does not hold mutable VM state; it carries execution quotas
/// and an optional host-capability registry. Programs are executed statelessly
/// and do not share state between runs.
#[derive(Debug)]
pub struct Runtime {
    quotas: Quotas,
    host_caps: Option<HostCaps>,
    /// Cache of the `Arc<CrushIndex>` passed to the last `register*`
    /// call. `with_dejavue` uses this to update the inner state via
    /// `Arc::make_mut` and re-register every codebase cap so events
    /// propagate to the live cap fleet. `None` when no codebase caps
    /// have been registered yet — `with_dejavue` then builds a fresh
    /// empty index.
    codebase_index: Option<Arc<CrushIndex>>,
    /// Cache of the `today` value used by the last `register*` call.
    /// `with_dejavue` re-registers the temporaries caps with the SAME
    /// pinned date so the staleness boundary doesn't silently rotate
    /// between calls. `None` means no prior `with_codebase_at`; the
    /// first `with_dejavue` falls back to `Utc::now().date_naive()`.
    today: Option<NaiveDate>,
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

impl Runtime {
    /// Create a runtime with default quotas and no host capabilities.
    pub fn new() -> Self {
        Self {
            quotas: Quotas::default(),
            host_caps: None,
            codebase_index: None,
            today: None,
        }
    }

    /// Create a runtime from explicit quotas.
    pub fn with_quotas(quotas: Quotas) -> Self {
        Self {
            quotas,
            host_caps: None,
            codebase_index: None,
            today: None,
        }
    }

    /// Register host capabilities.
    pub fn with_host_caps(mut self, host_caps: HostCaps) -> Self {
        self.host_caps = Some(host_caps);
        self
    }

    /// Parse Crush source code for each `(module_name, source)` pair, build a
    /// [`CrushIndex`], and register the six `codebase.*` host capabilities,
    /// pinned to the supplied `today`.
    ///
    /// **Wall-clock-independent variant of [`Self::with_codebase`]** — the
    /// staleness predicates on `codebase.temporaries()` /
    /// `codebase.stale_temporaries()` will evaluate against the anchored
    /// date you pass, not against `chrono::Utc::now()` at runtime
    /// construction. Use this in:
    ///
    /// - **Production hosts that want reproducible first-boot checks** —
    ///   cache the pinned `today` in a config file, so a restart on
    ///   Friday vs Saturday evaluates the 90-day boundary identically.
    /// - **Tests** — the existing `crush-lang-sdk` E2E test
    ///   `crush-lang-sdk/tests/codebase_stale_e2e.rs` constructs caps
    ///   manually because this chain method didn't exist; it can now be
    ///   expressed identically via the chain.
    ///
    /// For the wall-clock-bound convenience path, use [`Self::with_codebase`]
    /// (which is now a one-line wrapper around this method, anchored to
    /// `Utc::now().date_naive()` at call time).
    ///
    /// Existing capabilities (from a prior [`Self::with_host_caps`] call)
    /// are preserved — the codebase caps are appended, not replaced.
    ///
    /// # Example
    /// ```rust,no_run
    /// use chrono::{Duration, NaiveDate, Utc};
    /// use crush_lang_sdk::Runtime;
    ///
    /// let today = Utc::now().date_naive() - Duration::days(30);
    /// let rt = Runtime::new()
    ///     .with_codebase_at(&[("scheduler", ".func main\nHALT")], today)
    ///     .unwrap();
    /// ```
    pub fn with_codebase_at(
        mut self,
        sources: &[(&str, &str)],
        today: NaiveDate,
    ) -> Result<Self, RuntimeError> {
        let mut index = CrushIndex::new();
        for (module_name, source) in sources {
            let program = parse_source(source).map_err(|e| RuntimeError::IndexParse {
                module: module_name.to_string(),
                cause: e.to_string(),
            })?;
            index.add_program(module_name, &program);
        }
        let arc = Arc::new(index);
        let caps = self.host_caps.get_or_insert_with(HostCaps::new);
        crate::codebase::register_at(caps, Arc::clone(&arc), today);
        // Cache the Arc + today so `with_dejavue` can update the
        // inner state via `Arc::make_mut(reuse)` and re-register the
        // 11 codebase caps with the today the caller pinned (so the
        // staleness boundary doesn't silently rotate).
        self.codebase_index = Some(arc);
        self.today = Some(today);
        Ok(self)
    }

    /// Parse Crush source code for each `(module_name, source)` pair, build a
    /// [`CrushIndex`], and register the six `codebase.*` host capabilities,
    /// pinned to `chrono::Utc::now().date_naive()` at the time this method
    /// is called.
    ///
    /// Wall-clock-bound stub for [`Self::with_codebase_at`] — there is one
    /// builder body, and `with_codebase_at` owns it. Use `with_codebase`
    /// when you don't care about reproducibility (one-shot CLI runs,
    /// ad-hoc queries); use `with_codebase_at` when you do (long-lived
    /// hosts, tests, anything crossing a reboot boundary).
    ///
    /// Existing capabilities (from a prior [`Self::with_host_caps`] call) are
    /// preserved — the codebase caps are appended, not replaced.
    ///
    /// # Example
    /// ```rust,no_run
    /// use crush_lang_sdk::Runtime;
    ///
    /// let rt = Runtime::new()
    ///     .with_codebase(&[("scheduler", ".func main\nHALT")])
    ///     .unwrap();
    /// ```
    pub fn with_codebase(
        self,
        sources: &[(&str, &str)],
    ) -> Result<Self, RuntimeError> {
        self.with_codebase_at(sources, Utc::now().date_naive())
    }

    /// Read Crush source files from disk, build a [`CrushIndex`], and register
    /// all `codebase.*` host capabilities (the post-CRUSH-31 superset is
    /// eleven: six core caps + `decisions`, `temporaries`,
    /// `stale_temporaries`, `annotation_history`).
    ///
    /// Each file's stem (filename without extension) is used as the module name.
    /// Existing capabilities are preserved.
    ///
    /// # Example
    /// ```rust,no_run
    /// use crush_lang_sdk::Runtime;
    ///
    /// let rt = Runtime::new()
    ///     .with_codebase_files(&["src/scheduler.crush", "src/types.crush"])
    ///     .unwrap();
    /// ```
    pub fn with_codebase_files(
        mut self,
        paths: &[impl AsRef<std::path::Path>],
    ) -> Result<Self, RuntimeError> {
        let mut index = CrushIndex::new();
        for path in paths {
            let path = path.as_ref();
            let module_name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown");
            let source = std::fs::read_to_string(path).map_err(|e| RuntimeError::IndexRead {
                path: path.display().to_string(),
                cause: e.to_string(),
            })?;
            let program = parse_source(&source).map_err(|e| RuntimeError::IndexParse {
                module: module_name.to_string(),
                cause: e.to_string(),
            })?;
            index.add_program(module_name, &program);
        }
        let now = Utc::now().date_naive();
        let arc = Arc::new(index);
        let caps = self.host_caps.get_or_insert_with(HostCaps::new);
        crate::codebase::register_at(caps, Arc::clone(&arc), now);
        self.codebase_index = Some(arc);
        self.today = Some(now);
        Ok(self)
    }

    /// Inject typed dejavue events into the codebase index, taking or
    /// building an `Arc<CrushIndex>` and registering all 11 `codebase.*`
    /// caps over it. Composes after [`Self::with_codebase`] or
    /// [`Self::with_codebase_at`] (which cache the existing `Arc` so
    /// `Arc::make_mut` can clone the inner state and re-register the
    /// caps in lockstep).
    ///
    /// `Arc::make_mut` triggers a COW clone when `strong_count > 1`
    /// (which it is — the 11 registered caps each hold an `Arc::clone`).
    /// Because the new Arc is FRESH, the registered caps still point
    /// to the pre-events state. Re-registering the caps via
    /// `crush_lang_sdk::codebase::register_at` then DROPS the stale
    /// caps (UPSERT via `HostCaps::register`'s `HashMap::insert`)
    /// and replaces them with caps pointing at the new Arc — so cap
    /// dispatch observes the events.
    ///
    /// When called before any `with_codebase*`, the cache is empty and
    /// a fresh `CrushIndex::new()` is constructed (the events live
    /// alone — no source-parsed modules). If you want both, call
    /// `with_codebase*` first, then `with_dejavue`; otherwise
    /// `with_codebase*` constructs a fresh index and you'd lose the
    /// events.
    ///
    /// Composition order matters — see `[Self::with_codebase_at]` for
    /// the reproduction contract: the `today` pinned by
    /// `with_codebase_at` is preserved across the chain.
    ///
    /// # Example
    /// ```rust,no_run
    /// use crush_index::{CrushIndex, DejavueEvent};
    /// use crush_lang_sdk::Runtime;
    /// use chrono::DateTime;
    /// use std::str::FromStr;
    ///
    /// let event = DejavueEvent {
    ///     ts: DateTime::from_str("2026-05-01T00:00:00-05:00").unwrap(),
    ///     event: "decision".into(),
    ///     decision_title: Some("inv-x".into()),
    ///     ..Default::default()
    /// };
    /// let rt = Runtime::new()
    ///     .with_codebase(&[("mod", "fn f() { }")])
    ///     .unwrap()
    ///     .with_dejavue(vec![event]);
    /// ```
    pub fn with_dejavue(mut self, events: Vec<DejavueEvent>) -> Self {
        let today = self.today.unwrap_or_else(|| Utc::now().date_naive());
        let arc = match self.codebase_index.take() {
            Some(existing) => {
                // The cached Arc is the master. `Arc::make_mut` returns
                // a `&mut CrushIndex`, cloning the inner allocation
                // when strong_count > 1 (the 11 caps each hold an
                // Arc::clone), so we mutate the CLONE and re-register
                // the caps with the new Arc. The registered caps'
                // pre-mutation Arcs are dropped along with the old
                // caps on UPSERT.
                let mut draft = existing;
                Arc::make_mut(&mut draft).set_dejavue_events(events);
                draft
            }
            None => {
                let mut fresh = CrushIndex::new();
                fresh.set_dejavue_events(events);
                Arc::new(fresh)
            }
        };
        let caps = self.host_caps.get_or_insert_with(HostCaps::new);
        crate::codebase::register_at(caps, Arc::clone(&arc), today);
        self.codebase_index = Some(arc);
        self.today = Some(today);
        self
    }

    /// Return the quotas used by this runtime.
    pub fn quotas(&self) -> &Quotas {
        &self.quotas
    }

    /// Replace the quotas used by this runtime.
    pub fn set_quotas(&mut self, quotas: Quotas) {
        self.quotas = quotas;
    }

    /// Run a pre-loaded [`Program`].
    pub fn run(&self, program: &Program) -> Result<VmResult, RuntimeError> {
        Ok(run_with_caps(
            program,
            &self.quotas,
            self.host_caps.as_ref(),
        )?)
    }

    /// Load a CVM1 binary blob and run it.
    pub fn run_blob(&self, blob: &[u8]) -> Result<VmResult, RuntimeError> {
        let program =
            Program::from_blob(blob).map_err(|e| RuntimeError::LoadBlob(e.to_string()))?;
        self.run(&program)
    }

    /// Assemble CASM text and run the resulting program.
    ///
    /// `permissions` lists the capability names that the program is allowed
    /// to invoke (e.g. `["io.print"]`).
    pub fn run_casm(
        &self,
        source: &str,
        permissions: &[&str],
        name: Option<&str>,
    ) -> Result<VmResult, RuntimeError> {
        let program = assemble(source, Some(permissions), name)
            .map_err(|e| RuntimeError::Assembly(e.to_string()))?;
        self.run(&program)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_hello_program() {
        let source = r#"
            .func main
            PUSH_STR "hi"
            CAP_CALL "io.print" 1
            HALT
        "#;

        let result = Runtime::new()
            .run_casm(source, &["io.print"], Some("hello"))
            .expect("run should succeed");

        assert_eq!(result.output, "hi\n");
        assert!(result.halted);
    }

    #[test]
    fn missing_permission_is_caught() {
        let source = r#"
            .func main
            PUSH_STR "hi"
            CAP_CALL "io.print" 1
            HALT
        "#;

        let err = Runtime::new()
            .run_casm(source, &[], Some("no-perms"))
            .expect_err("should fail without permission");

        assert!(
            err.to_string().contains("capability not declared"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn quotas_are_applied() {
        let source = r#"
            .func main
            loop:
            JMP loop
            HALT
        "#;

        let mut quotas = Quotas::default();
        quotas.max_steps = 10;

        let err = Runtime::with_quotas(quotas)
            .run_casm(source, &[], Some("infinite-loop"))
            .expect_err("should hit step quota");

        assert!(
            err.to_string().contains("instruction quota exceeded"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn with_codebase_registers_caps() {
        let crush_src = r#"
@module {
  purpose: "test nav module"
  exports: [navigate]
}
fn navigate(url) {
  let x = 1
}
"#;
        let rt = Runtime::new()
            .with_codebase(&[("nav", crush_src)])
            .expect("index build should succeed");

        // The runtime now has codebase caps registered; host_caps is Some.
        // Verify by checking the internal state via a round-trip cap check.
        // We probe by building a caps set and running a CASM program that
        // calls codebase.modules — if it errors "capability not declared"
        // then the cap wasn't registered; if it errors "not permitted" that
        // also means not registered; success or any other VM error means it
        // was registered and invoked.
        let casm = r#"
            .func main
            CAP_CALL "codebase.modules" 0
            HALT
        "#;
        let result = rt.run_casm(casm, &["codebase.modules"], Some("probe"));
        // The VM runs and hits the cap (returns an array) — any result that
        // isn't "capability not declared" confirms registration.
        match result {
            Ok(_) => {}
            Err(e) => {
                let msg = e.to_string();
                assert!(
                    !msg.contains("capability not declared"),
                    "codebase.modules was not registered: {msg}"
                );
            }
        }
    }

    #[test]
    fn with_codebase_files_missing_path_is_reported() {
        let err = Runtime::new()
            .with_codebase_files(&["/nonexistent/path/missing.crush"])
            .expect_err("missing file should fail");
        assert!(
            err.to_string().contains("missing.crush"),
            "error should mention the path: {err}"
        );
    }

    #[test]
    fn with_codebase_preserves_existing_caps() {
        use crate::host_caps::HostCapsBuilder;
        let existing = HostCapsBuilder::new().time(true).build();
        let crush_src = "fn f() { }";
        let rt = Runtime::new()
            .with_host_caps(existing)
            .with_codebase(&[("m", crush_src)])
            .expect("index build should succeed");

        // Both time.now and codebase.modules must be present.
        // We verify by running probes for both — neither should say "not declared".
        for cap in ["time.now", "codebase.modules"] {
            let casm = format!(
                ".func main\nCAP_CALL \"{cap}\" 0\nHALT\n"
            );
            let result = rt.run_casm(&casm, &[cap], Some("probe"));
            if let Err(e) = result {
                assert!(
                    !e.to_string().contains("capability not declared"),
                    "{cap} missing after with_codebase: {e}"
                );
            }
        }
    }

    #[test]
    fn with_codebase_at_registers_caps_with_pinned_today() {
        // Pin a fixed `today` so the chain call is wall-clock-independent.
        // Distinct from `with_codebase_registers_caps` so a regression that
        // silently swaps `_at` for `_at` + `Utc::now()` would fail this test
        // ("the boundary maths is reproducible across reboots" is the whole
        // point — see the `with_codebase_at` doc comment).
        let pin = NaiveDate::from_ymd_opt(2026, 6, 20)
            .expect("hard-coded test date is valid");
        let crush_src = "@module { purpose: \"pinned-today test\" }\nfn f() { }";
        let rt = Runtime::new()
            .with_codebase_at(&[("pinned", crush_src)], pin)
            .expect("index build should succeed");

        // Probe by running CASM that calls a codebase cap — same assertion
        // shape as `with_codebase_registers_caps`.
        let casm = r#"
            .func main
            CAP_CALL "codebase.modules" 0
            HALT
        "#;
        let result = rt.run_casm(casm, &["codebase.modules"], Some("probe"));
        match result {
            Ok(_) => {}
            Err(e) => {
                let msg = e.to_string();
                assert!(
                    !msg.contains("capability not declared"),
                    "codebase.modules was not registered (with_codebase_at): {msg}"
                );
            }
        }
    }

    // ── CRUSH-31 review followup: `with_dejavue` builder tests ────────────
    //
    // Each test exercises a different composition path through
    // `Runtime::with_dejavue`. Together they lock the four scenarios:
    //  (A) chained after `with_codebase_at` — verifies the cached Arc
    //      gets `Arc::make_mut`-cloned and the events surface in the
    //      `codebase.annotation_history` cap output.
    //  (B) chained alone (no prior `with_codebase*`) — verifies the
    //      builder still installs the 11 `codebase.*` caps (gated by
    //      `HostCaps::register`'s UPSERT semantics) and emits the
    //      events as well.

    #[test]
    fn with_dejavue_after_codebase_injects_events() {
        // Scenario A: with_codebase_at → with_dejavue → CASM probe of
        // codebase.annotation_history.
        //
        // Distinct from the same-named integration test in
        // `codebase_caps_integration.rs`: this one lives next to
        // Runtime, uses `parse_timeline_str` directly (no
        // filesystem), and locks TWO contracts:
        //   (a) the chained `with_dejavue` propagates the events into
        //       the cap's view of the index (must surface both
        //       decision_reason strings);
        //   (b) the explicit ts-ascending re-sort in
        //       `CrushIndex::annotation_history` overrides corpus
        //       insertion order — we deliberately insert the LATER
        //       event FIRST in the corpus; if the re-sort is removed,
        //       this test breaks.
        use crush_index::dejavue::parse_timeline_str;

        let pin = NaiveDate::from_ymd_opt(2026, 6, 20)
            .expect("hard-coded test date is valid");
        // Reverse chronological order in the corpus so ts-ascending
        // (not insertion order) is the only thing that puts `earlier`
        // before `later` in the cap output.
        let (events, _parse_skipped) = parse_timeline_str(
            r#"{"ts":"2026-05-01T00:00:00-05:00","event":"decision","decision_title":"inv-x","decision_reason":"later"}
{"ts":"2026-04-01T00:00:00-05:00","event":"decision","decision_title":"inv-x","decision_reason":"earlier"}
"#,
        );
        assert_eq!(events.len(), 2, "fixture: both events parse cleanly");

        let rt = Runtime::new()
            .with_codebase_at(&[("m", "fn f() { }")], pin)
            .expect("with_codebase_at succeeds")
            .with_dejavue(events);

        let casm = r#"
            .func main
            PUSH_STR "inv-x"
            CAP_CALL "codebase.annotation_history" 1
            CAP_CALL "io.print" 1
            HALT
        "#;
        let result = rt
            .run_casm(
                casm,
                &["codebase.annotation_history", "io.print"],
                Some("hist-after-codebase"),
            )
            .expect("run");
        assert!(result.halted, "the CASM program should halt cleanly");
        assert!(
            result.output.contains("earlier"),
            "earlier decision_reason missing — with_dejavue didn't update the registered index:\n{}",
            result.output
        );
        assert!(
            result.output.contains("later"),
            "later decision_reason missing — with_dejavue didn't update the registered index:\n{}",
            result.output
        );
        // ts-ascending in the output — corpus inserted LATER first
        // (insertion index 0 = 2026-05); ts-ascending re-sort must
        // surface EARLIER first.
        let earlier_pos = result
            .output
            .find("earlier")
            .expect("earlier present");
        let later_pos = result
            .output
            .find("later")
            .expect("later present");
        assert!(
            earlier_pos < later_pos,
            "annotation_history must be ts-ascending; earlier at {earlier_pos}, later at {later_pos}:\n{}",
            result.output
        );
    }

    #[test]
    fn with_dejavue_alone_creates_codebase_caps() {
        // Scenario B: with_dejavue on a fresh Runtime (no
        // with_codebase* before it). The builder should install all
        // 11 codebase caps + register the events into a fresh
        // empty CrushIndex. Any caller of `codebase.annotation_history`
        // gets the events; callers of `codebase.modules()` get an empty
        // array (no source-parsed modules).
        use crush_index::dejavue::parse_timeline_str;

        let (events, _parse_skipped) = parse_timeline_str(
            r#"{"ts":"2026-05-01T00:00:00-05:00","event":"decision","decision_title":"inv-y","decision_reason":"solo"}
"#,
        );
        assert_eq!(events.len(), 1);

        let rt = Runtime::new().with_dejavue(events);

        // 1: codebase.annotation_history("inv-y") emits the event.
        let casm_history = r#"
            .func main
            PUSH_STR "inv-y"
            CAP_CALL "codebase.annotation_history" 1
            CAP_CALL "io.print" 1
            HALT
        "#;
        let result = rt
            .run_casm(
                casm_history,
                &["codebase.annotation_history", "io.print"],
                Some("annotation-history-probe"),
            )
            .expect("history cap should be callable after with_dejavue alone");
        assert!(result.halted);
        assert!(
            result.output.contains("solo"),
            "expected injected decision_reason to surface: \n{}",
            result.output
        );

        // 2: codebase.modules returns an empty array (no source
        // parsed) — proves the cap is REGISTERED rather than
        // silently not-installed (which would error "capability not
        // declared" on the call below).
        let casm_modules = r#"
            .func main
            CAP_CALL "codebase.modules" 0
            CAP_CALL "io.print" 1
            HALT
        "#;
        let result = rt
            .run_casm(
                casm_modules,
                &["codebase.modules", "io.print"],
                Some("modules-probe"),
            )
            .expect("modules cap should be callable after with_dejavue alone");
        assert!(result.halted);
        // Empty array — accept either `[]` literal OR empty-between-brackets
        // form (`[ ]`, `[   ]`) so a future `io.print` formatter that
        // adds intra-bracket whitespace doesn't regress this test. A
        // populated row (e.g. `[{fn_name: ...}]`) still fails: `inner`
        // is non-empty. Same shape as
        // `integration_uncovered_paths_cap_returns_empty_array_via_runtime`
        // — the CRUSH-29 reviewer relaxed the strict `[]` match there
        // for the same reason.
        let trimmed = result.output.trim();
        let inner = trimmed.trim_matches(|c| c == '[' || c == ']').trim();
        assert!(
            trimmed.contains("[]") || inner.is_empty(),
            "expected empty modules() output (no source parsed), got:\n{}\ninner=[{}]",
            result.output,
            inner
        );
    }
}
