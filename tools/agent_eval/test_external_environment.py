"""Host lint policy must not change historical third-party task outcomes."""

import importlib.util
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

TOOLS = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(TOOLS))
from agent_eval import external_environment, investigation, regex_workspace, regex_escape_len

SPEC = importlib.util.spec_from_file_location("external_test_harness", TOOLS / "agent-eval.py")
harness = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(harness)

HOST_FLAGS = {
    "RUSTFLAGS": "-D warnings",
    "CARGO_ENCODED_RUSTFLAGS": "-D\x1fwarnings",
    "RUSTDOCFLAGS": "-D warnings",
    "CARGO_ENCODED_RUSTDOCFLAGS": "-D\x1fwarnings",
    "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS": "-D warnings",
}


class ExternalEnvironment(unittest.TestCase):
    def test_flags_removed_without_mutating_host_or_resource_limits(self):
        with mock.patch.dict(os.environ, {**HOST_FLAGS, "CARGO_BUILD_JOBS": "1",
                                         "RAYON_NUM_THREADS": "1", "PATH": "/compiler/bin"}):
            env = external_environment.environment()
            self.assertTrue(set(HOST_FLAGS).isdisjoint(env))
            self.assertEqual(env["CARGO_BUILD_JOBS"], "1")
            self.assertEqual(env["RAYON_NUM_THREADS"], "1")
            self.assertEqual(env["PATH"], "/compiler/bin")
            self.assertEqual(env["CARGO_NET_OFFLINE"], "true")
            self.assertEqual(env["CARGO_HOME"], str(TOOLS.parent / "target/cargo-home"))
            self.assertEqual({key: os.environ[key] for key in HOST_FLAGS}, HOST_FLAGS)

    def test_real_child_does_not_inherit_host_flags(self):
        code = "import os; print(any(k in os.environ for k in " + repr(list(HOST_FLAGS)) + "))"
        with mock.patch.dict(os.environ, HOST_FLAGS):
            result = harness.process([sys.executable, "-c", code], TOOLS)
        self.assertEqual(result["exit_code"], 0)
        self.assertEqual(result["result"].strip(), "False")

    def test_both_oracles_build_without_host_flags_and_keep_real_errors(self):
        for oracle in (regex_workspace, regex_escape_len):
            with self.subTest(oracle=oracle.__name__), tempfile.TemporaryDirectory() as tmp:
                failed = subprocess.CompletedProcess([], 1, b"", b"error: invalid source")
                with mock.patch.dict(os.environ, HOST_FLAGS), mock.patch.object(
                        oracle.subprocess, "run", return_value=failed) as run:
                    result = oracle.verify(Path(tmp))
                self.assertFalse(result["passed"])
                self.assertEqual(result["stage"], 0)
                self.assertIn("invalid source", result["detail"])
                self.assertTrue(set(HOST_FLAGS).isdisjoint(run.call_args.kwargs["env"]))

    def test_investigation_commands_keep_serial_limits(self):
        with mock.patch.dict(os.environ, HOST_FLAGS):
            env = investigation.environment()
        self.assertTrue(set(HOST_FLAGS).isdisjoint(env))
        for key in ("CARGO_BUILD_JOBS", "RAYON_NUM_THREADS", "RUST_TEST_THREADS"):
            self.assertEqual(env[key], "1")

    def test_new_evaluations_bind_environment_policy(self):
        path = "tools/agent_eval/external_environment.py"
        self.assertIn(path, harness.evaluator_fingerprints())
        self.assertIn(path, investigation.bindings())


if __name__ == "__main__":
    unittest.main()
