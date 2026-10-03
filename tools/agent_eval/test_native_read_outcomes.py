"""Read-only attempt costs preserve failures, missing cells and original verdicts."""
import copy
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval import native_read_outcomes as outcomes, opencode_native as native
from agent_eval.study import load

ROOT = Path(__file__).resolve().parents[2]
COHORT = ROOT / "tests/agent-eval/opencode/results/2026-10-02-source-references"


class ReadOutcomes(unittest.TestCase):
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
