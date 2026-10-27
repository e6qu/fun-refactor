"""Reject unexplained repository facts while preserving the original baseline."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock

spec = importlib.util.spec_from_file_location('consumers', Path(__file__).with_name('index-consumers-acceptance.py'))
consumers = importlib.util.module_from_spec(spec)
spec.loader.exec_module(consumers)


class ConsumerEvidence(unittest.TestCase):
    def packet(self, root):
        sites = [{'file': 'Proof.lean', 'span': {'start': 10, 'end': 11}},
                 {'file': 'Proof.lean', 'span': {'start': 30, 'end': 31}}]
        baseline = {'samples': [{'workload': workload, 'symbols': 4, 'references': 10, 'queries': 2,
                                 'answer_bytes': 12} for workload in ('128', '512', 'repository')]}
        (root / 'baseline.json').write_text(json.dumps(baseline))
        (root / 'task.json').write_text(json.dumps({'baseline': 'a' * 40, 'reader_review': 'review.json'}))
        inventory = {'lean_facts_sha256': 'e' * 64,
            'changes': {'added_symbols': [{}], 'removed_symbols': [], 'added_references': [], 'removed_references': [{}]},
            'corpora': {'a' * 40: {'original_counts': {'symbols': 4, 'references': 10},
                'before': {'symbols': 4, 'references': 8}, 'after': {'symbols': 5, 'references': 7}}}}
        (root / 'review.json').write_text(json.dumps(inventory))
        selection = {'repository_revision': 'a' * 40, 'queries': [{'name': str(i)} for i in range(512)]}
        (root / 'repository-queries.json').write_text(json.dumps(selection))
        baseline['samples'][-1]['queries'] = 512
        (root / 'baseline.json').write_text(json.dumps(baseline))
        samples = copy.deepcopy(baseline['samples'])
        for row in samples:
            row.update(removed_wildcard_references=sites if row['workload'] == 'repository' else [],
                       lean_reader_review=None, query_selection_sha256=None, oracle_agrees=True, query_seconds=[0.1] * 3, peak_rss_bytes=1, process_seconds=1)
        samples[-1].update(symbols=5, references=7, lean_reader_review={
            'facts_sha256': 'e' * 64, 'inventory_sha256': hashlib.sha256((root / 'review.json').read_bytes()).hexdigest()},
            query_selection_sha256=hashlib.sha256((root / 'repository-queries.json').read_bytes()).hexdigest())
        value = {'schema': 'fr-index-consumers-result-1', 'repository_revision': 'a' * 40,
                 'bindings': {'source': 'exact'}, 'samples': samples,
                 'baseline_sha256': hashlib.sha256((root / 'baseline.json').read_bytes()).hexdigest()}
        return value, sites

    def check(self, value, sites, root):
        path = root / 'result.json'
        path.write_text(json.dumps(value))
        with mock.patch.object(consumers, 'TASK', root / 'task.json'), \
             mock.patch.object(consumers, 'native_bindings', return_value={'source': 'exact'}), \
             mock.patch.object(consumers, 'wildcard_sites', return_value=sites), \
             mock.patch('lean_reference_corrections.wildcard_sites', return_value=sites):
            consumers.verify(path)

    def test_exact_removed_wildcards_preserve_the_original_baseline(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            value, sites = self.packet(root)
            original = (root / 'baseline.json').read_bytes()
            self.check(value, sites, root)
            self.assertEqual((root / 'baseline.json').read_bytes(), original)

    def test_wrong_counts_sites_or_current_answers_are_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            value, sites = self.packet(root)
            mutations = [lambda v: v['samples'][-1].update(references=6),
                         lambda v: v['samples'][-1].update(removed_wildcard_references=[]),
                         lambda v: v['samples'][-1]['removed_wildcard_references'][0]['span'].update(start=9),
                         lambda v: v['samples'][0].update(references=9),
                         lambda v: v['samples'][0].update(oracle_agrees=False),
                         lambda v: v['samples'][-1].update(symbols=4),
                         lambda v: v['samples'][-1]['lean_reader_review'].update(facts_sha256='d' * 64),
                         lambda v: v['samples'][-1].update(query_selection_sha256='f' * 64),
                         lambda v: v.update(bindings={})]
            for mutate in mutations:
                changed = copy.deepcopy(value)
                mutate(changed)
                with self.assertRaises(AssertionError):
                    self.check(changed, sites, root)


if __name__ == '__main__':
    unittest.main()
