import copy
import json
from pathlib import Path
import tempfile
import unittest

from . import product_gruszka as candidate
from . import test_product_toolchain_memory


class GruszkaEvaluationTests(unittest.TestCase):
    def fixture(self, root):
        result = test_product_toolchain_memory.ToolchainMemoryTests().fixture(root)
        result.update(schema='fr-gruszka-evaluation-result-1', protocol=candidate.policy(),
                      protocol_sha256=candidate.memory.tasks.common.digest(candidate.PROTOCOL))
        for row in result['cases']:
            if row['condition'] == 'candidate':
                old = candidate.controls.identity(row['condition'], row['case'], row['repetition'])
                row['condition'] = 'gruszka'
                path = root / candidate.controls.identity(row['condition'], row['case'], row['repetition'])
                (root / old).rename(path)
                if row['case'] == 'reference':
                    with (path / 'stdout').open('a') as stream:
                        stream.write('\n' + '\n'.join('FR_GRUSZKA_DONE ' + stage for stage in ('package', 'export', 'kernel', 'axioms')))
                    row['stdout_sha256'] = candidate.memory.tasks.common.digest(path / 'stdout')
        result['decision'] = candidate.decision(result['cases'])
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


if __name__ == '__main__':
    unittest.main()
