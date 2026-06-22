#!/usr/bin/env python3
"""CRUSHTESTSSPLIT-1 atomic split of tests.rs.

Reads tests.rs once, classifies the inline section banners into one of
the seven target sub-files, writes tests/mod.rs (helpers + sub-mod
declarations) + tests/<sub>.rs (each section's content) in one pass,
removes tests.rs.

Usage: python3 run_split.py <WORKTREE_PATH>
"""

import re
import sys
from collections import defaultdict
from pathlib import Path

DASH = "\u2500"
WORKTREE = Path(sys.argv[1])
SRC = WORKTREE / "crates" / "crush-vm" / "src" / "tests.rs"
TESTS_DIR = WORKTREE / "crates" / "crush-vm" / "src" / "tests"
TESTS_MOD = TESTS_DIR / "mod.rs"

if not SRC.exists():
    raise SystemExit("SOURCE_NOT_FOUND: " + str(SRC))

raw = SRC.read_text(encoding="utf-8")
lines = raw.split("\n")

BANNER_RE = re.compile(r"^//\s*" + DASH + r"+\s*(.+?)\s*" + DASH + r"+")
banners = []
for i, ln in enumerate(lines):
    m = BANNER_RE.match(ln)
    if m:
        name = m.group(1).strip().rstrip(DASH).strip()
        banners.append((i, name))

if not banners:
    raise SystemExit("NO_BANNERS_FOUND")

NAME_MAP = {
    "arithmetic": "arith",
    "stack ops": "arith",
    "bitwise ops": "arith",
    "control flow": "control_flow",
    "slots (memory)": "control_flow",
    "functions (v2)": "control_flow",
    "strings": "data_types",
    "arrays": "data_types",
    "new types: Bool, Map, Error, Bytes, ARR_PUSH/POP": "data_types",
    "native string ops": "data_types",
    "capabilities": "capabilities",
    "capability tests": "capabilities",
    "binary round-trip": "surfaces",
    "disassembler": "surfaces",
    "step quota": "surfaces",
    "async / green threads": "async_green",
    "Combined round-trip matrix (single source-of-truth for the canonical": "matrix",
    "cross-parser matrix": "matrix",
}

LABELS = {
    "arith": "arithmetic, stack ops, bitwise ops",
    "control_flow": "control flow, slots (memory), function calls",
    "data_types": "strings, arrays, new types, native string ops",
    "capabilities": "capability dispatch",
    "surfaces": "binary round-trip, disassembler, step quota",
    "async_green": "async green threads (spawn / await / yield)",
    "matrix": "combined round-trip + cross-parser matrix",
}

groups = defaultdict(list)
all_sections = []
for i, (idx, name) in enumerate(banners):
    end_idx = banners[i + 1][0] if i + 1 < len(banners) else len(lines)
    nlow = name.lower()
    cls = None
    for key, val in NAME_MAP.items():
        if key.lower() in nlow:
            cls = val
            break
    if cls is None:
        raise SystemExit("UNCLASSIFIED_SECTION: " + repr(name))
    groups[cls].append((idx, end_idx, name))
    all_sections.append((idx, end_idx, name, cls))

TESTS_DIR.mkdir(exist_ok=True)

helpers_lines = lines[:banners[0][0]]

# Build pre-imports for each sub-file (all 7 use the same minimal imports).
imports_block = (
    "use super::*;\n"
    + "use crate::assembler::{assemble, disassemble};\n"
    + "use crate::vm::{Quotas, Value, run};\n"
)

for sub in ["arith", "control_flow", "data_types", "capabilities", "surfaces", "async_green", "matrix"]:
    out_lines = []
    for idx, end_idx, name in groups[sub]:
        out_lines.extend(lines[idx:end_idx])
    while out_lines and out_lines[-1].strip() == "":
        out_lines.pop()
    body = "\n".join(out_lines) if out_lines else ""
    header = (
        "//! Tests for the " + LABELS[sub] + " domain.\n"
        + "//!\n"
        + "//! Auto-extracted from `tests.rs` as part of CRUSHTESTSSPLIT-1.\n"
        + "//!\n"
        + "//! Each fn preserves its original body verbatim; only the\n"
        + "//! section-banner organizer moved into a sub-file.\n"
        + "\n"
    )
    out = header + imports_block + "\n" + body + "\n"
    out_path = TESTS_DIR / (sub + ".rs")
    out_path.write_text(out, encoding="utf-8")
    print("WROTE", out_path, len(out), "bytes")

# Write tests/mod.rs (helpers + sub-mod declarations).
mod_lines = []
mod_lines.extend(helpers_lines)
mod_lines.append("")
mod_lines.append("// ---- Sub-module declarations ----")
mod_lines.append("")
for sub in ["arith", "control_flow", "data_types", "capabilities", "surfaces", "async_green", "matrix"]:
    mod_lines.append("#[cfg(test)]")
    mod_lines.append("mod " + sub + ";")
    mod_lines.append("")
TESTS_MOD.write_text("\n".join(mod_lines), encoding="utf-8")
print("WROTE", TESTS_MOD, len(mod_lines), "lines")

SRC.unlink()
print("REMOVED", SRC)

print("ATOMIC_SPLIT_DONE")
print("sections classified:", len(all_sections))
print("sub files written:", len(groups))
