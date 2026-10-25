"""Keep command-use and token summaries honest without publishing native logs."""
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval.selected_public import command_tokens, completed_tool_time, observed_fr, sum_counts


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


if __name__ == '__main__':
    unittest.main()
