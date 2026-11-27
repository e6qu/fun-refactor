import copy
import json
from pathlib import Path
import tempfile
import unittest
import zipfile

from . import product_toolchain_memory as comparison
from . import test_product_allocator


class ToolchainMemoryTests(unittest.TestCase):
    def fixture(self, root):
        # Reuse realistic process samples, then independently construct the new schema.
        base = test_product_allocator.AllocatorTests().fixture(root)
        protocol = comparison.policy()
        result = {key: base[key] for key in ('status', 'admitted', 'live_model_calls',
                   'qualification_run', 'protected_source_sha256')}
        result.update(schema='fr-proof-toolchain-memory-1', protocol=protocol,
                      protocol_sha256=comparison.memory.tasks.common.digest(comparison.PROTOCOL),
                      platform='linux', tools={}, cases=[])
        for condition, version in protocol['versions'].items():
            result['tools'][condition] = {'version_output': f'Lean (version {version}, Release)',
                'lake_path': f'/{condition}/bin/lake', 'sha256': {k: 'a' * 64 for k in ('lean', 'lake', 'leanc')},
                'archive_sha256': protocol['archives']['linux'][condition]}
        for row in base['cases']:
            old = comparison.controls.identity(row['condition'], row['case'], row['repetition'])
            if row['condition'] == 'immediate-purge':
                row['condition'] = 'candidate'
                (root / old).rename(root / comparison.controls.identity(row['condition'], row['case'], row['repetition']))
            row.pop('environment')
            row['command'][0] = result['tools'][row['condition']]['lake_path']
            row['source_tree_sha256'] = {'reference': 'b', 'unfinished': 'c', 'wrong-proof': 'd'}[row['case']] * 64
            result['cases'].append(row)
        result['decision'] = comparison.decision(result['cases'])
        result['unstarted'] = comparison.unstarted(result['cases'])
        (root / 'summary.json').write_text(json.dumps(result))
        return result

    def test_clean_environment_and_exact_version(self):
        env = comparison.environment(Path('/candidate'), {
            'PATH': '/bin', 'LEAN_PATH': '/wrong', 'LEAN_NUM_THREADS': '8',
            'MIMALLOC_PURGE_DELAY': '0', 'ELAN_TOOLCHAIN': 'nightly', 'LAKE_HOME': '/wrong'})
        self.assertEqual(env, {'PATH': '/candidate/bin:/bin', 'LEAN_NUM_THREADS': '1', 'FR_LEAN_JOBS': '1'})
        comparison.verify_version('Lean (version 4.36.0, Release)', '4.36.0')
        for version in ('4.36.0-rc1', '4.36.01', '4.28.0'):
            with self.assertRaises(ValueError):
                comparison.verify_version(f'Lean (version {version}, Release)', '4.36.0')

    def test_replay_rejects_changed_inputs_and_outcomes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            original = self.fixture(root)
            self.assertEqual(comparison.audit(root)['decision']['outcome'], 'qualify-migration')
            mutations = [lambda s: s.update(status='collecting'), lambda s: s.update(admitted=True),
                lambda s: s.update(live_model_calls=1), lambda s: s['protocol']['limits'].update(rss_bytes=2**30),
                lambda s: s['tools']['candidate'].update(version_output='Lean (version 4.28.0)'),
                lambda s: s['tools']['candidate'].update(archive_sha256='b' * 64),
                lambda s: s['cases'][1]['command'].__setitem__(0, '/baseline/bin/lake'),
                lambda s: s['cases'][1].update(source_tree_sha256='different'),
                lambda s: s['cases'][-1].update(source_tree_sha256=s['cases'][-2]['source_tree_sha256']),
                lambda s: s['cases'][0].update(passed=False),
                lambda s: s['cases'][0]['resources'].update(sampled_aggregate_rss_bytes=0),
                lambda s: s['cases'].pop(), lambda s: s['cases'].reverse(),
                lambda s: s['decision'].update(outcome='discard'), lambda s: s.update(unstarted=['hidden'])]
            for mutate in mutations:
                changed = copy.deepcopy(original)
                mutate(changed)
                (root / 'summary.json').write_text(json.dumps(changed))
                with self.assertRaises(ValueError):
                    comparison.audit(root)
            (root / 'summary.json').write_text(json.dumps(original))
            (root / 'baseline-reference-0/profile.jsonl').write_text('{}\n')
            with self.assertRaises((ValueError, KeyError)):
                comparison.audit(root)

    def test_stops_and_insufficient_headroom_cannot_admit_migration(self):
        with tempfile.TemporaryDirectory() as temporary:
            rows = self.fixture(Path(temporary))['cases']
            for index in (0, 1):
                changed = copy.deepcopy(rows)
                changed[index]['passed'] = False
                expected = comparison.FIRST + (comparison.NEGATIVE if index == 0 else [])
                self.assertEqual(comparison.schedule(changed), expected)
                self.assertEqual(comparison.decision(changed)['outcome'], 'not-demonstrated')
            for row in rows:
                if row['case'] == 'reference':
                    row['resources']['sampled_aggregate_rss_bytes'] = (
                        600 if row['condition'] == 'baseline' else 500) * 1024**2
            self.assertEqual(comparison.decision(rows)['outcome'], 'discard')
            for row in rows:
                if row['condition'] == 'candidate' and row['case'] == 'reference':
                    row['resources']['sampled_aggregate_rss_bytes'] = 436 * 1024**2
            self.assertEqual(comparison.decision(rows)['outcome'], 'qualify-migration')
            self.assertFalse(comparison.decision(rows)['admitted'])

    def test_archive_and_retained_results(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            records = root / 'records'
            records.mkdir()
            expected = self.fixture(records)
            archive = root / 'result.zip'
            with zipfile.ZipFile(archive, 'w') as output:
                output.write(records / 'summary.json', 'summary.json')
                for entry in comparison.FIRST + comparison.SECOND + comparison.NEGATIVE:
                    name = comparison.controls.identity(*entry)
                    for file in comparison.FILES:
                        output.write(records / name / file, f'{name}/{file}')
            self.assertEqual(comparison.audit(archive), expected)
            with zipfile.ZipFile(archive, 'a') as output:
                output.writestr('../escape', 'bad')
            with self.assertRaises(ValueError):
                comparison.audit(archive)
        retained = comparison.PROTOCOL.parent / '2026-11-27'
        for archive in sorted(retained.glob('*.zip')):
            with self.subTest(archive=archive.name):
                self.assertFalse(comparison.audit(archive)['admitted'])


if __name__ == '__main__':
    unittest.main()
