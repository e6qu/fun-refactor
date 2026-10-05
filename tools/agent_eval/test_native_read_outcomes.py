"""Read-only attempt costs preserve failures, missing cells and original verdicts."""
import copy
import hashlib
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval import native_mcp, native_read_outcomes as outcomes, opencode_native as native
from agent_eval.study import encode, load

ROOT = Path(__file__).resolve().parents[2]
COHORT = ROOT / "tests/agent-eval/opencode/results/2026-10-02-source-references"


class ReadOutcomes(unittest.TestCase):
    def test_recovery_cli_freeze_and_failed_call_keep_costs_and_pending_cells(self):
        manifest = ROOT / "tests/agent-eval/opencode/explanations.json"
        raw = subprocess.check_output([sys.executable, str(ROOT / "tools/native-rehearsal.py"),
            "freeze", str(manifest), "--binary", __file__, "--recovery-hints"], timeout=10)
        frozen = json.loads(raw)
        self.assertEqual(frozen["plan"]["tools_schema_version"], 6)
        source = native.checked(frozen)
        cell = next(c for c in source["cells"] if c["arm"] == "fr")
        task = next(t for t in source["tasks"] if t["id"] == cell["task"])
        log = io.BytesIO()
        server = native_mcp.Server({"files": task["files"], "arm": "fr", "binary": "unused",
                                    "workspace": "unused", "tools_schema_version": 6}, log)
        refusal = server.call({"name": "fr_explore", "arguments": {"term": "value", "mode": "behavior"}})
        self.assertTrue(refusal["isError"])
        self.assertIn("next", json.loads(refusal["content"][0]["text"]))
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "plan.json").write_bytes(encode(frozen))
            output = root / "attempts" / cell["id"]
            output.mkdir(parents=True)
            record = {"cell": cell, "plan_sha256": frozen["sha256"], "status": "failed", "passed": False,
                      "failure": "synthetic stopped session", "processes": [], "seconds": 0.25}
            (output / "record.json").write_bytes(encode(record))
            (output / "tools.jsonl").write_bytes(log.getvalue())
            (output / "opencode.stdout").write_bytes(b"")
            inventory = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in output.iterdir()}
            (output / "manifest.json").write_bytes(encode({"files": inventory}))
            report = outcomes.cohort(root)
        self.assertEqual(report["outcomes"], {"failed": 1, "pending": len(source["cells"]) - 1})
        cost = next(r["costs"] for r in report["attempts"] if r["cell"] == cell)
        self.assertEqual(cost["observed"]["refused_calls"], 1)
        self.assertEqual(cost["observed"]["source_page_bytes"], 0)
        self.assertGreater(cost["observed"]["produced_result_bytes"], 100)
        self.assertFalse(cost["coverage"]["collection_complete"])
        self.assertFalse(cost["coverage"]["export_and_model_identity_verified"])
        self.assertTrue(all(g["collection_wall_seconds_per_pass"] is None for g in report["groups"]))

    def test_retained_report_includes_failed_work_without_changing_outcomes(self):
        result = outcomes.cohort(COHORT)
        self.assertEqual(load(ROOT / "tests/agent-eval/opencode/native-read-outcomes.json"),
                         {"schema": "fr-native-read-comparison-1", "cohorts": [result]})
        self.assertEqual((ROOT / "docs/native-read-outcomes.md").read_text(), outcomes.markdown([result]))
        self.assertEqual(result["outcomes"], {"failed": 4, "passed": 2})
        for row in result["attempts"]:
            self.assertGreater(row["costs"]["observed"]["host_calls"], 0)
            self.assertGreater(row["costs"]["usage"]["finished_steps"], 0)
            self.assertIsNone(row["costs"]["usage"]["actual_usd"])
            if row["outcome"] == "failed":
                self.assertFalse(row["costs"]["coverage"]["reported_step_usage_complete"])
                self.assertFalse(row["costs"]["coverage"]["export_and_model_identity_verified"])
        self.assertTrue(all(not pair["both_passed"] and pair["result_bytes_difference"] is None for pair in result["pairs"]))
        for group in result["groups"]:
            selected = [r for r in result["attempts"] if r["cell"]["model"] == group["model"] and r["cell"]["arm"] == group["arm"]]
            calls = sum(r["costs"]["observed"]["host_calls"] for r in selected)
            self.assertEqual(group["observed_work_all_attempts"]["host_calls"], calls)
            if not group["outcomes"].get("passed"):
                self.assertIsNone(group["collection_wall_seconds_per_pass"])

    def test_missing_attempt_has_unknown_costs_and_unresolved_group(self):
        frozen = load(COHORT / "plan.json")
        original = native.report(frozen, COHORT / "attempts")
        altered = copy.deepcopy(original)
        chosen = next(row for row in altered["attempts"] if row["passed"])
        cell = chosen["cell"]
        chosen.clear()
        chosen.update(cell=cell, status="pending", passed=False)
        with patch.object(native, "report", return_value=altered):
            result = outcomes.cohort(COHORT)
        pending = next(row for row in result["attempts"] if row["cell"] == cell)
        self.assertIsNone(pending["costs"])
        group = next(g for g in result["groups"] if g["model"] == cell["model"] and g["arm"] == cell["arm"])
        self.assertFalse(group["all_outcomes_resolved"])
        self.assertEqual(group["attempts_with_retained_costs"], 0)
        self.assertIsNone(group["collection_wall_seconds_per_pass"])


if __name__ == "__main__":
    unittest.main()
