"""Keep the hosted memory diagnostic separate from admission and live work."""
from pathlib import Path
import json
import tempfile
import unittest
from unittest.mock import patch
import zipfile

from . import product_memory as memory


class ProofMemoryTests(unittest.TestCase):
    def test_refuses_local_measurement_before_creating_files(self):
        with tempfile.TemporaryDirectory() as temporary, patch.dict(memory.os.environ, {}, clear=True):
            root = Path(temporary)
            with self.assertRaisesRegex(ValueError, 'on GitHub'):
                memory.measure(root, root / 'run', root, root / 'output')
            self.assertFalse((root / 'output').exists())

    def test_direct_proof_and_package_keep_the_original_source_and_check(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            proof = root / memory.tasks.PROOF
            proof.parent.mkdir(parents=True)
            proof.write_text('protected source')
            self.assertEqual(memory.command('package', Path('/lean'), root),
                             ['/lean/bin/lake', '-d', 'specs', 'build', '--wfail'])
            self.assertEqual(memory.command('direct-proof', Path('/lean'), root),
                             ['/lean/bin/lean', '-j1', '-DwarningAsError=true', memory.tasks.PROOF])
            for case, module in [('std-import', 'Std'), ('init-import', 'Init')]:
                memory.command(case, Path('/lean'), root)
                self.assertEqual((root / 'Import.lean').read_text(),
                                 f'import {module}\nexample : True := by trivial\n')
            self.assertEqual(proof.read_text(), 'protected source')
            with self.assertRaisesRegex(ValueError, 'unknown'):
                memory.command('narrowed-proof', Path('/lean'), root)

    def test_resource_and_monitor_stops_cannot_count_as_completed(self):
        good = {'exit_code': 0, 'stop_reason': None, 'monitor_error': None, 'launch_error': None}
        self.assertTrue(memory.completed(good))
        for key, value in [('exit_code', 125), ('stop_reason', 'rss_bytes'),
                           ('monitor_error', 'missing sample'), ('launch_error', 'missing Lean')]:
            self.assertFalse(memory.completed({**good, key: value}))

    def test_refuses_unsuccessful_or_unrelated_qualification(self):
        good = {'status': 'completed', 'conclusion': 'success',
                'path': '.github/workflows/product-validation.yml',
                'head_repository': {'full_name': 'e6qu/fun-refactor'}}
        for key, value in [('status', 'in_progress'), ('conclusion', 'failure'),
                           ('path', 'another-workflow.yml'),
                           ('head_repository', {'full_name': 'another/repository'})]:
            with self.subTest(key=key), self.assertRaisesRegex(ValueError, 'successful product qualification'):
                memory.qualified_inputs(Path('/missing'), {**good, key: value})

    def test_reference_replay_refuses_changed_proof_and_archive(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / 'source'
            proof = source / memory.tasks.PROOF
            proof.parent.mkdir(parents=True)
            fixture = memory.ROOT / memory.tasks.plan.DIRECTORY / 'initial/fixtures/Clamp.lean'
            proof.write_bytes(fixture.read_bytes())
            prepared = {'task': 'clamp-proof', 'original': memory.tasks.common.sources(source),
                        'protected_source': memory.tasks.masked(proof.read_text())}
            inputs = root / 'inputs'
            inputs.mkdir()
            seed = memory.tasks.common.retain_seed(source, prepared, inputs)
            git = memory.tasks.common.comparison_grade.BASE.git
            git(source, 'init', '-q')
            git(source, 'add', '.')
            memory.tasks.reference(source, prepared)
            reference = git(source, 'diff', '--binary').stdout
            (inputs / 'reference.patch').write_bytes(reference)
            first = root / 'first'
            memory.workspace(inputs, seed, prepared, first / 'project')
            self.assertEqual((first / 'project' / memory.tasks.PROOF).read_bytes(), proof.read_bytes())
            changed = reference.replace(b'by omega', b'by trivial')
            self.assertNotEqual(changed, reference)
            (inputs / 'reference.patch').write_bytes(changed)
            with self.assertRaisesRegex(ValueError, 'reviewed qualification answer'):
                memory.workspace(inputs, seed, prepared, root / 'changed' / 'project')
            (inputs / 'original.tar.gz').write_bytes(b'changed archive')
            with self.assertRaisesRegex(ValueError, 'archive differs'):
                memory.workspace(inputs, seed, prepared, root / 'archive' / 'project')

    def test_audit_refuses_admission_claims_and_omitted_cases(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            summary = {'schema': 'fr-product-proof-memory-1', 'admitted': False,
                       'live_model_calls': 0, 'limits': memory.runtime.CONTROL_LIMITS, 'cases': []}
            for key, value in [('admitted', True), ('live_model_calls', 1),
                               ('limits', {**memory.runtime.CONTROL_LIMITS, 'rss_bytes': 2**30})]:
                (root / 'summary.json').write_text(json.dumps({**summary, key: value}))
                with self.subTest(key=key), self.assertRaisesRegex(ValueError, 'cannot change limits or grant admission'):
                    memory.audit(root)
            (root / 'summary.json').write_text(json.dumps(summary))
            with self.assertRaisesRegex(ValueError, 'retain every diagnostic case'):
                memory.audit(root)

    def test_retained_artifact_replays_and_changed_measurements_refuse(self):
        data = memory.ROOT / memory.tasks.plan.DIRECTORY / 'memory/2026-11-19'
        for name in ('linux', 'linux-final', 'macos-first', 'macos-final'):
            with self.subTest(artifact=name):
                archive = data / (name + '.zip')
                provenance = json.loads((data / (name + '-provenance.json')).read_text())
                self.assertEqual(memory.tasks.common.digest(archive), provenance['artifact_sha256'])
                original = memory.audit(archive)
                self.assertFalse(original['admitted'])
                self.assertEqual(len(original['cases']), 8)
                self.assertEqual(original, json.loads((data / (name + '.json')).read_text()))
        with tempfile.TemporaryDirectory() as temporary, zipfile.ZipFile(archive) as source:
            root = Path(temporary)
            source.extractall(root)
            path = root / 'package-0/resources.json'
            resources = json.loads(path.read_text())
            resources['sampled_aggregate_rss_bytes'] = 0
            path.write_text(json.dumps(resources))
            with self.assertRaisesRegex(ValueError, 'resource record or verdict changed'):
                memory.audit(root)

    def test_archive_refuses_unexpected_paths(self):
        with tempfile.TemporaryDirectory() as temporary:
            archive = Path(temporary) / 'changed.zip'
            with zipfile.ZipFile(archive, 'w') as output:
                output.writestr('../escaped', 'changed')
            with self.assertRaisesRegex(ValueError, 'unexpected or oversized'):
                memory.audit(archive)


if __name__ == '__main__':
    unittest.main()
