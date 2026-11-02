"""Incomplete work cannot turn into success or evidence for a new method."""
import copy
import unittest

from . import algorithm_results as results
from .selected_plan import ROOT, prepare


class AlgorithmResultsChecks(unittest.TestCase):
    def row(self):
        return {'algorithm_observations': {**{name: True for name in results.CHECKS},
                **{name: None for name in results.TIMINGS}, 'execution_validity': 'valid'},
                'native_status': 'completed', 'stop_reason': None, 'complete_attempt_deadline_missed': False,
                'total_seconds': 140, 'checking_input_changes': [], 'collection_error_recorded': False}

    def test_both_proofs_and_delivery_are_required(self):
        row = self.row()
        self.assertTrue(results.complete(row, True))
        self.assertFalse(results.complete(row, False))
        for name in results.CHECKS:
            for value in (False, None):
                changed = copy.deepcopy(row)
                changed['algorithm_observations'][name] = value
                self.assertFalse(results.complete(changed, True))

    def test_a_valid_late_patch_stays_failed(self):
        for change in ({'total_seconds': 150.001}, {'native_status': 'failed'},
                       {'stop_reason': 'wall_seconds'}, {'complete_attempt_deadline_missed': True},
                       {'checking_input_changes': ['.fr/declarations.txt']}, {'collection_error_recorded': True}):
            self.assertFalse(results.complete({**self.row(), **change}, True))

    def test_unknown_service_validity_and_invented_times_do_not_pass(self):
        row = self.row()
        row['algorithm_observations']['execution_validity'] = 'unknown'
        self.assertFalse(results.complete(row, True))
        for seconds in (-1, 151, float('inf'), float('nan'), True):
            row['algorithm_observations']['first_check_seconds'] = seconds
            with self.assertRaises(ValueError):
                results.complete(row, True)

    def test_decision_uses_all_failures_and_original_order(self):
        study = 'M3-algorithm-presentation'
        plan = prepare(ROOT, study)
        rows = [{**entry, **self.row(), 'independent_outcome': 'passed' if entry['setup'] == 'graph' else 'failed',
                 'independent_check': {'patch_accepted': entry['setup'] == 'graph'}} for entry in plan['attempts']]
        summary = {'study': study, 'attempts': rows}
        decisions = results.decisions(summary)
        for value in decisions.values():
            self.assertEqual(value['accepted'], {'checklist': 0, 'graph': 2})
            self.assertEqual(value['total_seconds_including_failures'], 560)
            self.assertTrue(value['transfer_candidate'])
            self.assertEqual(value['decision'], plan['requirements']['decisions']['2,0'])
        summary['attempts'] = rows[:-1]
        with self.assertRaises(ValueError):
            results.decisions(summary)


if __name__ == '__main__':
    unittest.main()
