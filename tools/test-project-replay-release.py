"""Refuse a replay from another release before compiling its checkers."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('recheck', Path(__file__).with_name('recheck-project-changes.py'))
recheck = importlib.util.module_from_spec(spec)
spec.loader.exec_module(recheck)


class ReplayRelease(unittest.TestCase):
    def test_matching_release_passes(self):
        recheck.require_current_release({'release': '0.89.0'}, '0.89.0')

    def test_mismatched_release_reports_both_versions_and_the_required_action(self):
        with self.assertRaisesRegex(AssertionError,
                r'records release 0\.88\.0; this checkout builds 0\.89\.0.*collect a fresh replay'):
            recheck.require_current_release({'release': '0.88.0'}, '0.89.0')

    def test_missing_release_cannot_silently_inherit_the_current_one(self):
        with self.assertRaises(KeyError):
            recheck.require_current_release({}, '0.89.0')


if __name__ == '__main__':
    unittest.main()
