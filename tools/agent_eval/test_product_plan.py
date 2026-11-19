"""Reject changed limits, source substitutions, missing slots and premature validation."""
import copy
from collections import defaultdict
import json
from pathlib import Path
import shutil
import tempfile
import unittest

from . import product_plan as product, selected_plan as selected


class ProductPlanTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        for relative in (selected.DIRECTORY, product.DIRECTORY):
            shutil.copytree(selected.ROOT / relative, self.root / relative)

    def edit(self, relative, change):
        path = self.root / relative
        value = json.loads(path.read_text())
        change(value)
        path.write_text(json.dumps(value))

    def test_keep_all_96_slots_before_validation_sources_are_opened(self):
        campaign = product.campaign(self.root)
        self.assertFalse(campaign['live_calls_enabled'])
        self.assertEqual(len(campaign['attempts']), 96)
        self.assertEqual(len({row['id'] for row in campaign['attempts']}), 96)
        self.assertFalse((self.root / product.DIRECTORY / 'validation/tasks.json').exists())
        pairs = defaultdict(list)
        for row in campaign['attempts']:
            pairs[row['round'], row['profile'], row['task'], row['repetition']].append(row)
            self.assertEqual(row['status'], 'unstarted')
            for name in ('outcome', 'usage', 'cost', 'fr_contribution'):
                self.assertIsNone(row[name])
        self.assertEqual(len(pairs), 48)
        for (phase, profile, task, repeat), rows in pairs.items():
            self.assertEqual({row['setup'] for row in rows}, {'files', 'fr'})
            self.assertEqual([row['position'] for row in rows], [1, 2])
            if repeat == 1:
                self.assertNotEqual(rows[0]['setup'], pairs[phase, profile, task, 2][0]['setup'])

    def test_initial_plan_binds_requests_and_the_fixed_proof(self):
        planned = selected.prepare(self.root, 'P5-initial')
        self.assertEqual(len(planned['attempts']), 48)
        self.assertFalse(planned['live_calls_enabled'])
        self.assertEqual(planned['billing']['authorization']['maximum_attempts'], 96)
        self.assertIsNone(planned['billing']['actual'])
        fixture = product.DIRECTORY / 'initial/fixtures/Clamp.lean'
        self.assertIn(str(fixture), planned['bindings'])
        with (self.root / fixture).open('a') as stream:
            stream.write('\n-- changed proof input\n')
        with self.assertRaisesRegex(ValueError, 'differs'):
            selected.verify(planned, self.root, 'P5-initial')

    def test_source_substitution_refuses(self):
        self.edit(product.DIRECTORY / 'initial/tasks.json', lambda value:
                  value['tasks']['utils-rename'].update(revision='0' * 40))
        with self.assertRaisesRegex(ValueError, 'reserved'):
            selected.prepare(self.root, 'P5-initial')

    def test_missing_or_reordered_task_refuses(self):
        self.edit(product.DIRECTORY / 'initial/tasks.json', lambda value: value['tasks'].pop('sqlparse-graph'))
        with self.assertRaisesRegex(ValueError, 'four tasks'):
            selected.prepare(self.root, 'P5-initial')

    def test_limits_retries_and_false_numeric_values_cannot_expand_the_study(self):
        path = self.root / product.DIRECTORY / 'protocol.json'
        original = json.loads(path.read_text())
        changes = [lambda value: value['limits'].update(wall_seconds=300),
                   lambda value: value['limits'].update(parallel_attempts=True),
                   lambda value: value.update(retry_failed_slots=True),
                   lambda value: value.update(maximum_live_attempts=192),
                   lambda value: value['workstation'].update(free_disk_bytes=32 * 1024**3)]
        for change in changes:
            altered = copy.deepcopy(original)
            change(altered)
            path.write_text(json.dumps(altered))
            with self.assertRaises(ValueError):
                product.campaign(self.root)

    def test_validation_refuses_before_opening_its_task_definitions(self):
        path = self.root / product.DIRECTORY / 'validation/tasks.json'
        path.parent.mkdir()
        path.write_text('This must not be parsed before the release.')
        with self.assertRaisesRegex(ValueError, 'stays reserved'):
            selected.prepare(self.root, 'P5-validation')

    def test_forged_completion_or_deleted_attempt_invalidates_frozen_plan(self):
        original = selected.prepare(self.root, 'P5-initial')
        for change in (lambda value: value['attempts'].pop(),
                       lambda value: value.update(live_calls_enabled=True),
                       lambda value: value['attempts'][0].update(status='completed', outcome='passed')):
            altered = copy.deepcopy(original)
            change(altered)
            with self.assertRaisesRegex(ValueError, 'differs'):
                selected.verify(altered, self.root, 'P5-initial')


if __name__ == '__main__':
    unittest.main()
