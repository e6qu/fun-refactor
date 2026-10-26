"""Incomplete or mismatched hosted controls must never enable paid proof attempts."""
import copy
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval import proof_writing_collect as collector
from agent_eval import proof_writing_results as results
from agent_eval import proof_writing_tasks as tasks
from agent_eval.selected_report import AGENTS


class CompleteProofControls(unittest.TestCase):
    def reports(self):
        values = {}
        for case, entry in tasks.CASES.items():
            values['m3-proof-controls-' + case] = {
                'case': case, 'live_model_calls': 0, 'source_implementation_proved': False,
                **{flag: True for flag in tasks.CONTROL_FLAGS - {'runtime_checked'}}}
            values['m3-source-runtime-' + case] = {
                'case': case, 'live_model_calls': 0, 'source_implementation_proved': False,
                'passed': True, 'checks': [{'returncode': 0}], 'input_cases': 4,
                'source_sha256': hashlib.sha256((tasks.ROOT / tasks.DIRECTORY / 'fixtures' / entry['path']).read_bytes()).hexdigest()}
        values['m3-native-controls'] = {'passed': True, 'live_model_calls': 0,
            'exchanges': {name + '-' + setup: {'status': 'passed'} for name in AGENTS for setup in ('files', 'fr')}}
        values['m3-private-controls'] = {'passed': True, 'live_model_calls': 0, 'checked_controls': 18,
            'image': 'sha256:' + 'a' * 64, 'image_archive_sha256': 'b' * 64,
            'controls': {task + '-' + variant: {'status': 'passed', 'accepted': variant == 'reference'}
                         for task in results.TASKS for variant in results.VARIANTS}}
        return values

    def combine(self, values):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name, value in values.items():
                path = root / name / ('summary.json' if name in {'m3-native-controls', 'm3-private-controls'} else 'result.json')
                path.parent.mkdir()
                path.write_text(json.dumps(value))
            return collector.combine(root, 'a' * 40, '123', {'checker': 'c' * 64})

    def test_complete_controls_are_separate_from_source_equivalence(self):
        result = self.combine(self.reports())
        self.assertTrue(result['passed'])
        self.assertEqual(result['live_model_calls'], 0)
        self.assertTrue(all(row['source_implementation_proved'] is False for row in result['cases'].values()))

    def test_source_proof_or_native_failures_and_misidentified_cases_are_refused(self):
        mutations = [('m3-source-runtime-access-rust', 'passed', False),
                     ('m3-source-runtime-page-count', 'source_sha256', 'wrong-source'),
                     ('m3-proof-controls-access-python', 'case', 'access-rust'),
                     ('m3-proof-controls-fits-window', 'stale_records_checked', False),
                     ('m3-native-controls', 'passed', False)]
        values = self.reports()
        for name, key, value in mutations:
            changed = copy.deepcopy(values)
            changed[name][key] = value
            with self.subTest(name=name, key=key), self.assertRaises(ValueError):
                self.combine(changed)

    def test_private_checker_must_reject_changed_claims(self):
        values = self.reports()
        values['m3-private-controls']['controls']['page-count-changed-claim']['accepted'] = True
        with self.assertRaises(ValueError):
            self.combine(values)


if __name__ == '__main__':
    unittest.main()
