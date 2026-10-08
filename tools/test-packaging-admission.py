"""Check the retained policy evidence and bounded admission without running source."""
import hashlib
import json
from pathlib import Path
import runpy
import unittest

from agent_eval import isolated_grade, source_reviews
from agent_eval.study import digest

ROOT = Path(__file__).resolve().parents[1]
HERE = ROOT / "tests/agent-eval/opencode/reviews/2026-10-07-packaging-scoped"


class Admission(unittest.TestCase):
    def test_hosted_policy_result_binds_exact_source_program_and_grader(self):
        check = runpy.run_path(str(HERE / "verify-policy.py"))
        frozen, candidate, unchanged = check["inputs"]()
        directory = HERE / "policy-verification"
        provenance = json.loads((directory / "provenance.json").read_bytes())
        for name, sha in provenance["files"].items():
            self.assertEqual(source_reviews.identity(directory / name), sha)
        result = json.loads((directory / "result.json").read_bytes())
        grader = json.loads((directory / "grader.json").read_bytes())
        isolated_grade.validate(grader)
        original = json.loads((ROOT / "tests/agent-eval/opencode/candidates/packaging-prerelease-grader.json").read_bytes())
        self.assertEqual(grader["limits"], original["limits"])
        self.assertEqual(grader["image"], original["image"])
        self.assertEqual(grader["command"], ["python3", "-I", "-c", check["PROGRAM"]])
        self.assertEqual(result["plan_sha256"], frozen["sha256"])
        self.assertEqual(result["checker_sha256"], source_reviews.identity(HERE / "verify-policy.py"))
        self.assertEqual(result["candidate_sha256"], digest(candidate))
        self.assertEqual(result["unchanged"], unchanged)
        self.assertEqual(result["policy_cases"], 324)
        self.assertEqual(result["grade"]["grader_sha256"], source_reviews.identity(directory / "grader.json"))
        self.assertEqual(result["grade"]["outcome"], "passed")
        self.assertEqual([c["id"] for c in result["grade"]["cases"]], ["ordinary", "inferred", "empty"])
        for case in result["grade"]["cases"]:
            self.assertTrue(case["passed"])
            self.assertIsNone(case["execution"]["stop_reason"])
            self.assertFalse(case["container_state"]["OOMKilled"])
            self.assertEqual(case["stdout_sha256"], hashlib.sha256(b"ok\n").hexdigest())

    def test_admission_covers_every_area_without_rewriting_failures(self):
        admission = json.loads((HERE / "admission.json").read_bytes())
        coverage = json.loads((HERE / "coverage.json").read_bytes())
        for name, sha in admission["evidence"].items():
            self.assertEqual(source_reviews.identity(HERE / name), sha)
        expected = {(r["id"], q["id"]) for q in coverage["questions"] for r in q["assignments"]}
        self.assertEqual({(r["id"], r["question"]) for r in admission["requirements"]}, expected)
        self.assertEqual(len(admission["requirements"]), 11)
        self.assertEqual(admission["plan_sha256"], coverage["plan_sha256"])
        self.assertTrue(admission["task_accepted_for_bounded_pilot"])
        self.assertFalse(admission["pilot_started"] or admission["all_model_reviews_completed"])
        self.assertFalse(coverage["task_accepted"] or coverage["claims_verified"])
        self.assertEqual(coverage["failed"], 1)
        self.assertEqual(len(admission["execution_prerequisites"]), 4)


if __name__ == "__main__":
    unittest.main()
