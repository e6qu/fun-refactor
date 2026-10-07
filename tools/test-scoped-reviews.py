"""Replay scoped preparation and retained outcomes without model or code execution."""
import base64
import gzip
import json
from pathlib import Path
import runpy
import sys
import unittest

from agent_eval import review_coverage, source_reviews, terminal_reviews

REPO = Path(__file__).resolve().parents[1]
HERE = REPO / "tests/agent-eval/opencode/reviews/2026-10-07-packaging-scoped"


class ScopedReviews(unittest.TestCase):
    def test_frozen_questions_preserve_sources_limits_and_assign_all_requirements(self):
        prepare = runpy.run_path(str(HERE / "prepare.py"))
        design = json.loads((HERE / "design.json").read_bytes())
        self.assertEqual(prepare["prepare"](), design)
        collector = runpy.run_path(str(HERE / "collect.py"))
        frozen = json.loads((HERE / "plan.json").read_bytes())
        snapshots = source_reviews.read_inputs(HERE)
        plan = terminal_reviews.checked(frozen, snapshots)
        self.assertEqual(plan["provenance"]["bindings"], collector["bindings"]())
        self.assertEqual(len(plan["cells"]), 6)
        self.assertEqual(len(plan["provenance"]["coverage"]), 11)
        self.assertEqual(plan["provenance"]["coverage"], design["coverage"])
        original = source_reviews.read_inputs((REPO / design["source_archive"]["path"]).parent)["packaging-prerelease"]
        self.assertTrue(all(files == original for files in snapshots.values()))
        old = json.loads((HERE.with_name("2026-10-07-packaging-low") / "plan.json").read_bytes())["plan"]
        self.assertEqual(plan["limits"], old["limits"])
        self.assertEqual(plan["models"], old["models"])
        with gzip.open(HERE.parents[1] / "reasoning/2026-10-07-low/workstation.json.gz", "rb") as stream:
            raw = stream.read(8 * 1024**2 + 1)
        self.assertLessEqual(len(raw), 8 * 1024**2)
        admitted = json.loads(base64.b64decode(json.loads(raw)["plan.json"], validate=True))["plan"]
        for field in ("binary_sha256", "opencode_sha256"):
            self.assertEqual(plan[field], admitted[field])

    def test_retained_report_and_coverage_replay_without_upgrading_failures(self):
        frozen = json.loads((HERE / "plan.json").read_bytes())
        snapshots = source_reviews.read_inputs(HERE)
        report = terminal_reviews.report(frozen, snapshots, HERE / "attempts")
        coverage = review_coverage.report(frozen, snapshots, HERE / "attempts")
        for name, result in (("report.json", report), ("coverage.json", coverage)):
            if (HERE / name).exists():
                self.assertEqual(result, json.loads((HERE / name).read_bytes()))
        self.assertFalse(coverage["task_accepted"] or coverage["claims_verified"])
        self.assertEqual(report["completed"] + report["failed"] + report["not_started"], 6)
        if (HERE / "stop.json").exists():
            self.assertFalse(json.loads((HERE / "stop.json").read_bytes())["resume_allowed"])
            collector = runpy.run_path(str(HERE / "collect.py"))
            with self.assertRaisesRegex(ValueError, "permanently stopped"):
                collector["collect"]("unused", Path(sys.executable), Path(sys.executable))


if __name__ == "__main__":
    unittest.main()
