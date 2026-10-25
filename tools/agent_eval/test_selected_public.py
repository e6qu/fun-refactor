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

    def test_missing_token_categories_are_unknown_instead_of_zero(self):
        self.assertIsNone(sum_counts([]))
        self.assertEqual(sum_counts([{'input': 3, 'cache': {'read': 1}}, {'input': 4, 'cache': {'read': 2}}]),
                         {'input': 7, 'cache': {'read': 3}})
        self.assertEqual(sum_counts([{'input': 3}, {'output': 4}]), {'input': None, 'output': None})

    def test_overlapping_tools_count_once_and_absent_timing_stays_unknown(self):
        events = [{'type': 'tool_use', 'part': {'state': {'time': {'start': start, 'end': end}}}}
                  for start, end in ((1000, 4000), (2000, 5000), (7000, 8000))]
        self.assertEqual(completed_tool_time(events)['completed_tool_wall_seconds'], 5)
        self.assertIsNone(completed_tool_time([])['completed_tool_wall_seconds'])

    def test_export_keeps_raw_prompts_and_exception_commands_private(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            schedule = prepare(ROOT, 'M2')
            plan = {'schedule': schedule, 'implementation': {}, 'source': {'native_inputs': {}, 'commit': 'source'},
                    'seeds': {}, 'versions': {}, 'checker_versions': {}, 'limits': {}, 'controls': {}, 'shared_setup': {},
                    'binaries': {}, 'components': {},
                    'fr_build': {'source_commit': 'built-source', 'source_tree': 'built-tree', 'executable_sha256': 'binary'}}
            plan['sha256'] = hashlib.sha256(encoded(plan)).hexdigest()
            plan_path = root / 'plan.json'
            plan_path.write_text(json.dumps(plan))
            canary = 'PRIVATE-PROMPT-AND-ACCOUNT-DETAIL'
            review = {}
            for row in schedule['attempts']:
                directory = root / 'attempts' / row['id']
                directory.mkdir(parents=True)
                record = {key: row[key] for key in ('id', 'profile', 'task', 'setup')}
                record.update(status='failed', plan_sha256=plan['sha256'], elapsed_seconds=1,
                              verification_seconds=0.1, patch_delivered=False,
                              process=None, usage=None, collection_errors=[], complete_attempt_deadline_missed=False,
                              error='TimeoutExpired: Command [native-cli, ' + canary + '] timed out')
                (directory / 'result.json').write_text(json.dumps(record))
                (directory / 'prompt.txt').write_text(canary)
                (directory / 'native.stdout').write_text('incomplete-json-' + canary)
                review[row['id']] = {'record_sha256': public.common.digest(directory / 'result.json'),
                                    'contributing_actions': [], 'failure_summary': 'Native command timed out before delivery.'}
            destination = root / 'approved'
            with patch.object(public.local, 'implementation_bindings', return_value={}), \
                    patch.object(public.grading, 'core_digest', return_value='checker'):
                result = public.export(plan_path, review, destination)
            self.assertEqual(len(result['attempts']), 48)
            self.assertTrue(all(row['native_status'] == 'failed' and row['collection_error_recorded']
                                for row in result['attempts']))
            self.assertNotIn(canary, (destination / 'summary.json').read_text())
            self.assertEqual([path.name for path in destination.rglob('*') if path.is_file()], ['summary.json'])


if __name__ == '__main__':
    unittest.main()
