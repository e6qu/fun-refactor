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


def declarations(raw):
    tree = ast.parse(raw)
    functions = (ast.FunctionDef, ast.AsyncFunctionDef)
    names = {(node.name,) for node in tree.body if isinstance(node, functions)}
    for parent in ast.walk(tree):
        if isinstance(parent, ast.ClassDef):
            names.update((parent.name, node.name) for node in parent.body if isinstance(node, functions))
    return names


class CandidateControls(unittest.TestCase):
    def test_all_candidate_edits_parse_without_running_source(self):
        tasks = load(PACK / "manifest.json")["tasks"]
        controls.verify_sources(PACK, tasks)
        self.assertEqual(len(tasks), 3)
        for task in tasks:
            original, rows, public = controls.definitions(PACK, task)
            before = copy.deepcopy(original)
            self.assertEqual(len(rows), 7 if task["id"] == "platformdirs-xdg" else 8)
            self.assertTrue(all(row["expected_public"] == "passed" for row in rows[1:]))
            for row in rows:
                with self.subTest(task=task["id"], control=row["id"]):
                    files = controls.apply(original, row)
                    self.assertEqual(original, before)
                    self.assertEqual(set(files), set(original))
                    for name, item in files.items():
                        if name.endswith(".py"):
                            current = declarations(base64.b64decode(item["data"]))
                            existing = declarations(base64.b64decode(original[name]["data"]))
                            self.assertLessEqual(existing, current, "control removed declarations from " + name)
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
            seen.append((candidate, sha, profile.name == "public.json"))
            return {"outcome": "failed", "cases": []}
        rows = controls.grade_controls(PACK, task, grader=fake)
        self.assertEqual(len(seen), 19)
        self.assertEqual(len(rows), 8)
        self.assertEqual(sum("baseline_grade" in row for row in rows), 3)
        for candidate in {item[0] for item in seen}:
            executions = [item for item in seen if item[0] == candidate]
            self.assertIn(len(executions), (2, 3))
            self.assertEqual(sum(item[2] for item in executions), 1)
            self.assertEqual(len({item[1] for item in executions}), len(executions))
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

    def test_baseline_grader_paths_hashes_and_images_are_bound(self):
        task = load(PACK / "manifest.json")["tasks"][0]
        row = load(PACK / "controls.json")[task["id"]][-1]
        image = load(PACK / task["grader"])["image"]
        path, sha = controls.baseline_grader(PACK, row, image)
        self.assertEqual(hashlib.sha256(path.read_bytes()).hexdigest(), sha)
        for key, value in [("grader", "../outside.json"), ("grader", "/tmp/outside.json"),
                           ("sha256", "0" * 64), ("expected", "failed")]:
            changed = {**row, "baseline": {**row["baseline"], key: value}}
            with self.subTest(key=key), self.assertRaises(ValueError):
                controls.baseline_grader(PACK, changed, image)
        with self.assertRaisesRegex(ValueError, "image"):
            controls.baseline_grader(PACK, row, "different-image")

    def test_comparison_requires_an_earlier_pass_on_the_same_candidate(self):
        row = {"id": "task/gap", "expected": "failed", "expected_baseline": "passed",
               "baseline_grader_sha256": "old", "grade": {"outcome": "failed", "candidate": {"sha256": "same"}},
               "baseline_grade": {"outcome": "passed", "grader_sha256": "old", "candidate": {"sha256": "same"}}}
        controls.verify([row])
        for key, value in [("outcome", "failed"), ("grader_sha256", "different"), ("candidate", {"sha256": "other"})]:
            with self.subTest(key=key), self.assertRaises(ValueError):
                controls.verify([{**row, "baseline_grade": {**row["baseline_grade"], key: value}}])


if __name__ == "__main__":
    unittest.main()
