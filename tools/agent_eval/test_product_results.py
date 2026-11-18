"""Protect complete resource accounting and keep raw client data out of publication."""
import copy
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from . import product_plan as plan, product_results as results, selected_local as local
from .selected_plan import ROOT, encoded


class ProductResultsTests(unittest.TestCase):
    def test_missing_or_exceeded_whole_attempt_resources_cannot_pass(self):
        original = {'exit_code': 0, 'stop_reason': None, 'monitor_error': None, 'launch_error': None,
            'elapsed_seconds': 100, 'sampled_aggregate_rss_bytes': 100000000,
            'sampled_cpu_seconds': 20, 'sampled_disk_growth_bytes': 10000, 'transcript_bytes': 1000}
        self.assertTrue(results.resource_pass(original))
        for field in original:
            changed = dict(original)
            del changed[field]
            with self.subTest(missing=field):
                self.assertFalse(results.resource_pass(changed))
        for field, value in (('elapsed_seconds', 151), ('sampled_aggregate_rss_bytes', 805306369),
                             ('sampled_cpu_seconds', 76), ('transcript_bytes', 1048577),
                             ('exit_code', False), ('elapsed_seconds', True), ('elapsed_seconds', float('nan'))):
            with self.subTest(field=field, value=value):
                self.assertFalse(results.resource_pass({**original, field: value}))

    def test_unstarted_export_keeps_all_slots_without_inventing_cost_or_completion(self):
        schedule = plan.prepare(ROOT, 'P5-initial')
        value = {'schedule': schedule, 'implementation': {'fixed': 'input'}, 'controls': {}, 'admission': {},
                 'binaries': {}, 'versions': {}, 'checker_versions': {}}
        value['sha256'] = hashlib.sha256(encoded(value)).hexdigest()
        review = {row['id']: {'record_sha256': None, 'contributing_actions': [], 'failure_summary': None}
                  for row in schedule['attempts']}
        with tempfile.TemporaryDirectory() as temporary, \
                patch.object(local, 'implementation_bindings', return_value={'fixed': 'input'}):
            directory = Path(temporary)
            frozen = directory / 'plan.json'
            frozen.write_text(json.dumps(value))
            outcome = results.export(frozen, review, directory / 'published')
            self.assertEqual(len(outcome['attempts']), 48)
            self.assertTrue(all(row['status'] == 'unstarted' and row['usage'] is None and row['cost'] is None
                                for row in outcome['attempts']))
            self.assertEqual(sorted(path.name for path in (directory / 'published').iterdir()),
                             ['collection.json', 'observations.json', 'patches', 'summary.json'])
            broken = copy.deepcopy(review)
            broken[next(iter(broken))]['raw_prompt'] = 'must never be published'
            with self.assertRaisesRegex(ValueError, 'only reviewed'):
                results.export(frozen, broken, directory / 'refused')


if __name__ == '__main__':
    unittest.main()
