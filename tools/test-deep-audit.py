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

    def test_prepared_audit_rejects_stale_swapped_and_changed_binaries(self):
        for corrupt in (None, "revision", "target", "sha256", "extra", "binary"):
            with self.subTest(corrupt=corrupt), tempfile.TemporaryDirectory() as temp:
                directory = Path(temp)
                binary = directory / "commands_agree"
                binary.write_bytes(b"compiled audit")
                manifest = {"revision": "revision", "target": "commands_agree",
                            "sha256": audit.digest(binary)}
                if corrupt == "binary":
                    binary.write_bytes(b"changed executable")
                elif corrupt:
                    manifest[corrupt] = "different"
                (directory / "manifest.json").write_text(json.dumps(manifest))
                if corrupt:
                    with self.assertRaises(ValueError):
                        audit.prepared_command(directory, "commands_agree", "revision")
                else:
                    self.assertEqual(audit.prepared_command(directory, "commands_agree", "revision"),
                                     [str(binary.resolve())])
                    with self.assertRaises(ValueError):
                        audit.prepared_command(directory, "conformance", "revision")

    def test_build_retains_only_one_matching_test_executable(self):
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp)
            executable = directory / "compiled"
            executable.write_bytes(b"compiled audit")
            row = {"reason": "compiler-artifact", "target": {"name": "commands_agree"},
                   "profile": {"test": True}, "executable": str(executable)}
            for rows in ([], [row, row], [row]):
                output = directory / str(len(rows))
                with patch.object(audit.subprocess, "check_output", return_value="\n".join(map(json.dumps, rows))):
                    if len(rows) != 1:
                        with self.assertRaises(ValueError):
                            audit.build(output, "revision")
                        self.assertFalse(output.exists())
                    else:
                        audit.build(output, "revision")
                        self.assertEqual(audit.prepared_command(output, "commands_agree", "revision"),
                                         [str((output / "commands_agree").resolve())])

    def test_prepared_execution_keeps_inventory_and_failure_checks(self):
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp)
            with patch.object(audit, "prepared_command", return_value=["/prepared/audit"]), \
                    patch.object(audit.subprocess, "check_output", return_value="one: test\n"), \
                    patch.object(audit.subprocess, "run", side_effect=subprocess.CalledProcessError(1, "audit")) as run:
                with self.assertRaises(subprocess.CalledProcessError):
                    audit.run("commands_agree", 0, directory, "revision", directory)
                self.assertEqual(run.call_args.args[0][0], "/prepared/audit")
                self.assertIn("--include-ignored", run.call_args.args[0])
                self.assertFalse(list(directory.glob("*.json")))

    def test_expensive_usage_scans_get_separate_shards(self):
        names = sorted(audit.WEIGHTS)
        groups = audit.shards.partition(names, audit.COUNTS["commands_agree"], audit.WEIGHTS)
        slow = {"the_self_hosted_recipe_replays_without_changing_the_workspace",
                "usages_reports_the_references_that_resolved_to_the_symbol",
                "usages_reports_the_name_where_it_appears_in_prose"}
        self.assertTrue(all(len(slow.intersection(group)) <= 1 for group in groups))

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
