"""Offline checks of process attribution, bounds and unchanged guard accounting."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from agent_eval import bounded_host, client_memory_profile as profile
from agent_eval.study import encode


class ClientMemoryProfile(unittest.TestCase):
    def test_sampler_uses_only_target_group_and_preserves_exited_process_cpu(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            sampler = profile.Sampler(root)
            sampler.attempt.mkdir(parents=True)
            first = "10 10 20 00:01 /usr/bin/python3\n11 10 100 00:02 /tmp/opencode\n12 10 10 00:03 /tmp/tool worker\n90 90 999999 99:00 /private/unrelated\n"
            second = "10 10 20 00:02 /usr/bin/python3\n11 10 200 00:04 /tmp/opencode\n"
            with patch.object(profile.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, first)) as ps:
                self.assertEqual(sampler(10), (130 * 1024, {10: 1, 11: 2, 12: 3}))
                self.assertEqual(ps.call_args.args[0], ["ps", "-axo", "pid=,pgid=,rss=,time=,comm="])
            (sampler.attempt / "request.json").write_text("{}")
            with patch.object(profile.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, second)):
                self.assertEqual(sampler(10), (220 * 1024, {10: 2, 11: 4}))
            result = profile.audit(root, {"sampled_aggregate_rss_bytes": 220 * 1024, "sampled_cpu_seconds": 9})
            self.assertEqual(result["samples"], 2)
            self.assertEqual(result["peak"]["sample"]["phase_before"], "message-in-flight")
            self.assertEqual([p["executable"] for p in result["peak"]["sample"]["processes"]], ["python3", "opencode"])
            self.assertNotIn("unrelated", (root / "profile.jsonl").read_text())
            for field in ("sampled_aggregate_rss_bytes", "sampled_cpu_seconds"):
                changed = {"sampled_aggregate_rss_bytes": 220 * 1024, "sampled_cpu_seconds": 9}
                changed[field] += 1
                with self.assertRaisesRegex(ValueError, "differs from guard"):
                    profile.audit(root, changed)

    def test_profile_bounds_and_malformed_processes_refuse(self):
        for rows in ("1 1 -1 0 python", "1 1 1 0 python\n1 1 1 0 python", "1 1 1 0 " + "x" * 129,
                     "\n".join(f"{i} 1 1 0 python" for i in range(1, profile.MAX_PROCESSES + 2))):
            with self.assertRaises(ValueError):
                profile.processes(rows, 1)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            sampler = profile.Sampler(root)
            sampler.written = profile.MAX_TRACE
            with patch.object(profile.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, "1 1 1 0 python")):
                with self.assertRaisesRegex(ValueError, "trace exceeds budget"):
                    sampler(1)
            self.assertFalse(sampler.path.exists())

    def test_phase_transition_is_not_assigned_to_a_single_stage(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            sample = {"group": 1, "started_seconds": 0, "finished_seconds": 1,
                      "phase_before": "server-startup", "phase_after": "message-in-flight",
                      "processes": [{"pid": 1, "rss_bytes": 100, "cpu_seconds": 2, "executable": "opencode"}]}
            path = root / "profile.jsonl"
            path.write_bytes(encode(sample) + b"\n")
            measured = {"sampled_aggregate_rss_bytes": 100, "sampled_cpu_seconds": 2}
            self.assertEqual(profile.audit(root, measured)["phase_peak_rss_bytes"], {"phase-transition": 100})
            for key, value in (("phase_before", "invented"), ("finished_seconds", -1), ("group", False)):
                path.write_bytes(encode({**sample, key: value}) + b"\n")
                with self.assertRaises(ValueError):
                    profile.audit(root, measured)
            path.write_bytes(encode(sample))
            with self.assertRaisesRegex(ValueError, "incomplete"):
                profile.audit(root, measured)

    def test_profile_sampler_failure_still_kills_guarded_work(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            import io, sys
            with patch.object(bounded_host, "sample", side_effect=ValueError("profile trace exceeds budget")):
                result = bounded_host.run([sys.executable, "-c", "import time; time.sleep(30)"], b"",
                    io.BytesIO(), io.BytesIO(), root, wall_seconds=2)
            self.assertEqual(result["stop_reason"], "monitor_error")
            self.assertEqual(result["process_exit_code"], -9)

    def test_client_profiling_refuses_local_execution_before_creating_files(self):
        spec = importlib.util.spec_from_file_location("profile_cli", Path(__file__).with_name("profile-client-memory.py"))
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        with tempfile.TemporaryDirectory() as temporary, patch.dict(os.environ, {}, clear=True):
            root = Path(temporary) / "unstarted"
            with self.assertRaisesRegex(ValueError, "only on GitHub"):
                module.check(root, Path("unused"), Path("unused"))
            self.assertFalse(root.exists())


if __name__ == "__main__":
    unittest.main()
