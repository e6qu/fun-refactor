"""A successful inner command cannot hide a stopped complete attempt."""
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from agent_eval import selected_guarded as guarded


class GuardedAttemptChecks(unittest.TestCase):
    def test_unadmitted_plan_cannot_start_a_native_client(self):
        with tempfile.TemporaryDirectory() as temporary:
            plan = Path(temporary) / 'plan.json'
            plan.write_text(json.dumps({'schedule': {'milestone': 'M3-delivery'}, 'live_calls_enabled': False}))
            with patch.object(guarded.sys, 'platform', 'darwin'), self.assertRaisesRegex(ValueError, 'admitted'):
                guarded.attempt(plan, 'unstarted')
            self.assertEqual(sorted(path.name for path in Path(temporary).iterdir()), ['plan.json'])
    def test_sampler_counts_detached_workers_and_keeps_compact_samples(self):
        raw = '10 1 10 20 00:01 Python\n11 10 11 100 00:02 opencode\n12 11 12 500 00:03 lean\n90 1 90 9999 00:01 other\n'
        with tempfile.TemporaryDirectory() as temporary:
            sampler = guarded.IndependentSampler(Path(temporary))
            with patch.object(guarded.subprocess, 'run', return_value=subprocess.CompletedProcess([], 0, raw)):
                self.assertEqual(sampler(10), (620 * 1024, {10: 1, 11: 2, 12: 3}))
            self.assertEqual(sampler.peak['rss_bytes'], 620 * 1024)
            self.assertEqual([row['executable'] for row in sampler.peak['processes']], ['Python', 'opencode', 'lean'])
            self.assertLess(sampler.written * 1500, guarded.client_memory_profile.MAX_TRACE)
            sampler.written = guarded.client_memory_profile.MAX_TRACE
            with patch.object(guarded.subprocess, 'run', return_value=subprocess.CompletedProcess([], 0, raw)), \
                    self.assertRaisesRegex(ValueError, 'trace exceeds budget'):
                sampler(10)

    def test_complete_receipt_overrides_inner_success_and_preserves_interruption(self):
        for status in ('completed', 'started'):
            with tempfile.TemporaryDirectory() as temporary:
                directory = Path(temporary)
                resources = {'elapsed_seconds': 80, 'stop_reason': 'rss_bytes', 'exit_code': 125,
                             'suspended_seconds': 1.5}
                (directory / 'resources.json').write_text(json.dumps(resources))
                record = {'status': status, 'collection_errors': [], 'elapsed_seconds': 50}
                guarded.attach(record, resources, directory)
                self.assertEqual(record['status'], 'failed')
                self.assertEqual(record['elapsed_seconds'], 80)
                self.assertEqual(record['suspended_seconds'], 1.5)
                self.assertEqual(bool(record['collection_errors']), status == 'started')

    def test_running_fr_command_survives_an_interruption_without_reading_other_processes(self):
        raw = '10 1 10 20 00:01 Python\n12 10 12 500 00:03 fr\n90 1 90 9999 00:01 other\n'
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            sampler = guarded.IndependentSampler(directory)
            sampled = subprocess.CompletedProcess([], 0, raw)
            command = subprocess.CompletedProcess([], 0, 'fr -C /library/file.lean project find tdiv\n')
            with patch.object(guarded.subprocess, 'run', side_effect=[sampled, command, sampled]) as run:
                sampler(10)
                sampler(10)
            self.assertEqual(run.call_args_list[1].args[0], ['ps', '-p', '12', '-o', 'args='])
            self.assertEqual(len(run.call_args_list), 3)
            rows = [json.loads(line) for line in (directory / 'commands.jsonl').read_text().splitlines()]
            self.assertEqual(len(rows), 1)
            self.assertEqual(rows[0]['status'], 'captured')
            self.assertEqual(rows[0]['command'], command.stdout.strip())
            self.assertEqual(rows[0]['omitted_characters'], 0)

    def test_process_exit_before_argv_read_stays_explicitly_unknown(self):
        raw = '10 1 10 20 00:01 Python\n12 10 12 500 00:03 fr\n'
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            with patch.object(guarded.subprocess, 'run', side_effect=[
                    subprocess.CompletedProcess([], 0, raw), subprocess.CompletedProcess([], 1, '')]):
                guarded.IndependentSampler(directory)(10)
            row = json.loads((directory / 'commands.jsonl').read_text())
            self.assertIsNone(row['command'])
            self.assertEqual(row['status'], 'unavailable')

    def test_command_timeout_preserves_resource_sampling_and_marks_missing_detail(self):
        raw = '10 1 10 20 00:01 Python\n12 10 12 500 00:03 fr\n'
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            with patch.object(guarded.subprocess, 'run', side_effect=[
                    subprocess.CompletedProcess([], 0, raw), subprocess.TimeoutExpired('ps', 2)]):
                measured = guarded.IndependentSampler(directory)(10)
            self.assertEqual(measured, (520 * 1024, {10: 1, 12: 3}))
            row = json.loads((directory / 'commands.jsonl').read_text())
            self.assertEqual(row['error'], 'TimeoutExpired')
            self.assertIsNone(row['command'])

    def test_expected_deadline_is_a_failed_attempt_not_a_broken_collection(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            resources = {'elapsed_seconds': 150.01, 'stop_reason': 'wall_seconds', 'exit_code': 124,
                         'suspended_seconds': 0.2}
            (directory / 'resources.json').write_text(json.dumps(resources))
            record = {'status': 'started', 'collection_errors': [], 'elapsed_seconds': 0}
            guarded.attach(record, resources, directory)
            self.assertEqual(record['status'], 'failed')
            self.assertTrue(record['complete_attempt_deadline_missed'])
            self.assertTrue(record['controller_interrupted'])
            self.assertEqual(record['collection_errors'], [])


if __name__ == '__main__':
    unittest.main()
