"""Keep exact small answers and repeatable full answers after the Lean reader fix."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock

spec = importlib.util.spec_from_file_location('resolution', Path(__file__).with_name('index-resolution-acceptance.py'))
resolution = importlib.util.module_from_spec(spec)
spec.loader.exec_module(resolution)


class ResolutionEvidence(unittest.TestCase):
    def packet(self, root):
        sites = [{'file': 'Proof.lean', 'span': {'start': 10, 'end': 11}}]
        workloads = ['members-128', 'members-256', 'assignments-128', 'repository']
        (root / 'task.json').write_text(json.dumps({'baseline': 'a' * 40, 'reader_review': 'review.json', 'budgets': {
            'workloads': workloads, 'repetitions': 2, 'measurement_timeout_seconds': 60}}))
        samples = [{'workload': workload, 'symbols': 4, 'references': 10, 'answer_bytes': 12,
                    'answer_sha256': 'b' * 64, 'build_seconds': 0.1, 'process_seconds': 1,
                    'peak_rss_bytes': 1} for workload in workloads for _ in range(2)]
        baseline = {'schema': 'fr-index-resolution-result-1', 'repository_revision': 'a' * 40,
                    'samples': samples}
        (root / 'baseline.json').write_text(json.dumps(baseline))
        inventory = {'lean_facts_sha256': 'e' * 64,
            'changes': {'added_symbols': [{}], 'removed_symbols': [], 'added_references': [], 'removed_references': [{}]},
            'corpora': {'a' * 40: {'original_counts': {'symbols': 4, 'references': 10},
                'before': {'symbols': 4, 'references': 9}, 'after': {'symbols': 5, 'references': 8}}}}
        (root / 'review.json').write_text(json.dumps(inventory))
        value = copy.deepcopy(baseline)
        value.update(bindings={'source': 'exact'}, comparison=resolution.COMPARISON,
                     baseline_sha256=hashlib.sha256((root / 'baseline.json').read_bytes()).hexdigest())
        for row in value['samples']:
            row['lean_reader_review'] = None
            row['removed_wildcard_references'] = sites if row['workload'] == 'repository' else []
            if row['workload'] == 'repository':
                row.update(symbols=5, references=8, answer_sha256='c' * 64, lean_reader_review={
                    'facts_sha256': 'e' * 64, 'inventory_sha256': hashlib.sha256((root / 'review.json').read_bytes()).hexdigest()})
        return value, sites

    def check(self, value, sites, root):
        path = root / 'result.json'
        path.write_text(json.dumps(value))
        with mock.patch.object(resolution, 'TASK', root / 'task.json'), \
             mock.patch.object(resolution, 'BASELINE', root / 'baseline.json'), \
             mock.patch.object(resolution, 'bindings', return_value={'source': 'exact'}), \
             mock.patch.object(resolution, 'wildcard_sites', return_value=sites), \
             mock.patch('lean_reference_corrections.wildcard_sites', return_value=sites):
            resolution.verify(path)

    def test_exact_correction_keeps_the_original_baseline(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            value, sites = self.packet(root)
            original = (root / 'baseline.json').read_bytes()
            self.check(value, sites, root)
            self.assertEqual((root / 'baseline.json').read_bytes(), original)

    def test_unexplained_counts_changed_small_answers_and_unstable_repository_answers_fail(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            value, sites = self.packet(root)
            mutations = [lambda v: v['samples'][-1].update(references=7),
                         lambda v: v['samples'][-1].update(removed_wildcard_references=[]),
                         lambda v: v['samples'][0].update(answer_sha256='d' * 64),
                         lambda v: v['samples'][-1].update(answer_sha256='d' * 64),
                         lambda v: v['samples'][-1].update(symbols=4),
                         lambda v: v['samples'][-1]['lean_reader_review'].update(facts_sha256='d' * 64),
                         lambda v: v.update(bindings={})]
            for mutate in mutations:
                changed = copy.deepcopy(value)
                mutate(changed)
                with self.assertRaises(AssertionError):
                    self.check(changed, sites, root)


if __name__ == '__main__':
    unittest.main()
