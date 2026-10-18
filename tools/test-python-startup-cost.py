"""Reject incomplete, inconsistent and misleading saved cost comparisons."""
import copy
import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('startup_cost', Path(__file__).with_name('python-startup-cost.py'))
cost = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cost)


def fixture():
    rows, answers, observations = [], {}, []
    for case in cost.CASES:
        for stage, (name, _, _) in enumerate(cost.STAGES):
            wanted = cost.expected(case, stage)
            answer = {'case': case, 'stage': name, 'complete': wanted['complete'],
                      'witnesses': [17] if wanted['python']['returned'] == 17 else [],
                      'cutoffs': [] if wanted['complete'] else [
                          'step-budget' if case == 'unfinished-budget' else 'module-effects-unchecked']}
            key = cost.digest(answer)
            answers[key] = answer
            observations.append({'case': case, 'stage': name, 'result': wanted['python'], 'elapsed_seconds': 0.01})
            for repetition in range(3):
                for strategy in cost.STRATEGIES:
                    metrics = dict.fromkeys(cost.FIELDS, 0)
                    metrics.update(elapsed_seconds=0.03, worker_seconds=0.02, native_seconds=0.01,
                                   native_calls=3 if strategy == 'saved-answer' else 2)
                    rows.append({'case': case, 'stage': name, 'repetition': repetition, 'strategy': strategy,
                                 'position': (cost.STRATEGIES.index(strategy) - repetition) % 3,
                                 'answer_digest': key, 'startup_checks': int(not wanted['startup_reused'])
                                 if strategy == 'saved-startup' else None,
                                 'startup_reused': strategy == 'saved-startup' and wanted['startup_reused'],
                                 'answer_reused': strategy == 'saved-answer' and wanted['answer_reused'],
                                 'metrics': metrics})
    return {'schema': 'fr-python-startup-cost', 'bindings': {'source': 'frozen'}, 'scope': cost.SCOPE,
            'cases': cost.CASES, 'stages': cost.STAGES, 'strategies': cost.STRATEGIES, 'repetitions': 3,
            'samples': rows, 'answers': answers, 'observations': observations, 'summary': cost.summarize(rows)}


class Controls(unittest.TestCase):
    def setUp(self):
        self.report = fixture()
        self.binding = patch.object(cost, 'bindings', return_value={'source': 'frozen'})
        self.binding.start()
        self.addCleanup(self.binding.stop)

    def check(self, change):
        changed = copy.deepcopy(self.report)
        change(changed)
        changed['summary'] = cost.summarize(changed['samples'])
        with self.assertRaises(ValueError):
            cost.audit(changed)

    def test_complete_control_is_accepted(self):
        cost.audit(self.report)

    def test_all_rows_are_required(self):
        self.check(lambda report: report['samples'].pop())

    def test_duplicate_requests_are_rejected(self):
        self.check(lambda report: report['samples'].__setitem__(1, report['samples'][0]))

    def test_source_edit_requires_new_collection(self):
        self.check(lambda report: report['bindings'].update(source='changed'))

    def test_strategy_order_cannot_be_hidden(self):
        self.check(lambda report: report['samples'][0].update(position=2))

    def test_whole_answer_preflight_is_counted(self):
        self.check(lambda report: report['samples'][2]['metrics'].update(native_calls=2))

    def test_fresh_baseline_cannot_read_a_store(self):
        self.check(lambda report: report['samples'][0]['metrics'].update(store_gets=1))

    def test_invalid_timing_is_rejected(self):
        for value in (-1, float('inf'), float('nan'), True):
            with self.subTest(value=value):
                self.check(lambda report: report['samples'][0]['metrics'].update(elapsed_seconds=value))

    def test_hidden_extra_time_is_rejected(self):
        self.check(lambda report: report['samples'][0]['metrics'].update(native_seconds=0.1))

    def test_budget_cannot_increase(self):
        self.check(lambda report: report['samples'][0]['metrics'].update(analysis_steps=4097))

    def test_reuse_cannot_be_invented(self):
        self.check(lambda report: report['samples'][0].update(startup_reused=True))

    def test_completion_cannot_be_invented(self):
        def change(report):
            row = next(row for row in report['samples'] if row['case'] == 'unfinished-budget')
            answer = report['answers'][row['answer_digest']]
            answer['complete'] = True
        self.check(change)

    def test_saved_and_fresh_answers_must_agree(self):
        def change(report):
            row = report['samples'][1]
            answer = copy.deepcopy(report['answers'][row['answer_digest']])
            answer['extra'] = 'different'
            key = cost.digest(answer)
            report['answers'][key] = answer
            row['answer_digest'] = key
        self.check(change)

    def test_python_failures_cannot_be_erased(self):
        def change(report):
            row = next(row for row in report['observations'] if row['stage'] == 'unknown-startup')
            row['result'] = {'error': None, 'returned': 17}
        self.check(change)

    def test_python_observations_cannot_be_duplicated(self):
        self.check(lambda report: report['observations'].__setitem__(1, report['observations'][0]))

    def test_summary_cannot_be_edited_independently(self):
        self.report['summary']['local']['first-question']['fresh']['elapsed_seconds']['median'] = 0.001
        with self.assertRaises(ValueError):
            cost.audit(self.report)

    def test_semantic_normalization_preserves_everything_except_execution_metadata(self):
        original = {'execution': {'kind': 'analyzed'}, 'context_basis': 'basis',
                    'complete': False, 'cutoffs': ['unknown'], 'witnesses': [], 'revision': 'current'}
        normalized = cost.evidence(original)
        self.assertEqual(normalized, {'complete': False, 'cutoffs': ['unknown'],
                                      'witnesses': [], 'revision': 'current'})
        self.assertIn('execution', original)


if __name__ == '__main__':
    unittest.main()
