#!/usr/bin/env python3
"""Audit retained route outcomes and reject corrupt comparison evidence offline."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("workflow_route_context", ROOT / "tools/workflow-route-context.py")
routes = importlib.util.module_from_spec(spec)
spec.loader.exec_module(routes)
REPORT = ROOT / "tests/agent-eval/results/2026-10-09-workflow-routes/result.json"


class RouteEvidence(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        manifest = json.loads((REPORT.parent / "manifest.json").read_text())
        for name, digest in manifest["files"].items():
            path = (REPORT.parent / name).resolve()
            if not path.is_relative_to(REPORT.parent.resolve()) or hashlib.sha256(path.read_bytes()).hexdigest() != digest:
                raise ValueError("retained route artifact digest changed")
        cls.report = json.loads(REPORT.read_text())
        if (manifest["collection"]["source_commit"] != cls.report["source_commit"]
                or manifest["binary_sha256"] != cls.report["binary_sha256"]):
            raise ValueError("retained route collection identity changed")

    def test_all_planned_cells_and_guide_reduction_replay(self):
        summary = routes.audit(self.report)
        self.assertEqual(summary["cells"], 38)
        self.assertEqual(summary["successes"], 15)
        self.assertEqual(summary["refusals"], 23)
        for difference in summary["guide_difference"].values():
            self.assertEqual(difference["process_calls"], -1)
            self.assertLess(difference["exchange_bytes"], 0)

    def test_missing_or_duplicate_cells_do_not_count_as_complete(self):
        for replacement in ([], [self.report["runs"][0]]):
            report = copy.deepcopy(self.report)
            report["runs"] = report["runs"][:-1] + replacement
            with self.assertRaisesRegex(ValueError, "missing or duplicate"):
                routes.audit(report)

    def test_traffic_is_recomputed_from_retained_events(self):
        report = copy.deepcopy(self.report)
        report["runs"][0]["events"][0]["response"]["report"]["extra"] = "unaccounted text"
        with self.assertRaisesRegex(ValueError, "traffic accounting"):
            routes.audit(report)

    def test_refusal_cannot_hide_a_source_write_or_patch(self):
        for field, value in (("source", {}), ("patch", "unexpected patch")):
            report = copy.deepcopy(self.report)
            row = next(r for r in report["runs"] if r["fault"])
            row[field] = value
            with self.assertRaisesRegex(ValueError, "refused route"):
                routes.audit(report)

    def test_checks_and_behavior_cannot_be_dropped_from_success(self):
        for field in ("oracle", "required_checks_bound"):
            report = copy.deepcopy(self.report)
            row = next(r for r in report["runs"] if r["arm"] == "task" and r["fault"] is None)
            if field == "oracle":
                row[field]["exit_code"] = 1
            else:
                row[field] = False
            with self.assertRaisesRegex(ValueError, "outcome, behavior"):
                routes.audit(report)

    def test_result_must_match_the_recorded_native_response(self):
        report = copy.deepcopy(self.report)
        report["runs"][0]["result"]["passed"] = False
        with self.assertRaisesRegex(ValueError, "recorded native response"):
            routes.audit(report)

    def test_patch_must_match_the_delivery_receipt(self):
        report = copy.deepcopy(self.report)
        report["runs"][0]["patch"] += "unreported bytes"
        with self.assertRaisesRegex(ValueError, "delivery receipt"):
            routes.audit(report)


if __name__ == "__main__":
    unittest.main()
