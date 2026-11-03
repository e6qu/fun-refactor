"""Do not credit a proved algorithm until its complete, unchanged patch arrives."""
import copy
import json
from pathlib import Path
import tempfile
import unittest

from . import proof_delivery as delivery, selected_tasks as common
from .selected_plan import ROOT


class DeliveryChecks(unittest.TestCase):
    def test_all_twelve_slots_stay_unstarted_and_order_is_balanced(self):
        plan = delivery.protocol()
        self.assertFalse(plan['live_calls_enabled'])
        self.assertEqual(len(plan['attempts']), 12)
        self.assertEqual(len({row['id'] for row in plan['attempts']}), 12)
        for profile in plan['profiles']:
            rows = [row for row in plan['attempts'] if row['profile'] == profile]
            self.assertEqual([row['setup'] for row in rows[:2]], [row['setup'] for row in rows[2:]][::-1])
            self.assertTrue(all(row['status'] == 'unstarted' and row['outcome'] is None for row in rows))
        for name, digest in plan['bindings'].items():
            self.assertEqual(common.digest(ROOT / name), digest)
        self.assertEqual(delivery.protocol()['sha256'], plan['sha256'])
        self.assertNotIn('finish-card.txt', plan['requests']['shared-skill'])
        self.assertIn('Read .fr/finish-card.txt before acting.', plan['requests']['finish-card'])

    def test_conditions_supply_identical_answers_settings_and_skills(self):
        with tempfile.TemporaryDirectory() as temporary:
            roots = [Path(temporary) / name for name in delivery.SETUPS]
            manifests = []
            for root, setup in zip(roots, delivery.SETUPS):
                root.mkdir()
                manifests.append(delivery.install(root, setup, Path('/opt/lean/bin/lake')))
            common_inputs = set(manifests[0])
            self.assertEqual(set(manifests[1]) - common_inputs, {'.fr/finish-card.txt'})
            for name in common_inputs:
                self.assertEqual(manifests[0][name], manifests[1][name])
                self.assertEqual((roots[0] / name).read_bytes(), (roots[1] / name).read_bytes())
            checks = json.loads((roots[0] / '.fr/checks.json').read_text())['checks']
            self.assertEqual(checks[0]['argv'], ['/opt/lean/bin/lake', '-d', 'specs', 'build', '--wfail'])
            self.assertEqual(checks[0]['timeout_seconds'], 120)

    def test_only_the_regular_root_patch_can_be_delivered(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            protected = delivery.install(root, 'shared-skill', Path('/opt/lean/bin/lake'))
            nested = root / '.fr/change.patch'
            nested.write_text('a misplaced valid-looking patch')
            self.assertIsNone(delivery.submission(root, protected)[0])
            patch = root / 'change.patch'
            patch.symlink_to(nested)
            self.assertIsNone(delivery.submission(root, protected)[0])
            patch.unlink()
            patch.write_bytes(b'')
            self.assertIsNone(delivery.submission(root, protected)[0])
            patch.write_bytes(b'x' * (1024**2 + 1))
            self.assertIsNone(delivery.submission(root, protected)[0])
            patch.write_text('independent grader must check this content')
            self.assertEqual(delivery.submission(root, protected), (patch, None))

    def test_modified_missing_and_symlinked_protected_inputs_fail(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            protected = delivery.install(root, 'finish-card', Path('/opt/lean/bin/lake'))
            (root / 'change.patch').write_text('patch')
            for name in protected:
                path = root / name
                original = path.read_bytes()
                path.write_bytes(original + b'\n')
                self.assertIsNone(delivery.submission(root, protected)[0], name)
                path.unlink()
                self.assertIsNone(delivery.submission(root, protected)[0], name)
                elsewhere = root / 'elsewhere'
                elsewhere.write_bytes(original)
                path.symlink_to(elsewhere)
                self.assertIsNone(delivery.submission(root, protected)[0], name)
                path.unlink()
                path.write_bytes(original)
                self.assertIsNotNone(delivery.submission(root, protected)[0], name)
                path.chmod(0o755)
                self.assertIsNone(delivery.submission(root, protected)[0], name)
                path.chmod(protected[name]['mode'])

    def test_symlinked_parent_cannot_hide_unchanged_protected_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            protected = delivery.install(root, 'shared-skill', Path('/opt/lean/bin/lake'))
            (root / 'change.patch').write_text('patch')
            (root / '.fr').rename(root / 'elsewhere')
            (root / '.fr').symlink_to(root / 'elsewhere', target_is_directory=True)
            self.assertIsNone(delivery.submission(root, protected)[0])

    def test_no_installation_through_a_symlink(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'elsewhere').mkdir()
            (root / '.fr').symlink_to(root / 'elsewhere', target_is_directory=True)
            with self.assertRaises(ValueError):
                delivery.install(root, 'shared-skill', Path('/opt/lean/bin/lake'))
            self.assertEqual(list((root / 'elsewhere').iterdir()), [])

    def test_valid_patch_does_not_rescue_late_or_incomplete_session(self):
        controls = delivery.timing_controls()
        self.assertTrue(controls['on-time']['completion_accepted'])
        self.assertTrue(all(row['synthetic_observation'] for row in controls.values()))
        self.assertTrue(all(not row['completion_accepted'] for name, row in controls.items() if name != 'on-time'))

    def resources(self):
        return {'exit_code': 0, 'stop_reason': None, 'monitor_error': None, 'launch_error': None,
                'elapsed_seconds': 150, 'sampled_aggregate_rss_bytes': delivery.LIMITS['rss_bytes'],
                'sampled_cpu_seconds': 75, 'sampled_disk_growth_bytes': delivery.LIMITS['disk_bytes'],
                'transcript_bytes': delivery.LIMITS['transcript_bytes']}

    def test_exact_limits_pass_but_each_exceeded_limit_fails(self):
        record = self.resources()
        self.assertTrue(delivery.completion(record, 150, True))
        for key in ('elapsed_seconds', 'sampled_aggregate_rss_bytes', 'sampled_cpu_seconds',
                    'sampled_disk_growth_bytes', 'transcript_bytes'):
            changed = {**record, key: record[key] + 1}
            self.assertFalse(delivery.completion(changed, 149, True), key)

    def test_missing_invalid_or_boolean_resource_measurements_fail(self):
        record = self.resources()
        for key in record:
            changed = copy.deepcopy(record)
            del changed[key]
            self.assertFalse(delivery.completion(changed, 149, True), key)
        for key in ('elapsed_seconds', 'sampled_aggregate_rss_bytes', 'sampled_cpu_seconds',
                    'sampled_disk_growth_bytes', 'transcript_bytes'):
            for value in (None, True, -1, 10**500, float('nan'), float('inf'), '1'):
                self.assertFalse(delivery.completion({**record, key: value}, 149, True), (key, value))

    def test_patch_observation_must_be_real_and_within_the_completed_attempt(self):
        for value in (None, True, -1, 151, 10**500, float('nan'), float('inf'), '149'):
            self.assertFalse(delivery.completion(self.resources(), value, True), value)
        self.assertFalse(delivery.completion(self.resources(), 149, 1))
        for field in ('stop_reason', 'monitor_error', 'launch_error'):
            self.assertFalse(delivery.completion({**self.resources(), field: 'failed'}, 149, True), field)
        self.assertFalse(delivery.completion({**self.resources(), 'exit_code': False}, 149, True))


if __name__ == '__main__':
    unittest.main()
