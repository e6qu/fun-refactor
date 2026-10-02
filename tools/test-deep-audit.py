"""Check audit selection and coverage without running local builds."""

import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("deep_audit", Path(__file__).with_name("deep-audit.py"))
assert spec and spec.loader
audit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(audit)


class AuditTests(unittest.TestCase):
    def test_inventory_rejects_empty_duplicates_and_unknown_output(self):
        self.assertEqual(audit.inventory("nested::b: test\na: test\n\n2 tests, 0 benchmarks\n"), ["a", "nested::b"])
        for text in ("", "a: test\na: test\n", "a: benchmark\n", "error: listing failed\n", ": test\n"):
            with self.subTest(text=text), self.assertRaises(ValueError):
                audit.inventory(text)

    def test_each_discovered_test_runs_once_including_unknown_names(self):
        names = sorted([*audit.WEIGHTS, "new_audit", "nested::future", "another_test"])
        assigned = []
        def listed(command, **kwargs):
            skipped = [command[i + 1] for i, arg in enumerate(command) if arg == "--skip"]
            return "".join(name + ": test\n" for name in names if not any(s in name for s in skipped))
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp)
            for index in range(audit.COUNTS["commands_agree"]):
                with patch.object(audit.subprocess, "check_output", side_effect=listed), patch.object(audit.subprocess, "run") as run:
                    audit.run("commands_agree", index, directory, "revision")
                report = json.loads((directory / f"commands_agree-{index}.json").read_text())
                assigned.extend(report["selected"])
                command = run.call_args.args[0]
                self.assertIn("--include-ignored", command)
                self.assertEqual(command[command.index("--test-threads") + 1], "1")
                self.assertTrue(run.call_args.kwargs["check"])
            self.assertEqual(sorted(assigned), names)

    def test_filter_collisions_and_failed_tests_produce_no_report(self):
        for failed in (False, True):
            with self.subTest(failed=failed), tempfile.TemporaryDirectory() as temp:
                directory = Path(temp)
                output = ["one: test\n", "one: test\n" if failed else "other: test\n"]
                with patch.object(audit.subprocess, "check_output", side_effect=output), patch.object(audit.subprocess, "run", side_effect=subprocess.CalledProcessError(1, "cargo")) as run:
                    with self.assertRaises(subprocess.CalledProcessError if failed else ValueError):
                        audit.run("conformance", 0, directory, "revision")
                self.assertEqual(run.call_count, int(failed))
                self.assertFalse(list(directory.iterdir()))

    def evidence(self, directory):
        for target, count in audit.COUNTS.items():
            names = [f"test_{index}" for index in range(count)]
            for index in range(count):
                (directory / f"{target}-{index}.json").write_text(json.dumps({
                    "revision": "revision", "target": target, "index": index,
                    "count": count, "inventory": names, "selected": [names[index]],
                }))

    def test_complete_coverage_rejects_missing_duplicate_stale_and_mixed_reports(self):
        for corrupt in (None, "missing", "extra", "revision", "inventory", "selected", "index", "count", "target"):
            with self.subTest(corrupt=corrupt), tempfile.TemporaryDirectory() as temp:
                directory = Path(temp)
                self.evidence(directory)
                path = directory / "commands_agree-0.json"
                report = json.loads(path.read_text())
                if corrupt == "missing":
                    path.unlink()
                elif corrupt == "extra":
                    (directory / "unexpected.json").write_text("{}")
                elif corrupt:
                    report[corrupt] = {"revision": "stale", "inventory": ["different"],
                                       "selected": ["test_0", "test_1"], "index": 5,
                                       "count": 7, "target": "other"}[corrupt]
                    path.write_text(json.dumps(report))
                if corrupt:
                    with self.assertRaises(ValueError):
                        audit.verify(directory, "revision")
                else:
                    audit.verify(directory, "revision")


if __name__ == "__main__":
    unittest.main()
