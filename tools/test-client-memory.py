"""Offline checks of memory-control admission and retained failures."""
import copy
import base64
import gzip
import importlib.util
import io
import json
import os
from pathlib import Path
import tempfile
import sys
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("client_memory", Path(__file__).with_name("check-client-memory.py"))
memory = importlib.util.module_from_spec(spec)
spec.loader.exec_module(memory)


def fixture(root):
    plan = {"schema": memory.SCHEMA, "cells": memory.CELLS, "limits": memory.LIMITS,
            "admission_rss_bytes": memory.HEADROOM_RSS, "fr_sha256": "a" * 64,
            "opencode_sha256": "b" * 64, "script_sha256": memory.identity(Path(memory.__file__))}
    (root / "plan.json").write_text(json.dumps(plan))
    audits = {}
    for cell in memory.CELLS:
        directory = root / cell["id"]
        (directory / "configured/attempts").mkdir(parents=True)
        (directory / "configured/plan.json").write_text(json.dumps({"plan": {
            key: plan[key] for key in ("opencode_sha256", "fr_sha256")}}).replace('"fr_sha256"', '"binary_sha256"'))
        audit = {"attempts": [{"status": "completed", "failure": None, "process": {
            "limits": copy.deepcopy(memory.LIMITS), "sampled_aggregate_rss_bytes": 500 * 1024**2}}]}
        audits[cell["id"]] = audit
        (directory / "command.json").write_text(json.dumps({"cell": cell, "exit_code": 0}))
        (directory / "result.json").write_text(json.dumps([{
            "case": "configured", "provider_requests": 2, "report": audit}]))
    return audits


class ClientMemory(unittest.TestCase):
    def test_measurement_success_does_not_grant_admission(self):
        for admitted in (False, True):
            for command in ("measure", "check", "report", "admit"):
                args = ["memory", command, "unused", "--fr", "fr", "--opencode", "opencode"]
                with patch.object(sys, "argv", args), patch.object(memory, "check", return_value={"admitted": admitted}), \
                        patch.object(memory, "report", return_value={"admitted": admitted}), patch("sys.stdout", new=io.StringIO()):
                    if command in ("check", "admit") and not admitted:
                        with self.assertRaisesRegex(ValueError, "required headroom"):
                            memory.main()
                    else:
                        memory.main()

    def test_measurement_refuses_broken_evidence_instead_of_ignoring_errors(self):
        for command, function in (("measure", "check"), ("report", "report"), ("admit", "report")):
            args = ["memory", command, "unused", "--fr", "fr", "--opencode", "opencode"]
            with patch.object(sys, "argv", args), patch.object(memory, function, side_effect=ValueError("artifact changed")):
                with self.assertRaisesRegex(ValueError, "artifact changed"):
                    memory.main()

    def test_retained_hosted_rejection_replays_without_relaxing_limits(self):
        here = Path(__file__).resolve().parents[1] / "tests/agent-eval/opencode/memory/2026-10-06"
        manifest = json.loads((here / "manifest.json").read_bytes())
        checker = here / "check-client-memory.py"
        self.assertEqual(memory.identity(checker), manifest["checker_sha256"])
        spec = importlib.util.spec_from_file_location("retained_memory", checker)
        retained = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(retained)
        expected = json.loads((here / "report.json").read_bytes())
        self.assertEqual(set(expected), {"ubuntu-latest", "macos-14"})
        self.assertEqual(retained.LIMITS, memory.LIMITS)
        self.assertEqual(retained.HEADROOM_RSS, memory.HEADROOM_RSS)
        for platform, identity in manifest["platforms"].items():
            archive = here / (platform + ".json.gz")
            self.assertEqual(memory.identity(archive), identity["archive_sha256"])
            with gzip.open(archive, "rb") as stream:
                raw = stream.read(8 * 1024**2 + 1)
            self.assertLessEqual(len(raw), 8 * 1024**2)
            files = json.loads(raw)
            self.assertEqual(len(files), identity["files"])
            with tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                for name, data in files.items():
                    relative = Path(name)
                    self.assertFalse(relative.is_absolute() or ".." in relative.parts)
                    path = root / relative
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_bytes(base64.b64decode(data, validate=True))
                report = retained.report(root)
                self.assertEqual(report, expected[platform])
                self.assertEqual(report["plan_sha256"], identity["plan_sha256"])
                self.assertFalse(report["admitted"])
                self.assertEqual(len(report["cases"]), 6)
                for row in report["cases"]:
                    if platform == "macos-14":
                        self.assertFalse(row["completed"])
                        self.assertEqual(row["process"]["stop_reason"], "rss_bytes")
                    else:
                        self.assertTrue(row["completed"])
                        self.assertGreater(row["process"]["sampled_aggregate_rss_bytes"], memory.HEADROOM_RSS)

    def replay(self, root, audits):
        with patch.object(memory.source_reviews, "read_inputs", return_value={}), patch.object(
                memory.terminal_reviews, "report", side_effect=lambda plan, files, path: audits[path.parent.parent.name]):
            return memory.report(root)

    def test_headroom_is_required_for_every_smol_sample_and_failures_stay_visible(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            audits = fixture(root)
            self.assertTrue(self.replay(root, audits)["admitted"])
            failed = audits["default-0"]["attempts"][0]
            failed.update(status="failed", failure="capture process did not complete")
            (root / "default-0/result.json").unlink()
            result = self.replay(root, audits)
            self.assertTrue(result["admitted"])
            self.assertFalse(result["cases"][0]["completed"])
            current = audits["smol-2"]
            current["attempts"][0]["process"]["sampled_aggregate_rss_bytes"] = memory.HEADROOM_RSS + 1
            (root / "smol-2/result.json").write_text(json.dumps([{
                "case": "configured", "provider_requests": 2, "report": current}]))
            self.assertFalse(self.replay(root, audits)["admitted"])

    def test_changed_limits_runtime_options_and_missing_attempts_refuse(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            audits = fixture(root)
            audits["default-0"]["attempts"][0]["process"]["limits"]["rss_bytes"] += 1
            with self.assertRaisesRegex(ValueError, "resource limit"):
                self.replay(root, audits)
            audits["default-0"]["attempts"][0]["process"]["limits"] = copy.deepcopy(memory.LIMITS)
            path = root / "default-0/command.json"
            command = json.loads(path.read_bytes())
            command["cell"]["bun_options"] = "--smol"
            path.write_text(json.dumps(command))
            with self.assertRaisesRegex(ValueError, "runtime option"):
                self.replay(root, audits)
            path.unlink()
            with self.assertRaises(FileNotFoundError):
                self.replay(root, audits)

    def test_source_delivery_must_still_be_verified(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            audits = fixture(root)
            path = root / "smol-0/result.json"
            record = json.loads(path.read_bytes())
            record[0]["provider_requests"] = 1
            path.write_text(json.dumps(record))
            with self.assertRaisesRegex(ValueError, "source-delivery"):
                self.replay(root, audits)

    def test_complete_gate_refuses_local_execution_before_creating_files(self):
        with tempfile.TemporaryDirectory() as temporary, patch.dict(os.environ, {}, clear=True):
            root = Path(temporary) / "unstarted"
            with self.assertRaisesRegex(ValueError, "GitHub"):
                memory.check(root, Path("unused"), Path("unused"))
            self.assertFalse(root.exists())


if __name__ == "__main__":
    unittest.main()
