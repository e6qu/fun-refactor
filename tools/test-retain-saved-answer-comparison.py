#!/usr/bin/env python3
"""Refresh the selected comparison without replacing its historical record."""
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('retained_answers', Path(__file__).with_name('retain-saved-answer-comparison.py'))
retained = importlib.util.module_from_spec(spec)
spec.loader.exec_module(retained)


class CurrentSelection(unittest.TestCase):
    def test_the_catalog_selects_an_intact_current_record_without_a_fixed_run_number(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            relative = 'tests/agent-eval/results/another-run/result.json'
            path = root / relative
            path.parent.mkdir(parents=True)
            path.write_text(json.dumps({'schema': 'fr-saved-answer-improvement', 'passed': True}))
            fingerprint = hashlib.sha256(path.read_bytes()).hexdigest()
            catalog = {'evidence': [{'id': 'current-saved-work-cost', 'path': relative, 'sha256': fingerprint}]}
            before = path.read_bytes()
            self.assertEqual(retained.previous_record(root, catalog), {'path': relative, 'sha256': fingerprint})
            self.assertEqual(path.read_bytes(), before)
            path.write_text('{}')
            with self.assertRaisesRegex(ValueError, 'previous current cost record changed'):
                retained.previous_record(root, catalog)

    def test_missing_and_escaping_current_selections_fail(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for path in ('/escape.json', 'tests/agent-eval/results/../escape.json', 'outside/result.json'):
                catalog = {'evidence': [{'id': 'current-saved-work-cost', 'path': path, 'sha256': 'invalid'}]}
                with self.subTest(path=path), self.assertRaisesRegex(ValueError, 'unsafe current cost record'):
                    retained.previous_record(root, catalog)
            with self.assertRaises(ValueError):
                retained.previous_record(root, {'evidence': []})


if __name__ == '__main__':
    unittest.main()
