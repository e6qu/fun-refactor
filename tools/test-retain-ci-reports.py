"""Reject mixed sources and extra reports; preserve the original numeric bytes."""
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock

spec = importlib.util.spec_from_file_location('reports', Path(__file__).with_name('retain-ci-reports.py'))
reports = importlib.util.module_from_spec(spec)
spec.loader.exec_module(reports)


class ReportRetention(unittest.TestCase):
    def retain(self, root, *, mixed=False, extra=False, reject=False, only=None):
        prefix = 'test-reports'
        payload = b'{"sample":0.10000000000000001}\n'
        groups = list(reports.GROUPS.values())
        def collect(run, *_):
            data = {f'refinement-evidence/{prefix}-{name}/result.json': payload for name in groups[run - 1]}
            if extra:
                data['refinement-evidence/unexpected.json'] = b'{}'
            identity = {'source_commit': ('b' if mixed and run == 2 else 'a') * 40, 'run_id': str(run)}
            return data, identity
        catalog = root / 'tests/agent-eval/roadmap.json'
        catalog.parent.mkdir(parents=True)
        catalog.write_text(json.dumps({'evidence': [{'id': 'host-recovery'}]}))
        (catalog.parent / 'results').mkdir()
        for group, name in reports.ACTIVE_REFERENCES.items():
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            previous = path.read_text() if path.exists() else ''
            path.write_text(previous + f'"tests/agent-eval/results/previous-{group}/result.json"\n')
        def execute(command, **_):
            if reject and '--audit' in command:
                raise subprocess.CalledProcessError(1, command)
        runs = {group: index for index, group in enumerate(reports.GROUPS, 1)}
        if only is not None:
            runs = {name: runs[name] for name in only}
        with mock.patch.object(reports, 'ROOT', root), \
             mock.patch.object(reports.scalar, 'collect', side_effect=collect), \
             mock.patch.object(reports.scalar, 'verify_sources'), \
             mock.patch.object(reports.scalar.retain, 'command'), \
             mock.patch.object(reports.subprocess, 'run', side_effect=execute), \
             mock.patch.dict('os.environ', {'GITHUB_ACTIONS': 'true', 'GITHUB_REF': 'refs/heads/test',
                                           'GITHUB_REPOSITORY': 'owner/repo'}), \
             mock.patch('sys.argv', ['retain-ci-reports.py', '--runs', json.dumps(runs), '--prefix', prefix]):
            reports.main()
        return payload

    def test_original_numbers_and_collection_identity_are_preserved(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            original = self.retain(root)
            saved = root / 'tests/agent-eval/results/test-reports-host-recovery/result.json'
            self.assertEqual(saved.read_bytes(), original)
            manifest = json.loads((saved.parent.with_name('test-reports-ci-collection') / 'manifest.json').read_text())
            receipt = manifest['groups']['host-recovery']
            self.assertEqual(receipt['collection']['source_commit'], 'a' * 40)
            self.assertEqual(receipt['sha256'], hashlib.sha256(original).hexdigest())
            self.assertEqual((root / 'tests/retained_proofs.rs').read_text(),
                             '"tests/agent-eval/results/test-reports-retained-proofs/result.json"\n')
            for group, name in reports.ACTIVE_REFERENCES.items():
                self.assertIn(f'"tests/agent-eval/results/test-reports-{group}/result.json"',
                    (root / name).read_text())

    def test_mixed_collection_sources_are_rejected_before_saving(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with self.assertRaises(ValueError):
                self.retain(root, mixed=True)
            self.assertEqual(list((root / 'tests/agent-eval/results').iterdir()), [])

    def test_refreshing_proofs_preserves_unrequested_reports(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.retain(root, only=['retained-proofs'])
            saved = root / 'tests/agent-eval/results'
            self.assertEqual({path.name for path in saved.iterdir()},
                             {'test-reports-retained-proofs', 'test-reports-ci-collection'})
            self.assertEqual(json.loads((root / 'tests/agent-eval/roadmap.json').read_text()),
                             {'evidence': [{'id': 'host-recovery'}]})
            manifest = json.loads((saved / 'test-reports-ci-collection/manifest.json').read_text())
            self.assertEqual(set(manifest['groups']), {'retained-proofs'})
            for group, name in reports.ACTIVE_REFERENCES.items():
                if group != 'retained-proofs':
                    self.assertIn(f'"tests/agent-eval/results/previous-{group}/result.json"',
                        (root / name).read_text())

    def test_empty_refresh_refuses_before_any_report_is_saved(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with self.assertRaisesRegex(ValueError, 'requested hosted collections'):
                self.retain(root, only=[])
            self.assertEqual(list((root / 'tests/agent-eval/results').iterdir()), [])

    def test_extra_reports_are_rejected_before_saving(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with self.assertRaises(ValueError):
                self.retain(root, extra=True)
            self.assertEqual(list((root / 'tests/agent-eval/results').iterdir()), [])

    def test_failed_behavioral_audit_prevents_a_collection_receipt(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with self.assertRaises(subprocess.CalledProcessError):
                self.retain(root, reject=True)
            self.assertFalse((root / 'tests/agent-eval/results/test-reports-ci-collection').exists())


if __name__ == '__main__':
    unittest.main()
