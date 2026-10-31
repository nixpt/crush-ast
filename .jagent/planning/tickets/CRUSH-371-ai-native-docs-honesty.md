# CRUSH-371 — AI-native docs honesty + dead code

| Field | Value |
|-------|-------|
| **ID** | CRUSH-371 |
| **Priority** | P3 |
| **Status** | In review |
| **Assignee** | nova |
| **Dependencies** | CRUSH-234, CRUSH-368..370 (the status table assumes them) |

## Done

- [x] ROADMAP M5 status and CRUSH-1 "fail-loud" note corrected.
- [x] `docs/design/CRUSH-AI-ANNOTATIONS.md`: SemanticMatch is a host cap not vector similarity; W-WIP warns, not blocks; statement fields match `crush-cast/src/ai.rs`; invariant `check` not evaluated; notebook section marked a proposal.
- [x] `docs/design/ai-native-roadmap.md`: no `semantic_search`; what the compiler actually checks.
- [x] New `docs/design/ai-native-status.md` (works / partial / stub per surface, what a provider needs).
- [x] Deleted dead `crush-frontend/src/ai_runtime.rs` and `cson_desugar.rs` (no users, tests included).
- Left: `Program.ai_meta` (in the serialized CAST schema, bindings, fixtures; never populated).
