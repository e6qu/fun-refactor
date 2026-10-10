#!/usr/bin/env python3
"""Replay body preparation and recovery evidence, including corruption counterexamples."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("bodies", ROOT / "tools/guide-body-context.py")
bodies = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bodies)
REPORT = ROOT / "tests/agent-eval/results/2026-10-10-selection-workflows/guide-body-context.json"


class BodyEvidence(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        manifest = json.loads((REPORT.parent / "manifest.json").read_text())
        for name, expected in manifest["files"].items():
            path = (REPORT.parent / name).resolve()
            if not path.is_relative_to(REPORT.parent.resolve()) or hashlib.sha256(path.read_bytes()).hexdigest() != expected:
                raise ValueError("retained body artifact digest changed")
        cls.report = json.loads(REPORT.read_text())
        if (manifest["collection"]["source_commit"] != cls.report["source_commit"]
                or manifest["binary_sha256"] != cls.report["binary_sha256"]):
            raise ValueError("retained body collection identity changed")

    def changed(self, fault=None):
        report = copy.deepcopy(self.report)
        row = next(r for r in report["runs"] if r["arm"] == "builder" and r["fault"] == fault)
        return report, row

    def test_all_programs_refusals_and_recoveries_replay(self):
        summary = bodies.audit(self.report)
        self.assertEqual(summary["cells"], 40)
        for difference in summary["difference"].values():
            self.assertEqual(difference, {"program_bytes": -595, "agent_bytes": -595})

    def test_missing_or_duplicate_cells_are_not_complete(self):
        for replacement in ([], [self.report["runs"][0]]):
            report = copy.deepcopy(self.report)
            report["runs"] = report["runs"][:-1] + replacement
            with self.assertRaisesRegex(ValueError, "missing or duplicate"):
                bodies.audit(report)

    def test_caller_and_traffic_bytes_are_recomputed(self):
        for field in ("program_bytes", "agent_bytes"):
            report, row = self.changed()
            row[field] -= 1
            with self.assertRaisesRegex(ValueError, "accounting"):
                bodies.audit(report)
        report, row = self.changed()
        row["events"][0]["response"]["report"]["extra"] = "unaccounted"
        with self.assertRaisesRegex(ValueError, "accounting"):
            bodies.audit(report)

    def test_failed_delivery_cannot_hide_applied_source(self):
        report, row = self.changed("applied-check")
        row["source"] = row["before"]
        with self.assertRaisesRegex(ValueError, "unexpected source"):
            bodies.audit(report)

    def test_failed_delivery_cannot_claim_a_patch(self):
        report, row = self.changed("applied-check")
        row["patch"] = "unearned patch"
        with self.assertRaisesRegex(ValueError, "withhold delivery"):
            bodies.audit(report)

    def test_recovery_must_restore_source_and_pass_checks(self):
        report, row = self.changed("applied-check")
        row["recovery"]["source"] = row["source"]
        with self.assertRaisesRegex(ValueError, "restore checked source"):
            bodies.audit(report)

    def test_recovery_must_use_the_recorded_transaction(self):
        report, row = self.changed("applied-check")
        row["recovery"]["events"][2]["request"]["arguments"][2] = "999"
        with self.assertRaisesRegex(ValueError, "retained transaction"):
            bodies.audit(report)

    def test_undo_must_preserve_a_later_conflicting_edit(self):
        report, row = self.changed("undo-conflict")
        row["recovery"]["source"] = row["before"]
        with self.assertRaisesRegex(ValueError, "overwrote a later edit"):
            bodies.audit(report)


if __name__ == "__main__":
    unittest.main()
