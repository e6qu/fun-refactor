"""Host lint policy must not change historical third-party task outcomes."""

import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

TOOLS = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(TOOLS))
from agent_eval import external_environment

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
            result = subprocess.run([sys.executable, str(TOOLS / "external-eval.py"), "-c", code],
                                    capture_output=True, text=True, check=True, timeout=10)
        self.assertEqual(result.stdout.strip(), "False")

    def test_child_failure_and_diagnostics_are_preserved(self):
        code = "import sys; print('invalid source', file=sys.stderr); sys.exit(7)"
        result = subprocess.run([sys.executable, str(TOOLS / "external-eval.py"), "-c", code],
                                capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 7)
        self.assertEqual(result.stderr.strip(), "invalid source")

    def test_child_keeps_working_directory_arguments_and_limits(self):
        with tempfile.TemporaryDirectory() as tmp:
            code = "import os,sys; print(os.getcwd()); print(sys.argv[1]); print(os.environ['CARGO_BUILD_JOBS'])"
            with mock.patch.dict(os.environ, {"CARGO_BUILD_JOBS": "1"}):
                result = subprocess.run([sys.executable, str(TOOLS / "external-eval.py"),
                                         "-c", code, "argument with spaces"], cwd=tmp,
                                        capture_output=True, text=True, check=True, timeout=10)
            self.assertEqual(result.stdout.splitlines(), [str(Path(tmp).resolve()), "argument with spaces", "1"])

    def test_missing_script_fails(self):
        result = subprocess.run([sys.executable, str(TOOLS / "external-eval.py")],
                                capture_output=True, text=True, timeout=10)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("usage:", result.stderr)


if __name__ == "__main__":
    unittest.main()
