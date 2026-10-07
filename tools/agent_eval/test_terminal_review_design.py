"""Verify reusable source identities before freezing a new terminal review."""
import copy
import gzip
from pathlib import Path
import runpy
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval import native_mcp as mcp, source_reviews, terminal_reviews as review
from agent_eval.study import encode

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "tools/prepare-terminal-reviews.py"
DESIGN = ROOT / "tests/agent-eval/opencode/reviews/2026-10-06-terminal-design/design.json"


class Design(unittest.TestCase):
    def test_optional_coverage_is_validated_and_frozen_without_changing_questions(self):
        original, _ = self.cli["prepare"](self.design, ROOT)
        rows = [{"id": "area-" + str(i), "description": "Requirement family " + str(i), "question": q["id"]}
                for i, q in enumerate(self.design["questions"])]
        design = {**self.design, "coverage": rows}
        questions, provenance = self.cli["prepare"](design, ROOT)
        self.assertEqual(questions, original)
        self.assertEqual(provenance["coverage"], rows)
        rows[0]["question"] = "missing"
        with self.assertRaisesRegex(ValueError, "unknown coverage question"):
            self.cli["prepare"](design, ROOT)

    def setUp(self):
        self.cli = runpy.run_path(str(SCRIPT))
        self.design = mcp.decode(review.read(DESIGN))

    def test_new_questions_reuse_unchanged_source_within_existing_budgets(self):
        questions, provenance = self.cli["prepare"](self.design, ROOT)
        frozen, snapshots = review.freeze(questions, self.design["models"],
            Path(sys.executable), Path(sys.executable), provenance)
        self.assertEqual(len(frozen["plan"]["cells"]), 6)
        self.assertEqual(len(snapshots), 3)
        review.checked(frozen, snapshots, execution=True)
        self.assertLessEqual(len(gzip.compress(encode(snapshots))), review.MAX_BYTES)
        self.assertFalse(frozen["plan"]["claims_verified"])

    def test_changed_archive_and_escaping_paths_refuse(self):
        for changes in ({"sha256": "0" * 64}, {"path": "../inputs.json.gz"}, {"path": "/tmp/inputs.json.gz"}):
            design = copy.deepcopy(self.design)
            design["source_archive"].update(changes)
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                self.cli["prepare"](design, ROOT)

    def test_stale_selection_and_false_comparison_refuse(self):
        questions, provenance = self.cli["prepare"](self.design, ROOT)
        questions[0]["selections"][0]["sha256"] = "0" * 64
        with self.assertRaisesRegex(ValueError, "stale packet source"):
            review.freeze(questions, self.design["models"], Path(sys.executable), Path(sys.executable), provenance)
        snapshots = source_reviews.read_inputs((ROOT / self.design["source_archive"]["path"]).parent)
        question = self.design["questions"][0]
        snapshots[question["source_task"]][question["comparison"]]["data"] = "e30="
        with patch.object(source_reviews, "read_inputs", return_value=snapshots):
            with self.assertRaisesRegex(ValueError, "invalid file comparison"):
                self.cli["prepare"](self.design, ROOT)

    def test_freeze_creates_a_replayable_plan_and_will_not_overwrite_it(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "frozen"
            args = [str(SCRIPT), "freeze", str(DESIGN), "--repository", str(ROOT), "--output", str(output),
                    "--fr", sys.executable, "--opencode", sys.executable]
            with patch.object(sys, "argv", args), patch("builtins.print"):
                self.cli["main"]()
                original = (output / "plan.json").read_bytes()
                with self.assertRaises(FileExistsError):
                    self.cli["main"]()
            frozen = mcp.decode(original)
            review.checked(frozen, source_reviews.read_inputs(output), execution=True)
            self.assertEqual(frozen["plan"]["provenance"]["preparation_sha256"], source_reviews.identity(SCRIPT))
            self.assertEqual((output / "plan.json").read_bytes(), original)


if __name__ == "__main__":
    unittest.main()
