"""Retain failed controls and require cleanup before reporting success."""
from contextlib import nullcontext
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
spec = importlib.util.spec_from_file_location("comparison_rehearsal", Path(__file__).resolve().parents[1] / "agent-comparison-rehearsal.py")
control = importlib.util.module_from_spec(spec)
spec.loader.exec_module(control)


class FailureRetention(unittest.TestCase):
    def exercise(self, *, create_fails, cleanup_fails, audit_fails):
        scope = Mock()
        scope.last = {"populated": 1}
        scope.finish.return_value = {"retained": True}
        def docker(argv, *_):
            if argv[3] == "create" and create_fails:
                return {"exit_code": 1, "stop_reason": None}, b"", b"deliberate create failure"
            data = b'{"setup":"files"}' if argv[3] == "exec" and "--setup" in argv else b""
            return {"exit_code": 0, "stop_reason": None}, data, b""
        scope.invoke.side_effect = docker
        def cleanup(argv, *_):
            return {"exit_code": 1 if cleanup_fails and argv[3] == "rm" else 0,
                    "stop_reason": None}, b"", b"deliberate cleanup failure" if cleanup_fails else b""
        checked = {"complete": not audit_fails, "stop_reason": None, "cpu_seconds": 0.1, "memory_peak_bytes": 1024}
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "result"
            with patch.dict(control.os.environ, {"GITHUB_ACTIONS": "true"}), \
                 patch.object(control, "isolated_budget", return_value=nullcontext("fixture.slice")), \
                 patch.object(control, "ContainerResources", return_value=Mock(start=Mock(return_value=scope))), \
                 patch.object(control, "invoke", side_effect=cleanup) as invocation, \
                 patch.object(control, "audit", return_value=checked):
                if create_fails or cleanup_fails or audit_fails:
                    with self.assertRaises(ValueError):
                        control.rehearsal("sha256:" + "a" * 64, "files", output, False, "tools")
                else:
                    control.rehearsal("sha256:" + "a" * 64, "files", output, False, "tools")
            retained = json.loads((output / "result.json").read_text())
            self.assertEqual(retained["passed"], not (create_fails or cleanup_fails or audit_fails))
            self.assertEqual(retained["resources"], {"retained": True})
            self.assertEqual(retained["model_calls"], 0)
            scope.finish.assert_called_once()
            self.assertEqual(invocation.call_args.args[0][3], "rm")
            self.assertIn("--force", invocation.call_args.args[0])
            if create_fails:
                self.assertIn("deliberate create failure", retained["error"])
            if create_fails or cleanup_fails or audit_fails:
                self.assertIn("error", retained)

    def test_container_creation_failure_keeps_diagnostics_and_still_cleans_up(self):
        self.exercise(create_fails=True, cleanup_fails=False, audit_fails=False)

    def test_failed_container_removal_cannot_report_success(self):
        self.exercise(create_fails=False, cleanup_fails=True, audit_fails=False)

    def test_surviving_processes_cannot_report_success(self):
        self.exercise(create_fails=False, cleanup_fails=False, audit_fails=True)

    def test_success_is_written_only_after_cleanup_and_audit(self):
        self.exercise(create_fails=False, cleanup_fails=False, audit_fails=False)

    def test_budget_stop_keeps_output_from_the_command_that_triggered_it(self):
        scope = Mock()
        scope.finish.return_value = {"stop_reason": "pids"}
        retained = None
        def resources(*_, execute):
            nonlocal retained
            retained = execute
            return Mock(start=Mock(return_value=scope))
        def bounded(argv, *args):
            if "--setup" in argv:
                retained(argv, *args)
                raise ValueError("container resource budget stopped the attempt")
            return {"exit_code": 0, "stop_reason": None}, b"", b""
        scope.invoke.side_effect = bounded
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "result"
            with patch.dict(control.os.environ, {"GITHUB_ACTIONS": "true"}), \
                 patch.object(control, "isolated_budget", return_value=nullcontext("fixture.slice")), \
                 patch.object(control, "ContainerResources", side_effect=resources), \
                 patch.object(control, "invoke", return_value=({"exit_code": 0, "stop_reason": None}, b"", b"thread diagnosis")):
                with self.assertRaisesRegex(ValueError, "budget stopped"):
                    control.rehearsal("sha256:" + "a" * 64, "files", output, False, "tools")
            result = json.loads((output / "result.json").read_text())
            self.assertFalse(result["passed"])
            self.assertEqual(result["commands"][0]["stderr"], "thread diagnosis")
            self.assertEqual(result["resources"]["stop_reason"], "pids")


if __name__ == "__main__":
    unittest.main()
