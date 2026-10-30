"""A diagnostic keeps one changed factor, balanced conditions and independent outcomes."""
import copy
import json
from pathlib import Path
import subprocess
import shutil
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval import proof_feedback as feedback, selected_local as local, selected_plan as plan


class ProofFeedbackTests(unittest.TestCase):
    def rows(self, supplied, self_check):
        counts = {'supplied-feedback': supplied, 'self-check': self_check}
        return [{**row, 'native_status': 'completed', 'stop_reason': None, 'total_seconds': 10.0,
                 'complete_attempt_deadline_missed': False, 'independent_check': {'patch_accepted': True},
                 'feedback_observations': {'execution_validity': 'valid'}, 'collection_error_recorded': False,
                 'checking_input_changes': [], 'suspended_seconds': 0,
                 'independent_outcome': 'passed' if row['repetition'] <= counts[row['setup']] else 'failed'}
                for row in plan.prepare(plan.ROOT, 'M3-feedback')['attempts']]

    def test_eight_balanced_attempts_keep_both_models_and_one_task(self):
        schedule = plan.prepare(plan.ROOT, 'M3-feedback')
        self.assertEqual(len(schedule['attempts']), 8)
        self.assertEqual(set(schedule['profiles']), {'opencode2-glm-flash', 'opencode2-kimi-k3'})
        self.assertFalse(schedule['live_calls_enabled'])
        self.assertEqual(schedule['requirements']['limits']['wall_seconds'], 150)
        for profile in schedule['profiles']:
            conditions = []
            for repeat in (1, 2):
                rows = [row for row in schedule['attempts'] if row['profile'] == profile and row['repetition'] == repeat]
                self.assertEqual({row['setup'] for row in rows}, {'self-check', 'supplied-feedback'})
                self.assertEqual({row['task'] for row in rows}, {'division-repair'})
                conditions.append(rows[0]['setup'])
            self.assertNotEqual(*conditions)

    def test_prompt_and_tool_access_are_identical_before_initial_feedback(self):
        checkers = {'lean': {'source_root': '/pinned/lean'}, 'lake': {'path': '/pinned/lake'}}
        first = local.prompt(feedback.TASK, 'self-check', checkers, 'M3-feedback')
        second = local.prompt(feedback.TASK, 'supplied-feedback', checkers, 'M3-feedback')
        self.assertEqual(first, second)
        self.assertNotIn('Int.tdiv_nonneg ha', first)
        self.assertIn('initial-tactics.txt', first)
        self.assertIn('Preserve provided .fr/checks.json', first)
        self.assertIn('changing or deleting them makes the attempt fail', first)
        self.assertIn('Run the supplied checks once', first)
        self.assertIn('do not repeat checks already passed', first)
        self.assertIn('Do not start a nested guard', first)
        self.assertIn('call another model', first)
        self.assertIn('Fr and its project skill are installed', first)
        self.assertNotIn('Fr is not provided', first)

    def test_all_decisions_include_harm_and_keep_mixed_results_inconclusive(self):
        expected = {(2, 0): 'clearer feedback exposure', (2, 2): 'full-proof construction',
                    (0, 0): 'diagnostic comprehension', (0, 2): 'supplied feedback obstructs'}
        for supplied in (0, 1, 2):
            for self_check in (0, 1, 2):
                result = feedback.decisions(self.rows(supplied, self_check))
                for profile in result:
                    self.assertIn(expected[(supplied, self_check)] if (supplied, self_check) in expected else 'Inconclusive',
                                  result[profile]['decision'])

    def test_late_stopped_or_unaccepted_patch_never_becomes_a_success(self):
        for mutation in ({'total_seconds': 150.1}, {'complete_attempt_deadline_missed': True},
                         {'stop_reason': 'wall_seconds'}, {'native_status': 'failed'},
                         {'independent_check': {'patch_accepted': False}}):
            rows = self.rows(2, 2); rows[0].update(mutation)
            with self.subTest(mutation=mutation), self.assertRaisesRegex(ValueError, 'remains failed'):
                feedback.decisions(rows)

    def test_missing_reordered_or_relabeled_outcomes_refuse(self):
        original = self.rows(2, 2)
        for rows in (original[:-1], list(reversed(original)),
                     [{**row, 'setup': 'supplied-feedback'} for row in original]):
            with self.assertRaises(ValueError):
                feedback.decisions(rows)
        rows = copy.deepcopy(original); rows[0]['total_seconds'] = float('nan')
        with self.assertRaisesRegex(ValueError, 'finite nonnegative'):
            feedback.decisions(rows)

    def test_broken_execution_cannot_become_a_comprehension_conclusion(self):
        for mutation in ({'feedback_observations': {'execution_validity': 'service-failed'}},
                         {'feedback_observations': {'execution_validity': 'unknown'}},
                         {'collection_error_recorded': True}, {'checking_input_changes': ['.fr/checks.json']},
                         {'stop_reason': 'rss_bytes'}, {'suspended_seconds': 2}):
            rows = self.rows(0, 0); rows[0].update(mutation)
            result = feedback.decisions(rows)
            affected = result[rows[0]['profile']]
            self.assertFalse(affected['execution_usable'])
            self.assertIn('experiment execution', affected['decision'])

    def test_initial_check_requires_actual_rejection_and_preserves_the_report(self):
        value = {'passed': False, 'receipt': None, 'diagnostics': [{'message': 'Argument type mismatch'}]}
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); output = root / 'check.json'
            result = subprocess.CompletedProcess([], 0, json.dumps(value), '')
            with mock.patch.object(feedback.subprocess, 'run', return_value=result) as run:
                text, size = feedback.initial_check('/fr', root, {'target': 'spec::fixed'}, {}, output, 20)
            self.assertEqual(json.loads(text), value)
            self.assertEqual(json.loads(output.read_text()), value)
            self.assertEqual(size, len(text.encode()))
            self.assertEqual(run.call_args.kwargs['timeout'], 20)
            value['passed'] = True
            with mock.patch.object(feedback.subprocess, 'run', return_value=subprocess.CompletedProcess([], 0, json.dumps(value), '')), \
                    self.assertRaisesRegex(ValueError, 'argument-type error'):
                feedback.initial_check('/fr', root, {'target': 'spec::fixed'}, {}, output, 20)

    def test_freeze_captures_only_selected_clients_and_their_companions(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name in ('fr', 'opencode2', 'opencode'):
                (root / name).write_bytes(name.encode())
            binaries, components = local.freeze_binaries(
                {name: root / name for name in ('fr', 'opencode2')}, root / 'capture', {'opencode2'})
            self.assertEqual(set(binaries), {'fr', 'opencode2'})
            self.assertEqual(set(components), {'opencode'})

    def test_controller_charges_initial_check_to_both_conditions_and_changes_only_visibility(self):
        prompts = {}
        for condition in ('self-check', 'supplied-feedback'):
            with tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary).resolve(); original = root / 'original'; original.mkdir()
                (original / 'source.rs').write_text('pub fn quotient(a:i64,b:i64)->i64 { a / b }\n')
                inputs = root / 'inputs'; inputs.mkdir()
                receipt = local.tasks.retain_seed(original, {'task': feedback.TASK, 'original': local.tasks.sources(original)}, inputs)
                executable = root / 'executable'; executable.write_text('#!/bin/sh\nexit 0\n'); executable.chmod(0o755)
                binary = {'path': str(executable), 'sha256': local.hash_file(executable)}
                library = root / 'library'; library.mkdir()
                checkers = {name: dict(binary) for name in ('python', 'rustc', 'cargo', 'rustdoc', 'lean', 'lake', 'leanc')}
                checkers['lean']['source_root'] = str(library)
                schedule = plan.prepare(plan.ROOT, 'M3-feedback')
                row = next(r for r in schedule['attempts'] if r['setup'] == condition)
                packet = {'source': {'native_inputs': {}}, 'schedule': schedule, 'implementation': {},
                          'limits': dict(local.LIMITS), 'client_finish_seconds': local.CLIENT_FINISH_SECONDS,
                          'public_check_environment': dict(local.PUBLIC_CHECK_ENVIRONMENT),
                          'binaries': {'fr': dict(binary), 'opencode2': dict(binary)}, 'components': {},
                          'checkers': checkers, 'seeds': {feedback.TASK: {'archive': str(inputs / 'original.tar.gz'), 'receipt': receipt}},
                          'formal_inputs': {feedback.TASK: {'spec': 'specs/Fixed.lean', 'obligation': 'fixed'}},
                          'prompts': {r['id']: 'Fixed repair request.' for r in schedule['attempts']}}
                packet['sha256'] = feedback.hashlib.sha256(plan.encoded(packet)).hexdigest()
                self.addCleanup(shutil.rmtree, root / 'workspaces' / ('fr-m3-feedback-' + packet['sha256'][:12]), ignore_errors=True)
                path = root / 'plan.json'; path.write_text(json.dumps(packet))
                clock = [0.0]
                initial = json.dumps({'passed': False, 'receipt': None, 'diagnostics': [{'message': 'argument type mismatch'}]})

                def initial_check(binary, project, prepared, env, output, seconds):
                    self.assertEqual(seconds, 150)
                    clock[0] += 3
                    output.write_text(initial)
                    return initial, len(initial.encode())

                def native(argv, stdin, stdout, stderr, workspace, **kwargs):
                    self.assertEqual(kwargs['wall_seconds'], 147)
                    self.assertEqual(kwargs['transcript_bytes'], local.LIMITS['transcript_bytes'] - len(initial.encode()))
                    prompts[condition] = argv[-1]
                    session = argv[argv.index('--session') + 1]
                    stdout.write((json.dumps({'type': 'step_finish', 'sessionID': session,
                        'part': {'tokens': {}, 'cost': 0}}) + '\n').encode())
                    clock[0] += 5
                    return {'exit_code': 0, 'stop_reason': None}

                def controller_path(value):
                    return root / 'workspaces' if value == '/private/tmp' else Path(value)

                with mock.patch.object(local, 'sys', mock.Mock(platform='darwin')), \
                     mock.patch.object(local, 'Path', side_effect=controller_path), \
                     mock.patch.object(local, 'identity', return_value={'native_inputs': {}}), \
                     mock.patch.object(local, 'prepare', return_value=schedule), \
                     mock.patch.object(local, 'implementation_bindings', return_value={}), \
                     mock.patch.object(local, 'elapsed_time', side_effect=lambda: clock[0]), \
                     mock.patch.object(local.time, 'monotonic', side_effect=lambda: clock[0]), \
                     mock.patch.object(local.time, 'time', return_value=10000), \
                     mock.patch.object(feedback, 'initial_check', side_effect=initial_check), \
                     mock.patch.object(local, 'bounded_run', side_effect=native), \
                     mock.patch.object(local, 'record_opencode_accounting'):
                    record = local.attempt(path, row['id'])
                self.assertEqual(record['initial_check_seconds'], 3)
                self.assertEqual(record['elapsed_seconds'], 8)
                self.assertEqual(record['initial_feedback_supplied'], condition == 'supplied-feedback')
                self.assertEqual(record['checking_input_changes'], [])
                shutil.rmtree(Path(record['project']).parent.parent)
        self.assertEqual(prompts['self-check'], prompts['supplied-feedback'].split('\n\nInitial fr rejection report')[0])


if __name__ == '__main__':
    unittest.main()
