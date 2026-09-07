"""Regressions for acceptance grading and evidence boundaries, without an agent service."""

import copy
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest

TOOLS = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(TOOLS))
spec = importlib.util.spec_from_file_location("agent_eval_harness", TOOLS / "agent-eval.py")
harness = importlib.util.module_from_spec(spec)
spec.loader.exec_module(harness)


class WorkflowEvidence(unittest.TestCase):
    def setUp(self):
        self.original = {"src/lib.rs": {"sha256": "original", "mode": 420}}
        self.changed = {"src/lib.rs": {"sha256": "changed", "mode": 420}}

        def event(tool, before, after, result=None):
            if tool == "check":
                result = {"exit_code": 0, "result": {"schema": "fr-checks-1", "executed": True, "passed": True}}
            return {"request": {"tool": tool}, "before": before, "after": after,
                    "visible": json.dumps(result or {}), "sentinel": "Preserve this independent later edit.\n"}

        a, b = self.original, self.changed
        self.events = [event("check", a, a), event("replace", a, b), event("check", b, b),
                       event("reverse", b, a), event("check", a, a), event("apply", a, b),
                       event("check", b, b), event("receiver", b, b, {"patch_applied": True, "matches": True})]

    def grade(self, events=None):
        return harness.workflow(self.events if events is None else events, self.original, self.changed)

    def test_exact_ordered_workflow_passes(self):
        self.assertTrue(self.grade()["workflow_ordered"])

    def test_missing_check_at_any_required_stage_fails(self):
        for index in (0, 2, 4, 6):
            with self.subTest(index=index):
                self.assertFalse(self.grade(self.events[:index] + self.events[index+1:])["workflow_ordered"])

    def test_repeated_initial_checks_do_not_substitute_for_reversal_checks(self):
        events = [self.events[0]] * 4 + [self.events[i] for i in (1, 2, 3, 5, 6, 7)]
        self.assertFalse(self.grade(events)["workflow_ordered"])

    def test_previews_and_lost_sentinel_do_not_count_as_exact_undo(self):
        for modification in ("preview", "sentinel", "mode"):
            with self.subTest(modification=modification):
                events = copy.deepcopy(self.events)
                if modification == "preview":
                    events[3]["after"] = self.changed
                elif modification == "sentinel":
                    events[3]["sentinel"] = "Lost edit"
                else:
                    events[3]["after"]["src/lib.rs"]["mode"] = 493
                self.assertFalse(self.grade(events)["undo_exact"])

    def test_a_check_that_mutates_source_cannot_validate_a_snapshot(self):
        events = copy.deepcopy(self.events)
        events[2]["before"] = self.original
        self.assertFalse(self.grade(events)["workflow_ordered"])

    def test_failed_check_needs_a_passing_retry_in_the_same_stage(self):
        events = copy.deepcopy(self.events)
        events[2]["visible"] = json.dumps({"exit_code": 1, "result": {"schema": "fr-checks-1", "executed": True, "passed": False}})
        self.assertFalse(self.grade(events)["workflow_ordered"])
        events.insert(3, self.events[2])
        self.assertTrue(self.grade(events)["workflow_ordered"])

    def test_incremental_edits_can_reach_the_validated_result(self):
        events = copy.deepcopy(self.events)
        intermediate = {"src/lib.rs": {"sha256": "partial", "mode": 420}}
        events[1]["after"] = intermediate
        events.insert(2, {**self.events[1], "before": intermediate})
        self.assertTrue(self.grade(events)["workflow_ordered"])


class Boundaries(unittest.TestCase):
    def test_cargo_ancestor_refuses_before_creating_a_trial(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "Cargo.toml").write_text("[workspace]\n")
            with self.assertRaisesRegex(ValueError, "outside Cargo"):
                harness.prepare(root / "sessions", Path("unused"))
            self.assertFalse((root / "sessions").exists())

    def test_source_archive_is_pinned_and_upstream_oracles_fail_for_the_expected_reason(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "project"
            harness.unpack(root)
            self.assertEqual(harness.digest(harness.ARCHIVE.read_bytes()), harness.ARCHIVE_SHA)
            self.assertTrue((root / "LICENSE").is_file())
            for task, stage in (("unicode-dice", 2), ("normalized-osa", 1)):
                result = harness.verify(root, task)
                self.assertFalse(result["passed"])
                self.assertEqual(result["stage"], stage, result)

    def test_path_escape_and_cross_arm_source_access_refuse(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            with self.assertRaisesRegex(ValueError, "leaves"):
                harness.within(root, "../escape")
            with self.assertRaisesRegex(ValueError, "project show"):
                harness.action(root, {"arm": "fr"}, {"tool": "read", "path": "src/lib.rs"})
            with self.assertRaisesRegex(ValueError, "ordinary-file arm"):
                harness.action(root, {"arm": "fr"}, {"tool": "replace", "path": "src/lib.rs"})
            with self.assertRaisesRegex(ValueError, "shared"):
                harness.action(root, {"arm": "files"}, {"tool": "fr", "args": ["project", "map"]})


if __name__ == "__main__":
    unittest.main()
