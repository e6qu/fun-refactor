"""Reject incomplete checking and false M2 completion claims before live calls."""
import copy
import hashlib
from pathlib import Path
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval import project_edits_results as results
from agent_eval import selected_local_grade as public
from agent_eval.selected_plan import ROOT, encoded, prepare


class ProjectEditResults(unittest.TestCase):
    def setUp(self):
        plan = prepare(ROOT, 'M2')
        self.rows = [{**row, 'native_status': 'completed', 'patch_sha256': row['id'],
            'independent_outcome': 'passed', 'fr_use': {'contributing_actions': ['Reviewed saved edit and applied workflow']}}
            for row in plan['attempts']]
        self.summary = {'core_checker_sha256': 'checker', 'controls': {'private': {'image': 'image', 'cache_archive_sha256': 'cache'}},
                        'attempts': self.rows}
        digest = hashlib.sha256(results.encoded_summary(self.summary)).hexdigest()
        self.cells = [{'passed': True, 'study': 'M2', 'live_model_calls': 0, 'profile': profile, 'task': task,
            'core_checker_sha256': 'checker', 'image': 'image', 'control_policy': 'frozen',
            'cache_archive_sha256': 'cache', 'summary_sha256': digest,
            'attempts': {row['id']: {'status': 'checked', 'native_status': 'completed', 'outcome': 'passed',
                                    'patch_sha256': row['patch_sha256'], 'patch_accepted': True}
                         for row in self.rows if row['profile'] == profile and row['task'] == task}}
            for profile in plan['profiles'] for task in plan['requirements']['tasks']]

    def merge(self, cells):
        with patch.object(public, 'verify', return_value=self.rows):
            return results.merge(self.summary, cells)

    def test_all_48_checked_slots_include_both_setups_and_repetitions(self):
        checked = self.merge(self.cells)
        self.assertEqual(checked['checked_attempts'], 48)
        self.assertTrue(checked['all_48_checked'])
        for altered in (self.cells[:-1], self.cells[:-1] + [self.cells[0]]):
            with self.assertRaisesRegex(ValueError, 'twelve distinct'):
                self.merge(altered)
        altered = copy.deepcopy(self.cells)
        altered[0]['attempts'].pop(next(iter(altered[0]['attempts'])))
        with self.assertRaisesRegex(ValueError, 'omits'):
            self.merge(altered)

    def test_checks_cannot_substitute_the_patch_or_summary(self):
        altered = copy.deepcopy(self.cells)
        altered[0]['summary_sha256'] = 'another-summary'
        with self.assertRaisesRegex(ValueError, 'submitted summary'):
            self.merge(altered)
        altered = copy.deepcopy(self.cells)
        next(iter(altered[0]['attempts'].values()))['patch_sha256'] = 'another-patch'
        with self.assertRaisesRegex(ValueError, 'independent patch'):
            self.merge(altered)

    def test_timed_out_patch_remains_a_failed_attempt(self):
        row = self.rows[0]
        row['native_status'] = 'failed'
        digest = hashlib.sha256(results.encoded_summary(self.summary)).hexdigest()
        for cell in self.cells:
            cell['summary_sha256'] = digest
            if row['id'] in cell['attempts']:
                cell['attempts'][row['id']]['native_status'] = 'failed'
        with self.assertRaisesRegex(ValueError, 'timed-out'):
            self.merge(self.cells)
        for cell in self.cells:
            if row['id'] in cell['attempts']:
                cell['attempts'][row['id']]['outcome'] = 'failed'
        self.assertEqual(self.merge(self.cells)['attempts'][row['id']]['outcome'], 'failed')

    def test_command_counts_do_not_close_missing_contributions_or_failed_cases(self):
        self.assertTrue(results.completion(self.summary)['complete'])
        selected = [row for row in self.rows if row['profile'] == self.rows[0]['profile'] and
                    row['task'] == self.rows[0]['task'] and row['setup'] == 'fr']
        selected[0]['fr_use']['contributing_actions'] = []
        selected[1]['independent_outcome'] = 'failed'
        selected[1]['fr_use']['commands_observed'] = 100
        self.assertEqual(results.completion(self.summary)['missing_accepted_fr_cases'],
                         [{'profile': self.rows[0]['profile'], 'task': self.rows[0]['task']}])

    def test_compiler_archive_cannot_write_through_a_link(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            archive = directory / 'cache.tar.gz'
            with tarfile.open(archive, 'w:gz') as stream:
                link = tarfile.TarInfo('target/escape')
                link.type, link.linkname = tarfile.SYMTYPE, '/tmp/outside'
                stream.addfile(link)
            with self.assertRaisesRegex(ValueError, 'unsafe'):
                results.unpack_cache(archive, directory, results.common.digest(archive), 'target', 1024)
            self.assertFalse((directory / 'target').exists())


if __name__ == '__main__':
    unittest.main()
