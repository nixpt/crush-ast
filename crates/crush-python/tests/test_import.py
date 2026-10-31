"""Smoke test for the crush-python extension (CRUSH-206).

Run after `maturin develop --release` (or install a built wheel):

    python -I -m unittest discover -s crates/crush-python/tests

`-I` keeps the interpreter from importing anything from the test's own
directory or the cwd, so this exercises the installed `crush` package.
"""

import unittest

import crush
from crush import validate_cast, parse_cast, cast_version

VALID = '{"cast_version": "0.1.0", "entry": "main", "functions": {}}'


class ImportSmoke(unittest.TestCase):
    def test_module_imports_and_version_matches_crate(self):
        # Must equal crush_cast::CAST_VERSION; the crate's own unit test pins it.
        self.assertEqual(cast_version(), "0.1.0")
        self.assertEqual(crush.cast_version(), "0.1.0")

    def test_validate_cast_verdicts(self):
        self.assertIs(validate_cast(VALID), True)
        self.assertIs(validate_cast('{"invalid_field": true}'), False)
        self.assertIs(validate_cast("not json"), False)
        self.assertIs(
            validate_cast('{"cast_version": "9.0.0", "entry": "main", "functions": {}}'),
            False,
        )

    def test_parse_cast_raises_value_error_on_bad_input(self):
        with self.assertRaises(ValueError):
            parse_cast('{"invalid_field": true}')


if __name__ == "__main__":
    unittest.main()
