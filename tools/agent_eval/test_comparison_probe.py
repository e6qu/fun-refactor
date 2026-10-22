"""Keep tool errors even when the tool writes diagnostics to standard output."""
from pathlib import Path
import subprocess
import sys
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval import comparison_probe as probe


class ToolDiagnostics(unittest.TestCase):
    def test_nonzero_exit_retains_both_output_streams_and_status(self):
        result = subprocess.CompletedProcess(["codegraph", "init", "--yes"], 1,
            b"Refusing to initialize: a parent of your home directory", b"stderr detail")
        with patch.object(probe.subprocess, "run", return_value=result):
            with self.assertRaises(ValueError) as failed:
                probe.command(result.args)
        for expected in ("exit=1", "parent of your home directory", "stderr detail"):
            self.assertIn(expected, str(failed.exception))

    def test_oversized_error_is_rejected_before_building_a_large_diagnostic(self):
        result = subprocess.CompletedProcess(["tool"], 1, b"x" * (1024**2 + 1), b"")
        with patch.object(probe.subprocess, "run", return_value=result):
            with self.assertRaisesRegex(ValueError, "exceeds 1 MiB"):
                probe.command(result.args)


if __name__ == "__main__":
    unittest.main()
