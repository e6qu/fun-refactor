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

    def test_construction_keeps_four_fresh_slots_and_the_shared_execution_rules(self):
        schedule = plan.prepare(plan.ROOT, 'M3-construction')
        self.assertEqual(len(schedule['attempts']), 4)
        self.assertFalse(schedule['live_calls_enabled'])
        self.assertEqual({row['setup'] for row in schedule['attempts']}, {'fr'})
        self.assertEqual({row['task'] for row in schedule['attempts']}, {'scaled-total'})
        for profile in schedule['profiles']:
            self.assertEqual([row['repetition'] for row in schedule['attempts'] if row['profile'] == profile], [1, 2])
            self.assertEqual(schedule['profiles'][profile]['model']['variant'], 'low')
        checkers = {'lean': {'source_root': '/pinned/lean'}, 'lake': {'path': '/pinned/lake'}}
        prompt = local.prompt('scaled-total', 'fr', checkers, 'M3-construction')
        for required in ('theorem starts without a proof', 'Preserve provided .fr/checks.json',
                         'Run the supplied checks once', 'Do not start a nested guard', 'call another model'):
            self.assertIn(required, prompt)
        self.assertNotIn('Int.mul_le_mul_of_nonneg_left', prompt)
        self.assertNotIn('initial-tactics.txt', prompt)

    def test_construction_claim_matches_the_fixed_model_claim(self):
        claim = feedback.property_for({'object_digest': 'fixed'}, 'M3-construction')
        self.assertEqual(claim['name'], 'at_least_input')
        self.assertEqual(claim['parameters'], [{'name': name, 'lean_type': 'Int'} for name in ('a', 'b')])
        self.assertEqual(claim['proposition']['conclusion'], {'kind': 'less-or-equal',
            'left': {'kind': 'variable', 'name': 'a'}, 'right': {'kind': 'model', 'arguments': [
                {'kind': 'variable', 'name': 'a'}, {'kind': 'variable', 'name': 'b'}]}})
        self.assertEqual(claim['proposition']['premise']['kind'], 'and')
        self.assertEqual([item['left']['value'] for item in claim['proposition']['premise']['propositions']], [0, 1])
        self.assertIsNone(feedback.case_details('M3-construction')['draft'])

    def test_page_count_has_a_separate_four_slot_plan_and_the_original_claim(self):
        schedule = plan.prepare(plan.ROOT, 'M3-page-count')
        self.assertEqual(len(schedule['attempts']), 4)
        self.assertFalse(schedule['live_calls_enabled'])
        self.assertEqual([row['profile'] for row in schedule['attempts']],
                         ['opencode2-glm-flash', 'opencode2-kimi-k3'] * 2)
        self.assertEqual({row['task'] for row in schedule['attempts']}, {'page-count'})
        self.assertEqual({row['setup'] for row in schedule['attempts']}, {'fr'})
        self.assertEqual(schedule['requirements']['limits'], {**local.LIMITS, 'wall_seconds': 300, 'parallel_attempts': 1})
        self.assertEqual(schedule['requirements']['client_finish_seconds'], 10)
        self.assertTrue(all(profile['model']['variant'] == 'low' for profile in schedule['profiles'].values()))
        self.assertEqual(feedback.property_for({'object_digest': 'fixed'}, 'M3-page-count'),
                         feedback.proof.property_for('page-count', {'object_digest': 'fixed'}))
        self.assertIn('tests/agent-eval/proof-writing/fixtures/src/pages.rs', schedule['bindings'])
        self.assertIn('tests/agent-eval/proof-writing/controls/page-count.lean', schedule['bindings'])
        with mock.patch.object(feedback.proof, 'prepare', return_value='canonical') as preparing:
            self.assertEqual(feedback.prepare_inputs('fr', 'project', 'output', 'M3-page-count'), 'canonical')
        preparing.assert_called_once_with('page-count', 'fr', 'project', 'output')
        prompt = local.prompt('page-count', 'fr',
                              {'lean': {'source_root': '/pinned/lean'}, 'lake': {'path': '/pinned/lake'}},
                              'M3-page-count')
        for required in ('eleven items', 'invalid-input', 'Preserve provided .fr/checks.json',
                         'Run the supplied checks once', 'Do not start a nested guard', 'call another model'):
            self.assertIn(required, prompt)
        self.assertNotIn('controls/page-count.lean', prompt)
        self.assertNotIn('initial-tactics.txt', prompt)

    def test_page_count_prediction_requires_each_agent_and_all_four_honest_outcomes(self):
        rows = [{**row, 'native_status': 'completed', 'stop_reason': None, 'total_seconds': 30.0,
                 'complete_attempt_deadline_missed': False, 'independent_check': {'patch_accepted': True},
                 'proof_observations': {'execution_validity': 'valid', 'proof_check_passed': True,
                                        'proof_written_with_fr': True, 'package_check_passed': True},
                 'collection_error_recorded': False, 'checking_input_changes': [], 'suspended_seconds': 0,
                 'independent_outcome': 'passed' if row['repetition'] == 1 else 'failed'}
                for row in plan.prepare(plan.ROOT, 'M3-page-count')['attempts']]
        self.assertTrue(feedback.page_count_decision(rows)['prediction_met'])
        no_kimi = copy.deepcopy(rows)
        for row in no_kimi:
            row['independent_outcome'] = 'passed' if row['profile'] == 'opencode2-glm-flash' else 'failed'
        self.assertFalse(feedback.page_count_decision(no_kimi)['prediction_met'])
        for alteration in ({'checking_input_changes': ['.fr/checks.json']},
                           {'collection_error_recorded': True}, {'suspended_seconds': 2}):
            changed = copy.deepcopy(rows)
            changed[-1].update(alteration)
            self.assertFalse(feedback.page_count_decision(changed)['prediction_met'])
        late = copy.deepcopy(rows)
        late[0].update(total_seconds=150.01, native_status='failed', stop_reason='wall_seconds')
        with self.assertRaisesRegex(ValueError, 'remains failed'):
            feedback.page_count_decision(late)
        unobserved = copy.deepcopy(rows)
        unobserved[0]['proof_observations']['package_check_passed'] = None
        with self.assertRaisesRegex(ValueError, 'unobserved construction checks'):
            feedback.page_count_decision(unobserved)
        with self.assertRaises(ValueError):
            feedback.page_count_decision(rows[:-1])
        with self.assertRaises(ValueError):
            feedback.page_count_decision(list(reversed(rows)))
        all_failed = copy.deepcopy(rows)
        for row in all_failed:
            row.update(independent_outcome='failed', patch_file=None)
        report = feedback.markdown({'study': 'M3-page-count', 'attempts': all_failed})
        self.assertNotIn('model is proved', report)
        self.assertIn('asks for proofs of the fixed mathematical model', report)

    def test_construction_requires_all_four_complete_outcomes(self):
        rows = [{**row, 'native_status': 'completed', 'stop_reason': None, 'total_seconds': 30.0,
                 'complete_attempt_deadline_missed': False, 'independent_check': {'patch_accepted': True},
                 'proof_observations': {'execution_validity': 'valid', 'proof_check_passed': True,
                                        'proof_written_with_fr': True, 'package_check_passed': True},
                 'collection_error_recorded': False,
                 'checking_input_changes': [], 'suspended_seconds': 0, 'independent_outcome': 'passed'}
                for row in plan.prepare(plan.ROOT, 'M3-construction')['attempts']]
        self.assertTrue(feedback.construction_decision(rows)['all_four_completed'])
        failed = copy.deepcopy(rows)
        failed[0].update(native_status='failed', stop_reason='wall_seconds', total_seconds=150.1,
                         complete_attempt_deadline_missed=True, independent_outcome='failed')
        self.assertFalse(feedback.construction_decision(failed)['all_four_completed'])
        changed = copy.deepcopy(rows); changed[0].update(checking_input_changes=['.fr/checks.json'],
                                                         native_status='failed', independent_outcome='failed')
        self.assertFalse(feedback.construction_decision(changed)['all_four_completed'])
        changed[0]['independent_outcome'] = 'passed'
        with self.assertRaisesRegex(ValueError, 'remains failed'):
            feedback.construction_decision(changed)
        unobserved = copy.deepcopy(rows); unobserved[0]['proof_observations']['package_check_passed'] = None
        with self.assertRaisesRegex(ValueError, 'unobserved construction checks'):
            feedback.construction_decision(unobserved)
        unobserved[0]['independent_outcome'] = 'failed'
        self.assertFalse(feedback.construction_decision(unobserved)['all_four_completed'])
        with self.assertRaises(ValueError):
            feedback.construction_decision(rows[:-1])

    def test_all_decisions_include_harm_and_keep_mixed_results_inconclusive(self):
        expected = {(2, 0): 'clearer feedback exposure', (2, 2): 'full-proof construction',
                    (0, 0): 'diagnostic comprehension', (0, 2): 'supplied feedback obstructs'}
        for supplied in (0, 1, 2):
            for self_check in (0, 1, 2):
                result = feedback.decisions(self.rows(supplied, self_check), milestone='M3-feedback')
                for profile in result:
                    self.assertIn(expected[(supplied, self_check)] if (supplied, self_check) in expected else 'Inconclusive',
                                  result[profile]['decision'])

    def test_late_stopped_or_unaccepted_patch_never_becomes_a_success(self):
        for mutation in ({'total_seconds': 150.1}, {'complete_attempt_deadline_missed': True},
                         {'stop_reason': 'wall_seconds'}, {'native_status': 'failed'},
                         {'independent_check': {'patch_accepted': False}}):
            rows = self.rows(2, 2); rows[0].update(mutation)
            with self.subTest(mutation=mutation), self.assertRaisesRegex(ValueError, 'remains failed'):
                feedback.decisions(rows, milestone='M3-feedback')

    def test_missing_reordered_or_relabeled_outcomes_refuse(self):
        original = self.rows(2, 2)
        for rows in (original[:-1], list(reversed(original)),
                     [{**row, 'setup': 'supplied-feedback'} for row in original]):
            with self.assertRaises(ValueError):
                feedback.decisions(rows, milestone='M3-feedback')
        rows = copy.deepcopy(original); rows[0]['total_seconds'] = float('nan')
        with self.assertRaisesRegex(ValueError, 'finite nonnegative'):
            feedback.decisions(rows, milestone='M3-feedback')

    def test_broken_execution_cannot_become_a_comprehension_conclusion(self):
        for mutation in ({'feedback_observations': {'execution_validity': 'service-failed'}},
                         {'feedback_observations': {'execution_validity': 'unknown'}},
                         {'collection_error_recorded': True}, {'checking_input_changes': ['.fr/checks.json']},
                         {'stop_reason': 'rss_bytes'}, {'suspended_seconds': 2}):
            rows = self.rows(0, 0); rows[0].update(mutation)
            result = feedback.decisions(rows, milestone='M3-feedback')
            affected = result[rows[0]['profile']]
            self.assertFalse(affected['execution_usable'])
            self.assertIn('experiment execution', affected['decision'])

    def test_initial_check_requires_actual_rejection_and_preserves_the_report(self):
        value = {'passed': False, 'receipt': None, 'diagnostics': [{'message': 'Argument type mismatch'}]}
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); output = root / 'check.json'
            result = subprocess.CompletedProcess([], 1, json.dumps(value), '')
            with mock.patch.object(feedback.subprocess, 'run', return_value=result) as run:
                text, size = feedback.initial_check('/fr', root, {'target': 'spec::fixed'}, {}, output, 20)
            self.assertEqual(json.loads(text), value)
            self.assertEqual(json.loads(output.read_text()), value)
            self.assertEqual(size, len(text.encode()))
            self.assertEqual(run.call_args.kwargs['timeout'], 20)
            value['passed'] = True
            with mock.patch.object(feedback.subprocess, 'run', return_value=subprocess.CompletedProcess([], 1, json.dumps(value), '')), \
                    self.assertRaisesRegex(ValueError, 'argument-type error'):
                feedback.initial_check('/fr', root, {'target': 'spec::fixed'}, {}, output, 20)

            value['passed'] = False
            for wrong_exit in (0, 2):
                with mock.patch.object(feedback.subprocess, 'run', return_value=subprocess.CompletedProcess([], wrong_exit, json.dumps(value), '')), \
                        self.assertRaisesRegex(ValueError, 'rejected proof with exit 1'):
                    feedback.initial_check('/fr', root, {'target': 'spec::fixed'}, {}, output, 20)

    def test_freeze_captures_single_file_opencode_without_other_clients(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name in ('fr', 'opencode2'):
                (root / name).write_bytes(name.encode())
            binaries, components = local.freeze_binaries(
                {name: root / name for name in ('fr', 'opencode2')}, root / 'capture', {'opencode2'})
            self.assertEqual(set(binaries), {'fr', 'opencode2'})
            self.assertEqual(components, {})
            self.assertEqual({path.name for path in (root / 'capture/bin').iterdir()}, {'fr', 'opencode2'})

    def test_controller_charges_initial_check_to_both_conditions_and_changes_only_visibility(self):
        prompts = {}
        for milestone, condition, handout_change in (
                ('M3-feedback', 'self-check', 'none'), ('M3-feedback', 'supplied-feedback', 'none'),
                ('M3-construction', 'fr', 'none'), ('M3-page-count', 'fr', 'none'),
                ('M3-facts', 'select-facts', 'none'), ('M3-facts', 'supplied-declarations', 'none'),
                ('M3-facts', 'supplied-declarations', 'edit'), ('M3-facts', 'select-facts', 'delete'),
                ('M3-facts', 'supplied-declarations', 'skill-edit'), ('M3-facts', 'select-facts', 'skill-delete'),
                (plan.REPAIR_STUDY, 'shared-skill', 'none'), (plan.REPAIR_STUDY, 'argument-repair', 'none'),
                (plan.REPAIR_STUDY, 'argument-repair', 'edit'), (plan.REPAIR_STUDY, 'shared-skill', 'delete'),
                (plan.REPAIR_STUDY, 'argument-repair', 'skill-edit')):
            with tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary).resolve(); original = root / 'original'; original.mkdir()
                (original / 'source.rs').write_text('pub fn quotient(a:i64,b:i64)->i64 { a / b }\n')
                inputs = root / 'inputs'; inputs.mkdir()
                schedule = plan.prepare(plan.ROOT, milestone)
                row = next(r for r in schedule['attempts'] if r['setup'] == condition)
                receipt = local.tasks.retain_seed(original, {'task': row['task'], 'original': local.tasks.sources(original)}, inputs)
                executable = root / 'executable'; executable.write_text('#!/bin/sh\nexit 0\n'); executable.chmod(0o755)
                binary = {'path': str(executable), 'sha256': local.hash_file(executable)}
                library = root / 'library'; library.mkdir()
                checkers = {name: dict(binary) for name in ('python', 'rustc', 'cargo', 'rustdoc', 'lean', 'lake', 'leanc')}
                checkers['lean']['source_root'] = str(library)
                packet = {'source': {'native_inputs': {}}, 'schedule': schedule, 'implementation': {},
                          'live_calls_enabled': True,
                          'limits': local.study_limits(milestone, schedule), 'client_finish_seconds': local.CLIENT_FINISH_SECONDS,
                          'public_check_environment': dict(local.PUBLIC_CHECK_ENVIRONMENT),
                          'binaries': {'fr': dict(binary), 'opencode2': dict(binary)}, 'components': {},
                          'checkers': checkers, 'seeds': {row['task']: {'archive': str(inputs / 'original.tar.gz'), 'receipt': receipt}},
                          'formal_inputs': {row['task']: {'spec': 'specs/Fixed.lean', 'obligation': 'fixed'}},
                          'prompts': {r['id']: 'Fixed repair request.' for r in schedule['attempts']}}
                packet['sha256'] = feedback.hashlib.sha256(plan.encoded(packet)).hexdigest()
                self.addCleanup(shutil.rmtree, root / 'workspaces' / ('fr-' + milestone.lower() + '-' + packet['sha256'][:12]), ignore_errors=True)
                path = root / 'plan.json'; path.write_text(json.dumps(packet))
                clock = [0.0]
                initial = json.dumps({'passed': False, 'receipt': None, 'diagnostics': [{'message': 'argument type mismatch'}]})

                def initial_check(binary, project, prepared, env, output, seconds):
                    self.assertEqual(milestone, 'M3-feedback')
                    self.assertEqual(seconds, 150)
                    clock[0] += 3
                    output.write_text(initial)
                    return initial, len(initial.encode())

                def native(argv, stdin, stdout, stderr, workspace, **kwargs):
                    wall = local.study_limits(milestone, schedule)['wall_seconds'] - local.CLIENT_FINISH_SECONDS
                    self.assertEqual(kwargs['wall_seconds'], wall - 3 if milestone == 'M3-feedback' else wall)
                    self.assertEqual(kwargs['transcript_bytes'], local.LIMITS['transcript_bytes'] -
                                     (len(initial.encode()) if milestone == 'M3-feedback' else 0))
                    prompts[condition] = argv[-1]
                    if milestone in {'M3-facts', plan.REPAIR_STUDY}:
                        handout = workspace / ('.fr/declarations.txt' if milestone == 'M3-facts' else '.fr/argument-repair.txt')
                        if milestone == 'M3-facts':
                            self.assertEqual('Int.tdiv_nonneg' in handout.read_text(), condition == 'supplied-declarations')
                        else:
                            self.assertEqual('argument-type error' in handout.read_text(), condition == 'argument-repair')
                        if handout_change == 'edit':
                            handout.write_text('changed\n')
                        elif handout_change == 'delete':
                            handout.unlink()
                        elif handout_change == 'skill-edit':
                            (workspace / '.agents/skills/fr/SKILL.md').write_text('changed\n')
                        elif handout_change == 'skill-delete':
                            (workspace / '.agents/skills/fr/SKILL.md').unlink()
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
                if milestone == 'M3-feedback':
                    self.assertEqual(record['initial_check_seconds'], 3)
                    self.assertEqual(record['initial_feedback_supplied'], condition == 'supplied-feedback')
                else:
                    self.assertNotIn('initial_check_seconds', record)
                self.assertEqual(record['elapsed_seconds'], 8 if milestone == 'M3-feedback' else 5)
                expected_path = ('.agents/skills/fr/SKILL.md' if handout_change in {'skill-edit', 'skill-delete'} else
                                 '.fr/argument-repair.txt' if milestone == plan.REPAIR_STUDY else '.fr/declarations.txt')
                self.assertEqual(record['checking_input_changes'], [] if handout_change == 'none' else [expected_path])
                if milestone in {'M3-facts', plan.REPAIR_STUDY}:
                    self.assertIn(expected_path, record['provided_checking_inputs'])
                    self.assertEqual(record['status'], 'completed' if handout_change == 'none' else 'failed')
                shutil.rmtree(Path(record['project']).parent.parent)
        self.assertEqual(prompts['self-check'], prompts['supplied-feedback'].split('\n\nInitial fr rejection report')[0])
        self.assertEqual(prompts['select-facts'], prompts['supplied-declarations'])
        self.assertEqual(prompts['shared-skill'], prompts['argument-repair'])


if __name__ == '__main__':
    unittest.main()
