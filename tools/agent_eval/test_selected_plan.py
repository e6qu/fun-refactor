"""Check M1 pairing, unchanged repetitions, explicit settings and honest unknowns."""
import copy
from collections import defaultdict
import json
from pathlib import Path
import shutil
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval import selected_plan as plan


class SelectedPlanTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        shutil.copytree(plan.ROOT / plan.DIRECTORY, self.root / plan.DIRECTORY)

    def test_all_profiles_tasks_setups_and_repetitions_are_matched(self):
        frozen = plan.prepare(self.root, 'M1')
        pairs = defaultdict(list)
        for row in frozen['attempts']:
            pairs[row['profile'], row['task'], row['repetition']].append(row)
        self.assertEqual(len(pairs), 18)
        for rows in pairs.values():
            self.assertEqual({row['setup'] for row in rows}, {'files', 'fr'})
            self.assertEqual([row['position'] for row in rows], [1, 2])
        for profile, task, _ in pairs:
            first = pairs[profile, task, 1]
            second = pairs[profile, task, 2]
            self.assertNotEqual(first[0]['setup'], second[0]['setup'])

    def test_unstarted_work_has_no_success_usage_cost_or_fr_contribution(self):
        frozen = plan.prepare(self.root, 'M1')
        self.assertFalse(frozen['live_calls_enabled'])
        self.assertEqual(frozen['billing'], {'actual': None, 'estimated': None, 'authorization': None})
        for row in frozen['attempts']:
            self.assertEqual(row['status'], 'unstarted')
            for name in ('outcome', 'usage', 'cost', 'fr_contribution'):
                self.assertIsNone(row[name])

    def test_forged_success_dropped_failures_and_partial_schedules_refuse(self):
        frozen = plan.prepare(self.root, 'M1')
        changes = [lambda value: value['attempts'][0].update(status='completed', outcome='passed'),
                   lambda value: value['attempts'].pop(),
                   lambda value: value.update(live_calls_enabled=True),
                   lambda value: value['billing'].update(actual=0)]
        for change in changes:
            altered = copy.deepcopy(frozen)
            change(altered)
            with self.assertRaisesRegex(ValueError, 'differs'):
                plan.verify(altered, self.root, 'M1')

    def test_changed_requirements_invalidate_the_plan(self):
        frozen = plan.prepare(self.root, 'M1')
        path = self.root / plan.DIRECTORY / 'tasks.json'
        value = json.loads(path.read_text())
        value['tasks']['access-proof']['request'] += ' Weaken the requested property.'
        path.write_text(json.dumps(value))
        with self.assertRaisesRegex(ValueError, 'differs'):
            plan.verify(frozen, self.root, 'M1')

    def test_wrong_model_reasoning_or_speed_refuse_before_execution(self):
        path = self.root / plan.DIRECTORY / 'profiles.json'
        original = json.loads(path.read_text())
        for field, wrong in [('model', 'gpt-6-astra'), ('reasoning_effort', 'low'), ('service_tier', 'default')]:
            value = copy.deepcopy(original)
            value['codex-luna-fast'][field] = wrong
            path.write_text(json.dumps(value))
            with self.assertRaisesRegex(ValueError, 'model, effort and speed'):
                plan.prepare(self.root, 'M1')

    def test_m2_retains_each_of_48_slots_and_binds_fixture_changes(self):
        directory = plan.CASE_DIRECTORIES['M2']
        shutil.copytree(plan.ROOT / directory, self.root / directory)
        frozen = plan.prepare(self.root, 'M2')
        self.assertEqual(len(frozen['attempts']), 48)
        self.assertEqual(frozen['milestone'], 'M2')
        self.assertEqual(frozen['requirements']['limits']['wall_seconds'], 150)
        self.assertEqual({row['status'] for row in frozen['attempts']}, {'unstarted'})
        fixture = self.root / directory / 'fixtures/python-invoice/invoice.py'
        fixture.write_bytes(fixture.read_bytes() + b'\n')
        with self.assertRaisesRegex(ValueError, 'differs'):
            plan.verify(frozen, self.root, 'M2')
        with self.assertRaisesRegex(ValueError, 'explicitly'):
            plan.prepare(self.root, 'latest')


if __name__ == '__main__':
    unittest.main()
