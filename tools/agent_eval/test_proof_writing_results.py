"""Historical rechecks must retain failed attempts and one exact checking environment."""
import copy
import hashlib
from pathlib import Path
import sys
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval import proof_writing_results as results
from agent_eval import selected_plan


class ProofOutcomeChecks(unittest.TestCase):
    def packet(self):
        schedule = selected_plan.prepare(selected_plan.ROOT, 'M3')
        rows = [{**row, 'native_status': 'failed' if index == 0 else 'completed', 'patch_sha256': 'd' * 64}
                for index, row in enumerate(schedule['attempts'])]
        supplied = {'attempts': rows, 'core_checker_sha256': 'b' * 64,
                    'controls': {'source_commit': 'a' * 40, 'run': '123', 'private': {'image': 'sha256:' + 'c' * 64}}}
        cells = []
        digest = hashlib.sha256(results.encoded_summary(supplied)).hexdigest()
        for profile in results.report.AGENTS:
            for task in results.TASKS:
                selected = [row for row in rows if row['profile'] == profile and row['task'] == task]
                cells.append({'passed': True, 'study': 'M3', 'profile': profile, 'task': task, 'live_model_calls': 0,
                    'image': 'sha256:' + 'e' * 64, 'control_policy': 'fresh', 'control_run': '456',
                    'control_source_commit': 'a' * 40, 'core_checker_sha256': 'b' * 64, 'summary_sha256': digest,
                    'attempts': {row['id']: {'status': 'checked', 'native_status': row['native_status'],
                        'patch_sha256': row['patch_sha256'], 'patch_accepted': True,
                        'outcome': 'failed' if row['native_status'] == 'failed' else 'passed'} for row in selected}})
        return supplied, cells, rows

    def test_fresh_audit_can_check_original_patches_without_upgrading_a_failed_attempt(self):
        supplied, cells, rows = self.packet()
        with mock.patch.object(results.public, 'verify', return_value=rows):
            checked = results.merge(supplied, cells)
        self.assertTrue(checked['all_36_checked'])
        self.assertEqual(checked['control_policy'], 'fresh')
        self.assertEqual(checked['attempts'][rows[0]['id']]['outcome'], 'failed')
        self.assertNotEqual(checked['image'], supplied['controls']['private']['image'])

    def test_mixed_images_missing_cells_and_changed_failure_outcomes_are_rejected(self):
        supplied, cells, rows = self.packet()
        mutations = [lambda value: value.pop(),
                     lambda value: value[0].update(image='sha256:' + 'f' * 64),
                     lambda value: value[0].update(control_source_commit='f' * 40),
                     lambda value: value[0]['attempts'][rows[0]['id']].update(outcome='passed')]
        for mutate in mutations:
            changed = copy.deepcopy(cells)
            mutate(changed)
            with mock.patch.object(results.public, 'verify', return_value=rows), self.assertRaises(ValueError):
                results.merge(supplied, changed)

    def test_initial_grading_cannot_substitute_a_fresh_image(self):
        supplied, cells, rows = self.packet()
        for cell in cells:
            cell.update(control_policy='frozen', control_run='123')
        with mock.patch.object(results.public, 'verify', return_value=rows), self.assertRaises(ValueError):
            results.merge(supplied, cells)


if __name__ == '__main__':
    unittest.main()
