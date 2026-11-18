"""Reject attractive summaries made by dropping failures, time or unknown costs."""
import copy
import unittest

from . import product_plan as plan, product_report as report
from .selected_plan import ROOT


class ProductReportTests(unittest.TestCase):
    def records(self):
        rows = [row for row in plan.campaign(ROOT)['attempts'] if row['round'] == 'initial']
        observations = {row['id']: {'status': 'completed', 'independent_outcome': 'passed',
            'patch_delivered': True, 'patch_sha256': 'a' * 64, 'checking_inputs_preserved': True,
            'accounting_complete': True, 'resource_stop': None,
            'fr_contribution': ['Source lookup used in the delivered change.'] if row['setup'] == 'fr' else [],
            'elapsed_seconds': 80 if row['setup'] == 'fr' else 100} for row in rows}
        grading = {'schema': 'fr-product-grading-1', 'round': 'initial', 'run': '123',
            'source_commit': 'b' * 40, 'checker_bindings': {'tools/agent_eval/product_tasks.py': 'c' * 64},
            'attempts': {key: {'passed': True, 'patch_sha256': 'a' * 64} for key in observations}}
        return rows, observations, grading

    def test_unstarted_is_not_free_or_successful(self):
        value = report.summary(ROOT, 'initial', {})
        self.assertEqual(len(value['attempts']), 48)
        self.assertIsNone(value['billing']['total'])
        self.assertEqual(value['billing']['known_attempts'], 0)
        for row in value['attempts']:
            self.assertEqual(row['status'], 'unstarted')
            self.assertIsNone(row['elapsed_seconds'])
            self.assertIsNone(row['complete_accepted'])
        self.assertTrue(all(row['benefit_demonstrated'] is None for row in value['profiles'].values()))

    def test_both_repetitions_and_two_families_are_required(self):
        rows, observations, grading = self.records()
        good = report.summary(ROOT, 'initial', observations, grading)
        self.assertTrue(all(row['benefit_demonstrated'] for row in good['profiles'].values()))
        profile = rows[0]['profile']
        for row in rows:
            if row['profile'] == profile and row['setup'] == 'fr' and row['family'] != 'refactoring':
                observations[row['id']]['elapsed_seconds'] = 99
        value = report.summary(ROOT, 'initial', observations, grading)
        self.assertFalse(value['profiles'][profile]['benefit_demonstrated'])

    def test_late_artifact_resource_stop_or_missing_accounting_is_not_complete(self):
        for change in ({'elapsed_seconds': 150.01}, {'resource_stop': 'rss_bytes'},
                       {'checking_inputs_preserved': False}, {'accounting_complete': False}):
            rows, observations, grading = self.records()
            key = next(row['id'] for row in rows if row['setup'] == 'fr')
            observations[key].update(change)
            value = report.summary(ROOT, 'initial', observations, grading)
            row = next(row for row in value['attempts'] if row['id'] == key)
            self.assertEqual(row['independent_outcome'], 'passed')
            self.assertFalse(row['complete_accepted'])
            self.assertFalse(value['profiles'][row['profile']]['benefit_demonstrated'])

    def test_independent_receipt_must_bind_every_patch(self):
        _, observations, grading = self.records()
        key = next(iter(observations))
        for mutate in (lambda value: value['attempts'].pop(key),
                       lambda value: value['attempts'][key].update(patch_sha256='d' * 64),
                       lambda value: value['attempts'][key].update(passed=False)):
            broken = copy.deepcopy(grading)
            mutate(broken)
            with self.assertRaises(ValueError):
                report.summary(ROOT, 'initial', observations, broken)

    def test_ordinary_work_in_fr_setup_does_not_demonstrate_fr_benefit(self):
        _, observations, grading = self.records()
        for row in observations.values():
            row['fr_contribution'] = []
        value = report.summary(ROOT, 'initial', observations, grading)
        self.assertTrue(all(not row['benefit_demonstrated'] for row in value['profiles'].values()))
        self.assertTrue(all(row['complete_accepted'] for row in value['attempts']))

    def test_other_round_unknown_fields_and_false_numbers_refuse(self):
        rows, observations, _ = self.records()
        key = rows[0]['id']
        for change in ({'round': 'validation'}, {'raw_transcript': 'private'}, {'elapsed_seconds': True},
                       {'cost': float('nan')}, {'status': 'unstarted', 'elapsed_seconds': 0}):
            broken = copy.deepcopy(observations)
            broken[key].update(change)
            with self.assertRaises(ValueError):
                report.summary(ROOT, 'initial', broken)


if __name__ == '__main__':
    unittest.main()
