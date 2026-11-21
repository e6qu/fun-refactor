#!/usr/bin/env python3
"""Refresh the selected comparison without replacing its historical record."""
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock

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
            catalog = {'evidence': [
                {'id': 'current-saved-work-cost', 'path': 'tests/agent-eval/results/session-cost/result.json', 'sha256': 'another-study'},
                {'id': 'current-answer-storage-cost', 'path': relative, 'sha256': fingerprint}]}
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
                catalog = {'evidence': [{'id': 'current-answer-storage-cost', 'path': path, 'sha256': 'invalid'}]}
                with self.subTest(path=path), self.assertRaisesRegex(ValueError, 'unsafe current cost record'):
                    retained.previous_record(root, catalog)
            with self.assertRaises(ValueError):
                retained.previous_record(root, {'evidence': []})


class HistoricalProfile(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.source = self.root / retained.BEFORE_PUBLICATION
        self.source.mkdir(parents=True)
        for name in ('publication.json', 'profile-before.zip', 'profile-before.json'):
            self.source.joinpath(name).write_bytes((retained.ROOT / retained.BEFORE_PUBLICATION / name).read_bytes())
        self.publication = self.root / 'new-publication'
        self.publication.mkdir()
        patcher = mock.patch.object(retained, 'ROOT', self.root)
        patcher.start()
        self.addCleanup(patcher.stop)

    def test_expired_artifact_uses_the_authenticated_retained_bytes(self):
        with mock.patch.object(retained, 'authenticate') as authenticate, \
                mock.patch.object(retained.cost, 'artifact_bytes', side_effect=AssertionError('expired artifact')):
            record = retained.retain_before_profile(self.publication)
        authenticate.assert_called_once_with(retained.BEFORE_RUN, retained.BEFORE_SOURCE)
        self.assertEqual(record['retained_from']['sha256'], retained.BEFORE_PUBLICATION_SHA256)
        for name in ('profile-before.zip', 'profile-before.json'):
            self.assertEqual((self.publication / name).read_bytes(), (self.source / name).read_bytes())

    def test_changed_publication_archive_or_profile_fails_before_writing(self):
        for name, message in (('publication.json', 'retained baseline publication changed'),
                              ('profile-before.zip', 'retained profile archive changed'),
                              ('profile-before.json', 'retained profile changed')):
            with self.subTest(name=name), mock.patch.object(retained, 'authenticate'):
                path = self.source / name
                original = path.read_bytes()
                path.write_bytes(original + b' ')
                with self.assertRaisesRegex(ValueError, message):
                    retained.retain_before_profile(self.publication)
                self.assertEqual(list(self.publication.iterdir()), [])
                path.write_bytes(original)

    def test_failed_original_run_authentication_prevents_publication(self):
        with mock.patch.object(retained, 'authenticate', side_effect=ValueError('original run failed')):
            with self.assertRaisesRegex(ValueError, 'original run failed'):
                retained.retain_before_profile(self.publication)
        self.assertEqual(list(self.publication.iterdir()), [])


if __name__ == '__main__':
    unittest.main()
