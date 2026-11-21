"""Reject false savings, changed checks, missing failures and incomplete proof controls."""
import copy
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import zipfile

from . import product_allocator as allocator


class AllocatorTests(unittest.TestCase):
    def test_hosted_only_and_fresh_outputs(self):
        with tempfile.TemporaryDirectory() as temporary, patch.dict(allocator.os.environ, {}, clear=True):
            root = Path(temporary)
            with self.assertRaisesRegex(ValueError, 'on GitHub'):
                allocator.measure(root, root / 'run', root, root / 'output')
            self.assertFalse((root / 'output').exists())

    def test_comparison_changes_only_the_one_allocator_setting(self):
        ambient = {'PATH': '/bin', 'MIMALLOC_PURGE_DELAY': '300', 'mimalloc_verbose': '1',
                   'MIMALLOC_ALLOW_LARGE_OS_PAGES': '1', 'LEAN_NUM_THREADS': '8', 'KEEP': 'same'}
        baseline = allocator.environment('baseline', Path('/pinned'), ambient)
        candidate = allocator.environment('immediate-purge', Path('/pinned'), ambient)
        self.assertEqual(candidate, {**baseline, 'MIMALLOC_PURGE_DELAY': '0'})
        self.assertFalse(any(key.upper().startswith('MIMALLOC_') for key in baseline))
        self.assertEqual(baseline['KEEP'], 'same')
        self.assertEqual(baseline['LEAN_NUM_THREADS'], '1')
        with self.assertRaisesRegex(ValueError, 'unknown'):
            allocator.environment('unfrozen-variant', Path('/pinned'), ambient)

    def test_probe_must_confirm_the_pinned_runtime_and_setting(self):
        for condition, value in [('baseline', '10'), ('immediate-purge', '0')]:
            allocator.verify_probe(condition, 'Lean (version 4.28.0, Release)',
                                   f"mimalloc: option 'purge_delay': {value}\n")
            for stdout, stderr in [('Lean (version 4.29.0)', f"option 'purge_delay': {value}"),
                                   ('Lean (version 4.28.0)', ''),
                                   ('Lean (version 4.28.0)', "option 'purge_delay': 100")]:
                with self.subTest(condition=condition, stdout=stdout, stderr=stderr), self.assertRaises(ValueError):
                    allocator.verify_probe(condition, stdout, stderr)

    def resources(self, rss, code=0):
        limits = dict(allocator.memory.runtime.CONTROL_LIMITS)
        limits['cpu_seconds'] = limits.pop('cpu_limit_seconds')
        return {'limits': limits, 'exit_code': code, 'process_exit_code': code, 'timed_out': False,
                'stop_reason': None, 'monitor_error': None, 'launch_error': None,
                'elapsed_seconds': 2, 'sampled_aggregate_rss_bytes': rss, 'sampled_cpu_seconds': 1,
                'sampled_disk_growth_bytes': 100, 'transcript_bytes': 100}

    def test_errors_and_resource_stops_are_not_proof_rejections(self):
        resources = self.resources(100, 1)
        self.assertTrue(allocator.verdict('unfinished', resources, "warning: declaration uses 'sorry'"))
        self.assertTrue(allocator.verdict('wrong-proof', resources, 'error: type mismatch\nTrue.intro'))
        for changes in ({'stop_reason': 'rss_bytes'}, {'timed_out': True}, {'exit_code': 125},
                        {'monitor_error': 'ps failed'}, {'launch_error': 'missing Lean'},
                        {'sampled_aggregate_rss_bytes': 2**30}, {'sampled_cpu_seconds': 26},
                        {'elapsed_seconds': float('nan')}, {'process_exit_code': -9}):
            with self.subTest(changes=changes):
                self.assertFalse(allocator.verdict('unfinished', {**resources, **changes}, "error: sorry"))
        self.assertFalse(allocator.verdict('unfinished', resources, 'error: missing import'))
        self.assertFalse(allocator.verdict('wrong-proof', resources, 'error: invalid command'))

    def test_wrong_proof_preserves_the_executable_claim_and_imports(self):
        fixture = allocator.ROOT / allocator.memory.tasks.plan.DIRECTORY / 'initial/fixtures/Clamp.lean'
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            path = root / allocator.memory.tasks.PROOF
            path.parent.mkdir(parents=True)
            path.write_bytes(fixture.read_bytes())
            before = allocator.memory.tasks.masked(path.read_text())
            allocator.proof_control(root, 'wrong-proof', None, None)
            self.assertEqual(allocator.memory.tasks.masked(path.read_text()), before)
            self.assertIn('exact True.intro', path.read_text())
            self.assertNotIn('  sorry\n', path.read_text())

    def fixture(self, root):
        protocol = allocator.policy()
        summary = {'schema': 'fr-product-proof-allocator-1', 'status': 'complete', 'unstarted': [], 'protocol': protocol,
                   'protocol_sha256': allocator.memory.tasks.common.digest(allocator.PROTOCOL),
                   'admitted': False, 'live_model_calls': 0, 'qualification_run': protocol['qualification_run'],
                   'lake_path': '/pinned/bin/lake', 'protected_source_sha256': 'a' * 64, 'probes': {}, 'cases': []}
        for condition in allocator.CONDITIONS:
            value = '10' if condition == 'baseline' else '0'
            (root / f'probe-{condition}.stdout').write_text('Lean (version 4.28.0)')
            (root / f'probe-{condition}.stderr').write_text(f"mimalloc: option 'purge_delay': {value}\n")
            summary['probes'][condition] = {stream + '_sha256': allocator.memory.tasks.common.digest(root / f'probe-{condition}.{stream}')
                                             for stream in ('stdout', 'stderr')}
        for condition, case, repetition in allocator.FIRST + allocator.SECOND + allocator.NEGATIVE:
            row = root / allocator.identity(condition, case, repetition)
            row.mkdir()
            rss = (100 if condition == 'baseline' else 80) * 1024**2
            resources = self.resources(rss, 0 if case == 'reference' else 1)
            sample = {'group': 10, 'started_seconds': 0, 'finished_seconds': 1,
                      'phase_before': 'capture-startup', 'phase_after': 'capture-startup',
                      'processes': [{'pid': 10, 'rss_bytes': rss, 'cpu_seconds': 1, 'executable': 'lean'}]}
            (row / 'profile.jsonl').write_text(json.dumps(sample) + '\n')
            profile = allocator.memory.profile.audit(row, resources)
            (row / 'resources.json').write_text(json.dumps(resources))
            (row / 'profile.json').write_text(json.dumps(profile))
            (row / 'stdout').write_text('')
            output = {'reference': '', 'unfinished': "warning: declaration uses 'sorry'",
                      'wrong-proof': 'error: type mismatch\nTrue.intro'}[case]
            (row / 'stderr').write_text(output)
            summary['cases'].append({'condition': condition, 'case': case, 'repetition': repetition,
                'environment': allocator.CONDITIONS[condition], 'resources': resources, 'profile': profile,
                'command': ['/pinned/bin/lake', '-d', 'specs', 'build', '--wfail'],
                'source_tree_sha256': ('b' if case == 'reference' else 'c') * 64,
                'protected_source_sha256': 'a' * 64, 'passed': True,
                **{stream + '_sha256': allocator.memory.tasks.common.digest(row / stream) for stream in ('stdout', 'stderr')}})
        summary['decision'] = allocator.decision(summary['cases'])
        (root / 'summary.json').write_text(json.dumps(summary))
        return summary

    def test_retained_comparison_rejects_changed_conditions_costs_and_claims(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            original = self.fixture(root)
            self.assertEqual(allocator.audit(root)['decision']['outcome'], 'candidate')
            mutations = [lambda s: s.update(status='collecting'), lambda s: s.update(unstarted=['hidden']), lambda s: s.update(admitted=True), lambda s: s.update(live_model_calls=1),
                lambda s: s['protocol']['limits'].update(rss_bytes=2**30),
                lambda s: s['cases'][0].update(command=['lean', 'Import.lean']),
                lambda s: s['cases'][1].update(environment={}),
                lambda s: s['cases'][1].update(source_tree_sha256='changed'),
                lambda s: s['cases'][0].update(passed=False),
                lambda s: s['cases'][0]['resources'].update(sampled_aggregate_rss_bytes=0),
                lambda s: s['cases'].pop(), lambda s: s['cases'].reverse(),
                lambda s: s['decision'].update(admitted=True),
                lambda s: s['cases'][-1].update(protected_source_sha256='changed')]
            for mutate in mutations:
                changed = copy.deepcopy(original)
                mutate(changed)
                (root / 'summary.json').write_text(json.dumps(changed))
                with self.assertRaises(ValueError):
                    allocator.audit(root)
            (root / 'summary.json').write_text(json.dumps(original))
            (root / 'baseline-reference-0/profile.jsonl').write_text('{}\n')
            with self.assertRaises((ValueError, KeyError)):
                allocator.audit(root)

    def test_stopped_first_pair_cannot_be_retried_or_support_a_saving_claim(self):
        with tempfile.TemporaryDirectory() as temporary:
            rows = self.fixture(Path(temporary))['cases']
            for failed in (0, 1):
                changed = copy.deepcopy(rows)
                changed[failed]['passed'] = False
                expected = allocator.FIRST + (allocator.NEGATIVE if failed == 0 else [])
                self.assertEqual(allocator.expected_schedule(changed), expected)
                self.assertEqual(allocator.decision(changed)['outcome'], 'not-demonstrated')
            rows[1]['resources']['sampled_aggregate_rss_bytes'] = 95 * 1024**2
            self.assertEqual(allocator.decision(rows)['outcome'], 'discard')
            rows[1]['resources']['sampled_aggregate_rss_bytes'] = 90 * 1024**2
            self.assertEqual(allocator.decision(rows)['outcome'], 'candidate')

    def test_archive_refuses_path_escape(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = root / 'archive.zip'
            with zipfile.ZipFile(archive, 'w') as output:
                output.writestr('../escape', 'bad')
            with self.assertRaisesRegex(ValueError, 'unexpected or oversized'):
                allocator.audit(archive)

    def test_original_upload_prefix_preserves_records_and_rejects_mixed_paths(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            records = root / 'records'
            records.mkdir()
            expected = self.fixture(records)
            for prefix in ('', 'proof-allocator/'):
                archive = root / 'records.zip'
                with zipfile.ZipFile(archive, 'w') as output:
                    for path in records.rglob('*'):
                        if path.is_file():
                            output.write(path, prefix + path.relative_to(records).as_posix())
                self.assertEqual(allocator.audit(archive), expected)
            with zipfile.ZipFile(archive, 'a') as output:
                output.writestr('summary.json', '{}')
            with self.assertRaisesRegex(ValueError, 'unexpected or oversized'):
                allocator.audit(archive)

    def test_retained_hosted_results_match_original_artifacts_and_run(self):
        data = allocator.PROTOCOL.parent / '2026-11-21'
        for name in ('linux', 'macos'):
            with self.subTest(platform=name):
                archive = data / (name + '.zip')
                provenance = json.loads((data / (name + '-provenance.json')).read_text())
                self.assertEqual(allocator.memory.tasks.common.digest(archive), provenance['artifact_sha256'])
                original = allocator.audit(archive)
                self.assertEqual(original, json.loads((data / (name + '.json')).read_text()))
                self.assertEqual(original['run'], provenance['run'])
                self.assertEqual(original['source_commit'], provenance['head_sha'])
                self.assertEqual(provenance['workflow'], '.github/workflows/product-memory.yml')
                self.assertEqual(provenance['workflow_conclusion'], 'success')
                self.assertFalse(original['admitted'])
                self.assertEqual(original['live_model_calls'], 0)


if __name__ == '__main__':
    unittest.main()
