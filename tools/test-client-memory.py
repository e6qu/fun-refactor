"""Offline checks of memory-control admission and retained failures."""
import copy
import importlib.util
import json
import os
from pathlib import Path
import tempfile
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
