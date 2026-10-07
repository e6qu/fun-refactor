"""Offline checks of process attribution, bounds and unchanged guard accounting."""
import importlib.util
import base64
import gzip
import hashlib
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
    def test_retained_profiles_reproduce_guard_peaks_and_client_attribution(self):
        here = Path(__file__).resolve().parents[1] / "tests/agent-eval/opencode/memory/2026-10-07-profile"
        manifest = json.loads((here / "manifest.json").read_bytes())
        for name, expected in manifest["sources"].items():
            self.assertEqual(hashlib.sha256((here / name).read_bytes()).hexdigest(), expected)
        def load(name, path):
            spec = importlib.util.spec_from_file_location(name, path)
            module = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(module)
            return module
        checker = load("retained_profile_checker", here / "check-client-memory.py")
        checker.client_memory_profile = load("agent_eval.retained_profile", here / "client_memory_profile.py")
        expected = json.loads((here / "report.json").read_bytes())
        self.assertEqual(set(expected), {"macos-14", "ubuntu-latest"})
        for name, identity in manifest["platforms"].items():
            archive = here / (name + ".json.gz")
            self.assertEqual(hashlib.sha256(archive.read_bytes()).hexdigest(), identity["archive_sha256"])
            with gzip.open(archive, "rb") as stream:
                raw = stream.read(8 * 1024**2 + 1)
            self.assertLessEqual(len(raw), 8 * 1024**2)
            files = json.loads(raw)
            self.assertEqual(len(files), identity["files"])
            with tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                for path, data in files.items():
                    relative = Path(path)
                    self.assertFalse(relative.is_absolute() or ".." in relative.parts)
                    destination = root / relative
                    destination.parent.mkdir(parents=True, exist_ok=True)
                    destination.write_bytes(base64.b64decode(data, validate=True))
                result = checker.report(root)
                self.assertEqual(result, expected[name])
                self.assertEqual(result["plan_sha256"], identity["plan_sha256"])
                self.assertFalse(result["admitted"])
                for cell in result["cases"]:
                    sample = cell["profile"]["peak"]["sample"]
                    self.assertEqual(sample["phase_before"], "message-in-flight")
                    self.assertEqual(sample["phase_after"], "message-in-flight")
                    rows = sample["processes"]
                    self.assertEqual(max(rows, key=lambda row: row["rss_bytes"])["executable"], "opencode")
                    self.assertNotIn("fr", [row["executable"] for row in rows])
                    if name == "macos-14":
                        client = next(row for row in rows if row["executable"] == "opencode")
                        self.assertGreater(client["rss_bytes"], checker.HEADROOM_RSS)
                        self.assertEqual(cell["process"]["stop_reason"], "rss_bytes")
                    else:
                        self.assertTrue(cell["completed"])

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
