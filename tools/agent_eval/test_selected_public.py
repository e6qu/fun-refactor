"""Keep command-use and token summaries honest without publishing native logs."""
from pathlib import Path
import hashlib
import json
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval.selected_public import command_tokens, completed_tool_time, observed_fr, sum_counts
from agent_eval import selected_public as public
from agent_eval.selected_plan import ROOT, encoded, prepare


class SelectedPublicTests(unittest.TestCase):
    def test_shell_wrappers_count_commands_but_quoted_help_text_does_not(self):
        self.assertEqual(command_tokens("/bin/zsh -lc 'fr --json project find total'"),
                         ['fr', '--json', 'project', 'find', 'total'])
        events = [{'type': 'item.completed', 'item': {'type': 'command_execution', 'command': command}}
                  for command in ("/bin/zsh -lc 'fr --json project find total'", "echo 'fr project find total'",
                                  '/tools/fr --json -C src author batch --from .fr/change')]
        used = observed_fr(events)
        self.assertEqual(used['commands_observed'], 2)
        self.assertEqual(used['operations_observed'], ['author batch', 'project find'])

    def test_newline_commands_are_observed_without_executing_quoted_examples(self):
        events = [{'type': 'item.completed', 'item': {'type': 'command_execution', 'command': command}}
                  for command in ('date +%s\nfr --json checks',
                                  '/bin/zsh -lc \'date +%s\nfr --json project find total\'',
                                  'echo "date +%s\nfr project find total"')]
        used = observed_fr(events)
        self.assertEqual(used['commands_observed'], 2)
        self.assertEqual(used['operations_observed'], ['checks', 'project find'])

    def test_current_global_option_values_are_not_mistaken_for_operations(self):
        events = [{'type': 'item.completed', 'item': {'type': 'command_execution', 'command':
            'fr --json --root src --max-file-size 4096 --context-basis previous project find total'}}]
        used = observed_fr(events)
        self.assertEqual(used['commands_observed'], 1)
        self.assertEqual(used['operations_observed'], ['project find'])

    def test_missing_token_categories_are_unknown_instead_of_zero(self):
        self.assertIsNone(sum_counts([]))
        self.assertEqual(sum_counts([{'input': 3, 'cache': {'read': 1}}, {'input': 4, 'cache': {'read': 2}}]),
                         {'input': 7, 'cache': {'read': 3}})
        self.assertEqual(sum_counts([{'input': 3}, {'output': 4}]), {'input': None, 'output': None})

    def test_interactive_proof_operations_are_reported_separately(self):
        events = [{'type': 'item.completed', 'item': {'type': 'command_execution', 'command': command}}
                  for command in ('fr --json spec proof-task target --token-limit 4096',
                                  'fr --json spec proof-state target --from draft --at 1:5 --token-limit 4096 --timeout-seconds 30')]
        used = observed_fr(events)
        self.assertEqual(used['commands_observed'], 2)
        self.assertEqual(used['operations_observed'], ['spec proof-state', 'spec proof-task'])

    def test_overlapping_tools_count_once_and_absent_timing_stays_unknown(self):
        events = [{'type': 'tool_use', 'part': {'state': {'time': {'start': start, 'end': end}}}}
                  for start, end in ((1000, 4000), (2000, 5000), (7000, 8000))]
        self.assertEqual(completed_tool_time(events)['completed_tool_wall_seconds'], 5)
        self.assertIsNone(completed_tool_time([])['completed_tool_wall_seconds'])

    def test_export_keeps_raw_prompts_and_exception_commands_private(self):
        self.assert_private_export('M2', 48)

    def test_m3_export_keeps_raw_proof_prompts_and_records_private(self):
        self.assert_private_export('M3', 36)

    def test_feedback_export_keeps_initial_reports_and_native_records_private(self):
        self.assert_private_export('M3-feedback', 8)

    def test_construction_export_keeps_prompts_and_native_records_private(self):
        self.assert_private_export('M3-construction', 4)

    def test_page_count_export_keeps_prompts_and_native_records_private(self):
        self.assert_private_export('M3-page-count', 4)

    def test_division_fact_export_keeps_handouts_and_native_records_private(self):
        self.assert_private_export('M3-facts', 8)

    def test_algorithm_exports_keep_all_records_private_and_all_failures_visible(self):
        for study, count in [('M3-algorithm-facts', 8), ('M3-algorithm-presentation', 12), ('M3-delivery', 12)]:
            self.assert_private_export(study, count)

    def assert_private_export(self, milestone, count):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            schedule = prepare(ROOT, milestone)
            plan = {'schedule': schedule, 'implementation': {}, 'source': {'native_inputs': {}, 'commit': 'source'},
                    'seeds': {}, 'versions': {}, 'checker_versions': {}, 'limits': {}, 'controls': {}, 'shared_setup': {},
                    'binaries': {}, 'components': {}, 'client_finish_seconds': 10, 'formal_inputs': {},
                    'checkers': {'lake': {'path': '/pinned/lake'}}, 'live_calls_enabled': True,
                    'public_check_environment': dict(public.local.PUBLIC_CHECK_ENVIRONMENT),
                    'fr_build': {'source_commit': 'built-source', 'source_tree': 'built-tree', 'executable_sha256': 'binary'}}
            if milestone in public.ALGORITHM_TASKS:
                plan['admission'] = {'profiles': {}, 'receipts_sha256': 'a' * 64}
            plan['sha256'] = hashlib.sha256(encoded(plan)).hexdigest()
            plan_path = root / 'plan.json'
            plan_path.write_text(json.dumps(plan))
            canary = 'PRIVATE-PROMPT-AND-ACCOUNT-DETAIL'
            review = {}
            for row in schedule['attempts']:
                directory = root / 'attempts' / row['id']
                directory.mkdir(parents=True)
                record = {key: row[key] for key in ('id', 'profile', 'task', 'setup')}
                record.update(status='failed', plan_sha256=plan['sha256'], elapsed_seconds=3,
                              timing_clock='includes-system-sleep',
                              suspended_seconds=1.25 if row == schedule['attempts'][0] else 0,
                              verification_seconds=0.1, patch_delivered=False,
                              process=None, usage=None, collection_errors=[], complete_attempt_deadline_missed=False,
                              checking_input_changes=['.fr/checks.json'],
                              error='TimeoutExpired: Command [native-cli, ' + canary + '] timed out')
                (directory / 'result.json').write_text(json.dumps(record))
                (directory / 'prompt.txt').write_text(canary)
                (directory / 'native.stdout').write_text('incomplete-json-' + canary)
                if milestone == 'M3-delivery' and row['profile'] == 'opencode2-glm-flash':
                    (directory / 'native.stdout').write_text(json.dumps({'type': 'step_finish',
                        'part': {'tokens': {'input': 2, 'output': 1}}}))
                    record['usage'] = {'exported': {'outcome': 'succeeded', 'usage_complete': False,
                        'unfinished_assistant_responses': 0, 'exported_assistant_responses': 1,
                        'stream_finished_responses': 1, 'model_observed': True}}
                if milestone == 'M3-feedback':
                    record.update(initial_check_seconds=0.2, initial_check_bytes=100,
                                  initial_check_sha256='a' * 64, initial_feedback_supplied=row['setup'] == 'supplied-feedback',
                                  native_stdout_sha256=public.common.digest(directory / 'native.stdout'))
                    (directory / 'result.json').write_text(json.dumps(record))
                elif milestone in {'M3-construction', 'M3-page-count', 'M3-facts'}:
                    record['native_stdout_sha256'] = public.common.digest(directory / 'native.stdout')
                    (directory / 'result.json').write_text(json.dumps(record))
                elif milestone in public.ALGORITHM_TASKS:
                    record['native_stdout_sha256'] = public.common.digest(directory / 'native.stdout')
                    resources = {'limits': {'wall_seconds': 150, 'rss_bytes': 805306368, 'cpu_seconds': 75,
                                 'disk_bytes': 536870912, 'transcript_bytes': 1048576},
                                 'stop_reason': 'wall_seconds', 'sampled_aggregate_rss_bytes': 100, 'elapsed_seconds': 3}
                    monitor = root / 'attempt-monitors' / row['id']
                    monitor.mkdir(parents=True)
                    (monitor / 'resources.json').write_text(json.dumps(resources))
                    record['complete_attempt_resources'] = resources
                    record['complete_attempt_resources_sha256'] = public.common.digest(monitor / 'resources.json')
                    (directory / 'result.json').write_text(json.dumps(record))
                review[row['id']] = {'record_sha256': public.common.digest(directory / 'result.json'),
                                    'contributing_actions': [], 'failure_summary': 'Native command timed out before delivery.'}
                if milestone == 'M3-feedback':
                    review[row['id']]['feedback_observations'] = {
                        'first_correct_repair_seconds': None, 'repeated_argument_type_errors': None,
                        'execution_validity': 'collection-failed'}
                elif milestone in {'M3-construction', 'M3-page-count', 'M3-facts'}:
                    review[row['id']]['proof_observations'] = {
                        'first_passing_proof_seconds': None, 'package_check_seconds': None,
                        'patch_delivery_seconds': None, 'proof_check_passed': None,
                        'proof_written_with_fr': None, 'package_check_passed': None,
                        'execution_validity': 'collection-failed'}
                    if milestone in {'M3-page-count', 'M3-facts'}:
                        review[row['id']]['proof_observations']['first_proof_check_seconds'] = None
                    if milestone == 'M3-facts':
                        review[row['id']]['proof_observations']['proof_write_seconds'] = None
                elif milestone in public.ALGORITHM_TASKS:
                    from agent_eval.algorithm_results import CHECKS, TIMINGS
                    review[row['id']]['algorithm_observations'] = {
                        **{name: None for name in CHECKS | TIMINGS}, 'execution_validity': 'collection-failed'}
            destination = root / 'approved'
            with patch.object(public.local, 'implementation_bindings', return_value={}), \
                    patch.object(public.grading, 'core_digest', return_value='checker'):
                result = public.export(plan_path, review, destination)
            self.assertEqual(len(result['attempts']), count)
            self.assertEqual(result['public_check_environment']['CARGO_PROFILE_DEV_OPT_LEVEL'], '0')
            self.assertEqual(result['client_finish_seconds'], 10)
            self.assertEqual(result['attempts'][0]['timing_clock'], 'includes-system-sleep')
            self.assertEqual(result['attempts'][0]['suspended_seconds'], 1.25)
            self.assertEqual(result['collection_notes']['system_sleep'], [{
                'attempt': schedule['attempts'][0]['id'], 'suspended_seconds': 1.25,
                'included_in_total_seconds': True}])
            self.assertTrue(all(row['checking_input_changes'] == ['.fr/checks.json'] for row in result['attempts']))
            self.assertTrue(all(row['native_status'] == 'failed' and row['collection_error_recorded']
                                for row in result['attempts']))
            self.assertNotIn(canary, (destination / 'summary.json').read_text())
            if milestone == 'M3-delivery':
                self.assertTrue(all(row['delivery_capture_bound_seconds'] is None for row in result['attempts']))
                self.assertTrue(all(row['reported_usage_complete'] is False for row in result['attempts']))
                checks = {'passed': True, 'all_12_checked': True, 'checked_attempts': 12,
                          'core_checker_sha256': result['core_checker_sha256'], 'control_policy': 'frozen',
                          'image': 'qualified-image', 'attempts': {row['id']: {
                              'status': 'checked', 'native_status': 'failed', 'patch_sha256': None,
                              'outcome': 'failed', 'patch_accepted': False, 'check_stage': 'missing-patch',
                              'undo_exact': False, 'redo_exact': False} for row in result['attempts']}}
                result['controls']['private_image'] = 'qualified-image'
                (destination / 'summary.json').write_text(json.dumps(result))
                checked_path = root / 'checked.json'
                checked_path.write_text(json.dumps(checks))
                run = 'https://github.com/e6qu/fun-refactor/actions/runs/123'
                public.accept_checks(destination, checked_path, run)
                accepted = json.loads((destination / 'summary.json').read_text())
                self.assertTrue(all(row['independent_outcome'] == 'failed' and
                                    row['independent_check_run'] == run for row in accepted['attempts']))
                del checks['attempts'][result['attempts'][-1]['id']]
                checked_path.write_text(json.dumps(checks))
                with self.assertRaisesRegex(ValueError, 'every original submission'):
                    public.accept_checks(destination, checked_path, run)
            self.assertEqual([path.name for path in destination.rglob('*') if path.is_file()], ['summary.json'])
            if milestone in {'M3-construction', 'M3-page-count', 'M3-facts'}:
                self.assertIsNone(result['attempts'][0]['proof_observations']['first_passing_proof_seconds'])
                self.assertIsNone(result['attempts'][0]['proof_observations']['package_check_seconds'])
                self.assertIsNone(result['attempts'][0]['proof_observations']['patch_delivery_seconds'])
            if milestone == 'M3-facts':
                self.assertIsNone(result['provider_request_count'])
                self.assertIsNone(result['attempts'][0]['proof_observations']['proof_write_seconds'])


if __name__ == '__main__':
    unittest.main()
