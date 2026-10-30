"""Historical rechecks must retain failed attempts and one exact checking environment."""
import copy
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval import proof_writing_results as results
from agent_eval import selected_plan


class ProofOutcomeChecks(unittest.TestCase):
    def test_checker_identity_ignores_reports_but_tracks_actual_checking_inputs(self):
        public = results.public
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            task = root / public.CASE_DIRECTORIES['M3']
            inputs = [root / name for name in public.CORE_FILES]
            inputs += [root / 'sdk/python/src/fr_ir/checker.py',
                       root / 'tests/agent-eval/selected-agents/Grader.Dockerfile',
                       task / 'tasks.json', task / 'checkers.json', task / 'cases.json', task / 'profiles.json',
                       task / 'fixtures/src/program.rs', task / 'controls/reference.lean']
            modules = [root / 'tools/agent_eval' / path.name
                       for path in (public.ROOT / 'tools/agent_eval').glob('*.py')]
            for path in inputs + modules:
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text('original checking input\n')
            with mock.patch.object(public, 'ROOT', root):
                original = public.core_digest('M3')
                for name in ('local-results/summary.json', 'low-reasoning-results/summary.json',
                             'low-reasoning-results/patches/submission.patch', 'low-reasoning-results.sha256',
                             'behavior-review.md'):
                    path = task / name
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_text('retained outcome\n')
                self.assertEqual(public.core_digest('M3'), original)
                for path in inputs:
                    with self.subTest(input=str(path.relative_to(root))):
                        path.write_text('changed checking input\n')
                        self.assertNotEqual(public.core_digest('M3'), original)
                        path.write_text('original checking input\n')

    def test_report_keeps_failures_visible_after_a_contributing_proof_step_and_discloses_sleep(self):
        supplied, _, rows = self.packet()
        for row in rows:
            row.update(independent_outcome='failed', total_seconds=150.0,
                fr_use={'contributing_actions': []}, failure_summary=None, reported_usage_complete=False)
        row = next(row for row in rows if row['setup'] == 'fr')
        row['fr_use']['contributing_actions'] = ['Checked the proposed proof with fr']
        row['failure_summary'] = 'Deleted the supplied check file during cleanup'
        row['total_seconds'] = 142.9
        supplied['collection_notes'] = {'system_sleep': [{'attempt': row['id'], 'suspended_seconds': 42.9,
                                                        'included_in_total_seconds': True}]}
        text = results.markdown(supplied)
        line = next(line for line in text.splitlines() if line.startswith('| ' + row['id'] + ' |'))
        self.assertIn('Checked the proposed proof', line)
        self.assertIn('Deleted the supplied check file', line)
        self.assertIn('| failed |', line)
        self.assertIn('42.9 seconds of detected system sleep in its 142.9-second total', text)

    def test_image_setup_failure_still_records_zero_completed_controls(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            output = root / 'controls'
            with mock.patch.dict('os.environ', {'GITHUB_ACTIONS': 'true', 'GITHUB_RUN_ID': '123'}), \
                    mock.patch.object(results.local, 'identity', return_value={'commit': 'a' * 40}), \
                    mock.patch.object(results.local, 'implementation_bindings', return_value={'checker': 'b' * 64}), \
                    mock.patch.object(results.common, 'digest', return_value='c' * 64), \
                    mock.patch.object(results.subprocess, 'check_output', return_value='/rust\n'), \
                    mock.patch.object(results.grading, 'build', side_effect=TimeoutError('image setup stopped')), \
                    self.assertRaises(TimeoutError):
                results.controls(root / 'inputs', root / 'fr', output)
            summary = json.loads((output / 'summary.json').read_text())
            self.assertFalse(summary['passed'])
            self.assertIsNone(summary['image'])
            self.assertEqual(summary['checked_controls'], 0)
            self.assertEqual(summary['controls'], {})

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
