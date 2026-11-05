"""No model requests: check the one changed input, protected handout and decisions."""
import copy
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval import proof_feedback as feedback, selected_local as local, selected_plan as plan


class DivisionFactsTests(unittest.TestCase):
    def rows(self, supplied, selected):
        counts = {'supplied-declarations': supplied, 'select-facts': selected}
        return [{**row, 'native_status': 'completed', 'stop_reason': None, 'total_seconds': 20.0,
                 'patch_file': 'patches/' + row['id'] + '.patch',
                 'complete_attempt_deadline_missed': False, 'independent_check': {'patch_accepted': True},
                 'proof_observations': {'execution_validity': 'valid', 'proof_check_passed': True,
                                        'proof_written_with_fr': True, 'package_check_passed': True},
                 'collection_error_recorded': False, 'checking_input_changes': [], 'suspended_seconds': 0,
                 'independent_outcome': 'passed' if row['repetition'] <= counts[row['setup']] else 'failed'}
                for row in plan.prepare(plan.ROOT, 'M3-facts')['attempts']]

    def test_eight_balanced_slots_keep_the_original_limits_and_low_profiles(self):
        frozen = plan.prepare(plan.ROOT, 'M3-facts')
        self.assertFalse(frozen['live_calls_enabled'])
        self.assertEqual(len(frozen['attempts']), 8)
        self.assertEqual(frozen['requirements']['limits'], {**local.LIMITS, 'parallel_attempts': 1})
        for profile, settings in frozen['profiles'].items():
            self.assertEqual(settings['model']['variant'], 'low')
            first = []
            for repetition in (1, 2):
                rows = [r for r in frozen['attempts'] if r['profile'] == profile and r['repetition'] == repetition]
                self.assertEqual({r['setup'] for r in rows}, {'select-facts', 'supplied-declarations'})
                self.assertEqual({r['task'] for r in rows}, {'division-facts'})
                first.append(rows[0]['setup'])
            self.assertNotEqual(*first)
        directory = 'tests/agent-eval/proof-division-facts/'
        for name in ['fixtures/declarations.txt', 'fixtures/parts.rs', 'controls/reference.lean', 'checkers.json']:
            self.assertIn(directory + name, frozen['bindings'])

    def test_both_conditions_have_the_same_complete_prompt_and_fr_access(self):
        tools = {'lean': {'source_root': '/pinned/lean'}, 'lake': {'path': '/pinned/lake'}}
        selected = local.prompt('division-facts', 'select-facts', tools, 'M3-facts')
        supplied = local.prompt('division-facts', 'supplied-declarations', tools, 'M3-facts')
        self.assertEqual(selected, supplied)
        for required in ['.fr/declarations.txt', 'Fr and its project skill are installed',
                         'Preserve provided .fr/checks.json', 'Do not start a nested guard',
                         'Run the supplied checks once', 'call another model']:
            self.assertIn(required, supplied)
        for hidden in ['Int.tdiv_nonneg', 'Int.tmod_add_mul_tdiv', 'controls/reference.lean', 'Fr is not provided']:
            self.assertNotIn(hidden, supplied)

    def test_fixed_claim_has_the_model_bounded_above_by_the_input(self):
        claim = feedback.property_for({'object_digest': 'fixed'}, 'M3-facts')
        self.assertEqual(claim['name'], 'at_most_input')
        self.assertEqual(claim['proposition']['conclusion'], {'kind': 'less-or-equal',
            'left': feedback.proof.model(feedback.proof.variable('a'), feedback.proof.variable('b')),
            'right': feedback.proof.variable('a')})
        self.assertEqual([p['left']['value'] for p in claim['proposition']['premise']['propositions']], [0, 1])
        self.assertIsNone(feedback.case_details('M3-facts')['draft'])

    def test_handout_is_the_only_different_input_and_cannot_be_overwritten(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for condition in ['select-facts', 'supplied-declarations']:
                project = root / condition;project.mkdir()
                handout = feedback.install_declarations(project, condition)
                expected = ((plan.ROOT / plan.CASE_DIRECTORIES['M3-facts'] / 'fixtures/declarations.txt').read_text()
                            if condition == 'supplied-declarations' else
                            'Select the needed current declarations from the installed Lean standard library.\n')
                self.assertEqual(handout.read_text(), expected)
                self.assertEqual([p.relative_to(project).as_posix() for p in project.rglob('*') if p.is_file()],
                                 ['.fr/declarations.txt'])
                protected = {'.fr/declarations.txt': local.hash_file(handout)}
                self.assertEqual(local.checking_input_changes(project, protected), [])
                with self.assertRaises(ValueError):
                    feedback.install_declarations(project, condition)
                handout.write_text('altered\n')
                self.assertEqual(local.checking_input_changes(project, protected), ['.fr/declarations.txt'])
                handout.unlink()
                self.assertEqual(local.checking_input_changes(project, protected), ['.fr/declarations.txt'])
            with self.assertRaises(ValueError):
                feedback.install_declarations(root, 'unknown-condition')

    def test_qualification_rejects_even_a_successful_probe_with_wrong_types(self):
        expected = (plan.ROOT / plan.CASE_DIRECTORIES['M3-facts'] / 'fixtures/declarations.txt').read_text()
        for stdout, accepted in [(expected, True), (expected.replace('0 ≤ a.tdiv b', 'a.tdiv b ≤ 0'), False)]:
            with tempfile.TemporaryDirectory() as temporary, patch.object(feedback.common, 'command',
                    return_value={'returncode': 0, 'stdout': stdout, 'stderr': ''}):
                if accepted:
                    self.assertEqual(feedback.qualify_declarations(Path(temporary)),
                                     local.hash_file(plan.ROOT / plan.CASE_DIRECTORIES['M3-facts'] / 'fixtures/declarations.txt'))
                else:
                    with self.assertRaisesRegex(ValueError, 'match the pinned Lean declarations'):
                        feedback.qualify_declarations(Path(temporary))

    def test_every_decision_including_harm_and_mixed_results_is_fixed(self):
        rules = plan.prepare(plan.ROOT, 'M3-facts')['requirements']['decisions']
        for supplied in range(3):
            for selected in range(3):
                decision = feedback.decisions(self.rows(supplied, selected), 'M3-facts')
                for row in decision.values():
                    key = str(supplied) + ',' + str(selected)
                    self.assertEqual(row['decision'], rules[key] if key in rules else rules['mixed'])

    def test_freeze_requires_the_exact_qualified_types_and_handout_checks(self):
        from agent_eval import selected_local_grade as public
        receipt = {'study': 'M3-facts', 'passed': True, 'live_model_calls': 0,
                   'source_commit': 'a' * 40, 'run': '123', 'bindings': {},
                   'unfinished_draft_rejected': True, 'native_exchanges': 4,
                   'controls': {v: {'status': 'passed', 'accepted': v == 'reference'} for v in feedback.VARIANTS},
                   'private': {'image': 'sha256:' + 'b' * 64, 'image_archive_sha256': 'c' * 64},
                   'fr_sha256': 'd' * 64, 'core_checker_sha256': 'checker',
                   'declarations_sha256': local.hash_file(plan.ROOT / plan.CASE_DIRECTORIES['M3-facts'] / 'fixtures/declarations.txt'),
                   'handout_changes_rejected': True, 'lean_arguments': ['-j1'],
                   'lean_imports': ['Init.Data.Int.DivMod.Lemmas'], 'run_attempt': 1}
        with patch.object(local, 'implementation_bindings', return_value={}), \
                patch.object(public, 'core_digest', return_value='checker'):
            feedback.validate_controls(receipt, 'M3-facts')
            for mutation in [{'declarations_sha256': '0' * 64}, {'handout_changes_rejected': False},
                             {'lean_arguments': []}, {'lean_imports': ['Init']}, {'run_attempt': 2}, {'run_attempt': True}]:
                with self.assertRaises(ValueError):
                    feedback.validate_controls({**receipt, **mutation}, 'M3-facts')
            missing = dict(receipt);missing.pop('declarations_sha256')
            with self.assertRaisesRegex(ValueError, 'exact declared fields'):
                feedback.validate_controls(missing, 'M3-facts')

    def test_complete_native_setup_keeps_reference_tactics_out_of_live_inputs(self):
        from agent_eval.algorithm_admission import script
        from agent_eval.proof_delivery import input_changes
        self.assertIn('M3-facts', plan.ADMISSION_STUDIES)
        for setup in ('select-facts', 'supplied-declarations'):
            commands = script(Path('/lean'), '/tools:/usr/bin:/bin', 'M3-facts', setup)
            for required in (' proof start ', ' proof state ', ' proof check ', ' proof save ',
                             '.fr/declarations.txt', '"context"', '"goal"', 'unsolved goals',
                             '/lean/bin/lake -d specs build --wfail', 'git diff --binary HEAD'):
                self.assertIn(required, commands)
            with tempfile.TemporaryDirectory() as temporary:
                project = Path(temporary)
                protected = feedback.install_diagnostic_admission(project, setup, Path('/lean'),
                    {'target': 'specs/Parts.lean::at_most_input'}, 'M3-facts')
                self.assertFalse((project / '.fr/admission-start.lean').exists())
                self.assertEqual(input_changes(project, protected), [])
                self.assertIn('.fr/declarations.txt', protected)
                self.assertIn('.agents/skills/fr/SKILL.md', protected)
                (project / '.fr/declarations.txt').write_text('changed')
                self.assertEqual(input_changes(project, protected), ['.fr/declarations.txt'])
                self.assertNotIn('.fr/admission-reference.lean', protected)

    def test_late_or_unobserved_success_is_refused_and_bad_execution_blocks_inference(self):
        for change in [{'total_seconds': 150.01}, {'checking_input_changes': ['.fr/declarations.txt']},
                       {'stop_reason': 'wall_seconds'}, {'native_status': 'failed'}]:
            rows = self.rows(2, 2);rows[0].update(change)
            with self.assertRaisesRegex(ValueError, 'remains failed'):
                feedback.decisions(rows, 'M3-facts')
        rows = self.rows(2, 2);rows[0]['proof_observations']['package_check_passed'] = None
        with self.assertRaisesRegex(ValueError, 'unobserved construction checks'):
            feedback.decisions(rows, 'M3-facts')
        rows = self.rows(0, 0);rows[0]['stop_reason'] = 'rss_bytes'
        decision = feedback.decisions(rows, 'M3-facts')
        self.assertIn('Repair or clarify experiment execution', decision[rows[0]['profile']]['decision'])

    def test_missing_reordered_or_relabeled_results_cannot_hide_an_attempt(self):
        rows = self.rows(1, 1)
        changed = copy.deepcopy(rows);changed[0]['setup'] = 'supplied-declarations'
        for invalid in [rows[:-1], list(reversed(rows)), changed]:
            with self.assertRaises(ValueError):
                feedback.decisions(invalid, 'M3-facts')
        report = feedback.markdown({'study': 'M3-facts', 'attempts': rows})
        self.assertIn('neither supplies a proof', report)
        self.assertIn('cannot establish full page-count success', report)
        self.assertNotIn('initial visibility', report)


if __name__ == '__main__':
    unittest.main()
