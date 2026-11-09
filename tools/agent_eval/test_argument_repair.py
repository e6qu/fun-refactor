"""Do not confuse delivered proofs, typed-argument repairs and unobserved mechanisms."""
import copy
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from agent_eval import proof_feedback as feedback, selected_local as local, selected_plan as plan

STUDY = plan.REPAIR_STUDY


class ArgumentRepairTests(unittest.TestCase):
    def rows(self):
        return [{**row, 'native_status': 'completed', 'stop_reason': None, 'total_seconds': 30.,
                 'complete_attempt_deadline_missed': False, 'independent_check': {'patch_accepted': True},
                 'proof_observations': {'execution_validity': 'valid', 'proof_check_passed': True,
                     'proof_written_with_fr': True, 'package_check_passed': True},
                 'reported_usage_complete': True, 'client_model_recorded': True,
                 'collection_error_recorded': False, 'checking_input_changes': [], 'suspended_seconds': 0,
                 'argument_repair_observations': {'first_argument_type_error': None, 'next_action': None,
                     'next_proof_check': None}, 'independent_outcome': 'passed'}
                for row in plan.prepare(plan.ROOT, STUDY)['attempts']]

    def observed(self):
        return {'first_argument_type_error': {'seconds': 5., 'diagnostic_sha256': 'a' * 64,
                    'summary': 'An integer was supplied where Lean required a proof of a lower bound.'},
                'next_action': {'seconds': 6., 'kind': 'read-diagnostic', 'summary': 'Read the mapped type context.'},
                'next_proof_check': {'seconds': 8., 'report_sha256': 'b' * 64, 'argument_mismatch_fixed': True,
                    'passed': False, 'theorem_search_before_check': False}}

    def test_fresh_balanced_slots_preserve_the_complete_claim_and_limits(self):
        frozen = plan.prepare(plan.ROOT, STUDY)
        self.assertEqual(len(frozen['attempts']), 8)
        self.assertFalse(frozen['live_calls_enabled'])
        self.assertEqual(frozen['requirements']['lean_imports'], ['Init.Data.Int.DivMod.Lemmas'])
        self.assertEqual(frozen['requirements']['limits'], {**local.LIMITS, 'parallel_attempts': 1})
        self.assertEqual(set(frozen['profiles']), {'opencode2-glm-flash', 'opencode2-kimi-k3'})
        for profile in frozen['profiles']:
            self.assertEqual(frozen['profiles'][profile]['model']['variant'], 'low')
            groups = [[r for r in frozen['attempts'] if r['profile'] == profile and r['repetition'] == n] for n in (1, 2)]
            self.assertNotEqual(groups[0][0]['setup'], groups[1][0]['setup'])
            for group in groups:
                self.assertEqual({r['setup'] for r in group}, {'shared-skill', 'argument-repair'})
                self.assertEqual({r['task'] for r in group}, {'page-count'})
        self.assertEqual(feedback.property_for({'object_digest': 'fixed'}, STUDY),
                         feedback.proof.property_for('page-count', {'object_digest': 'fixed'}))
        with patch.object(feedback.proof, 'prepare', return_value='original') as preparing:
            self.assertEqual(feedback.prepare_inputs('fr', 'project', 'out', STUDY), 'original')
        preparing.assert_called_once_with('page-count', 'fr', 'project', 'out')
        for name in ('cases.json', 'fixtures/src/pages.rs', 'controls/page-count.lean'):
            self.assertIn('tests/agent-eval/proof-writing/' + name, frozen['bindings'])

    def test_identical_prompts_and_one_protected_instruction_difference(self):
        tools = {'lean': {'source_root': '/lean'}, 'lake': {'path': '/lake'}}
        prompts = [local.prompt('page-count', setup, tools, STUDY) for setup in ('shared-skill', 'argument-repair')]
        self.assertEqual(*prompts)
        for phrase in ('invalid-input', 'valid-input', 'eleven items', '.fr/argument-repair.txt', 'Do not start a nested guard'):
            self.assertIn(phrase, prompts[0])
        with tempfile.TemporaryDirectory() as temp:
            for setup in ('shared-skill', 'argument-repair'):
                project = Path(temp) / setup;project.mkdir()
                handout = feedback.install_repair_instruction(project, setup)
                self.assertEqual([p.relative_to(project).as_posix() for p in project.rglob('*') if p.is_file()],
                                 ['.fr/argument-repair.txt'])
                self.assertNotIn('Int.', handout.read_text())
                expected = {'.fr/argument-repair.txt': local.hash_file(handout)}
                self.assertEqual(local.checking_input_changes(project, expected), [])
                handout.write_text('changed')
                self.assertEqual(local.checking_input_changes(project, expected), ['.fr/argument-repair.txt'])
                handout.unlink()
                self.assertEqual(local.checking_input_changes(project, expected), ['.fr/argument-repair.txt'])
            with self.assertRaises(ValueError):
                feedback.install_repair_instruction(Path(temp), 'implicit')

    def test_repaired_argument_does_not_require_whole_proof_success(self):
        observed = self.observed()
        self.assertEqual(feedback.validate_repair_observations(observed, 30), 'met')
        observed['next_proof_check']['theorem_search_before_check'] = True
        self.assertEqual(feedback.validate_repair_observations(observed, 30), 'not-met')
        observed['next_proof_check'] = None
        self.assertEqual(feedback.validate_repair_observations(observed, 30), 'not-met')

    def test_unobserved_mechanism_cannot_be_upgraded_by_an_accepted_proof(self):
        rows = self.rows()
        result = feedback.repair_decision(rows)
        self.assertTrue(result['completion_prediction_met'])
        for profile in result['profiles'].values():
            for condition in profile['mechanism'].values():
                self.assertEqual(condition, {'met': 0, 'not-met': 0, 'unobserved': 2})
        for changed in ({'reported_usage_complete': False}, {'client_model_recorded': False},
                        {'total_seconds': 150.01}, {'checking_input_changes': ['.fr/argument-repair.txt']}):
            invalid = self.rows();invalid[0].update(changed)
            with self.assertRaises(ValueError):
                feedback.repair_decision(invalid)
        for invalid in (rows[:-1], list(reversed(rows))):
            with self.assertRaises(ValueError):
                feedback.repair_decision(invalid)

    def test_chronology_and_private_report_identities_are_required(self):
        for mutate in (lambda r: r['next_proof_check'].update(seconds=4),
                       lambda r: r['first_argument_type_error'].update(diagnostic_sha256='missing'),
                       lambda r: r['next_action'].update(kind='theorem-search'),
                       lambda r: r.update(first_argument_type_error=None)):
            row = copy.deepcopy(self.observed());mutate(row)
            with self.assertRaises(ValueError):
                feedback.validate_repair_observations(row, 30)

    def test_native_qualification_checks_error_repair_save_package_and_delivery(self):
        from agent_eval.algorithm_admission import script
        for condition in ('shared-skill', 'argument-repair'):
            commands = script(Path('/lean'), '/tools:/usr/bin:/bin', STUDY, condition)
            for required in ('draft_location', 'context', '.fr/argument-repair.txt', ' proof check ',
                             ' proof state ', '"acceptance"',
                             ' proof save ', '/lean/bin/lake -d specs build --wfail', 'git diff --binary HEAD'):
                self.assertIn(required, commands)
            self.assertEqual(commands.count(' proof check '), 2)

    def test_native_setup_protects_the_shared_skill_and_keeps_control_drafts_separate(self):
        with tempfile.TemporaryDirectory() as temp:
            project = Path(temp)
            protected = feedback.install_diagnostic_admission(project, 'argument-repair', Path('/lean'),
                                                         {'target': 'specs/Fixed.lean::page_bounds'}, STUDY)
            self.assertIn('.agents/skills/fr/SKILL.md', protected)
            self.assertIn('.fr/argument-repair.txt', protected)
            self.assertIn('.fr/checks.json', protected)
            self.assertFalse((project / '.fr/admission-start.lean').exists())
            from agent_eval.proof_delivery import input_changes
            self.assertEqual(input_changes(project, protected), [])
            (project / 'change.patch').write_bytes(b'nonempty submitted patch\n')
            from agent_eval.proof_delivery import submission
            submitted, error = submission(project, protected)
            self.assertIsNone(error)
            self.assertEqual(submitted, project / 'change.patch')
            handout = project / '.fr/argument-repair.txt'
            handout.write_text('altered\n')
            self.assertEqual(input_changes(project, protected), ['.fr/argument-repair.txt'])
            self.assertIsNone(submission(project, protected)[0])
            self.assertIn('Int.tdiv_nonneg items size', (project / '.fr/admission-bad.lean').read_text())
            self.assertNotIn('Int.tdiv_nonneg items size', (project / '.fr/admission-reference.lean').read_text())
            self.assertEqual((project / '.fr/admission-target.txt').read_text(), 'specs/Fixed.lean::page_bounds')

    def test_frozen_controls_require_the_exact_instruction_and_actual_type_error(self):
        from agent_eval import selected_local_grade as public
        receipt = {'study': STUDY, 'passed': True, 'live_model_calls': 0,
                   'source_commit': 'a' * 40, 'run': '123', 'bindings': {},
                   'unfinished_draft_rejected': True, 'native_exchanges': 4,
                   'controls': {v: {'status': 'passed', 'accepted': v == 'reference'} for v in feedback.VARIANTS},
                   'private': {'image': 'sha256:' + 'b' * 64, 'image_archive_sha256': 'c' * 64},
                   'fr_sha256': 'd' * 64, 'core_checker_sha256': 'checker',
                   'repair_instruction_sha256': local.hash_file(plan.ROOT / plan.CASE_DIRECTORIES[STUDY] /
                       'fixtures/argument-repair.txt'), 'handout_changes_rejected': True,
                   'argument_type_error_rejected': True, 'lean_arguments': ['-j1'],
                   'lean_imports': ['Init.Data.Int.DivMod.Lemmas'], 'run_attempt': 1}
        with patch.object(local, 'implementation_bindings', return_value={}), \
                patch.object(public, 'core_digest', return_value='checker'):
            feedback.validate_controls(receipt, STUDY)
            for change in ({'repair_instruction_sha256': '0' * 64}, {'handout_changes_rejected': False},
                           {'argument_type_error_rejected': False}, {'lean_arguments': []},
                           {'lean_imports': ['Init']}, {'run_attempt': 2}, {'run_attempt': True}):
                with self.assertRaises(ValueError):
                    feedback.validate_controls({**receipt, **change}, STUDY)
            feedback.validate_controls({**receipt, 'run_attempt': 2, 'unexecuted_prior_attempts': 1}, STUDY)
            with self.assertRaises(ValueError):
                feedback.validate_controls({**receipt, 'run_attempt': 2, 'unexecuted_prior_attempts': 0}, STUDY)


if __name__ == '__main__':
    unittest.main()
