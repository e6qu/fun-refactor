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
                'total_seconds': 140, 'checking_input_changes': [], 'collection_error_recorded': False,
                'suspended_seconds': 0, 'complete_attempt_resources': {'stop_reason': None, 'exit_code': 0,
                'sampled_aggregate_rss_bytes': 500000000, 'sampled_cpu_seconds': 30}}

    def test_both_proofs_and_delivery_are_required(self):
        row = self.row()
        self.assertTrue(results.complete(row, True, 'M3-algorithm-facts'))
        self.assertFalse(results.complete(row, False, 'M3-algorithm-facts'))
        for name in results.CHECKS:
            for value in (False, None):
                changed = copy.deepcopy(row)
                changed['algorithm_observations'][name] = value
                self.assertFalse(results.complete(changed, True, 'M3-algorithm-facts'))

    def test_a_valid_late_patch_stays_failed(self):
        for change in ({'total_seconds': 150.001}, {'native_status': 'failed'},
                       {'stop_reason': 'wall_seconds'}, {'complete_attempt_deadline_missed': True},
                       {'checking_input_changes': ['.fr/declarations.txt']}, {'collection_error_recorded': True}):
            self.assertFalse(results.complete({**self.row(), **change}, True, 'M3-algorithm-facts'))

    def test_supplied_proofs_still_need_accounting_capture_bound_and_complete_resources(self):
        row = self.row()
        row['reported_usage_complete'] = True
        row['delivery_capture_bound_seconds'] = 140
        row['algorithm_observations']['patch_delivery_seconds'] = 90
        row['complete_attempt_resources'].update(elapsed_seconds=140, monitor_error=None, launch_error=None,
            sampled_disk_growth_bytes=1000, transcript_bytes=1000)
        self.assertTrue(results.complete(row, True, 'M3-delivery'))
        for field, value in (('reported_usage_complete', False), ('total_seconds', 150.01)):
            changed = {**row, field: value}
            self.assertFalse(results.complete(changed, True, 'M3-delivery'))
        for field, value in (('elapsed_seconds', 139), ('monitor_error', 'failed'), ('transcript_bytes', 1048577)):
            changed = copy.deepcopy(row)
            changed['complete_attempt_resources'][field] = value
            self.assertFalse(results.complete(changed, True, 'M3-delivery'))
        row['algorithm_observations']['patch_delivery_seconds'] = None
        self.assertTrue(results.complete(row, True, 'M3-delivery'))
        row['delivery_capture_bound_seconds'] = None
        self.assertFalse(results.complete(row, True, 'M3-delivery'))

    def test_delivery_decision_and_report_do_not_claim_proof_discovery_or_list_transfer(self):
        study = 'M3-delivery'
        plan = prepare(ROOT, study)
        rows = []
        for entry in plan['attempts']:
            accepted = entry['setup'] == 'finish-card'
            row = {**entry, **self.row(), 'reported_usage_complete': True,
                   'delivery_capture_bound_seconds': 140,
                   'independent_outcome': 'passed' if accepted else 'failed',
                   'independent_check': {'patch_accepted': accepted},
                   'failure_summary': None if accepted else 'Patch omitted the earlier proof.',
                   'patch_file': 'patches/' + entry['id'] + '.patch'}
            row['complete_attempt_resources'].update(elapsed_seconds=140, monitor_error=None,
                launch_error=None, sampled_disk_growth_bytes=1000, transcript_bytes=1000)
            row['algorithm_observations']['patch_delivery_seconds'] = 90
            rows.append(row)
        summary = {'study': study, 'attempts': rows, 'comparison': plan['requirements']['cost_comparison'],
                   'guidance_bytes': {'shared-skill': {'.fr/euclid_correct.tactics': 100},
                                      'finish-card': {'.fr/finish-card.txt': 200}}}
        for decision in results.decisions(summary).values():
            self.assertEqual(decision['accepted'], {'shared-skill': 0, 'finish-card': 2})
            self.assertTrue(decision['delivery_validation_candidate'])
            self.assertNotIn('transfer_candidate', decision)
        report = results.markdown(summary)
        self.assertIn('Both correct proofs are supplied.', report)
        self.assertIn('not proof discovery', report)
        self.assertIn('`.fr/finish-card.txt` | 200', report)
        self.assertNotIn('| Declarations | Plan |', report)

    def test_inner_success_cannot_hide_complete_attempt_resource_failure(self):
        for change in ({'stop_reason': 'rss_bytes'}, {'exit_code': 125},
                       {'sampled_aggregate_rss_bytes': 805306369}, {'sampled_cpu_seconds': 75.01}):
            row = self.row()
            row['complete_attempt_resources'].update(change)
            self.assertFalse(results.complete(row, True, 'M3-algorithm-facts'))

    def test_unknown_service_validity_and_invented_times_do_not_pass(self):
        row = self.row()
        row['algorithm_observations']['execution_validity'] = 'unknown'
        self.assertFalse(results.complete(row, True, 'M3-algorithm-facts'))
        for seconds in (-1, 151, float('inf'), float('nan'), True):
            row['algorithm_observations']['first_check_seconds'] = seconds
            with self.assertRaises(ValueError):
                results.complete(row, True, 'M3-algorithm-facts')

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
        rows[0]['algorithm_observations']['execution_validity'] = 'service-failed'
        decisions = results.decisions(summary)
        self.assertFalse(decisions[rows[0]['profile']]['transfer_candidate'])
        self.assertIn('Execution is invalid or unverified', decisions[rows[0]['profile']]['decision'])
        rows[0]['algorithm_observations']['execution_validity'] = 'unknown'
        rows[0]['failure_summary'] = 'Confirmed fr reader rejection; no provider error observed.'
        summary['comparison'] = 'A fixed comparison'
        summary['guidance_bytes'] = {}
        for row in rows:
            row.setdefault('failure_summary', '')
            row['patch_file'] = None
        report = results.markdown(summary)
        self.assertIn('Confirmed fr reader rejection; no provider error observed.', report)
        self.assertNotIn('when service failures', report)
        summary['attempts'] = rows[:-1]
        with self.assertRaises(ValueError):
            results.decisions(summary)


if __name__ == '__main__':
    unittest.main()
