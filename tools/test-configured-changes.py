#!/usr/bin/env python3
"""Replay retained configured attempts with their exact runner, without model calls."""
from pathlib import Path
import runpy
import subprocess
import sys
import tempfile
import unittest

from agent_eval.study import load

ROOT = Path(__file__).resolve().parents[1]
COHORT = ROOT / 'tests/agent-eval/opencode/changes/configured-2026-10-08'


class RetainedPilot(unittest.TestCase):
    def test_cpu_stop_preserves_partial_work_and_three_unstarted_cells(self):
        frozen = load(COHORT / 'plan.json')
        runpy.run_path(str(ROOT / 'tools/terminal-change-snapshot.py'))['verify'](frozen, COHORT / 'runner')
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / 'report.json'
            command = [sys.executable, '-B', str(COHORT / 'runner/terminal-change-results.py'), 'report',
                       str(COHORT), str(COHORT / 'attempts'), '--output', str(output)]
            grades = COHORT / 'grades.json'
            if grades.exists():
                command += ['--grades', str(grades)]
            subprocess.run(command, cwd=temporary, check=True, capture_output=True)
            result = load(output)
        expected = COHORT / ('results.json' if grades.exists() else 'results-pending.json')
        self.assertEqual(result, load(expected))
        self.assertEqual([r['outcome'] for r in result['attempts']], ['failed'] + ['not_started'] * 3)
        first = result['attempts'][0]
        self.assertEqual(first['process']['stop_reason'], 'cpu_seconds')
        self.assertEqual(first['observed']['host_calls'], 5)
        self.assertEqual(first['observed']['fr_calls'], 0)
        self.assertIsNone(first['submission_sha256'])
        self.assertTrue(all(r['observed'] is None for r in result['attempts'][1:]))
        self.assertFalse(load(COHORT / 'stop.json')['resume_allowed'])
        self.assertFalse(result['efficiency_advantage'])


if __name__ == '__main__':
    unittest.main()
