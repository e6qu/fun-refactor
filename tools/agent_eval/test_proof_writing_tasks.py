"""Keep the proof comparison complete, unaided and separate from model claims."""
import copy
import json
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval import proof_writing_tasks as tasks
from agent_eval import selected_local, selected_plan


class ProofWritingTasks(unittest.TestCase):
    def test_every_profile_has_both_setups_and_two_repetitions(self):
        plan = selected_plan.prepare(selected_plan.ROOT, 'M3')
        self.assertEqual(len(plan['attempts']), 36)
        self.assertFalse(plan['live_calls_enabled'])
        for profile in plan['profiles']:
            for case in ('access-rust', 'page-count', 'fits-window'):
                rows = [row for row in plan['attempts'] if row['profile'] == profile and row['task'] == case]
                self.assertEqual({(row['setup'], row['repetition']) for row in rows},
                                 {('files', 1), ('files', 2), ('fr', 1), ('fr', 2)})

    def test_prompts_keep_proofs_and_claims_separate_without_supplied_tactics(self):
        requirements = json.loads((tasks.ROOT / tasks.DIRECTORY / 'tasks.json').read_text())
        for case in requirements['tasks']:
            for setup in ('files', 'fr'):
                prompt = selected_local.prompt(case, setup, {
                    'lake': {'path': '/pinned/lake'},
                    'lean': {'path': '/pinned/Lean tools/bin/lean'},
                }, 'M3')
                self.assertIn(requirements['tasks'][case]['request'], prompt)
                self.assertIn('150-second', prompt)
                self.assertIn('passed=false', prompt)
                self.assertIn('Mathlib', prompt)
                self.assertIn("'/pinned/Lean tools/bin/lean' --print-prefix", prompt)
                self.assertNotIn('rfl', prompt)
                self.assertNotIn('omega', prompt)
                self.assertNotIn('Int.tdiv_nonneg', prompt)
                self.assertIn('Preserve provided .fr/checks.json', prompt)

    def test_fixed_properties_fit_the_disclosed_grammar(self):
        def nodes(value):
            if isinstance(value, dict):
                return int('kind' in value) + sum(nodes(child) for child in value.values())
            if isinstance(value, list):
                return sum(nodes(child) for child in value)
            return 0
        for case in tasks.CASES:
            claim = tasks.property_for(case, {'object_digest': '0' * 64})
            self.assertLessEqual(nodes(claim['proposition']), 64)
            self.assertEqual(claim['task_digest'], '0' * 64)
        self.assertEqual((tasks.ROOT / tasks.DIRECTORY / 'fixtures/src/pages.rs').read_bytes(),
                         (tasks.ROOT / 'tests/agent-eval/checked-translation/pages.rs').read_bytes())

    def test_failed_missing_or_assisted_controls_cannot_enable_calls(self):
        value = {'passed': True, 'live_model_calls': 0, 'source_commit': 'a' * 40, 'run': '123',
            'bindings': {'source': 'b' * 64},
            'cases': {name: {**{flag: True for flag in tasks.CONTROL_FLAGS}, 'source_implementation_proved': False}
                      for name in tasks.CASES},
            'native': {'passed': True, 'live_model_calls': 0, 'exchanges': 6},
            'private': {'passed': True, 'live_model_calls': 0, 'checked_controls': 18,
                        'image': 'sha256:' + 'a' * 64, 'image_archive_sha256': 'b' * 64}}
        self.assertEqual(tasks.validate_controls(value), value)
        changes = [lambda row: row.update(live_model_calls=1),
                   lambda row: row['cases'].pop('access-tsx'),
                   lambda row: row['cases']['page-count'].update(model_proof_checked=False),
                   lambda row: row['cases']['fits-window'].update(source_implementation_proved=True),
                   lambda row: row['native'].update(exchanges=5),
                   lambda row: row['private'].update(checked_controls=17),
                   lambda row: row['private'].update(image='mutable-image-tag')]
        for change in changes:
            invalid = copy.deepcopy(value)
            change(invalid)
            with self.assertRaises(ValueError):
                tasks.validate_controls(invalid)


if __name__ == '__main__':
    unittest.main()
