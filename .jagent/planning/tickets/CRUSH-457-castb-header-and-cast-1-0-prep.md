# CRUSH-457 — .castb gets a magic + version header; CAST 1.0 compatibility gate is factored, tested and written down

| Field | Value |
|-------|-------|
| **ID** | CRUSH-457 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#457](https://github.com/nixpt/crush-ast/issues/457) |
| **Size** | M |
| **Dependencies** | none |

## Problem

Binary CAST is bare CBOR with no header: the module doc says 'there is no custom header or magic' and the format is chosen by file extension only (crates/crush-cast/src/pack.rs:7-13, 85-92, 129-150; docs/FILE-FORMATS.md:24). A renamed or truncated file cannot be told from a CAST file and a future encoding change has nothing to key on (RC gate F3, RELEASE-1.0-CRITERIA.md:93). Separately CAST_VERSION is "0.1.0" (pack.rs:33) and check_version gates only on the major (pack.rs:97-119), so with major 0 every 0.x loads; the accept/reject behaviour of a 1.x reader is unstated and untested, and the rule lives only in code comments (pack.rs:15-31). Ruling 12/15: .castb gets a magic + version header and CAST goes to 1.0 at the 0.13 freeze, additive = minor, breaking = major.

## Evidence (at origin/main 7ecd2515)

- crates/crush-cast/src/pack.rs:7-13, 15-33, 85-92, 97-119, 129-150
- docs/FILE-FORMATS.md:24 and its 'CAST version' section
- docs/planning/RELEASE-1.0-CRITERIA.md:93,99 (F3, F9)
- contrib/magic/crush.magic, contrib/mime/crush.xml (CRUSH-366; no .castb magic)
- crates/crush-cast/tests/repo_cast_files.rs
- ruling 12/15

## Goal

(a) Format::Binary serialize writes a fixed magic plus a container version byte before the CBOR payload; the magic cannot be the first bytes of a CBOR map/array. deserialize(Binary) checks it and fails with a new distinct PackError variant on unknown magic or unsupported container version. Through 0.12 a headerless legacy blob is still read (detected by absence of the magic) with a deprecation warning through the existing diagnostics path; a test pins this and names the 0.13 flip. contrib magic(5)/MIME recognise the header. (b) The version gate becomes a pure function (supported, found) -> Result that check_version calls with CAST_VERSION, tested table-driven for a 0.x and a 1.x supported version. FILE-FORMATS.md states the CAST rule (minor = additive, old readers skip; major = breaking) and a '1.0 flip checklist' (constant, repo_cast_files, fixtures, .castb container version). CAST_VERSION is NOT bumped here.

## Non-goals

- Changing the CBOR payload shape or CAST node shapes
- Touching .casmb (MessagePack) or .cvm1
- Magic-sniffing for JSON files
- Flipping the legacy-read refusal or bumping CAST_VERSION to 1.0 (both 0.13)
- Per-crate versioning tooling (checkpoint 1: lock-step)

## Files

- crates/crush-cast/src/pack.rs
- crates/crush-cast/tests/pack_tests.rs
- crates/crush-lang-sdk/src/formats.rs
- crates/crush-lang-sdk/src/cast_cli.rs
- crates/crush-lang-sdk/tests/formats_cli.rs
- contrib/magic/crush.magic
- contrib/mime/crush.xml
- docs/FILE-FORMATS.md

## Done when

- cargo test -p crush-cast: serialize(Binary) starts with the magic and version byte and round-trips; a bad magic or a future container version returns the new PackError variant
- cargo test -p crush-cast: a headerless legacy CBOR blob still loads and the test names the 0.13 flip
- cargo test -p crush-cast: a table-driven test over {0.1.0, 0.9, 1.0, 1.7.2, 2.0, abc, ''} under supported 0.1.0 and under supported 1.0 asserts accept/reject as documented
- cargo test -p crush-cast --test repo_cast_files and cargo test -p crush-lang-sdk --test formats_cli pass
- `file -m contrib/magic/crush.magic x.castb` identifies a freshly packed file; FILE-FORMATS.md names the header layout, the additive-vs-breaking rule and the 1.0 flip checklist
- cargo +1.100.0 clippy -p crush-cast -p crush-lang-sdk --all-targets -- -D warnings clean

## Depends on

- none

## Refs

- ruling 12/15
- RC gates F3, F9
- CRUSH-366

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
