"""Failed-attempt costs, retained grade identities and honest paired comparisons."""
import copy
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval import native_costs as costs, native_outcomes as outcomes, opencode_changes as runner, opencode_native as native
from agent_eval.study import encode, load
from agent_eval.test_native_changes import FILES, edit, fixture, plan

ROOT = Path(__file__).resolve().parents[2]
OLD = ROOT / "tests/agent-eval/opencode/results/2026-10-02-code-changes"
NEW = ROOT / "tests/agent-eval/opencode/results/2026-10-02-public-edit-code-changes"


def host(rows):
    return b"".join(encode(r) + b"\n" for r in rows)


def observe(raw, rows):
    return costs.observed(raw, host(rows), {"files": FILES}, "files", 1)


class PrefixCosts(unittest.TestCase):
    def test_completed_events_count_disclosed_source_edits_and_reported_steps(self):
        raw, _, rows = fixture()
        result = observe(raw, rows)
        self.assertEqual(result["observed"]["host_calls"], 3)
        self.assertEqual(result["observed"]["native_confirmed_results"], 3)
        self.assertEqual(result["observed"]["source_page_bytes"], 27)
        self.assertEqual(result["observed"]["accepted_edits"], 1)
        self.assertTrue(result["observed"]["submission_observed"])
        self.assertEqual(result["usage"]["finished_steps"], 4)
        self.assertEqual(result["usage"]["reported_tokens"]["input"], 40)
        self.assertIsNone(result["usage"]["actual_usd"])
        self.assertFalse(result["coverage"]["export_and_model_identity_verified"])

    def test_stop_between_produced_result_and_native_delivery_keeps_both_counts(self):
        raw, _, rows = fixture()
        lines = raw.splitlines()
        result = observe(b"\n".join(lines[:4]), rows[:2])
        self.assertEqual(result["observed"]["host_calls"], 2)
        self.assertEqual(result["observed"]["native_confirmed_results"], 1)
        self.assertEqual(result["observed"]["produced_only_results"], 1)
        self.assertEqual(result["observed"]["accepted_edits"], 1)
        self.assertEqual(result["usage"]["finished_steps"], 1)
        self.assertEqual(result["coverage"]["unfinished_steps"], 1)

    def test_unfinished_final_json_lines_are_measured_without_inventing_records(self):
        raw, _, rows = fixture()
        prefix = b"\n".join(raw.splitlines()[:4]) + b"\n"
        result = costs.observed(prefix + b'{"type":', host(rows[:1]) + b'{"sequence":', {"files": FILES}, "files", 1)
        self.assertEqual(result["coverage"]["unparsed_stream_tail_bytes"], 8)
        self.assertEqual(result["coverage"]["unparsed_host_tail_bytes"], 12)
        self.assertEqual(result["observed"]["host_calls"], 1)
        for bad in (b"broken\n", b'{}\nbroken\n', b'{"x":1,"x":2}'):
            with self.assertRaises(ValueError):
                costs.prefix(bad, 1024)
        with self.assertRaises(ValueError):
            costs.prefix(b" " * 1025, 1024)

    def test_no_stream_never_claims_produced_results_reached_the_model(self):
        _, _, rows = fixture()
        result = observe(b"", rows[:2])
        self.assertEqual(result["observed"]["produced_only_results"], 2)
        self.assertEqual(result["observed"]["native_confirmed_result_bytes"], 0)
        self.assertEqual(result["usage"]["finished_steps"], 0)
        self.assertIsNone(result["session"])

    def test_duplicate_steps_calls_mixed_sessions_and_negative_usage_refuse(self):
        raw, _, rows = fixture()
        original = [json.loads(line) for line in raw.splitlines()]
        for change in (lambda e: e.insert(1, copy.deepcopy(e[0])),
                       lambda e: e.insert(2, copy.deepcopy(e[1])),
                       lambda e: e[1].update(sessionID="ses_other"),
                       lambda e: e[2]["part"]["tokens"].update(input=-1),
                       lambda e: e[2]["part"]["tokens"].update(input=True),
                       lambda e: e[2]["part"].update(cost=float("inf"))):
            events = copy.deepcopy(original)
            change(events)
            with self.assertRaises(ValueError):
                observe(b"\n".join(json.dumps(e).encode() for e in events), rows)

    def test_forged_host_results_and_unmatched_native_calls_refuse(self):
        raw, _, original = fixture()
        for change in (lambda r: r[0].update(sequence=2),
                       lambda r: r[0]["response"].update(isError=True),
                       lambda r: r[1]["result"].update(sha256="0" * 64),
                       lambda r: r.pop(1)):
            rows = copy.deepcopy(original)
            change(rows)
            with self.assertRaises(ValueError):
                observe(raw, rows)

    def test_overlapping_reads_count_again_but_new_file_hashes_do_not(self):
        read = {"name": "read_source", "arguments": {"path": "module.py", "offset": 0, "bytes": 8192, "sha256": ""}}
        raw, _, rows = fixture([read, read, edit(), read, {"name": "submit_patch", "arguments": {"summary": "done"}}])
        result = observe(raw, rows)
        self.assertEqual(result["observed"]["source_page_bytes"], 81)
        self.assertEqual(result["observed"]["repeated_source_page_bytes"], 27)
        self.assertEqual(result["observed"]["native_confirmed_repeated_source_page_bytes"], 27)


