"""Replay compact assertion packets against frozen source and retained outcomes."""
import base64
import json
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from agent_eval import source_packets as packets, source_reviews as reviews
from agent_eval.study import load

HERE = Path(__file__).resolve().parents[2] / "tests/agent-eval/opencode/reviews/2026-10-05-assertions"


class AssertionReviews(unittest.TestCase):
    def test_frozen_packets_disclose_changed_and_unchanged_files(self):
        frozen, snapshots = load(HERE / "plan.json"), reviews.read_inputs(HERE)
        plan = reviews.checked(frozen, snapshots)
        self.assertEqual(reviews.identity(HERE / "collect.py"), plan["provenance"]["collector_sha256"])
        self.assertEqual(plan["tools_schema_version"], 6)
        self.assertEqual(len(plan["cells"]), 6)
        for task in plan["tasks"]:
            files = snapshots[task["id"]]
            comparison = json.loads(base64.b64decode(files["review/file-comparison.json"]["data"]))
            packets.check_comparison(files, comparison)
            self.assertEqual([p["same_content"] for p in comparison["pairs"]], [False, True])
            self.assertTrue(all(p["same_executable"] for p in comparison["pairs"]))
            spans = packets.checked(files, task["packet"])
            selected = [s for s in spans if s["path"] == "review/file-comparison.json"]
            self.assertEqual(len(selected), 1)
            self.assertEqual(json.loads(selected[0]["text"]), comparison)
            grader = [s for s in spans if s["path"] == "review/grader.py"]
            self.assertEqual(len(grader), 1)
            self.assertLess(len(grader[0]["text"].encode()), 512)
            self.assertNotIn("elif case", grader[0]["text"])
            self.assertIn("Only the selected grader assertion", task["question"])

    def test_retained_reports_replay_without_promoting_review_claims(self):
        frozen, snapshots = load(HERE / "plan.json"), reviews.read_inputs(HERE)
        report = reviews.report(frozen, snapshots, HERE / "attempts")
        self.assertEqual(load(HERE / "report.json"), report)
        self.assertEqual(load(HERE / "source-reuse.json"), reviews.source_reuse(frozen, snapshots, HERE / "attempts"))
        self.assertFalse(report["claims_verified"])
        self.assertFalse(report["efficiency_comparison"])
        self.assertEqual(report["planned"], report["completed"] + report["failed"] + report["not_started"])


if __name__ == "__main__":
    unittest.main()
