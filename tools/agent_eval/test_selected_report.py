"""Keep failed work in cost comparisons and refuse incomplete completion reports."""
import copy
import hashlib
import json
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval.selected_report import AGENTS, TASKS, comparisons, token_means, verify_checks


class SelectedReportTests(unittest.TestCase):
    def setUp(self):
        self.rows = [{'id': f'{profile}-{task}-{setup}-{repeat}', 'profile': profile, 'task': task,
                      'setup': setup, 'repetition': repeat, 'native_status': 'completed',
                      'independent_outcome': 'passed', 'total_seconds': 10.0,
                      'stop_reason': None, 'complete_attempt_deadline_missed': False}
                     for profile in AGENTS for task in TASKS for setup in ('files', 'fr') for repeat in (1, 2)]

    def test_failed_work_counts_per_success_and_cannot_enter_matching_successes(self):
        self.rows[0].update(native_status='failed', independent_outcome='failed', total_seconds=150.0)
        groups, matched = comparisons({'attempts': self.rows, 'limits': {'wall_seconds': 150}}, TASKS)
        self.assertEqual(groups[0]['accepted'], 5)
        self.assertEqual(groups[0]['successful_mean_seconds'], 10.0)
        self.assertEqual(groups[0]['total_seconds'], 200.0)
        self.assertEqual(groups[0]['seconds_per_success'], 40.0)
        self.assertEqual(matched[0]['pairs'], 5)
        self.assertEqual(matched[0]['fr_change_percent'], 0.0)

    def test_no_success_has_undefined_time_per_success(self):
        for row in self.rows:
            if row['profile'] == 'opencode2-glm-flash' and row['setup'] == 'fr':
                row.update(native_status='failed', independent_outcome='failed')
        groups, matched = comparisons({'attempts': self.rows, 'limits': {'wall_seconds': 150}}, TASKS)
        self.assertIsNone(groups[3]['seconds_per_success'])
        self.assertIsNone(groups[3]['successful_mean_seconds'])
        self.assertEqual(matched[1]['pairs'], 0)
        self.assertIsNone(matched[1]['fr_change_percent'])

    def test_late_or_stopped_attempt_cannot_enter_success_counts(self):
        for mutation in ({'total_seconds': 150.1, 'complete_attempt_deadline_missed': True},
                         {'stop_reason': 'wall_seconds'}):
            with self.subTest(mutation=mutation):
                rows = copy.deepcopy(self.rows)
                rows[0].update(mutation)
                with self.assertRaisesRegex(ValueError, 'within its recorded limits'):
                    comparisons({'attempts': rows, 'limits': {'wall_seconds': 150}}, TASKS)

    def test_invalid_time_cannot_create_a_speedup(self):
        for value in (float('nan'), float('inf'), -1, True, '10'):
            with self.subTest(seconds=value):
                rows = copy.deepcopy(self.rows)
                rows[0]['total_seconds'] = value
                with self.assertRaisesRegex(ValueError, 'finite nonnegative seconds'):
                    comparisons({'attempts': rows, 'limits': {'wall_seconds': 150}}, TASKS)

    def test_report_deadline_is_explicit_and_finite(self):
        for value in (0, -1, float('nan'), float('inf'), True):
            with self.subTest(deadline=value), self.assertRaisesRegex(ValueError, 'finite positive seconds'):
                comparisons({'attempts': self.rows, 'limits': {'wall_seconds': value}}, TASKS)

    def test_stricter_checks_preserve_statistics_for_all_twelve_retained_comparisons(self):
        root = Path(__file__).resolve().parents[2]
        audit = json.loads((root / 'tests/agent-eval/proof-writing/report-audit.json').read_text())
        self.assertEqual(len(audit['reports']), 12)
        attempts = 0
        for retained in audit['reports']:
            with self.subTest(summary=retained['summary']):
                data = (root / retained['summary']).read_bytes()
                self.assertEqual(hashlib.sha256(data).hexdigest(), retained['sha256'])
                summary = json.loads(data)
                self.assertEqual(set(retained['tasks']), {row['task'] for row in summary['attempts']})
                groups, matched = comparisons(summary, retained['tasks'])
                self.assertEqual(groups, retained['groups'])
                self.assertEqual(matched, retained['matched'])
                attempts += len(summary['attempts'])
        self.assertEqual(attempts, 480)

    def test_missing_checks_duplicates_and_timeout_success_claims_refuse(self):
        for mutation, error in (({'independent_outcome': None}, 'independently check'),
                                ({'id': self.rows[1]['id']}, 'distinct'),
                                ({'native_status': 'failed'}, 'timed-out')):
            rows = [dict(row) for row in self.rows]
            rows[0].update(mutation)
            with self.assertRaisesRegex(ValueError, error):
                comparisons({'attempts': rows, 'limits': {'wall_seconds': 150}}, TASKS)

    def test_token_means_match_successful_pairs_and_keep_missing_usage_unknown(self):
        for row in self.rows:
            row.update(reported_usage_complete=True, reported_tokens={
                'input_tokens': 10, 'cached_input_tokens': 3, 'output_tokens': 4, 'reasoning_output_tokens': 2})
        self.rows[0].update(native_status='failed', independent_outcome='failed')
        self.rows[2]['reported_tokens'] = {**self.rows[2]['reported_tokens'], 'input_tokens': 999999}
        self.assertEqual(token_means(self.rows, 'codex-luna-fast', 'fr'), [10, 3, 4, 2])
        self.rows[3]['reported_usage_complete'] = False
        self.assertEqual(token_means(self.rows, 'codex-luna-fast', 'fr'), [None] * 4)
        self.rows[3]['reported_usage_complete'] = True
        del self.rows[3]['reported_tokens']['cached_input_tokens']
        self.assertEqual(token_means(self.rows, 'codex-luna-fast', 'fr'), [10, None, 4, 2])

    def test_published_passes_and_patch_claims_must_match_fresh_independent_checks(self):
        details = {'patch_accepted': True, 'check_stage': 'complete', 'undo_exact': True, 'redo_exact': True}
        summary = {'core_checker_sha256': 'original-checker', 'attempts': self.rows}
        for row in self.rows:
            row['independent_check'] = dict(details)
        checked = {'passed': True, 'all_36_checked': True, 'checked_attempts': 36,
            'core_checker_sha256': 'original-checker', 'attempts': {row['id']: {
                'status': 'checked', 'native_status': 'completed', 'outcome': 'passed', **details} for row in self.rows}}
        verify_checks(summary, checked, 36)
        altered = copy.deepcopy(summary)
        altered['attempts'][0]['independent_outcome'] = 'failed'
        with self.assertRaisesRegex(ValueError, 'published outcome'):
            verify_checks(altered, checked, 36)
        altered = copy.deepcopy(summary)
        altered['attempts'][0]['independent_check']['undo_exact'] = False
        with self.assertRaisesRegex(ValueError, 'published patch checks'):
            verify_checks(altered, checked, 36)
        checked['all_36_checked'] = False
        with self.assertRaisesRegex(ValueError, 'all 36'):
            verify_checks(summary, checked, 36)

    def test_four_task_comparison_keeps_all_eight_attempts_per_setup(self):
        tasks = ('rust-parameter', 'typescript-alias', 'regex-escape-len', 'python-invoice')
        rows = [{**row, 'id': f'{profile}-{task}-{setup}-{repeat}', 'profile': profile,
                 'task': task, 'setup': setup, 'repetition': repeat}
                for row in self.rows[:1] for profile in AGENTS for task in tasks
                for setup in ('files', 'fr') for repeat in (1, 2)]
        groups, matched = comparisons({'attempts': rows, 'limits': {'wall_seconds': 150}}, tasks)
        self.assertEqual([group['attempted'] for group in groups], [8] * 6)
        self.assertEqual([group['pairs'] for group in matched], [8] * 3)
        with self.assertRaisesRegex(ValueError, '48 distinct'):
            comparisons({'attempts': rows[:-1], 'limits': {'wall_seconds': 150}}, tasks)


if __name__ == '__main__':
    unittest.main()
