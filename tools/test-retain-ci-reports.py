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
    def retain(self, root, *, mixed=False, extra=False, reject=False, only=None, tamper_structural=False,
               omit_repository_artifact=False, wrong_repository_revision=False, format_rust=False):
        prefix = 'test-reports'
        payload = b'{"sample":0.10000000000000001}\n'
        groups = list(reports.GROUPS.values())
        def collect(run, *_):
            data = {f'refinement-evidence/{prefix}-{name}/result.json': payload for name in groups[run - 1]}
            identity = {'source_commit': ('b' if mixed and run == 2 else 'a') * 40, 'run_id': str(run)}
            if 'compiler-profile' in groups[run - 1]:
                data = {'result.json': json.dumps({'source_commit': identity['source_commit']}).encode()}
            if 'python-repositories' in groups[run - 1]:
                value = {'runner': {'revision': ('b' if wrong_repository_revision else 'a') * 40},
                         'artifacts': {'repair.patch': hashlib.sha256(payload).hexdigest()}}
                data = {'result.json': json.dumps(value).encode(), 'repair.patch': payload}
                if omit_repository_artifact:
                    del data['repair.patch']
            group, = groups[run - 1] if len(groups[run - 1]) == 1 else ['single-json']
            if group in reports.DIRECTORY_REPORTS:
                parent = f'refinement-evidence/{prefix}-{group}/'
                data[parent + 'objects/example.json'] = payload
                source_key = 'revision' if group == 'checked-outcomes' else 'source_commit'
                data[parent + 'manifest.json'] = json.dumps({source_key: identity['source_commit'],
                    'files': {name: hashlib.sha256(payload).hexdigest()
                              for name in ('result.json', 'objects/example.json')}}).encode()
                if tamper_structural:
                    data[parent + 'objects/example.json'] = b'{}'
            if extra:
                data['refinement-evidence/unexpected.json'] = b'{}'
            return data, identity
        catalog = root / 'tests/agent-eval/roadmap.json'
        catalog.parent.mkdir(parents=True)
        catalog.write_text(json.dumps({'evidence': [{'id': name} for name in
            ('host-recovery', 'structural', 'package-flow', 'repository-tasks', 'compiler-profile', 'model-relations')]}))
        (root / 'tools').mkdir()
        (root / 'tools/test-compiler-profile.py').write_text("REPORT = ROOT / 'previous.json'\n")
        (catalog.parent / 'current-scalar-reports.json').write_text(json.dumps({
            'schema': 'fr-current-scalar-reports', 'reports': {name: {
                'path': 'previous/' + name, 'sha256': 'previous', 'catalog_id': identifier}
                for name, identifier in (('package-flow', 'package-flow'), ('python-repositories', 'repository-tasks'),
                                          ('compiler-profile', 'compiler-profile'))}}))
        (catalog.parent / 'results').mkdir()
        (catalog.parent / 'representative-acceptance.json').write_text(json.dumps({'cases': [{
            'id': identifier, 'runner': reports.ACTIVE_REFERENCES[group],
            'runner_sha256': 'previous', 'fixture_revision': 'unchanged-fixture'}
            for group, identifier in reports.REGISTERED_REPORTS.items()]}))
        for group, name in reports.ACTIVE_REFERENCES.items():
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            previous = path.read_text() if path.exists() else ''
            suffix = '' if group in reports.DIRECTORY_REPORTS else '/result.json'
            path.write_text(previous + f'"tests/agent-eval/results/previous-{group}{suffix}"\n')
        test = root / 'tests/python_sdk.rs'
        test.write_text('\n'.join(
            '#[test]\nfn ' + function + '() {\n    let path = "tests/agent-eval/previous-' + group + '.json";\n}\n'
            for group, function in reports.ACTIVE_FUNCTIONS.items()))
        self.commands = []
        def execute(command, **_):
            self.commands.append(command)
            if format_rust and 'rustfmt' in command:
                for name in command[command.index('2021') + 1:]:
                    path = root / name
                    path.write_text(path.read_text() + '\n')
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
                suffix = '' if group in reports.DIRECTORY_REPORTS else '/result.json'
                self.assertIn(f'"tests/agent-eval/results/test-reports-{group}{suffix}"',
                    (root / name).read_text())
            self.assertEqual((root / 'tests/agent-eval/results/test-reports-structural-change/objects/example.json').read_bytes(), original)
            registry = json.loads((root / 'tests/agent-eval/representative-acceptance.json').read_text())
            for case in registry['cases']:
                self.assertEqual(case['runner_sha256'], hashlib.sha256((root / case['runner']).read_bytes()).hexdigest())
                self.assertEqual(case['fixture_revision'], 'unchanged-fixture')
            for group in reports.ACTIVE_FUNCTIONS:
                self.assertIn(f'"tests/agent-eval/results/test-reports-{group}/result.json"',
                              (root / 'tests/python_sdk.rs').read_text())

    def test_updated_rust_references_are_formatted_before_recording_runner_fingerprints(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.retain(root, only=['completion-workflows', 'structural-change'], format_rust=True)
            formatting, = [command for command in self.commands if 'rustfmt' in command]
            self.assertEqual(formatting, ['rustup', 'run', '1.99.1', 'rustfmt', '--edition', '2021',
                'tests/completion_workflows.rs', 'tests/structural_change.rs'])
            registry = json.loads((root / 'tests/agent-eval/representative-acceptance.json').read_text())
            case, = [row for row in registry['cases'] if row['runner'] == 'tests/structural_change.rs']
            formatted = (root / case['runner']).read_bytes()
            self.assertTrue(formatted.endswith(b'\n\n'))
            self.assertEqual(case['runner_sha256'], hashlib.sha256(formatted).hexdigest())
            self.assertLess(self.commands.index(formatting),
                self.commands.index(['python3', '-B', 'tools/representative-acceptance.py', 'audit']))

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
                             {'evidence': [{'id': name} for name in
                                 ('host-recovery', 'structural', 'package-flow', 'repository-tasks', 'compiler-profile', 'model-relations')]})
            manifest = json.loads((saved / 'test-reports-ci-collection/manifest.json').read_text())
            self.assertEqual(set(manifest['groups']), {'retained-proofs'})
            for group, name in reports.ACTIVE_REFERENCES.items():
                if group != 'retained-proofs':
                    suffix = '' if group in reports.DIRECTORY_REPORTS else '/result.json'
                    self.assertIn(f'"tests/agent-eval/results/previous-{group}{suffix}"',
                        (root / name).read_text())
            for group in reports.ACTIVE_FUNCTIONS:
                self.assertIn(f'"tests/agent-eval/previous-{group}.json"',
                              (root / 'tests/python_sdk.rs').read_text())

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

    def test_changed_directory_members_are_rejected_before_saving(self):
        for group in sorted(reports.DIRECTORY_REPORTS):
            with self.subTest(group=group), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                with self.assertRaisesRegex(ValueError, 'directory artifact changed'):
                    self.retain(root, only=[group], tamper_structural=True)
                self.assertEqual(list((root / 'tests/agent-eval/results').iterdir()), [])

    def test_directory_reports_keep_all_files_and_use_directory_audits(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            original = self.retain(root, only=['checked-outcomes', 'model-comparisons'])
            for group in ('checked-outcomes', 'model-comparisons'):
                saved = root / 'tests/agent-eval/results' / ('test-reports-' + group)
                self.assertEqual((saved / 'objects/example.json').read_bytes(), original)
                self.assertIn(['python3', '-B', 'tools/' + reports.SCRIPTS[group] + '.py',
                               '--audit', str(saved)], self.commands)
            entry, = [row for row in json.loads((root / 'tests/agent-eval/roadmap.json').read_text())['evidence']
                      if row['id'] == 'model-relations']
            manifest = root / entry['path']
            self.assertEqual(entry['sha256'], hashlib.sha256(manifest.read_bytes()).hexdigest())

    def test_model_refresh_updates_its_registered_runner_without_changing_the_fixture(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.retain(root, only=['model-comparisons'])
            registry = json.loads((root / 'tests/agent-eval/representative-acceptance.json').read_text())
            for case in registry['cases']:
                expected = (hashlib.sha256((root / case['runner']).read_bytes()).hexdigest()
                            if case['id'] == reports.REGISTERED_REPORTS['model-comparisons'] else 'previous')
                self.assertEqual(case['runner_sha256'], expected)
                self.assertEqual(case['fixture_revision'], 'unchanged-fixture')
            self.assertIn(['python3', '-B', 'tools/representative-acceptance.py', 'audit'], self.commands)

    def test_package_and_repository_refresh_retains_delivery_files_and_correct_audit_targets(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            original = self.retain(root, only=['package-flow', 'python-repositories'])
            base = root / 'tests/agent-eval/results'
            package = base / 'test-reports-package-flow/result.json'
            repository = base / 'test-reports-python-repositories'
            self.assertEqual(package.read_bytes(), original)
            self.assertEqual((repository / 'repair.patch').read_bytes(), original)
            self.assertIn(['python3', '-B', 'tools/python-repository-acceptance.py', '--audit', str(repository)],
                          self.commands)
            self.assertIn(['python3', '-B', 'tools/package-flow-acceptance.py', '--audit', str(package)], self.commands)
            entries = {row['id']: row for row in json.loads((root / 'tests/agent-eval/roadmap.json').read_text())['evidence']}
            self.assertEqual(entries['package-flow']['path'], str(package.relative_to(root)))
            self.assertEqual(entries['repository-tasks']['path'], str((repository / 'result.json').relative_to(root)))
            settings = json.loads((root / 'tests/agent-eval/current-scalar-reports.json').read_text())['reports']
            self.assertEqual(settings['package-flow']['path'], entries['package-flow']['path'])
            self.assertEqual(settings['package-flow']['sha256'], entries['package-flow']['sha256'])
            self.assertEqual(settings['python-repositories']['path'], entries['repository-tasks']['path'])
            self.assertEqual(settings['python-repositories']['sha256'], entries['repository-tasks']['sha256'])
            self.assertEqual(entries['host-recovery'], {'id': 'host-recovery'})

    def test_repository_source_and_missing_delivery_files_fail_before_retention(self):
        for mutation in ('omit_repository_artifact', 'wrong_repository_revision'):
            with tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                with self.assertRaisesRegex(ValueError, 'repository source or artifact inventory'):
                    self.retain(root, only=['python-repositories'], **{mutation: True})
                self.assertEqual(list((root / 'tests/agent-eval/results').iterdir()), [])

    def test_compiler_refresh_keeps_its_source_receipt_and_explicit_selection(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.retain(root, only=['compiler-profile'])
            family = root / 'tests/agent-eval/results/test-reports-scalar-collection/manifest.json'
            receipt = json.loads(family.read_text())['groups']['compiler-profile']
            self.assertEqual(receipt['collection']['source_commit'], 'a' * 40)
            settings = json.loads((root / 'tests/agent-eval/current-scalar-reports.json').read_text())
            entry = settings['reports']['compiler-profile']
            self.assertEqual(entry['sha256'], receipt['files']['result.json'])
            self.assertIn(entry['path'], (root / 'tools/test-compiler-profile.py').read_text())


if __name__ == '__main__':
    unittest.main()
