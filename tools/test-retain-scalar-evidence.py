#!/usr/bin/env python3
"""Reject substituted archives and unsafe paths before retaining scalar evidence."""
import hashlib
import importlib.util
import io
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import zipfile

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('scalar', ROOT / 'tools/retain-scalar-evidence.py')
scalar = importlib.util.module_from_spec(spec)
spec.loader.exec_module(scalar)


class ScalarRetention(unittest.TestCase):
    def archive(self, names):
        output = io.BytesIO()
        with zipfile.ZipFile(output, 'w') as archive:
            for name in names:
                archive.writestr(name, b'{}')
        data = output.getvalue()
        return data, 'sha256:' + hashlib.sha256(data).hexdigest()

    def test_exact_report_bytes_survive_admission(self):
        data, digest = self.archive(['result.json'])
        self.assertEqual(scalar.files(data, digest), {'result.json': b'{}'})

    def test_artifact_substitution_refuses(self):
        data, _ = self.archive(['result.json'])
        with self.assertRaisesRegex(ValueError, 'digest changed'):
            scalar.files(data, 'sha256:' + '0' * 64)

    def test_paths_cannot_escape_or_collide(self):
        for names in [['../result.json'], ['/result.json'], ['nested\\result.json'], ['directory/'],
                      ['result.json', 'result.json']]:
            with self.subTest(names=names), self.assertRaises(ValueError):
                scalar.files(*self.archive(names))

    def test_report_cannot_claim_a_different_collection_revision(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(scalar, 'ROOT', Path(directory)):
            identity = {'source_commit': 'a' * 40}
            data = {'result.json': b'{"source_commit":"wrong"}'}
            with self.assertRaisesRegex(ValueError, 'source changed'):
                scalar.keep('compiler-profile', data, identity, 'never-written')


if __name__ == '__main__':
    unittest.main()
