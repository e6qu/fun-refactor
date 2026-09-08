"""Regressions for acceptance grading and evidence boundaries, without an agent service."""

import copy
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

TOOLS = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(TOOLS))
spec = importlib.util.spec_from_file_location("agent_eval_harness", TOOLS / "agent-eval.py")
harness = importlib.util.module_from_spec(spec)
spec.loader.exec_module(harness)

cache_spec = importlib.util.spec_from_file_location("project_cache_measurement", TOOLS / "project-cache.py")
cache_measurement = importlib.util.module_from_spec(cache_spec)
cache_spec.loader.exec_module(cache_measurement)

profile_spec = importlib.util.spec_from_file_location("project_phase_measurement", TOOLS / "project-profile.py")
phase_measurement = importlib.util.module_from_spec(profile_spec)
profile_spec.loader.exec_module(phase_measurement)


class ProjectPhaseEvidence(unittest.TestCase):
    def test_profile_rejects_missing_negative_or_overlapping_phase_times(self):
        valid = {"schema": "fr-project-profile-1", "phases_seconds": {name: 0.01 for name in phase_measurement.PHASES},
                 "measured_seconds": 0.5, "report_stdout": "{}\n"}
        for mutation in ("none", "missing", "negative", "overlap", "outer"):
            with self.subTest(mutation=mutation):
                report = copy.deepcopy(valid)
                if mutation == "missing":
                    report["phases_seconds"].pop("verify")
                elif mutation == "negative":
                    report["phases_seconds"]["scan"] = -0.1
                elif mutation == "overlap":
                    report["phases_seconds"]["index"] = 0.49
                elif mutation == "outer":
                    report["measured_seconds"] = 2
                result = mock.Mock(returncode=0, stdout=json.dumps(report).encode(), stderr=b"")
                with mock.patch.object(phase_measurement.subprocess, "run", return_value=result), \
                     mock.patch.object(phase_measurement.time, "perf_counter_ns", side_effect=[0, 1_000_000_000]):
                    if mutation == "none":
                        self.assertEqual(phase_measurement.profile(Path("profile"), Path("."), Path("cache"),
                                                                  ["project", "find", "f"], True)["subprocess_seconds"], 1)
                    else:
                        with self.assertRaises(AssertionError):
                            phase_measurement.profile(Path("profile"), Path("."), Path("cache"),
                                                      ["project", "find", "f"], True)


