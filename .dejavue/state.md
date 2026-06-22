# State

Updated: 2026-06-22T15:28:21-05:00

CRUSHTESTSSPLIT-1 (v3) landed and merged into agent/buffy/network. tests.rs (1179 lines, 65 #[test] annotations) split into tests/mod.rs + 6 domain sub-files (arith, control_flow, data_types, capabilities, surfaces, async_green, matrix). All 5 helpers preserved in mod.rs. Build green, 81 tests pass, per-fn diff IDENTICAL. PR #15 squash-merged. crush-diagnostics + xtask workspace members committed to agent/buffy/network to fix pre-existing build issue. Orphan scripts (v2, v3) and worktrees cleaned up.
