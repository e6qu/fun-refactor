"""Adversarial study-accounting tests; no model services or compilers are used."""
import copy
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval.study import checked_plan, digest, load, plan
from agent_eval.study_report import report
from agent_eval.study_usage import MEASUREMENTS


def manifest():
    return {
        "schema": "fr-agent-study-1", "seed": 21, "repetitions": 2,
        "spend_cap_usd": 10, "cache_state": "uncontrolled", "max_children": 2,
        "budgets": {"wall_seconds": 100, "aggregate_agent_seconds": 200,
                    "aggregate_tokens": 10000, "rss_bytes": 1000000, "disk_bytes": 1000000,
                    "attempt_cap_usd": 1},
        "fr": {"version": "fixture", "binary_sha256": "a" * 64, "skill_sha256": "b" * 64},
        "tasks": [{"id": kind, "kind": kind, "repository": f"fixture-repo-{index % 3}",
                   "revision": str(index) * 40, "requirement": f"Synthetic {kind} requirement",
                   "grader_sha256": "c" * 64, "held_out": index == 3, "delegated": index < 2}
                  for index, kind in enumerate(("explain", "fix", "feature", "proof"))],
        "models": [{"id": name, "provider": "fixture", "model": name, "harness": "test-1",
                    "settings": {"effort": "low"},
                    "pricing": {"source": "synthetic test rates, not provider pricing", "as_of": "2026-09-30",
                                "usd_per_million": {"uncached_input": 2, "cache_read": 0.2, "cache_write": 2.5, "output": 10}}}
                   for name in ("model-a", "model-b")],
    }


class StudyTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.frozen = plan(manifest())

    def artifact(self, name, content):
        path = self.root / "artifacts" / name
        path.parent.mkdir(exist_ok=True)
        path.write_bytes(content)
        return {"path": f"artifacts/{name}", "sha256": hashlib.sha256(content).hexdigest()}

    def record(self, cell=None):
        cell = cell or self.frozen["cells"][0]
        model = next(row for row in self.frozen["manifest"]["models"] if row["id"] == cell["model"])
        task = next(row for row in self.frozen["manifest"]["tasks"] if row["id"] == cell["task"])
        raw = self.artifact("usage.txt", b"retained provider usage fixture")
        agent = {"id": f"{cell['id']}-parent", "parent": None, "children": [], "status": "completed",
                 **{field: model[field] for field in ("provider", "model", "harness", "settings")},
                 "seconds": 4, "usage_complete": True,
                 "invocations": [{"id": f"{cell['id']}-request", "raw_usage": raw,
                                  "tokens": {"uncached_input": 100, "cache_read": 200, "cache_write": 50,
                                             "output": 10, "reasoning": 3}}]}
        return {"schema": "fr-agent-study-attempt-1", "cell": cell["id"], "plan_sha256": digest(self.frozen),
                "status": "completed", "cache_state": "uncontrolled", "repository_revision": task["revision"],
                "requirement_sha256": digest(task["requirement"]), "fr": self.frozen["manifest"]["fr"],
                "wall_seconds": 5, "agents": [agent], "trace": self.artifact("trace.txt", b"host trace fixture"),
                "grade": {"outcome": "passed", "grader_sha256": task["grader_sha256"],
                          "evidence": self.artifact("grade.txt", b"independent grader fixture"),
                          "regressions": 0, "unsupported_claims": 0, "human_interventions": 0},
                "measurements": dict.fromkeys(MEASUREMENTS, 0), "actual_usd": None}

    def save(self, record):
        (self.root / f'{record["cell"]}.json').write_text(json.dumps(record))

    def audit(self, record):
        self.save(record)
        result = report(self.frozen, self.root)
        return next(row for row in result["attempts"] if row["cell"]["id"] == record["cell"])

    def test_pilot_shape_is_48_and_pairs_are_adjacent_and_matched(self):
        cells = self.frozen["cells"]
        self.assertEqual(len(cells), 48)
        self.assertEqual(sum(cell["mode"] == "delegated" for cell in cells), 16)
        self.assertEqual(len({cell["id"] for cell in cells}), 48)
        first_arms = set()
        for left, right in zip(cells[::2], cells[1::2]):
            self.assertEqual(left["pair"], right["pair"])
            self.assertEqual({left["arm"], right["arm"]}, {"files", "fr"})
            first_arms.add(left["arm"])
        self.assertEqual(first_arms, {"files", "fr"})
        self.assertEqual(self.frozen, plan(manifest()))
        changed = manifest()
        changed["seed"] += 1
        self.assertNotEqual(cells, plan(changed)["cells"])

    def test_arbitrary_task_and_model_names_work(self):
        value = manifest()
        value["tasks"][0]["id"] = "unfamiliar-project-question"
        value["models"][0]["id"] = "independent-host"
        self.assertEqual(len(plan(value)["cells"]), 48)

    def test_changed_plan_and_nonfinite_values_refuse(self):
        changed = copy.deepcopy(self.frozen)
        changed["cells"].reverse()
        with self.assertRaisesRegex(ValueError, "frozen manifest"):
            checked_plan(changed)
        for value in (-1, True, float("nan"), float("inf")):
            with self.subTest(value=value):
                invalid = manifest()
                invalid["spend_cap_usd"] = value
                with self.assertRaises(ValueError):
                    plan(invalid)

    def test_duplicate_ids_and_unpinned_revisions_refuse(self):
        for mutate in (lambda value: value["tasks"].append(value["tasks"][0]),
                       lambda value: value["tasks"][0].update(revision="main"),
                       lambda value: value["models"][0]["pricing"]["usd_per_million"].update(output=-1)):
            value = manifest()
            mutate(value)
            with self.assertRaises(ValueError):
                plan(value)

    def test_json_rejects_duplicate_keys_and_nonfinite_numbers(self):
        path = self.root / "bad.json"
        for content in ('{"x":1,"x":2}', '{"x":NaN}'):
            path.write_text(content)
            with self.assertRaises(ValueError):
                load(path)

    def test_empty_study_keeps_all_pending_cells(self):
        value = report(self.frozen, self.root)
        self.assertEqual(value["outcomes"], {"pending": 48})
        self.assertEqual(value["executed"], 0)
        self.assertTrue(all(not pair["comparable_success"] for pair in value["pairs"]))

    def test_disjoint_usage_and_reasoning_are_not_double_counted(self):
        value = self.audit(self.record())
        self.assertEqual(value["tokens"]["output"], 10)
        self.assertEqual(value["tokens"]["reasoning"], 3)
        self.assertAlmostEqual(value["estimated_usd"], 0.000465)
        self.assertTrue(value["usage_complete"])

    def test_missing_billable_field_does_not_become_zero(self):
        record = self.record()
        del record["agents"][0]["invocations"][0]["tokens"]["cache_write"]
        value = self.audit(record)
        self.assertIsNone(value["tokens"]["cache_write"])
        self.assertIsNone(value["estimated_usd"])
        self.assertFalse(value["usage_complete"])
        self.assertIsNone(report(self.frozen, self.root)["spend"]["remaining_usd"])

    def test_missing_reasoning_does_not_prevent_output_pricing(self):
        record = self.record()
        record["agents"][0]["invocations"][0]["tokens"]["reasoning"] = None
        value = self.audit(record)
        self.assertTrue(value["usage_complete"])
        self.assertIsNone(value["tokens"]["reasoning"])
        self.assertTrue(value["missing"])

    def test_incomplete_or_empty_invocation_ledger_is_not_free(self):
        for field, value in (("usage_complete", False), ("invocations", [])):
            record = self.record()
            record["agents"][0][field] = value
            self.assertIsNone(self.audit(record)["estimated_usd"])

    def test_failed_children_are_counted_separately_from_wall_time(self):
        cell = next(cell for cell in self.frozen["cells"] if cell["mode"] == "delegated")
        record = self.record(cell)
        parent = record["agents"][0]
        child = copy.deepcopy(parent)
        child.update(id="child", parent=parent["id"], status="failed", seconds=3)
        child["invocations"][0]["id"] = "child-request"
        parent["children"] = ["child"]
        record["agents"].append(child)
        value = self.audit(record)
        self.assertEqual(value["agents"], 2)
        self.assertEqual(value["agent_seconds"], 7)
        self.assertEqual(value["wall_seconds"], 5)
        self.assertAlmostEqual(value["estimated_usd"], 0.00093)

    def test_missing_child_and_duplicate_invocation_refuse(self):
        for corrupt in ("child", "turn"):
            record = self.record()
            parent = record["agents"][0]
            if corrupt == "child":
                parent["children"] = ["missing"]
            else:
                parent["invocations"] *= 2
            with self.assertRaises(ValueError):
                self.audit(record)

    def test_disconnected_agent_cycle_refuses(self):
        cell = next(cell for cell in self.frozen["cells"] if cell["mode"] == "delegated")
        record = self.record(cell)
        for identity, other in (("child-a", "child-b"), ("child-b", "child-a")):
            child = copy.deepcopy(record["agents"][0])
            child.update(id=identity, parent=other, children=[other])
            record["agents"].append(child)
        with self.assertRaisesRegex(ValueError, "cycle"):
            self.audit(record)

    def test_changed_model_task_or_grader_refuses(self):
        for corrupt in (lambda row: row["agents"][0].update(model="substitute"),
                        lambda row: row.update(repository_revision="f" * 40),
                        lambda row: row["grade"].update(grader_sha256="f" * 64),
                        lambda row: row.update(requirement_sha256="f" * 64),
                        lambda row: row.update(plan_sha256="f" * 64)):
            record = self.record()
            corrupt(record)
            with self.assertRaises(ValueError):
                self.audit(record)

    def test_artifact_tampering_and_path_escape_refuse(self):
        record = self.record()
        (self.root / record["trace"]["path"]).write_text("modified")
        with self.assertRaisesRegex(ValueError, "digest differs"):
            self.audit(record)
        for path in ("../outside", "/tmp/outside"):
            record = self.record()
            record["trace"]["path"] = path
            with self.assertRaisesRegex(ValueError, "unsafe"):
                self.audit(record)

    def test_symlink_escape_refuses(self):
        record = self.record()
        path = self.root / record["trace"]["path"]
        path.unlink()
        path.symlink_to(Path(__file__))
        with self.assertRaisesRegex(ValueError, "escapes"):
            self.audit(record)

    def test_success_cannot_hide_regressions_or_human_intervention(self):
        for key in ("regressions", "unsupported_claims", "human_interventions"):
            record = self.record()
            record["grade"][key] = 1
            with self.assertRaisesRegex(ValueError, key):
                self.audit(record)

    def test_failed_attempts_contribute_to_cost_per_success(self):
        cell = self.frozen["cells"][0]
        other = next(row for row in self.frozen["cells"] if all(row[key] == cell[key] for key in ("model", "mode", "arm"))
                     and row["id"] != cell["id"])
        self.save(self.record(cell))
        failure = self.record(other)
        failure.update(status="failed")
        failure["grade"]["outcome"] = "failed"
        self.save(failure)
        group = next(row for row in report(self.frozen, self.root)["groups"]
                     if all(row[key] == cell[key] for key in ("model", "mode", "arm")))
        self.assertEqual(group["executed"], 2)
        self.assertAlmostEqual(group["estimated_usd_per_success"], 0.00093)

    def test_zero_success_has_no_cost_per_success(self):
        record = self.record()
        record["grade"]["outcome"] = "failed"
        self.save(record)
        self.assertTrue(all(row["estimated_usd_per_success"] is None for row in report(self.frozen, self.root)["groups"]))

    def test_only_matched_successes_get_cost_differences(self):
        for cell in self.frozen["cells"][:2]:
            self.save(self.record(cell))
        pairs = report(self.frozen, self.root)["pairs"]
        successes = [row for row in pairs if row["comparable_success"]]
        self.assertEqual(len(successes), 1)
        self.assertEqual(successes[0]["estimated_fr_minus_files_usd"], 0)

    def test_overrun_is_retained_but_not_comparable(self):
        for cell in self.frozen["cells"][:2]:
            record = self.record(cell)
            record["wall_seconds"] = 101
            self.save(record)
        value = report(self.frozen, self.root)
        self.assertEqual(value["executed"], 2)
        self.assertTrue(all(not pair["comparable_success"] for pair in value["pairs"]))

    def test_actual_billing_stays_separate_from_estimates(self):
        record = self.record()
        record.update(actual_usd=0.123, billing_evidence=self.artifact("billing.txt", b"invoice fixture"))
        value = self.audit(record)
        self.assertEqual(value["actual_usd"], 0.123)
        self.assertAlmostEqual(value["estimated_usd"], 0.000465)
        self.assertEqual(report(self.frozen, self.root)["spend"]["known_usd"], 0.123)

    def test_blocked_cells_need_a_reason_and_cannot_hide_execution(self):
        cell = self.frozen["cells"][0]
        record = {"schema": "fr-agent-study-attempt-1", "cell": cell["id"], "plan_sha256": digest(self.frozen),
                  "status": "blocked", "reason": "model access unavailable"}
        self.assertEqual(self.audit(record)["outcome"], "blocked")
        record["agents"] = [self.record()["agents"][0]]
        with self.assertRaisesRegex(ValueError, "blocked means"):
            self.audit(record)

    def test_unplanned_attempts_refuse_instead_of_disappearing(self):
        (self.root / "retry.json").write_text("{}")
        with self.assertRaisesRegex(ValueError, "unplanned"):
            report(self.frozen, self.root)

    def test_cli_plan_and_pending_report(self):
        script = Path(__file__).resolve().parents[1] / "agent-eval-study.py"
        source, frozen = self.root / "manifest.json", self.root / "plan.json"
        source.write_text(json.dumps(manifest()))
        result = subprocess.run([sys.executable, str(script), "plan", str(source)], capture_output=True, text=True, check=True)
        frozen.write_text(result.stdout)
        attempts = self.root / "attempts"
        attempts.mkdir()
        result = subprocess.run([sys.executable, str(script), "report", str(frozen), str(attempts)], capture_output=True, text=True, check=True)
        self.assertEqual(json.loads(result.stdout)["outcomes"], {"pending": 48})


if __name__ == "__main__":
    unittest.main()
