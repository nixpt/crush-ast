# State

Updated: 2026-06-22T15:02:48-05:00

CRUSHTESTSSPLIT-1 root cause investigation complete. The split script failed because the CRUSHTESTSSPLIT-2 worktree was created from origin/main (660-line tests.rs) but the v2 split script was calibrated against agent/buffy/network (1179-line tests.rs). On origin/main, banners[0][0]=13, slicing helpers to only ~13 lines. The matrix section is absent from origin/main. Secondary bug: BANNER_RE indentation (^// should be ^\\s*//) for the indented cross-parser matrix banner. Audit confirms origin/main uses identical naming convention; all 16 banners recognized by NAME_MAP. Orphan worktrees/branches/PRs from both CRUSHTESTSSPLIT rounds cleaned up.
