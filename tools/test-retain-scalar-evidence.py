#!/usr/bin/env python3
"""Reject substituted archives and unsafe paths before retaining scalar evidence."""
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import zipfile

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('scalar', ROOT / 'tools/retain-scalar-evidence.py')
scalar = importlib.util.module_from_spec(spec)
spec.loader.exec_module(scalar)
profile_spec = importlib.util.spec_from_file_location('profile_controls', ROOT / 'tools/test-compiler-profile.py')
profile_controls = importlib.util.module_from_spec(profile_spec)
profile_spec.loader.exec_module(profile_controls)


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

    def compiler_receipt(self, root):
        path = root / 'current-compiler-profile/result.json'
        path.parent.mkdir()
        data = b'{"source_commit":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","binary_sha256":"binary"}'
        path.write_bytes(data)
        receipt = {'collection': {'source_commit': 'a' * 40},
                   'files': {'result.json': hashlib.sha256(data).hexdigest()}}
        manifest = root / 'current-scalar-collection/manifest.json'
        manifest.parent.mkdir()
        value = {'schema': 'fr-scalar-collection', 'groups': {'compiler-profile': receipt}}
        manifest.write_text(json.dumps(value))
        return path, manifest, value

    def test_compiler_receipt_authenticates_source_and_exact_report_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            path, manifest, value = self.compiler_receipt(Path(directory))
            self.assertEqual(profile_controls.retained_report(path)['source_commit'], 'a' * 40)
            value['groups']['compiler-profile']['collection']['source_commit'] = 'b' * 40
            manifest.write_text(json.dumps(value))
            with self.assertRaisesRegex(ValueError, 'collection identity changed'):
                profile_controls.retained_report(path)
            value['groups']['compiler-profile']['collection']['source_commit'] = 'a' * 40
            manifest.write_text(json.dumps(value))
            path.write_bytes(path.read_bytes().replace(b'"binary"', b'"changed"'))
            with self.assertRaisesRegex(ValueError, 'digest changed'):
                profile_controls.retained_report(path)

    def test_compiler_requires_the_current_family_receipt(self):
        with tempfile.TemporaryDirectory() as directory:
            path, manifest, value = self.compiler_receipt(Path(directory))
            value['schema'] = 'fr-scalar-collection-1'
            manifest.write_text(json.dumps(value))
            with self.assertRaisesRegex(ValueError, 'unsupported scalar collection receipt'):
                profile_controls.retained_report(path)
            manifest.unlink()
            (path.parent / 'manifest.json').write_text(json.dumps(value['groups']['compiler-profile']))
            with self.assertRaises(FileNotFoundError):
                profile_controls.retained_report(path)


if __name__ == '__main__':
    unittest.main()