class CacheMeasurementEvidence(unittest.TestCase):
    def test_timing_comparison_rejects_changed_source_coverage_or_revision(self):
        report = {"revision": "basis", "coverage": {"files": 2}, "source": "λ🙂", "omitted": 0}
        baseline = {"stdout": json.dumps(report)}
        cache_measurement.same_report(baseline, baseline)
        for key, replacement in (("revision", "stale"), ("coverage", {}), ("source", "λ"), ("omitted", 1)):
            with self.subTest(key=key):
                changed = {**report, key: replacement}
                with self.assertRaisesRegex(AssertionError, "complete query report"):
                    cache_measurement.same_report(baseline, {"stdout": json.dumps(changed)})

    def test_failed_invalidation_probe_restores_source_and_mode(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            path = root / "lib.rs"
            original = "pub fn probe() { let _ = 'λ'; }".encode()
            path.write_bytes(original)
            path.chmod(0o640)
            original_mode = path.stat().st_mode
            baseline = {"columns": ["path", "handle", "source"], "rows": [["lib.rs", "old", {
                "span": {"start": 0, "end": len(original)}, "text": original.decode(), "next_offset": None}]]}

            def failed_query(*args, **kwargs):
                self.assertIn(b"fr cache invalidation probe", path.read_bytes())
                raise RuntimeError("injected query failure")

            with mock.patch.object(cache_measurement, "query", side_effect=failed_query):
                with self.assertRaisesRegex(RuntimeError, "injected query failure"):
                    cache_measurement.invalidation(Path("fr"), root, root / "cache", [], baseline)
            self.assertEqual(path.read_bytes(), original)
            self.assertEqual(path.stat().st_mode, original_mode)

    def test_incomplete_source_cannot_start_an_invalidation_probe(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            path = root / "lib.rs"
            original = b"pub fn probe() {}"
            path.write_bytes(original)
            baseline = {"columns": ["path", "handle", "source"], "rows": [["lib.rs", "old", {
                "span": {"start": 0, "end": 4}, "text": "pub ", "next_offset": 4}]]}
            with mock.patch.object(cache_measurement, "query") as query:
                with self.assertRaises(AssertionError):
                    cache_measurement.invalidation(Path("fr"), root, root / "cache", [], baseline)
                query.assert_not_called()
            self.assertEqual(path.read_bytes(), original)


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

    def test_multiple_declared_checks_must_all_pass_at_every_stage(self):
        events = copy.deepcopy(self.events)
        for index in (0, 2, 4, 6):
            visible = json.loads(events[index]["visible"])
            visible["result"]["results"] = [{"name": name, "passed": True} for name in ("upstream", "minimal")]
            events[index]["visible"] = json.dumps(visible)
        required = ("upstream", "minimal")
        self.assertTrue(harness.workflow(events, self.original, self.changed, required)["workflow_ordered"])
        for index in (0, 2, 4, 6):
            changed = copy.deepcopy(events)
            visible = json.loads(changed[index]["visible"])
            visible["result"]["results"].pop()
            changed[index]["visible"] = json.dumps(visible)
            self.assertFalse(harness.workflow(changed, self.original, self.changed, required)["workflow_ordered"])


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

    def test_upstream_signal_failure_is_not_hidden_by_a_later_passing_check(self):
        with mock.patch.object(harness, "process", side_effect=[{"exit_code": -9}, {"exit_code": 0}]):
            self.assertEqual(harness.upstream(Path("unused"), "regex-escape-into")["exit_code"], -9)

    def test_repetitions_preserve_pairs_and_legacy_trial_names(self):
        self.assertEqual([name for name, _, _, _ in harness.trial_names("strsim", 1)],
                         ["unicode-dice-fr", "unicode-dice-files", "normalized-osa-fr", "normalized-osa-files"])
        names = harness.trial_names("regex", 2)
        self.assertEqual(len(set(name for name, _, _, _ in names)), 4)
        for repetition in (1, 2):
            self.assertEqual({arm for _, _, arm, repeat in names if repeat == repetition}, {"fr", "files"})
        for project, repetitions in (("unknown", 1), ("regex", 0), ("regex", 9)):
            with self.assertRaises(ValueError):
                harness.trial_names(project, repetitions)

    def test_regex_workspace_retains_real_package_boundaries_and_pinned_lock(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "project"
            harness.unpack(root, "regex-escape-into")
            self.assertIn('path = "regex-syntax"', (root / "Cargo.toml").read_text())
            self.assertIn("pub fn escape_into", (root / "regex-syntax/src/lib.rs").read_text())
            self.assertNotIn("pub fn escape_into", (root / "src/lib.rs").read_text())
            self.assertEqual(harness.digest((root / "Cargo.lock").read_bytes()), harness.regex_workspace.LOCK_SHA)
            harness.initialize(root)
            self.assertIn("Cargo.lock", harness.snapshot(root))
            self.assertTrue((root / "LICENSE-MIT").is_file())
            self.assertGreater(sum(p.stat().st_size for p in root.rglob("*.rs")), 1_000_000)

    def test_record_refuses_an_incomplete_pair_before_creating_evidence(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            harness.save(root / "experiment.json", {"project": "regex", "repetitions": 2, "trials": ["regex-escape-into-fr-r1"]})
            output = root / "evidence"
            with self.assertRaisesRegex(ValueError, "every planned paired"):
                harness.record(root, output)
            self.assertFalse(output.exists())

    def test_scored_failures_without_patches_remain_recordable_and_fail_replay(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            names = [name for name, _, _, _ in harness.trial_names("regex", 1)]
            harness.save(root / "experiment.json", {"project": "regex", "repetitions": 1, "trials": names})
            for name in names:
                trial = root / name
                (trial / "skill").mkdir(parents=True)
                (trial / "skill/SKILL.md").write_text("Synthetic test fixture")
                harness.save(trial / "session.json", {"task": "regex-escape-into"})
                harness.save(trial / "result.json", {"passed": False})
                (trial / "prompt.txt").write_text("Synthetic task")
                (trial / "events.jsonl").write_text("")
            output = root / "evidence"
            harness.record(root, output, execution_note="Synthetic regression; no agents")
            self.assertFalse((output / names[0] / "change.patch").exists())
            self.assertFalse(json.loads((output / names[0] / "result.json").read_text())["passed"])
            with self.assertRaisesRegex(ValueError, "Recorded trial failed"):
                harness.replay(output)

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
