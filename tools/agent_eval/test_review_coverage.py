"""Keep missing reviews, limitations and unverified findings visible."""
import copy
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval import review_coverage as coverage, terminal_review_runner as runner
from agent_eval import test_terminal_reviews as fixtures
from agent_eval.study import digest


def frozen():
    value, snapshots = fixtures.frozen(3)
    value["plan"]["provenance"]["coverage"] = [
        {"id": "requirement-" + str(i), "description": "Check behavior " + str(i), "question": "question-" + str(i)}
        for i in range(3)]
    value["sha256"] = digest(value["plan"])
    return value, snapshots


def capture(root, value, snapshots, index, failed=False, empty=False):
    cell = value["plan"]["cells"][index]
    folder = root / cell["id"]
    folder.mkdir()
    def mutate(messages, rows):
        if failed:
            messages[-1]["info"]["error"] = {"name": "StructuredOutputError"}
        if empty:
            messages[-1]["info"]["structured"]["answer"]["findings"] = []
            messages[-1]["parts"][1]["state"]["input"]["answer"]["findings"] = []
    process = fixtures.write_capture(folder, value["plan"], snapshots, cell, mutate=mutate)
    return runner.seal(value, snapshots, cell, folder, process)


class Coverage(unittest.TestCase):
    def test_one_model_submission_does_not_complete_another_models_assignment(self):
        value, snapshots = frozen()
        plan = value["plan"]
        plan["models"].append({**plan["models"][0], "modelID": "second-model"})
        plan["cells"] = [{"id": t["id"] + "-" + str(i), "task": t["id"], "model": i, "arm": "fr"}
                         for t in plan["tasks"] for i in range(2)]
        value["sha256"] = digest(plan)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            capture(root, value, snapshots, 0)
            result = coverage.report(value, snapshots, root)
            self.assertEqual(result["completed"], 1)
            self.assertEqual(result["requirements_with_complete_submissions"], 0)
            self.assertFalse(result["questions"][0]["all_submitted"])
            self.assertEqual([a["status"] for a in result["questions"][0]["attempts"]], ["completed", "not_started"])

    def test_missing_failed_and_completed_reviews_remain_distinct(self):
        value, snapshots = frozen()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            capture(root, value, snapshots, 0)
            capture(root, value, snapshots, 1, failed=True)
            result = coverage.report(value, snapshots, root)
            self.assertEqual((result["completed"], result["failed"], result["not_started"]), (1, 1, 1))
            self.assertEqual(result["requirements_with_complete_submissions"], 1)
            self.assertFalse(result["all_submitted"] or result["task_accepted"] or result["claims_verified"])
            first, second, third = [q["attempts"][0] for q in result["questions"]]
            self.assertEqual(first["limitations"], "One small source fixture only.")
            self.assertFalse(first["findings"][0]["verified_counterexample"])
            self.assertIsNone(second["findings"])
            self.assertIsNone(third["limitations"])
            self.assertTrue(second["failure"])

    def test_empty_submissions_do_not_accept_requirements(self):
        value, snapshots = frozen()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for i in range(3):
                capture(root, value, snapshots, i, empty=True)
            result = coverage.report(value, snapshots, root)
            self.assertTrue(result["all_submitted"])
            self.assertEqual(result["requirements_with_complete_submissions"], 3)
            self.assertFalse(result["task_accepted"] or result["claims_verified"])
            self.assertTrue(all(q["attempts"][0]["limitations"] for q in result["questions"]))

    def test_two_failures_preserve_unstarted_coverage(self):
        value, snapshots = frozen()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for i in range(2):
                capture(root, value, snapshots, i, failed=True)
            result = coverage.report(value, snapshots, root)
            self.assertEqual((result["failed"], result["not_started"]), (2, 1))
            self.assertEqual(result["requirements_with_complete_submissions"], 0)

    def test_unknown_duplicate_and_unassigned_requirements_refuse(self):
        value, _ = frozen()
        rows = value["plan"]["provenance"]["coverage"]
        names = ["question-" + str(i) for i in range(3)]
        malformed = [None, [], rows + [rows[0]], rows[:-1]]
        for field, invalid in (("question", "unknown"), ("question", []), ("id", "Bad ID"),
                               ("description", " "), ("description", "x" * 513)):
            changed = copy.deepcopy(rows)
            changed[0][field] = invalid
            malformed.append(changed)
        for rows in malformed:
            with self.subTest(rows=rows), self.assertRaises(ValueError):
                coverage.checked(rows, names)

    def test_changed_frozen_assignment_is_not_relabelled_after_capture(self):
        value, snapshots = frozen()
        value["plan"]["provenance"]["coverage"][0]["question"] = "question-1"
        with tempfile.TemporaryDirectory() as temporary, self.assertRaisesRegex(ValueError, "plan changed"):
            coverage.report(value, snapshots, Path(temporary))


if __name__ == "__main__":
    unittest.main()
