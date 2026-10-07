"""Replay scoped preparation and retained outcomes without model or code execution."""
import base64
import gzip
import json
from pathlib import Path
import runpy
import sys
import unittest
from unittest.mock import patch

from agent_eval import review_coverage, source_reviews, terminal_reviews

REPO = Path(__file__).resolve().parents[1]
HERE = REPO / "tests/agent-eval/opencode/reviews/2026-10-07-packaging-scoped"


class ScopedReviews(unittest.TestCase):
    def test_policy_verification_refuses_local_execution_and_checks_source_identity(self):
        import tempfile
        from agent_eval import isolated_grade
        check = runpy.run_path(str(HERE / "verify-policy.py"))
        frozen, candidate, unchanged = check["inputs"]()
        self.assertEqual(frozen["sha256"], json.loads((HERE / "plan.json").read_bytes())["sha256"])
        self.assertEqual(unchanged["unchanged_files"], len(candidate) - 1)
        with tempfile.TemporaryDirectory() as temporary, patch.dict("os.environ", {}, clear=True), \
                patch.object(isolated_grade, "invoke") as invoke:
            output = Path(temporary) / "unstarted"
            with self.assertRaisesRegex(ValueError, "on GitHub"):
                check["check"](output)
            self.assertFalse(output.exists())
            invoke.assert_not_called()

    def test_repeated_postrelease_claim_uses_identical_verified_source(self):
        from agent_eval.study import digest
        assessment = json.loads((HERE / "assessment.json").read_bytes())
        frozen = json.loads((HERE / "plan.json").read_bytes())
        snapshots = source_reviews.read_inputs(HERE)
        report = terminal_reviews.report(frozen, snapshots, HERE / "attempts")
        self.assertEqual(assessment["plan_sha256"], frozen["sha256"])
        self.assertFalse(assessment["task_accepted"] or assessment["pilot_started"])
        self.assertEqual((report["completed"], report["failed"], report["not_started"]), (5, 1, 0))
        decision, = assessment["finding_decisions"]
        attempt = next(a for a in report["attempts"] if a["cell"]["id"] == decision["cell"])
        finding, = attempt["audit"]["review"]["findings"]
        self.assertEqual(digest(finding), decision["finding_sha256"])
        self.assertEqual(decision["decision"], "rejected")
        path = REPO / decision["verification"]
        self.assertEqual(source_reviews.identity(path), decision["verification_sha256"])
        previous = json.loads(path.read_bytes())
        self.assertEqual(previous["decision"], "rejected")
        self.assertEqual(previous["controls_sha256"], decision["controls_sha256"])
        self.assertEqual(snapshots[attempt["cell"]["task"]], source_reviews.read_inputs(path.parent)["packaging-whole-task"])
        observed = assessment["fr_observation"]
        folder = HERE / "attempts" / observed["cell"]
        rows = [json.loads(line) for line in (folder / "tools.jsonl").read_bytes().splitlines()]
        query, = [r for r in rows if r["params"]["name"] == "fr_explore"]
        self.assertEqual(query["params"]["arguments"]["term"], observed["term"])
        self.assertEqual(query["result"]["rows"], [])
        self.assertEqual(query["result"]["page"]["total"], observed["returned_names"])
        self.assertTrue(any(r["params"]["name"] == "read_source" for r in rows[1:]))
        failed, = [a for a in report["attempts"] if a["status"] == "failed"]
        terminal = json.loads((HERE / "attempts" / failed["cell"]["id"] / "terminal.json").read_bytes())["info"]
        self.assertEqual(terminal["finish"], "length")
        self.assertEqual(terminal["tokens"]["reasoning"] + terminal["tokens"]["output"], 2048)
        self.assertIsNone(failed["process"]["stop_reason"])

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