class Outcomes(unittest.TestCase):
    def test_committed_report_matches_current_retained_evidence(self):
        reports = [outcomes.cohort(OLD), outcomes.cohort(NEW)]
        path = ROOT / "tests/agent-eval/opencode/native-change-outcomes.json"
        self.assertEqual(load(path), {"schema": "fr-native-change-comparison-1", "cohorts": reports})
        self.assertEqual((ROOT / "docs/native-change-outcomes.md").read_text(), outcomes.markdown(reports))

    def test_missing_resource_samples_stay_unknown_and_duplicate_processes_refuse(self):
        result = outcomes.process_cost({"processes": [], "wall_seconds": 1})
        self.assertIsNone(result["sampled_cpu_seconds"])
        self.assertIsNone(result["sampled_peak_rss_bytes"])
        with self.assertRaisesRegex(ValueError, "duplicate"):
            outcomes.process_cost({"processes": [{"name": "opencode"}] * 2, "wall_seconds": 1})

    def test_old_timeout_costs_stay_in_model_arm_totals_and_per_pass_time(self):
        report = outcomes.cohort(OLD)
        self.assertEqual(report["outcomes"], {"failed": 1, "passed": 7})
        failed = next(r for r in report["attempts"] if r["outcome"] == "failed")
        self.assertGreater(failed["costs"]["observed"]["host_calls"], 0)
        self.assertGreater(failed["costs"]["usage"]["finished_steps"], 0)
        self.assertFalse(failed["costs"]["coverage"]["reported_step_usage_complete"])
        group = next(g for g in report["groups"] if g["model"] == failed["cell"]["model"] and g["arm"] == "fr")
        self.assertEqual(group["outcomes"], {"failed": 1, "passed": 1})
        self.assertGreater(group["collection_wall_seconds_per_behavior_pass"], 220)
        self.assertFalse(group["tool_and_usage_totals_complete"])
        self.assertEqual(sum(p["both_passed"] for p in report["pairs"]), 3)

    def test_ungraded_submissions_stay_pending_and_zero_success_rates_are_undefined(self):
        with tempfile.TemporaryDirectory() as tmp:
            report = outcomes.cohort(NEW, Path(tmp) / "missing-grades.json")
        self.assertEqual(report["outcomes"], {"pending_grading": 3, "failed": 1})
        self.assertTrue(all(g["collection_wall_seconds_per_behavior_pass"] is None for g in report["groups"]))
        self.assertTrue(all(not p["both_passed"] and p["fr_minus_files_result_bytes"] is None for p in report["pairs"]))
        failed = next(r for r in report["attempts"] if r["outcome"] == "failed")
        self.assertEqual(failed["costs"]["usage"]["finished_steps"], 8)
        self.assertEqual(failed["costs"]["observed"]["host_calls"], 12)
        self.assertFalse(report["independent_efficiency_evidence"])

    def test_same_cell_ids_across_cohorts_never_create_the_same_trial_id(self):
        old, new = outcomes.cohort(OLD), outcomes.cohort(NEW)
        self.assertTrue({r["cell"]["id"] for r in old["attempts"]} & {r["cell"]["id"] for r in new["attempts"]})
        self.assertFalse({r["trial_id"] for r in old["attempts"]} & {r["trial_id"] for r in new["attempts"]})

    def test_grade_candidate_cases_verdicts_and_failure_labels_are_checked(self):
        frozen = load(OLD / "plan.json")
        attempts = runner.replay(frozen, OLD / "attempts")["attempts"]
        original = load(OLD / "github-grades.json")
        for change in (lambda r: r.update(plan_sha256="0" * 64), lambda r: r.update(passed=99),
                       lambda r: r["outcomes"][0].update(outcome="passed"),
                       lambda r: r["outcomes"][1].update(submission_sha256="0" * 64),
                       lambda r: r["outcomes"][1]["grade"]["candidate"].update(bytes=1),
                       lambda r: r["outcomes"][1]["grade"]["cases"].pop(),
                       lambda r: r["outcomes"][1]["grade"]["cases"][0].update(stdout_sha256="0" * 64),
                       lambda r: r["outcomes"][1]["grade"]["cases"][0]["container_state"].update(OOMKilled=True)):
            altered = copy.deepcopy(original)
            change(altered)
            with self.assertRaises(ValueError):
                outcomes.grades(frozen, altered, attempts, OLD)

    def test_report_cli_is_reproducible_and_does_not_rewrite_trial_artifacts(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            command = [sys.executable, "-B", str(ROOT / "tools/native-change-report.py"), str(OLD), str(NEW),
                       "--json", str(root / "report.json"), "--markdown", str(root / "report.md")]
            for extra in ([], ["--check"]):
                result = subprocess.run(command + extra, capture_output=True, timeout=10)
                self.assertEqual(result.returncode, 0, result.stderr)
            text = (root / "report.md").read_text()
            self.assertIn("Failed attempts remain", text)
            self.assertIn("undefined", text)
            (root / "report.md").write_text(text + "changed\n")
            self.assertNotEqual(subprocess.run(command + ["--check"], capture_output=True, timeout=10).returncode, 0)
            self.assertNotEqual(subprocess.run(command + [str(OLD)], capture_output=True, timeout=10).returncode, 0)
            self.assertNotEqual(subprocess.run(command + ["--json", str(OLD / "plan.json")], capture_output=True, timeout=10).returncode, 0)
            self.assertNotEqual(subprocess.run(command + ["--json", str(root / "report.md")], capture_output=True, timeout=10).returncode, 0)


class AggregateCpu(unittest.TestCase):
    def test_remaining_cpu_decreases_and_exhaustion_or_unknown_samples_refuse(self):
        self.assertEqual(native.cpu_remaining([]), 20)
        self.assertEqual(native.cpu_remaining([{"sampled_cpu_seconds": 2.5}, {"sampled_cpu_seconds": 7}]), 10.5)
        self.assertEqual(native.cpu_remaining([{"sampled_cpu_seconds": 20}], allow_zero=True), 0)
        for rows in ([{"sampled_cpu_seconds": 20}], [{"sampled_cpu_seconds": 21}],
                     [{"sampled_cpu_seconds": -1}], [{"sampled_cpu_seconds": float("nan")}], [{}]):
            with self.assertRaises((KeyError, ValueError)):
                native.cpu_remaining(rows)

    def test_each_collector_stops_before_launching_after_the_shared_budget_is_spent(self):
        for module in (runner, native):
            with tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                if module is runner:
                    frozen = plan(root)
                    cell = runner.checked(frozen)["cells"][0]
                    args = (frozen, cell["id"], root / "attempts", root / "fr", Path("opencode"))
                else:
                    manifest = ROOT / "tests/agent-eval/opencode/explanations.json"
                    binary = Path(__file__).resolve()
                    frozen = native.freeze(load(manifest), manifest.parent, binary)
                    cell = native.checked(frozen)["cells"][0]
                    args = (frozen, cell["id"], manifest.parent, root / "attempts", binary, Path("opencode"))
                calls = []
                def execute(command, data, out, err, directory, **kwargs):
                    calls.append((command, kwargs["cpu_limit_seconds"]))
                    out.write(b"test-version\n")
                    return {"exit_code": 0, "stop_reason": None, "sampled_cpu_seconds": 20}
                with patch.object(module, "bounded_run", side_effect=execute):
                    record = module.run_attempt(*args)
                self.assertEqual(record["status"], "failed")
                self.assertIn("CPU budget exhausted", record["failure"])
                self.assertEqual(len(calls), 1)
                self.assertEqual(calls[0][1], 20)
                self.assertEqual(len(record["processes"]), 1)

    def test_model_and_export_receive_only_the_remaining_allowance(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            frozen = plan(root)
            cell = next(c for c in runner.checked(frozen)["cells"] if c["arm"] == "files")
            raw, exported, rows = fixture()
            limits = []
            def execute(command, data, out, err, directory, **kwargs):
                limits.append(kwargs["cpu_limit_seconds"])
                if command[-1] == "--version":
                    out.write(b"test-version\n")
                    used = 2
                elif "export" in command:
                    out.write(encode(exported))
                    used = 1
                else:
                    out.write(raw)
                    (directory / "tools.jsonl").write_bytes(host(rows))
                    used = 7
                return {"exit_code": 0, "stop_reason": None, "sampled_cpu_seconds": used}
            with patch.object(runner, "bounded_run", side_effect=execute):
                record = runner.run_attempt(frozen, cell["id"], root / "attempts", root / "fr", Path("opencode"))
            self.assertEqual(record["status"], "submitted", record)
            self.assertEqual(limits, [20, 18, 11])


if __name__ == "__main__":
    unittest.main()
