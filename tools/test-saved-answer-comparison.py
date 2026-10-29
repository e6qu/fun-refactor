"""Reject incomplete comparisons, changed inputs and unsafe measurement archives."""
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
import zipfile

spec = importlib.util.spec_from_file_location('comparison', Path(__file__).with_name('saved-answer-comparison.py'))
comparison = importlib.util.module_from_spec(spec)
spec.loader.exec_module(comparison)


def projects():
    values = []
    for name in ('glob', 'boltons', 'jmespath'):
        sessions = []
        for repetition in range(3):
            for condition in ('saved', 'fresh'):
                sessions.append({'repetition': repetition, 'condition': condition,
                    'total_seconds': 20 + repetition, 'source_edit_rss_observation': 'sampled', 'stages': [
                        {'stage': stage, 'answer_digest': name + stage, 'seconds': 1,
                         'worker_cpu_seconds': 2, 'sampled_aggregate_rss_bytes': 4096,
                         'storage_after': {'saved_state': 10}}
                        for stage in ('first', 'reuse', 'edit')]})
        values.append({'project': {'id': name}, 'workspace': '/trial/project', 'source_files': {'code.py': 'before'},
                       'changed_files': {'code.py': 'after'}, 'sessions': sessions})
    return {'previous': values, 'current': copy.deepcopy(values)}


class ComparisonTests(unittest.TestCase):
    def test_identical_answers_cover_all_54_requests_per_version(self):
        comparison.compare_answers(projects())

    def test_changed_answer_fails(self):
        value = projects()
        value['current'][1]['sessions'][0]['stages'][0]['answer_digest'] = 'different'
        with self.assertRaisesRegex(ValueError, 'disagree'):
            comparison.compare_answers(value)

    def test_changed_source_and_question_fail(self):
        for field, replacement in (('source_files', {}), ('changed_files', {}),
                                   ('project', {'id': 'other'}), ('workspace', '/other/project')):
            value = projects()
            value['current'][0][field] = replacement
            with self.assertRaisesRegex(ValueError, 'inputs differ'):
                comparison.compare_answers(value)

    def test_omitting_the_same_request_on_both_sides_still_fails(self):
        value = projects()
        for version in value.values():
            version[0]['sessions'][0]['stages'].pop()
        with self.assertRaisesRegex(ValueError, 'omit'):
            comparison.compare_answers(value)

    def test_missing_project_fails(self):
        value = projects()
        value['current'].pop()
        with self.assertRaisesRegex(ValueError, 'missing project'):
            comparison.compare_answers(value)

    def test_summary_uses_complete_totals_and_keeps_request_cpu_separate(self):
        value = comparison.summary(projects())['glob']['current']['saved']
        self.assertEqual(value['median_session_seconds'], 21)
        self.assertEqual(value['median_reuse_seconds'], 1)
        self.assertEqual(value['median_worker_cpu_seconds'], 6)
        self.assertEqual(value['maximum_sampled_aggregate_rss_bytes'], 4096)
        self.assertEqual(value['median_final_saved_bytes'], 10)

    def test_summary_counts_unobserved_memory_without_inventing_a_peak(self):
        value = projects()
        for session in value['current'][0]['sessions']:
            for stage in session['stages']:
                stage['sampled_aggregate_rss_bytes'] = None
        summary = comparison.summary(value)['glob']['current']['saved']
        self.assertIsNone(summary['maximum_sampled_aggregate_rss_bytes'])
        self.assertEqual(summary['observed_memory_stages'], 0)
        self.assertEqual(summary['unobserved_memory_stages'], 9)
        value['current'][0]['sessions'][0]['stages'][0]['sampled_aggregate_rss_bytes'] = 4096
        summary = comparison.summary(value)['glob']['current']['saved']
        self.assertEqual(summary['maximum_sampled_aggregate_rss_bytes'], 4096)
        self.assertEqual(summary['observed_memory_stages'], 1)
        self.assertEqual(summary['unobserved_memory_stages'], 8)
        value['current'][0]['sessions'][0]['source_edit_rss_observation'] = 'unobserved'
        self.assertEqual(comparison.summary(value)['glob']['current']['saved']['source_edits_without_memory_sample'], 1)

    def test_archive_paths_and_duplicate_entries_fail(self):
        for names in (['../escape'], ['/absolute'], ['same', 'same']):
            with tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                with zipfile.ZipFile(root / 'bad.zip', 'w') as archive:
                    for index, name in enumerate(names):
                        if name in names[:index]:
                            with self.assertWarnsRegex(UserWarning, 'Duplicate name'):
                                archive.writestr(name, b'bad')
                        else:
                            archive.writestr(name, b'bad')
                with self.assertRaisesRegex(ValueError, 'unsafe'):
                    comparison.unpack(root / 'bad.zip', root / 'result')

    def test_archive_inventory_cannot_hide_modified_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with zipfile.ZipFile(root / 'bad.zip', 'w') as archive:
                archive.writestr('inventory.json', json.dumps({'answer.json': 'wrong'}))
                archive.writestr('answer.json', '{}')
            with self.assertRaisesRegex(ValueError, 'inventory differs'):
                comparison.unpack(root / 'bad.zip', root / 'result')


if __name__ == '__main__':
    unittest.main()
