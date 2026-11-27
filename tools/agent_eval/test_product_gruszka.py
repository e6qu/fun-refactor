import copy
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch
import zipfile

from . import product_gruszka as candidate
from . import test_product_toolchain_memory


class GruszkaEvaluationTests(unittest.TestCase):
    def fixture(self, root, experiment='original'):
        result = test_product_toolchain_memory.ToolchainMemoryTests().fixture(root)
        result.update(schema='fr-gruszka-evaluation-result-1', protocol=candidate.policy(experiment),
                      protocol_sha256=candidate.memory.tasks.common.digest(candidate.PROTOCOLS[experiment]),
                      tools={'sources': candidate.policy()['sources'],
                             'binaries': {k: 'a' * 64 for k in candidate.policy()['sources']},
                             'lockfiles': {k: 'b' * 64 for k in candidate.policy()['sources']}})
        for row in result['cases']:
            if row['condition'] == 'candidate':
                old = candidate.controls.identity(row['condition'], row['case'], row['repetition'])
                row['condition'] = 'gruszka'
                row['command'] = ['/python', '-B', '-m', 'agent_eval.product_gruszka', 'pipeline',
                                  '--project', '/work', '--lean', '/lean', '--binaries', '/tools']
                path = root / candidate.controls.identity(row['condition'], row['case'], row['repetition'])
                (root / old).rename(path)
                if row['case'] == 'reference':
                    with (path / 'stdout').open('a') as stream:
                        stream.write('\n' + '\n'.join('FR_GRUSZKA_DONE ' + stage for stage in ('package', 'export', 'kernel', 'axioms')))
                    row['stdout_sha256'] = candidate.memory.tasks.common.digest(path / 'stdout')
        if experiment != 'original':
            result['experiment'] = experiment
            for row in result['cases']:
                row['resources']['limits']['rss_bytes'] = result['protocol']['limits']['rss_bytes']
                path = root / candidate.controls.identity(row['condition'], row['case'], row['repetition'])
                (path / 'resources.json').write_text(json.dumps(row['resources']))
        result['decision'] = candidate.decision(result['cases'], experiment)
        result['unstarted'] = []
        (root / 'summary.json').write_text(json.dumps(result))
        return result

    def test_full_pipeline_and_isolated_environment(self):
        cmds = candidate.pipeline_commands(Path('/work'), Path('/lean'), Path('/tools'))
        self.assertEqual(cmds[0][1], ['/lean/bin/lake', '-d', 'specs', 'build', '--wfail'])
        self.assertEqual(cmds[2][1], ['/tools/gruszka', '/work/proof.susz', '1'])
        self.assertNotIn('--only', str(cmds))
        env = candidate.environment(Path('/lean'), Path('/work'), {
            'PATH': '/bin', 'SK_POISON': '1', 'LEAN_PATH': '/wrong', 'MIMALLOC_PURGE_DELAY': '0',
            'LD_LIBRARY_PATH': '/wrong', 'LD_PRELOAD': '/wrong', 'ELAN_TOOLCHAIN': 'nightly'})
        self.assertEqual(env['LEAN_NUM_THREADS'], '1')
        self.assertEqual(env['LEAN_SYSROOT'], '/lean')
        self.assertEqual(env['LEAN_PATH'], '/work/specs/.lake/build/lib/lean')
        for key in ('SK_POISON', 'LD_LIBRARY_PATH', 'LD_PRELOAD', 'ELAN_TOOLCHAIN', 'MIMALLOC_PURGE_DELAY'):
            self.assertNotIn(key, env)

    def test_axiom_policy_and_complete_acceptance(self):
        candidate.check_axioms('Classical.choice\nQuot.sound\npropext\n')
        for output in ('sorryAx\n', 'Product.falseAxiom\n', 'used sorryAx\n'):
            with self.assertRaises(ValueError):
                candidate.check_axioms(output)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            rows = self.fixture(root)['cases']
            row = rows[1]
            self.assertFalse(candidate.verdict(row, 'accepted\n'))
            self.assertFalse(candidate.verdict(row, 'FR_GRUSZKA_DONE package\n'))
            row['resources']['stop_reason'] = 'rss_bytes'
            self.assertEqual(candidate.decision(rows)['outcome'], 'not-demonstrated')

    def test_retained_record_mutations_fail(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            original = self.fixture(root)
            self.assertFalse(candidate.audit(root)['decision']['migration_qualified'])
            mutations = [lambda s: s.update(admitted=True), lambda s: s.update(live_model_calls=1),
                         lambda s: s['protocol']['limits'].update(rss_bytes=2**30),
                         lambda s: s['cases'].pop(), lambda s: s['cases'].reverse(),
                         lambda s: s['cases'][1].update(source_tree_sha256='changed'),
                         lambda s: s['cases'][1].update(passed=False),
                         lambda s: s['cases'][1]['command'].append('--only'),
                         lambda s: s['tools']['sources']['gruszka'].update(commit='changed'),
                         lambda s: s['decision'].update(migration_qualified=True),
                         lambda s: s['cases'][1].update(stdout_sha256='changed')]
            for mutate in mutations:
                changed = copy.deepcopy(original)
                mutate(changed)
                (root / 'summary.json').write_text(json.dumps(changed))
                with self.assertRaises(ValueError):
                    candidate.audit(root)

    def test_no_retry_after_stopped_candidate(self):
        rows = [{'condition': 'baseline', 'passed': True}, {'condition': 'gruszka', 'passed': False}]
        self.assertEqual(candidate.schedule(rows), candidate.FIRST)
        self.assertFalse(candidate.decision(rows)['migration_qualified'])

    def test_approved_limit_preserves_historical_audits(self):
        self.assertEqual(candidate.policy()['limits']['rss_bytes'], 768 * 1024**2)
        self.assertEqual(candidate.policy('1g')['limits']['rss_bytes'], 1024**3)
        with self.assertRaises(ValueError):
            candidate.policy('2g')
        for experiment in ('original', '1g'):
            with tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                result = self.fixture(root, experiment)
                self.assertEqual(candidate.audit(root), result)
                result['experiment'] = '1g' if experiment == 'original' else 'original'
                (root / 'summary.json').write_text(json.dumps(result))
                with self.assertRaises(ValueError):
                    candidate.audit(root)

    def test_completion_is_separate_from_reduction_and_still_bounded(self):
        with tempfile.TemporaryDirectory() as temporary:
            rows = self.fixture(Path(temporary), '1g')['cases']
            for row in rows:
                row['resources']['sampled_aggregate_rss_bytes'] = 800 * 1024**2
            result = candidate.decision(rows, '1g')
            self.assertEqual(result['outcome'], 'completed-within-limits')
            self.assertTrue(result['completed_within_limits'])
            self.assertEqual(result['original_reduction_outcome'], 'no-usable-reduction')
            self.assertFalse(result['migration_qualified'])
            for limit in (768 * 1024**2, 1024**3):
                changed = copy.deepcopy(rows)
                changed[1]['resources']['limits']['rss_bytes'] = limit
                changed[1]['resources']['sampled_aggregate_rss_bytes'] = limit + 1
                self.assertFalse(candidate.decision(changed, '1g')['completed_within_limits'])
            self.assertFalse(candidate.decision(rows[:-1], '1g')['completed_within_limits'])

    def test_kernel_declines_subset_verdicts_and_bad_axioms_stop_pipeline(self):
        for kernel_code, kernel_output, axiom_output in (
                (0, 'checked: Product.clamp_bounds (subset)\n', ''),
                (0, 'accepted\n', 'sorryAx\n'),
                (0, '', '')):
            calls = [SimpleNamespace(returncode=0), SimpleNamespace(returncode=0),
                     SimpleNamespace(returncode=kernel_code, stdout=kernel_output),
                     SimpleNamespace(returncode=0, stdout=axiom_output)]
            with patch.object(candidate.subprocess, 'run', side_effect=calls), patch('builtins.print'):
                with self.assertRaises(ValueError):
                    candidate.pipeline(Path('/work'), Path('/lean'), Path('/tools'))
        for code in (1, 2):
            calls = [SimpleNamespace(returncode=0), SimpleNamespace(returncode=0),
                     SimpleNamespace(returncode=code, stdout='declined: unsupported\n')]
            with patch.object(candidate.subprocess, 'run', side_effect=calls) as run, patch('builtins.print'):
                self.assertEqual(candidate.pipeline(Path('/work'), Path('/lean'), Path('/tools')), code)
                self.assertEqual(run.call_count, 3)

    def test_archive_boundaries_and_retained_measurements(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            records = root / 'records'
            records.mkdir()
            result = self.fixture(records)
            archive = root / 'records.zip'
            with zipfile.ZipFile(archive, 'w') as output:
                output.write(records / 'summary.json', 'gruszka-evaluation/summary.json')
                output.writestr('gruszka-tools/bin/provenance.json', json.dumps(result['tools']))
                for entry in candidate.FIRST + candidate.SECOND + candidate.NEGATIVE:
                    name = candidate.controls.identity(*entry)
                    for file in candidate.FILES:
                        output.write(records / name / file, f'gruszka-evaluation/{name}/{file}')
            self.assertEqual(candidate.audit(archive), result)
            with zipfile.ZipFile(archive, 'a') as output:
                output.writestr('../escape', 'bad')
            with self.assertRaises(ValueError):
                candidate.audit(archive)
        for directory, experiment, platforms in (
                ('2026-11-27-gruszka', 'original', {'linux', 'darwin_aarch64'}),
                ('2026-11-27-gruszka-1g', '1g', {'linux'})):
            archives = sorted((candidate.PROTOCOL.parent / directory).glob('*.zip'))
            self.assertEqual({p.stem for p in archives}, platforms)
            for archive in archives:
                with self.subTest(directory=directory, archive=archive.name):
                    provenance = json.loads(archive.with_suffix('.json').read_text())
                    self.assertEqual(candidate.memory.tasks.common.digest(archive), provenance['artifact_sha256'])
                    result = candidate.audit(archive)
                    self.assertEqual(result.get('experiment', 'original'), experiment)
                    self.assertEqual(result['source_commit'], provenance['head_sha'])
                    self.assertEqual(result['run'], provenance['run'])
                    self.assertEqual(result['platform'], provenance['platform'])
                    self.assertFalse(result['decision']['migration_qualified'])


if __name__ == '__main__':
    unittest.main()
