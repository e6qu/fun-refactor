"""A successful inner command cannot hide a stopped complete attempt."""
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from . import selected_guarded as guarded


class GuardedAttemptChecks(unittest.TestCase):
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
                resources = {'elapsed_seconds': 80, 'stop_reason': 'rss_bytes', 'exit_code': 125}
                (directory / 'resources.json').write_text(json.dumps(resources))
                record = {'status': status, 'collection_errors': [], 'elapsed_seconds': 50}
                guarded.attach(record, resources, directory)
                self.assertEqual(record['status'], 'failed')
                self.assertEqual(record['elapsed_seconds'], 80)
                self.assertEqual(bool(record['collection_errors']), status == 'started')


if __name__ == '__main__':
    unittest.main()
