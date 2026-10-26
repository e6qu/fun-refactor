"""Incomplete or mismatched hosted controls must never enable paid proof attempts."""
import copy
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

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

    def test_only_the_original_successful_control_run_can_load_its_checker(self):
        for changed_source, changed_archive in ((False, False), (True, False), (False, True)):
            with self.subTest(source=changed_source, archive=changed_archive), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                archive = root / 'checker.tar.gz'
                archive.write_bytes(b'test checker archive')
                receipt = self.combine(self.reports())
                receipt['private']['image_archive_sha256'] = hashlib.sha256(archive.read_bytes()).hexdigest()
                if changed_archive:
                    archive.write_bytes(b'changed checker archive')
                (root / 'summary.json').write_text(json.dumps({'controls': receipt}))

                def output(argv, **_settings):
                    if argv[0] != 'gh':
                        return receipt['private']['image'] + '\n'
                    if '/jobs?' in argv[-1]:
                        return json.dumps([{'jobs': [{'name': 'M3 / Require every proof, source and native-client control',
                                                     'conclusion': 'success'}]}])
                    return json.dumps({'head_sha': ('b' if changed_source else 'a') * 40,
                                       'head_repository': {'full_name': 'example/project'}})

                with mock.patch.dict('os.environ', {'GITHUB_ACTIONS': 'true', 'GITHUB_REPOSITORY': 'example/project'}), \
                     mock.patch.object(results.local, 'implementation_bindings', return_value=receipt['bindings']), \
                     mock.patch.object(results.subprocess, 'check_output', side_effect=output), \
                     mock.patch.object(results.subprocess, 'run') as loading:
                    if changed_source or changed_archive:
                        with self.assertRaises(ValueError):
                            results.load_checker(root, archive, '123')
                        loading.assert_not_called()
                    else:
                        loaded = results.load_checker(root, archive, '123')
                        self.assertEqual(loaded['image'], receipt['private']['image'])
                        self.assertEqual(loading.call_count, 1)


if __name__ == '__main__':
    unittest.main()
