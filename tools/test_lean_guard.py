#!/usr/bin/env python3
"""Exercise resource refusals with small synthetic children, without running Lean."""

import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest

GUARD = Path(__file__).with_name("lean-guard.py")


class LeanGuardTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def command(self, source, *limits):
        return [sys.executable, str(GUARD), "--lock", str(self.root / "lock"),
                "--seconds", "5", *limits, "--", sys.executable, "-c", source]

    def run_guard(self, source, *limits, **kwargs):
        return subprocess.run(self.command(source, *limits), capture_output=True,
                              text=True, timeout=15, **kwargs)

    def assert_stopped(self, pid):
        for _ in range(40):
            result = subprocess.run(["ps", "-p", str(pid), "-o", "stat="],
                                    capture_output=True, text=True, timeout=2)
            if not result.stdout.strip() or result.stdout.lstrip().startswith("Z"):
                return
            time.sleep(0.05)
        self.fail(f"child {pid} survived guard cleanup")

    def test_preserves_output_and_failure(self):
        result = self.run_guard("import sys; print('proof failed: é'); sys.exit(7)")
        self.assertEqual(result.returncode, 7)
        self.assertEqual(result.stdout, "proof failed: é\n")

    def test_memory_limit_counts_descendants(self):
        source = """import subprocess, sys, time
data = bytearray(64 * 1024 * 1024)
child = subprocess.Popen([sys.executable, '-c',
    'import time; data = bytearray(64 * 1024 * 1024); time.sleep(30)'])
print(child.pid, flush=True)
time.sleep(30)
"""
        result = self.run_guard(source, "--memory-mib", "96")
        self.assertEqual(result.returncode, 1)
        self.assertIn("exceeded 96 MiB RSS", result.stderr)
        self.assert_stopped(int(result.stdout.strip()))

    def test_deadline_stops_busy_cpu(self):
        result = self.run_guard("while True: pass", "--seconds", "0.5")
        self.assertEqual(result.returncode, 1)
        self.assertIn("exceeded 0.5 seconds", result.stderr)

    def test_output_limit(self):
        result = self.run_guard("while True: print('x' * 1024, flush=True)",
                                "--output-bytes", "4096")
        self.assertEqual(result.returncode, 1)
        self.assertIn("exceeded 4096 output bytes", result.stderr)
        self.assertLessEqual(len(result.stdout.encode()), 4096)

    def test_descendant_cannot_keep_output_pipe_open_after_parent_exit(self):
        source = """import subprocess, sys
child = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(30)'])
print(child.pid, flush=True)
"""
        result = self.run_guard(source, "--seconds", "0.5")
        self.assertEqual(result.returncode, 1)
        self.assertIn("exceeded 0.5 seconds", result.stderr)
        self.assert_stopped(int(result.stdout.strip()))

    def test_interrupt_stops_child(self):
        marker = self.root / "pid"
        source = f"import os, time; open({str(marker)!r}, 'w').write(str(os.getpid())); time.sleep(30)"
        child = subprocess.Popen(self.command(source), stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        try:
            for _ in range(100):
                if marker.exists():
                    break
                time.sleep(0.02)
            self.assertTrue(marker.exists())
            child.send_signal(signal.SIGTERM)
            _, stderr = child.communicate(timeout=5)
            self.assertEqual(child.returncode, 1)
            self.assertIn(b"interrupted by signal", stderr)
            self.assert_stopped(int(marker.read_text()))
        finally:
            if child.poll() is None:
                child.terminate()
            child.communicate(timeout=5)

    def test_serializes_proofs(self):
        events = self.root / "events"
        source = f"""import time
with open({str(events)!r}, 'a') as log:
    log.write('start\\n'); log.flush()
    time.sleep(0.3)
    log.write('end\\n')
"""
        children = [subprocess.Popen(self.command(source), stdout=subprocess.PIPE,
                                     stderr=subprocess.PIPE) for _ in range(2)]
        try:
            for child in children:
                _, stderr = child.communicate(timeout=10)
                self.assertEqual(child.returncode, 0, stderr)
            self.assertEqual(events.read_text().splitlines(), ["start", "end", "start", "end"])
        finally:
            for child in children:
                if child.poll() is None:
                    child.terminate()
                child.communicate(timeout=5)

    def test_missing_monitor_refuses_before_launch(self):
        marker = self.root / "started"
        result = self.run_guard(f"open({str(marker)!r}, 'w').close()",
                                env=dict(os.environ, PATH=str(self.root)))
        self.assertEqual(result.returncode, 1)
        self.assertFalse(marker.exists())
        self.assertIn("refused", result.stderr)


if __name__ == "__main__":
    unittest.main()
