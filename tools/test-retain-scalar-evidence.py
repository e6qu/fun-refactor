#!/usr/bin/env python3
"""Reject substituted archives and unsafe paths before retaining scalar evidence."""
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import subprocess
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
    def test_capture_names_do_not_choose_the_destination_collection(self):
        paths = {'facts': 'refinement-evidence/original-facts/result.json',
                 'calls': 'refinement-evidence/original-calls/result.json'}
        data = dict.fromkeys([*paths.values(), 'refinement-refresh-logs/original.log'], b'{}')
        self.assertEqual(scalar.report_paths(data, ['facts', 'calls']), paths)

    def test_mixed_missing_and_extra_capture_reports_refuse(self):
        facts = 'refinement-evidence/original-facts/result.json'
        calls = 'refinement-evidence/original-calls/result.json'
        for names in ([facts], [facts, 'refinement-evidence/other-calls/result.json'],
                      [facts, calls, 'refinement-evidence/original-extra/result.json'],
                      [facts, calls, 'unexpected.json'],
                      [facts, calls, 'refinement-evidence/other-facts/result.json']):
            with self.subTest(names=names), self.assertRaises(ValueError):
                scalar.report_paths(dict.fromkeys(names, b'{}'), ['facts', 'calls'])

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

    def test_matching_collection_identity_still_requires_the_current_report_audit(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(scalar, 'ROOT', Path(directory)):
            (Path(directory) / 'tests/agent-eval/results').mkdir(parents=True)
            identity = {'source_commit': 'a' * 40}
            data = {'result.json': json.dumps({'source_commit': 'a' * 40}).encode()}
            failed = subprocess.CompletedProcess([], 1, stdout='', stderr='stale recorded input')
            with patch.object(scalar.subprocess, 'run', return_value=failed) as audit:
                with self.assertRaisesRegex(ValueError, 'audit failed: stale recorded input'):
                    scalar.keep('compiler-profile', data, identity, 'checked')
            self.assertIn('tools/compiler-profile.py', audit.call_args.args[0])

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


class CollectionSources(unittest.TestCase):
    def git(self, root, *arguments):
        return subprocess.check_output(['git', *arguments], cwd=root, text=True,
                                       stderr=subprocess.STDOUT, timeout=5).strip()

    def repository(self, root):
        self.git(root, 'init', '-q', '-b', 'main')
        self.git(root, 'config', 'user.name', 'Source Test')
        self.git(root, 'config', 'user.email', 'source@example.test')
        for name in scalar.ANALYZER_PATHS:
            path = root / name
            if '.' not in path.name:
                path /= 'source.txt'
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text('original source\n')
        return self.commit(root, 'original source')

    def commit(self, root, message):
        self.git(root, 'add', '.')
        self.git(root, 'commit', '-qm', message)
        return self.git(root, 'rev-parse', 'HEAD')

    def test_non_analyzer_edits_preserve_exact_source_identity(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            original = self.repository(root)
            (root / 'README.md').write_text('Explain the same code.\n')
            later = self.commit(root, 'explain the source')
            self.assertNotEqual(original, later)
            scalar.verify_sources(root, {original, later})

    def test_committed_changes_in_every_analyzer_path_refuse(self):
        for name in scalar.ANALYZER_PATHS:
            with self.subTest(path=name), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                original = self.repository(root)
                path = root / name
                if path.is_dir():
                    path /= 'source.txt'
                path.write_text('changed source\n')
                changed = self.commit(root, 'change the source')
                with self.assertRaisesRegex(ValueError, 'differs from current code'):
                    scalar.verify_sources(root, {original, changed})

    def test_a_common_old_commit_cannot_hide_changed_current_code(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            original = self.repository(root)
            (root / 'src/source.txt').write_text('changed current source\n')
            self.commit(root, 'change current code')
            with self.assertRaisesRegex(ValueError, 'differs from current code'):
                scalar.verify_sources(root, {original})

    def test_dirty_and_untracked_analyzer_inputs_refuse(self):
        for name in ('src/source.txt', 'sdk/python/src/new.py'):
            with self.subTest(path=name), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                original = self.repository(root)
                (root / name).write_text('unchecked source\n')
                with self.assertRaisesRegex(ValueError, 'must be clean'):
                    scalar.verify_sources(root, {original})

    def test_missing_and_malformed_collection_commits_refuse(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.repository(root)
            for revisions in (set(), {'HEAD'}, {'--help'}, {'not a commit'}, {True}):
                with self.subTest(revisions=revisions), self.assertRaisesRegex(ValueError, 'recorded collection commits'):
                    scalar.verify_sources(root, revisions)


if __name__ == '__main__':
    unittest.main()
