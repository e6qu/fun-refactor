"""Offline control replay; candidate programs execute only in GitHub containers."""
import ast
import base64
import copy
import hashlib
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval import change_controls as controls, opencode_changes
from agent_eval.study import load

ROOT = Path(__file__).resolve().parents[2]
PACK = ROOT / "tests/agent-eval/opencode/candidates"


class CandidateControls(unittest.TestCase):
    def test_all_candidate_edits_parse_without_running_source(self):
        tasks = load(PACK / "manifest.json")["tasks"]
        controls.verify_sources(PACK, tasks)
        self.assertEqual(len(tasks), 3)
        for task in tasks:
            original, rows, public = controls.definitions(PACK, task)
            before = copy.deepcopy(original)
            self.assertEqual(len(rows), 5)
            self.assertTrue(all(row["expected_public"] == "passed" for row in rows[1:]))
            for row in rows:
                with self.subTest(task=task["id"], control=row["id"]):
                    files = controls.apply(original, row)
                    self.assertEqual(original, before)
                    self.assertEqual(set(files), set(original))
                    for name, item in files.items():
                        if name.endswith(".py"):
                            ast.parse(base64.b64decode(item["data"]), filename=name)
                    if row["id"] != "unchanged":
                        self.assertNotEqual(files, original)
            for spec in (load(PACK / task["grader"]), public):
                self.assertEqual(spec["command"][:4], ["python3", "-I", "-B", "-c"])
                ast.parse(spec["command"][4])

    def test_candidate_pack_freezes_with_public_feedback_and_without_model_calls(self):
        with tempfile.TemporaryDirectory() as temporary:
            binary = Path(temporary) / "fr"
            binary.write_bytes(b"identity-only fixture; never executed")
            frozen = opencode_changes.freeze(load(PACK / "manifest.json"), PACK, binary,
                                              public_checks=load(PACK / "public-checks.json"))
            source = opencode_changes.checked(frozen)
            self.assertEqual(frozen["plan"]["tools_schema_version"], 3)
            self.assertEqual(len(source["cells"]), 12)
            for task in source["tasks"]:
                self.assertTrue(all("grader" not in name and "controls" not in name for name in task["files"]))

    def test_historical_single_edit_controls_remain_supported(self):
        pack = ROOT / "tests/agent-eval/opencode/changes"
        for task in load(pack / "manifest.json")["tasks"]:
            files, rows, public = controls.definitions(pack, task)
            self.assertIsNone(public)
            for row in rows:
                updated = controls.apply(files, row)
                self.assertEqual(updated == files, row["id"] == "unchanged")

    def test_sequential_hashes_and_invalid_control_edits(self):
        files = {"a.py": {"data": base64.b64encode(b"value = 1\n").decode(), "executable": False}}
        edits = [{"path": "a.py", "old": "1", "new": "2"}, {"path": "a.py", "old": "2", "new": "3"}]
        updated = controls.apply(files, {"edits": edits})
        self.assertEqual(base64.b64decode(updated["a.py"]["data"]), b"value = 3\n")
        for row in ({"edits": edits[::-1]}, {"edits": edits * 13},
                    {"edits": [{"path": "absent.py", "old": "1", "new": "2"}]},
                    {"edits": [], **edits[0]}):
            with self.subTest(row=row), self.assertRaises(ValueError):
                controls.apply(files, row)

    def test_declared_failure_sets_and_public_success_are_both_checked(self):
        good = {"id": "task/mutation", "expected": "failed", "expected_public": "passed",
                "expected_failed_cases": ["boundary"], "grade": {"outcome": "failed", "cases": [
                    {"id": "boundary", "passed": False}, {"id": "preserved", "passed": True}]},
                "public_grade": {"outcome": "passed"}}
        controls.verify([good])
        for field, value in [("expected", "passed"), ("expected_failed_cases", ["preserved"]),
                             ("expected_public", "failed")]:
            with self.subTest(field=field), self.assertRaises(ValueError):
                controls.verify([{**good, field: value}])
        with self.assertRaises(ValueError):
            controls.verify([])

    def test_grader_dispatch_retains_public_and_private_results_for_same_candidate(self):
        task = load(PACK / "manifest.json")["tasks"][0]
        seen = []
        def fake(candidate, profile, sha):
            self.assertEqual(hashlib.sha256(profile.read_bytes()).hexdigest(), sha)
            self.assertTrue((candidate / "src/dotenv/main.py").is_file())
            seen.append((candidate, sha))
            return {"outcome": "failed", "cases": []}
        rows = controls.grade_controls(PACK, task, grader=fake)
        self.assertEqual(len(seen), 10)
        self.assertEqual(len(rows), 5)
        for private, public in zip(seen[::2], seen[1::2]):
            self.assertEqual(private[0], public[0])
            self.assertNotEqual(private[1], public[1])
        self.assertTrue(all("submission_sha256" in row and "public_grade" in row for row in rows))

    def test_changed_source_inventory_and_failure_case_names_refuse(self):
        tasks = load(PACK / "manifest.json")["tasks"]
        records = load(PACK / "sources.json")
        records[tasks[0]["id"]]["revision"] = "0" * 40
        with patch.object(controls, "load", return_value=records), self.assertRaisesRegex(ValueError, "origin"):
            controls.verify_sources(PACK, tasks)
        expected = load(PACK / "control-failures.json")
        expected[tasks[0]["id"]]["reference"] = ["not-a-case"]
        def changed(path):
            return expected if path.name == "control-failures.json" else load(path)
        with patch.object(controls, "load", side_effect=changed), self.assertRaisesRegex(ValueError, "unknown"):
            controls.definitions(PACK, tasks[0])

    def test_upstream_blob_inventory_rejects_changed_bytes_modes_and_truncated_trees(self):
        files = {"src/example.py": {"data": base64.b64encode(b"pass\n").decode(), "executable": False}}
        record = {"selection": {"prefix": "src/", "root_files": ["LICENSE"]}}
        row = {"path": "src/example.py", "type": "blob", "mode": "100644",
               "sha": hashlib.sha1(b"blob 5\0pass\n").hexdigest()}
        tree = {"truncated": False, "tree": [row]}
        controls.compare_upstream(files, record, tree)
        for bad in ({**tree, "truncated": True}, {**tree, "tree": []},
                    {**tree, "tree": [{**row, "mode": "120000"}]},
                    {**tree, "tree": [{**row, "sha": "0" * 40}]},
                    {**tree, "tree": [row, {**row, "path": "src/omitted.py"}]}):
            with self.subTest(tree=bad), self.assertRaises(ValueError):
                controls.compare_upstream(files, record, bad)


if __name__ == "__main__":
    unittest.main()
