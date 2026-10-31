#!/usr/bin/env python3
"""Generate tests/fixtures/c3_cases.json from real CPython.

Each case is a class hierarchy, declared in order. Every class is created
with `type(name, bases, {})`; the recorded expectation is CPython's own
`__mro__` (by class name) or, for the class whose creation fails, the exact
`TypeError` message. `object` is spelled out as an explicit root so the COSM
side can declare it as a kind, exactly as Python has it.

Never edit the JSON by hand: rerun this script
(`python3 tests/gen_c3_cases.py > tests/fixtures/c3_cases.json`).
"""
import json
import platform
import sys

CASES = [
    ("single_chain", [("A", []), ("B", ["A"]), ("C", ["B"])]),
    # The diamond from Python's docs (tutorial §9.5.1 / "The Python 2.3 MRO").
    ("diamond", [("A", []), ("B", ["A"]), ("C", ["A"]), ("D", ["B", "C"])]),
    ("diamond_reversed", [("A", []), ("B", ["A"]), ("C", ["A"]), ("D", ["C", "B"])]),
    # "The Python 2.3 Method Resolution Order", first worked example.
    ("mro23_example1", [("F", []), ("E", []), ("D", []), ("C", ["D", "F"]), ("B", ["D", "E"]), ("A", ["B", "C"])]),
    # Same paper, second example (B's bases swapped).
    ("mro23_example2", [("F", []), ("E", []), ("D", []), ("C", ["D", "F"]), ("B", ["E", "D"]), ("A", ["B", "C"])]),
    # Wikipedia's C3 example.
    ("wikipedia", [
        ("A", []), ("B", []), ("C", []), ("D", []), ("E", []),
        ("K1", ["A", "B", "C"]), ("K2", ["D", "B", "E"]), ("K3", ["D", "A"]),
        ("Z", ["K1", "K2", "K3"]),
    ]),
    # Mixins in front of a base (Dart/Python mixin style).
    ("mixins", [("Base", []), ("M1", []), ("M2", []), ("Widget", ["M1", "M2", "Base"]), ("Button", ["Widget", "M2"])]),
    ("deep_diamond", [
        ("A", []), ("B", ["A"]), ("C", ["A"]), ("D", ["B"]), ("E", ["C"]), ("F", ["D", "E"]),
        ("G", ["F", "C"]),
    ]),
    # The classic inconsistent hierarchy ("serious order disagreement").
    ("inconsistent_classic", [("X", []), ("Y", []), ("A", ["X", "Y"]), ("B", ["Y", "X"]), ("Z", ["A", "B"])]),
    # A base listed before its own subclass.
    ("inconsistent_base_before_subclass", [("X", []), ("Y", ["X"]), ("Z", ["X", "Y"])]),
    # Three lists stuck at once.
    ("inconsistent_three_way", [
        ("A", []), ("B", []), ("C", []),
        ("P", ["A", "B"]), ("Q", ["B", "C"]), ("R", ["C", "A"]),
        ("S", ["P", "Q", "R"]),
    ]),
    # Stuck where `object` is one of the remaining heads.
    ("inconsistent_object_head", [("A", []), ("B", ["object", "A"])]),
    ("duplicate_base", [("A", []), ("B", ["A", "A"])]),
    ("duplicate_base_later", [("A", []), ("B", []), ("C", ["A", "B", "A"])]),
]


def run(classes):
    env = {"object": object}
    out = []
    for name, bases in classes:
        parents = [env[b] for b in bases] if bases else [object]
        entry = {"name": name, "parents": [p.__name__ for p in parents]}
        try:
            cls = type(name, tuple(parents), {})
        except TypeError as e:
            entry["error"] = str(e)
            out.append(entry)
            return out
        env[name] = cls
        entry["mro"] = [c.__name__ for c in cls.__mro__]
        out.append(entry)
    return out


doc = {
    "generated_by": "crates/crush-cosm/tests/gen_c3_cases.py",
    "python": f"{platform.python_implementation()} {platform.python_version()}",
    "cases": [{"name": n, "classes": run(c)} for n, c in CASES],
}
json.dump(doc, sys.stdout, indent=1)
sys.stdout.write("\n")
