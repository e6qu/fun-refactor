"""Regressions for acceptance grading and evidence boundaries, without an agent service."""

import copy
import contextlib
import hashlib
import importlib.util
import io
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

construction_spec = importlib.util.spec_from_file_location("construction_measurement", TOOLS / "project-construction.py")
construction_measurement = importlib.util.module_from_spec(construction_spec)
construction_spec.loader.exec_module(construction_measurement)

batch_spec = importlib.util.spec_from_file_location("batch_measurement", TOOLS / "author-batch-context.py")
batch_measurement = importlib.util.module_from_spec(batch_spec)
batch_spec.loader.exec_module(batch_measurement)

project_batch_spec = importlib.util.spec_from_file_location(
    "project_batch_context_measurement", TOOLS / "project-batch-context.py")
project_batch_measurement = importlib.util.module_from_spec(project_batch_spec)
project_batch_spec.loader.exec_module(project_batch_measurement)

checks_policy_spec = importlib.util.spec_from_file_location("checks_policy_measurement", TOOLS / "checks-policy-context.py")
checks_policy = importlib.util.module_from_spec(checks_policy_spec)
checks_policy_spec.loader.exec_module(checks_policy)

context_spec = importlib.util.spec_from_file_location("agent_context_protocol", TOOLS / "agent-context-protocol.py")
context_protocol = importlib.util.module_from_spec(context_spec)
context_spec.loader.exec_module(context_protocol)

context_v3_spec = importlib.util.spec_from_file_location(
    "agent_context_protocol_v3", TOOLS / "agent-context-protocol-v3.py")
context_protocol_v3 = importlib.util.module_from_spec(context_v3_spec)
context_v3_spec.loader.exec_module(context_protocol_v3)

workflow_v4_spec = importlib.util.spec_from_file_location(
    "agent_workflow_v4", TOOLS / "agent-workflow-v4.py")
workflow_v4 = importlib.util.module_from_spec(workflow_v4_spec)
workflow_v4_spec.loader.exec_module(workflow_v4)


class ContextProtocolEvidence(unittest.TestCase):
    def payload(self, result):
        return json.dumps({"exit_code": 0, "result": result, "stdout_omitted_bytes": 0,
                           "stderr": "", "stderr_omitted_bytes": 0})

    def event(self, args, result):
        return {"request": {"tool": "fr", "args": args}, "visible": self.payload(result),
                "elapsed_seconds": 0.1}

    def test_related_project_response_reconstructs_exactly(self):
        common = {"revision": "0" * 64, "handle_prefix": "frp1:" + "0" * 32 + ":",
                  "coverage": {"indexed_files": 2}}
        first = {**common, "schema": "fr-project-1", "query": "map", "rows": []}
        second = {**common, "schema": "fr-project-1", "query": "find", "rows": [["x"]]}
        events = [self.event(["project", "map"], first), self.event(["project", "find", "x"], second)]
        outputs, requests, _ = context_protocol.project_events(events, [event["visible"] for event in events])
        reviewed = json.loads(outputs[0])["result"]
        compact = json.loads(outputs[1])["result"]
        self.assertEqual(compact["context_basis"], reviewed["context_basis"])
        self.assertEqual(compact.pop("context_omitted"), list(context_protocol.PROJECT_FIELDS))
        for field in context_protocol.PROJECT_FIELDS:
            self.assertNotIn(field, compact)
            compact[field] = reviewed[field]
        expected = copy.deepcopy(second)
        expected["context_basis"] = reviewed["context_basis"]
        self.assertEqual(compact, expected)
        self.assertEqual(requests[1]["args"][-2:], ["--context-basis", reviewed["context_basis"]])

    def test_author_transaction_basis_reconstructs_forward_history_diff(self):
        common = {"revision": "0" * 64, "handle_prefix": "frp1:" + "0" * 32 + ":",
                  "coverage": {"indexed_files": 1}}
        changes = [{"path": "app.rs", "before_exists": True, "after_exists": True,
                    "before_mode": 420, "after_mode": 420, "diff": "pπtch"}]
        author = {**common, "schema": "fr-author-1", "query": "replace-body",
                  "transaction": 1, "diff": "pπtch"}
        patch = {"id": 1, "record_basis": "a" * 64}
        preview = {"transaction": 1, "action": "redo", "applied": False, "changes": changes}
        smaller = copy.deepcopy(changes)
        smaller[0].pop("diff")
        completion = {"transaction": 1, "action": "redo", "applied": True,
                      "changes": smaller, "diffs_omitted": True}
        events = [self.event(["author", "replace-body"], author),
                  self.event(["history", "patch", "1"], patch),
                  self.event(["history", "redo", "1"], preview),
                  self.event(["history", "redo", "1", "--write", "--no-diff"], completion)]
        outputs, requests, _ = context_protocol.project_events(events, [event["visible"] for event in events])
        reviewed = json.loads(outputs[0])["result"]
        compact = json.loads(outputs[3])["result"]
        self.assertEqual(compact["context_omitted"], list(context_protocol.HISTORY_FIELDS))
        reconstructed = copy.deepcopy(compact)
        reconstructed.pop("context_omitted")
        reconstructed.pop("context_basis")
        reviewed_diff = reviewed["diff"].encode()
        offset = 0
        for change in reconstructed["changes"]:
            size = change.pop("diff_bytes")
            change["diff"] = reviewed_diff[offset:offset + size].decode()
            offset += size
        self.assertEqual(offset, len(reviewed_diff))
        expected = copy.deepcopy(preview)
        expected["applied"] = True
        self.assertEqual(reconstructed, expected)
        self.assertEqual(requests[3]["args"][-2:],
                         ["--context-basis", reviewed["transaction_context_basis"]])

    def test_patch_projection_preserves_exact_artifact_identity(self):
        patch = "diff --git a/app.rs b/app.rs\n+π\n"
        report = {"id": 1, "record_basis": "a" * 64, "patch": patch}
        event = self.event(["history", "patch", "1"], report)
        outputs, requests, _ = context_protocol.project_events([event], [event["visible"]])
        compact = json.loads(outputs[0])["result"]
        self.assertNotIn("patch", compact)
        self.assertEqual(compact["patch_bytes"], len(patch.encode()))
        self.assertEqual(compact["patch_sha256"], hashlib.sha256(patch.encode()).hexdigest())
        self.assertEqual(requests[0]["args"][-2:],
                         ["--output", "../artifacts/change.patch"])

    def test_complete_frozen_projection_preserves_all_passing_trials(self):
        report = context_protocol.measure(None)
        self.assertTrue(report["passed"])
        self.assertEqual(len(report["trials"]), 4)
        self.assertTrue(all(trial["recorded_passed"] for trial in report["trials"]))


class ContextProtocolV3Evidence(unittest.TestCase):
    def test_projection_changes_only_allowlisted_paths(self):
        report = context_protocol_v3.measure(None)
        self.assertEqual(report["schema"], "fr-agent-context-protocol-projection-2")
        self.assertEqual(report["feature_applicability"], {
            "multi_select_eligible_groups": 0,
            "plan_basis_eligible_pairs": 0,
        })
        contribution = report["contributions"]["bytes"]
        self.assertGreater(contribution["total"], 0)
        self.assertEqual(contribution["total"], contribution["skill_and_current_docs"])
        for trial in report["trials"]:
            for event in trial["allowlist_audit"]:
                self.assertTrue(set(event["output_paths"]) <=
                                set(report["allowlist"][event["rule"]]))

    def test_retained_projection_matches_recomputed_byte_evidence(self):
        retained = json.loads((TOOLS.parent / "tests/agent-eval/context-protocol-v3.json").read_text())
        actual = context_protocol_v3.measure(None)
        self.assertEqual(actual["summary"]["bytes"], retained["summary"]["bytes"])
        self.assertEqual(actual["contributions"]["bytes"], retained["contributions"]["bytes"])
        self.assertEqual(
            [trial["allowlist_audit"] for trial in actual["trials"]],
            [trial["allowlist_audit"] for trial in retained["trials"]],
        )

    def test_transaction_projection_changes_only_the_versioned_namespace(self):
        event = {
            "request": {"tool": "fr", "args": ["history", "patch", "7"]},
            "visible": json.dumps({"result": {"id": 7, "record_basis": "a" * 64}}),
        }
        bases = context_protocol_v3.projection_transaction_bases([event])
        self.assertEqual(bases, {7: "frtb2:" + "a" * 64})


class AgentWorkflowV4Evidence(unittest.TestCase):
    def test_counterfactual_normalizes_checkout_paths_and_opaque_live_ids(self):
        old_prompt = (workflow_v4.TRIAL / "prompt.txt").read_text()
        expected = workflow_v4.prescribed_prompt(old_prompt)
        checkout = Path("/home/runner/work/a-different-checkout")
        with mock.patch.object(workflow_v4, "ROOT", checkout), \
                mock.patch.object(workflow_v4.harness, "ROOT", checkout):
            self.assertEqual(workflow_v4.prescribed_prompt(old_prompt), expected)

        first = {"revision": "1" * 64, "handle": f"frp1:{'1' * 32}:abc",
                 "context_basis": f"frcb1:{'2' * 64}"}
        second = {"revision": "a" * 64, "handle": f"frp1:{'a' * 32}:abc",
                  "context_basis": f"frcb1:{'b' * 64}"}
        self.assertEqual(workflow_v4.stable_live_value(first),
                         workflow_v4.stable_live_value(second))

    def test_prescribed_trace_reduction_is_live_bounded_and_retained(self):
        retained = json.loads(
            (TOOLS.parent / "tests/agent-eval/agent-workflow-v4.json").read_text()
        )
        actual = workflow_v4.measure(TOOLS.parent / "target/debug/fr")
        self.assertTrue(actual["passed"])
        self.assertEqual(actual["observed"]["calls"], 42)
        self.assertEqual(actual["prescribed"]["calls"], 29)
        self.assertEqual(actual["difference"]["calls"], -13)
        self.assertEqual(len(actual["removed_calls"]), 13)
        for section in ("observed", "prescribed", "difference"):
            for field in ("calls", "prompt_bytes", "visible_output_bytes", "tool_request_bytes"):
                self.assertEqual(actual[section][field], retained[section][field])
        self.assertEqual(actual["removed_calls"], retained["removed_calls"])
        self.assertEqual(actual["measurement_files"], retained["measurement_files"])

    def test_fresh_cohorts_retain_failed_and_passing_acceptance(self):
        root = TOOLS.parent / "tests/agent-eval/results"
        expected = {
            "2026-09-11-workflow-v4-diagnostic-1": {
                "accepted": False, "commit": "882a13fd172796da340428de76747134c91a930d",
                "fr": (False, 23973, 49), "files": (False, 13852, 21),
            },
            "2026-09-11-workflow-v4": {
                "accepted": True, "commit": "d579f6b4ade93608d4b8043fc657e9b2323c6f88",
                "fr": (True, 13949, 30), "files": (True, 12815, 23),
            },
        }
        for directory, cohort in expected.items():
            evidence = root / directory
            manifest = json.loads((evidence / "manifest.json").read_text())
            self.assertEqual(manifest["acceptance"]["passed"], cohort["accepted"])
            self.assertEqual(manifest["implementation_commit"], cohort["commit"])
            for relative, sha256 in manifest["files"].items():
                self.assertEqual(harness.digest(harness.within(evidence, relative).read_bytes()), sha256)
            for arm in ("fr", "files"):
                result = json.loads((evidence / f"regex-escape-len-{arm}/result.json").read_text())
                self.assertEqual(
                    (result["passed"], result["context_tokens"], result["tool_calls"]),
                    cohort[arm],
                )
                for field in ("undo_exact", "redo_exact", "index_unchanged",
                              "receiver_index_unchanged", "receiver_matches"):
                    self.assertTrue(result[field])


class ProjectBatchAgentEvidence(unittest.TestCase):
    def test_token_counts_canonicalize_opaque_identity_spellings(self):
        first = json.dumps({"revision": "a" * 64, "handle_prefix": "b" * 32})
        second = json.dumps({"revision": "c" * 64, "handle_prefix": "d" * 32})
        self.assertEqual(project_batch_measurement.canonical_token_text(first),
                         project_batch_measurement.canonical_token_text(second))

    def test_passing_pair_is_immutable_and_uses_project_batches(self):
        evidence = TOOLS.parent / "tests/agent-eval/results/2026-09-11-project-batch"
        manifest = json.loads((evidence / "manifest.json").read_text())
        self.assertTrue(manifest["acceptance"]["passed"])
        self.assertEqual(manifest["implementation_commit"],
                         "f0de990391baf1a0aec75d12345f570672d15a8a")
        experiment = json.loads((evidence / "experiment.json").read_text())
        self.assertEqual(experiment["prompt_variant"]["name"],
                         "project-batch-broad-exploration-v1")
        for relative, sha256 in manifest["files"].items():
            actual = harness.digest(harness.within(evidence, relative).read_bytes())
            self.assertEqual(actual, sha256)

        expected = {"fr": (22185, 45), "files": (19610, 27)}
        for arm, metrics in expected.items():
            trial = evidence / f"regex-escape-len-{arm}"
            result = json.loads((trial / "result.json").read_text())
            self.assertTrue(result["passed"])
            self.assertEqual((result["context_tokens"], result["tool_calls"]), metrics)
            for field in ("undo_exact", "redo_exact", "workflow_ordered",
                          "index_unchanged", "receiver_index_unchanged", "receiver_matches"):
                self.assertTrue(result[field])

        events = [json.loads(line) for line in
                  (evidence / "regex-escape-len-fr/events.jsonl").read_text().splitlines()]
        batches = [event for event in events
                   if event["request"].get("args", [])[:2] == ["project", "batch"]]
        self.assertEqual(len(batches), 3)
        self.assertEqual(sum(json.loads(event["visible"])["exit_code"] == 0
                             for event in batches), 2)


class CheckPolicyEvidence(unittest.TestCase):
    def setUp(self):
        root = checks_policy.EVIDENCE / "regex-escape-len-files-r1"
        self.events = [json.loads(line) for line in (root / "events.jsonl").read_text().splitlines()]
        reports = [json.loads(event["visible"])["result"] for event in self.events
                   if event["request"].get("args", [])[:1] == ["checks"]]
        self.listing, self.report = reports[:2]

    def test_projection_preserves_transcript_and_all_non_execution_payloads(self):
        original = copy.deepcopy(self.events)
        for policy in checks_policy.POLICIES:
            outputs, changes = checks_policy.project_events(self.events, policy)
            self.assertEqual(len(outputs), len(self.events))
            indices = {change["event_index"] for change in changes}
            self.assertEqual(len(indices), 4)
            for index, (event, output) in enumerate(zip(self.events, outputs)):
                if index not in indices:
                    self.assertEqual(output, event["visible"])
                else:
                    before, after = json.loads(event["visible"]), json.loads(output)
                    before.pop("result")
                    after.pop("result")
                    self.assertEqual(before, after)
            self.assertEqual(self.events, original)

    def test_retained_cohort_summarizes_two_complete_pairs(self):
        manifest = checks_policy.verify_evidence(checks_policy.EVIDENCE)
        trials = []
        for name in manifest["trials"]:
            root = checks_policy.EVIDENCE / name
            config = json.loads((root / "session.json").read_text())
            events = [json.loads(line) for line in (root / "events.jsonl").read_text().splitlines()]
            outputs, _ = checks_policy.project_events(events, "quiet_success_no_declarations")
            trials.append({"arm": config["arm"],
                           "policies": {"quiet_success_no_declarations":
                                        checks_policy.totals((root / "prompt.txt").read_text(), events, outputs, None)}})
        result = checks_policy.comparison(trials, "quiet_success_no_declarations", "bytes")
        self.assertEqual(result["fr_minus_files"], result["fr_mean"] - result["files_mean"])
        self.assertGreater(result["fr_percent_difference"], 0)

    def test_failed_diagnostics_and_raw_byte_totals_survive_both_policies(self):
        report = copy.deepcopy(self.report)
        report["passed"] = False
        success, failure = report["results"]
        success["stdout"].update(text="\ufffd", retained_bytes=1, omitted_bytes=7)
        failure.update(passed=False, exit_code=7, timed_out=True, output_limit_exceeded=True,
                       error="retained diagnostic")
        failure["stderr"].update(text="\ufffd", retained_bytes=1, omitted_bytes=13)
        for policy in checks_policy.POLICIES:
            result = checks_policy.project(report, self.listing, policy)
            self.assertFalse(result["passed"])
            self.assertEqual(result["results"][0]["stdout"]["omitted_bytes"], 8)
            self.assertEqual(result["results"][0]["stdout"]["retained_bytes"], 0)
            for key in failure.keys() - set(checks_policy.DECLARATION_FIELDS):
                self.assertEqual(result["results"][1][key], failure[key])
            self.assertEqual(checks_policy.project(result, self.listing, policy), result)

    def test_declaration_omission_is_reversible_with_the_reviewed_listing(self):
        compact = checks_policy.project(self.report, self.listing, "quiet_success_no_declarations")
        restored = checks_policy.project(compact, self.listing, "quiet_success")
        self.assertEqual(restored, checks_policy.project(self.report, self.listing, "quiet_success"))
        for key in ("basis", "root", "configuration", "not_run", "execution", "coverage_authority",
                    "source_snapshot_checked", "passed"):
            self.assertEqual(compact[key], self.report[key])

    def test_missing_or_stale_listing_and_conflicting_declarations_are_rejected(self):
        for listing in (None, {**self.listing, "basis": "stale"}, {**self.listing, "root": "/elsewhere"}):
            with self.assertRaises(ValueError):
                checks_policy.project(self.report, listing, "quiet_success_no_declarations")
        for field in checks_policy.DECLARATION_FIELDS:
            report = copy.deepcopy(self.report)
            report["results"][0][field] = "changed"
            with self.assertRaisesRegex(ValueError, "declaration differs"):
                checks_policy.project(report, self.listing, "quiet_success")

    def test_truncated_or_unstructured_check_payloads_are_rejected(self):
        for patch in ({"stdout_omitted_bytes": 1}, {"result": "truncated JSON"}):
            events = copy.deepcopy(self.events)
            event = next(event for event in events if event["request"].get("args", [])[:1] == ["checks"])
            event["visible"] = json.dumps({**json.loads(event["visible"]), **patch})
            with self.assertRaisesRegex(ValueError, "truncated or unstructured"):
                checks_policy.project_events(events, "quiet_success")

    def test_tampered_evidence_is_rejected_before_measurement(self):
        manifest = json.loads((checks_policy.EVIDENCE / "manifest.json").read_text())
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            manifest["files"] = {"payload": harness.digest(b"original")}
            harness.save(root / "manifest.json", manifest)
            (root / "payload").write_bytes(b"tampered")
            with self.assertRaisesRegex(ValueError, "checksum mismatch"):
                checks_policy.verify_evidence(root)


class CoordinatedWorkspaceEvidence(unittest.TestCase):
    def test_new_task_preserves_old_scopes_and_has_complete_pairs(self):
        task = harness.regex_escape_len.TASK
        self.assertEqual(harness.edit_paths(task), ("regex-syntax/src/lib.rs", "src/lib.rs"))
        for old in (*harness.STRSIM_TASKS, harness.regex_workspace.TASK):
            self.assertEqual(harness.edit_paths(old), ("src/lib.rs",))
        names = harness.trial_names("regex-coordinated", 2)
        self.assertEqual(len(names), 4)
        self.assertEqual({name for name, _, _, _ in names}, {
            "regex-escape-len-fr-r1", "regex-escape-len-files-r1", "regex-escape-len-fr-r2", "regex-escape-len-files-r2"})
        self.assertEqual(harness.required_checks(task), ["upstream", "minimal"])
        prompt = harness.prompt(Path("session"), task, "fr")
        self.assertIn("regex-syntax/src/lib.rs, src/lib.rs", prompt)
        self.assertIn("one author batch saved transaction", prompt)
        self.assertIn('{"tool":"read","path":"skill/SKILL.md","start":1,"lines":80}', prompt)
        self.assertIn('{"tool":"read","path":"skill/references/author.md","start":1,"lines":160}', prompt)
        self.assertIn('{"tool":"read","path":"skill/references/workflow.md","start":1,"lines":160}', prompt)
        self.assertIn("do not pass --write to author batch", prompt)
        self.assertIn("exercise-reversal false", prompt)
        self.assertIn("patch output .fr-agent-change.patch", prompt)
        self.assertIn("do not repeat that check or call history apply or history patch", prompt)
        self.assertIn("both API insertion operations and the regex-syntax escape body replacement", prompt)
        self.assertIn("at most 200 lines per read", prompt)
        self.assertIn("omit the `fr` executable name", prompt)
        self.assertIn("never write placeholder references", prompt)
        self.assertIn("refuses every source-changing request until the original checks pass", prompt)
        self.assertIn("--request-stdin <<'FRJSON'", prompt)
        self.assertIn("Apply refuses until all declared checks pass", prompt)
        self.assertIn("It refuses until all declared checks pass after the final redo/apply", prompt)
        example = prompt.split("--request-stdin <<'FRJSON'\n", 1)[1].split("\nFRJSON", 1)[0]
        self.assertEqual(json.loads(example)["text"], "{\n    buf.push('\\\\');\n}")

    def test_stdin_request_preserves_source_apostrophes_and_backslashes(self):
        source = "{\n    buf.push('\\\\');\n}"
        request = harness.request_input(None, True, io.StringIO(json.dumps({
            "tool": "write", "path": "fragment.rs", "text": source,
        })))
        self.assertEqual(request["text"], source)
        with self.assertRaisesRegex(ValueError, "exactly one request source"):
            harness.request_input("{}", True, io.StringIO("{}"))
        with self.assertRaisesRegex(ValueError, "JSON object"):
            harness.request_input("[]", False, io.StringIO())

    def test_baseline_diagnostics_must_only_report_missing_requested_apis(self):
        valid = {"level": "error", "code": {"code": "E0425"}, "message": "cannot find function `escape_len` in crate `regex`"}
        encode = lambda value: json.dumps(value).encode() + b"\n"
        self.assertTrue(harness.regex_escape_len.missing_api_only(encode(valid)))
        self.assertTrue(harness.regex_escape_len.missing_api_only(encode({**valid, "message": "cannot find value `escape_len` in crate `regex_syntax`"})))
        for error in ({**valid, "message": "cannot find function `other` in crate `regex`"},
                      {**valid, "code": {"code": "E0308"}}, {**valid, "code": None}):
            self.assertFalse(harness.regex_escape_len.missing_api_only(encode(valid) + encode(error)))
        for diagnostic in (b"compiler crashed", b"", b"null\n"):
            self.assertFalse(harness.regex_escape_len.missing_api_only(diagnostic))

    def test_file_arm_edits_and_exports_only_the_task_paths(self):
        with tempfile.TemporaryDirectory() as tmp:
            session = Path(tmp)
            project = session / "project"
            (project / "src").mkdir(parents=True)
            (project / "regex-syntax/src").mkdir(parents=True)
            (session / "artifacts").mkdir()
            for path in harness.edit_paths(harness.regex_escape_len.TASK):
                (project / path).write_text("original\n")
            harness.initialize(project)
            config = {"arm": "files", "task": harness.regex_escape_len.TASK}
            for path in harness.edit_paths(config["task"]):
                result = harness.action(session, config, {"tool": "replace", "path": path, "old": "original", "new": "changed"})
                self.assertTrue(result["changed"])
            exported = harness.action(session, config, {"tool": "export"})["patch"]
            self.assertIn("a/src/lib.rs", exported)
            self.assertIn("a/regex-syntax/src/lib.rs", exported)
            for path in ("Cargo.toml", "../outside", "regex-syntax/src/../src/lib.rs"):
                with self.assertRaisesRegex(ValueError, "limited"):
                    harness.action(session, config, {"tool": "append", "path": path, "text": "bad"})
            with self.assertRaisesRegex(ValueError, "limited"):
                harness.action(session, {**config, "task": "regex-escape-into"},
                               {"tool": "append", "path": "regex-syntax/src/lib.rs", "text": "bad"})

    def test_coordinated_delivery_requires_one_saved_batch_and_its_history(self):
        def event(args, report):
            return {"request": {"tool": "fr", "args": args}, "visible": json.dumps({"exit_code": 0, "result": report})}
        saved = event(["author", "batch"], {"schema": "fr-author-batch-1", "saved": True, "applied": False,
                                             "files_changed": 2, "transaction": 1})
        events = [saved, *[event(["history", action, "1", "--write"], {}) for action in ("apply", "undo", "redo", "patch")]]
        self.assertTrue(harness.coordinated_batch(events))
        for index in range(len(events)):
            self.assertFalse(harness.coordinated_batch(events[:index] + events[index + 1:]))
        self.assertFalse(harness.coordinated_batch([*events, saved]))
        broken = copy.deepcopy(events)
        broken[-1]["request"]["args"][2] = "2"
        self.assertFalse(harness.coordinated_batch(broken))
        for key, value in (("schema", "fr-author-1"), ("files_changed", 1), ("applied", True), ("transaction", True)):
            broken = copy.deepcopy(events)
            payload = json.loads(broken[0]["visible"])
            payload["result"][key] = value
            broken[0]["visible"] = json.dumps(payload)
            self.assertFalse(harness.coordinated_batch(broken))

    def test_coordinated_delivery_reconstructs_compact_saved_batch(self):
        def event(args, report):
            return {"request": {"tool": "fr", "args": args},
                    "visible": json.dumps({"exit_code": 0, "result": report})}

        basis = "frpb1:" + "a" * 64
        preview = event(["author", "batch", "--from", "manifest.json"], {
            "schema": "fr-author-batch-1", "saved": False, "applied": False,
            "files_changed": 2, "plan_context_basis": basis,
        })
        saved = event(["author", "batch", "--from", "manifest.json", "--save-plan",
                       "--plan-basis", basis], {
            "schema": "fr-author-batch-1", "saved": True, "applied": False,
            "transaction": 1, "plan_context_basis": basis,
            "plan_context_omitted": ["files_changed"],
        })
        history = [event(["history", action, "1", "--write"], {})
                   for action in ("apply", "undo", "redo", "patch")]
        self.assertTrue(harness.coordinated_batch([preview, saved, *history]))
        self.assertFalse(harness.coordinated_batch([saved, *history]))

        wrong_basis = copy.deepcopy(preview)
        payload = json.loads(wrong_basis["visible"])
        payload["result"]["plan_context_basis"] = "frpb1:" + "b" * 64
        wrong_basis["visible"] = json.dumps(payload)
        self.assertFalse(harness.coordinated_batch([wrong_basis, saved, *history]))

    def test_coordinated_delivery_accepts_one_reviewed_workflow(self):
        def event(args, report, **payload):
            return {"request": {"tool": "fr", "args": args},
                    "visible": json.dumps({"exit_code": 0, "result": report, **payload})}

        basis = "frwb1:" + "a" * 64
        saved = event(["author", "batch"], {
            "schema": "fr-author-batch-1", "saved": True, "applied": False,
            "files_changed": 2, "transaction": 7,
        })
        preview = event(["workflow", "--from", "/tmp/workflow.json"], {
            "schema": "fr-workflow-1", "transaction": 7, "ready": True,
            "executed": False, "workflow_basis": basis,
        })
        completed = event(
            ["workflow", "--from", "/tmp/workflow.json", "--write", "--basis", basis],
            {"schema": "fr-workflow-1", "transaction": 7, "executed": True,
             "passed": True, "workflow_basis": basis,
             "stages": [{"stage": name, "status": "passed"}
                        for name in ("apply", "check-applied", "deliver-patch")]},
            patch_artifact="artifacts/change.patch",
        )
        history = [event(["history", action, "7", "--write"], {}) for action in ("undo", "redo")]
        events = [saved, preview, completed, *history]
        self.assertTrue(harness.coordinated_batch(events))
        for index in (1, 2, 3, 4):
            self.assertFalse(harness.coordinated_batch(events[:index] + events[index + 1:]))
        broken = copy.deepcopy(events)
        broken[2]["request"]["args"][-1] = "frwb1:" + "b" * 64
        self.assertFalse(harness.coordinated_batch(broken))

    def test_nested_workflow_check_preserves_ordered_acceptance(self):
        original = {"src/lib.rs": {"sha256": "old"}}
        final = {"src/lib.rs": {"sha256": "new"}}
        check = {"schema": "fr-checks-1", "executed": True, "passed": True,
                 "results": [{"name": "unit", "passed": True}]}

        def event(tool, before, after, payload, sentinel=None, args=None):
            return {"request": {"tool": tool, "args": args or []}, "before": before,
                    "after": after, "sentinel": sentinel, "visible": json.dumps(payload)}

        events = [
            event("fr", original, original, {"exit_code": 0, "result": check}, args=["checks"]),
            event("fr", original, final, {"exit_code": 0, "result": {
                "schema": "fr-workflow-1", "executed": True, "passed": True,
                "stages": [{"stage": "apply", "status": "passed"},
                           {"stage": "check-applied", "status": "passed", "result": {
                               "passed": True, "results": [{"name": "unit", "passed": True}],
                           }}],
            }}, args=["workflow"]),
            event("sentinel", final, final, {"created": "unrelated.txt"},
                  sentinel="Preserve this independent later edit.\n"),
            event("fr", final, original, {"exit_code": 0, "result": {}},
                  sentinel="Preserve this independent later edit.\n", args=["history", "undo"]),
            event("fr", original, original, {"exit_code": 0, "result": check},
                  sentinel="Preserve this independent later edit.\n", args=["checks"]),
            event("fr", original, final, {"exit_code": 0, "result": {}},
                  sentinel="Preserve this independent later edit.\n", args=["history", "redo"]),
            event("fr", final, final, {"exit_code": 0, "result": check},
                  sentinel="Preserve this independent later edit.\n", args=["checks"]),
            event("receiver", final, final, {"patch_applied": True, "matches": True},
                  sentinel="Preserve this independent later edit.\n"),
        ]
        observed = harness.workflow(events, original, final, ["unit"])
        self.assertTrue(observed["workflow_ordered"])
        self.assertTrue(any(check.get("workflow_stage") == "check-applied"
                            for check in observed["checks"]))
        self.assertTrue(harness.state_checked(events[1:2], final, ["unit"]))

    def test_receiver_requires_checks_after_the_latest_state_change(self):
        original = {"src/lib.rs": {"sha256": "old"}}
        changed = {"src/lib.rs": {"sha256": "new"}}

        def event(before, after, report=None):
            return {"before": before, "after": after, "visible": json.dumps({
                "exit_code": 0,
                "result": report or {},
            })}

        check = {"schema": "fr-checks-1", "executed": True, "passed": True,
                 "results": [{"name": "unit", "passed": True}]}
        events = [event(original, changed), event(changed, changed, check)]
        self.assertTrue(harness.current_state_checked(events, changed, ["unit"]))
        events.extend([event(changed, original), event(original, original, check), event(original, changed)])
        self.assertFalse(harness.current_state_checked(events, changed, ["unit"]))
        events.append(event(changed, changed, check))
        self.assertTrue(harness.current_state_checked(events, changed, ["unit"]))
        self.assertFalse(harness.current_state_checked(events, changed, ["unit", "missing"]))

    def test_source_mutations_require_a_complete_original_check(self):
        original = {"src/lib.rs": {"sha256": "old"}}
        check = {"schema": "fr-checks-1", "executed": True, "passed": True,
                 "results": [{"name": "upstream", "passed": True},
                             {"name": "minimal", "passed": True}]}
        event = {"before": original, "after": original,
                 "visible": json.dumps({"exit_code": 0, "result": check})}
        self.assertFalse(harness.state_checked([], original, ["upstream", "minimal"]))
        self.assertTrue(harness.state_checked([event], original, ["upstream", "minimal"]))
        incomplete = copy.deepcopy(event)
        payload = json.loads(incomplete["visible"])
        payload["result"]["results"].pop()
        incomplete["visible"] = json.dumps(payload)
        self.assertFalse(harness.state_checked([incomplete], original, ["upstream", "minimal"]))
        for request in (
            {"tool": "replace"}, {"tool": "append"}, {"tool": "reverse"},
            {"tool": "apply"}, {"tool": "fr", "args": ["history", "apply", "1", "--write"]},
        ):
            self.assertTrue(harness.source_mutation_requested(request))
        for request in ({"tool": "write"}, {"tool": "fr", "args": ["author", "batch"]}):
            self.assertFalse(harness.source_mutation_requested(request))

    def test_step_refuses_source_mutation_before_original_checks(self):
        with tempfile.TemporaryDirectory() as tmp:
            session = Path(tmp)
            project = session / "project"
            for name in harness.edit_paths(harness.regex_escape_len.TASK):
                path = project / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("original\n")
            harness.initialize(project)
            original = harness.snapshot(project)
            harness.save(session / "session.json", {
                "arm": "files", "task": harness.regex_escape_len.TASK, "original": original,
            })
            request = {"tool": "replace", "path": "src/lib.rs",
                       "old": "original", "new": "changed"}
            with contextlib.redirect_stdout(io.StringIO()):
                harness.step(session, request)
            self.assertEqual((project / "src/lib.rs").read_text(), "original\n")
            refusal = json.loads((session / "events.jsonl").read_text().splitlines()[0])
            self.assertIn("original source", json.loads(refusal["visible"])["error"])

            check = {"schema": "fr-checks-1", "executed": True, "passed": True,
                     "results": [{"name": name, "passed": True}
                                 for name in harness.required_checks(harness.regex_escape_len.TASK)]}
            event = {"request": {"tool": "fr", "args": ["checks", "--run"]},
                     "visible": json.dumps({"exit_code": 0, "result": check}),
                     "before": original, "after": original, "index_sha256": "same",
                     "sentinel": None}
            with (session / "events.jsonl").open("a") as log:
                log.write(json.dumps(event) + "\n")
            with contextlib.redirect_stdout(io.StringIO()):
                harness.step(session, request)
            self.assertEqual((project / "src/lib.rs").read_text(), "changed\n")

    def test_coordinated_manifest_requires_the_complete_single_transaction(self):
        with tempfile.TemporaryDirectory() as tmp:
            session = Path(tmp)
            project = session / "project"
            artifacts = session / "artifacts"
            project.mkdir()
            artifacts.mkdir()
            config = {"task": harness.regex_escape_len.TASK}
            path = artifacts / "manifest.json"
            complete = {
                "operations": [
                    {"op": "insert-declaration"},
                    {"op": "insert-declaration"},
                    {"op": "replace-body"},
                ],
                "postconditions": {
                    "files-changed": 2,
                    "edits": 3,
                    "changed-operations": 3,
                    "paths-changed": list(harness.regex_escape_len.PATHS),
                },
            }
            path.write_text(json.dumps(complete))
            args = ["author", "batch", "--from", "../artifacts/manifest.json"]
            harness.validate_coordinated_manifest(session, project, config, args)
            for broken in (
                {**complete, "operations": complete["operations"][:2]},
                {**complete, "postconditions": {**complete["postconditions"], "edits": 2}},
            ):
                path.write_text(json.dumps(broken))
                with self.assertRaisesRegex(ValueError, "coordinated batch"):
                    harness.validate_coordinated_manifest(session, project, config, args)

    def test_coordinated_manifest_validation_preserves_help_and_explains_artifact_paths(self):
        with tempfile.TemporaryDirectory() as tmp:
            session = Path(tmp)
            project = session / "project"
            artifacts = session / "artifacts"
            project.mkdir()
            artifacts.mkdir()
            config = {"task": harness.regex_escape_len.TASK}
            harness.validate_coordinated_manifest(
                session, project, config, ["author", "batch", "--help"]
            )
            with self.assertRaisesRegex(ValueError, "absolute fr_reference"):
                harness.validate_coordinated_manifest(
                    session,
                    project,
                    config,
                    ["author", "batch", "--from", "artifacts/manifest.json"],
                )
            with self.assertRaisesRegex(ValueError, "omits the executable name"):
                harness.action(
                    session,
                    {"arm": "fr", "task": harness.regex_escape_len.TASK},
                    {"tool": "fr", "args": ["fr", "author", "batch"]},
                )

    def test_write_returns_the_exact_fr_artifact_reference(self):
        with tempfile.TemporaryDirectory() as tmp:
            session = Path(tmp)
            (session / "project").mkdir()
            (session / "artifacts").mkdir()
            result = harness.action(
                session,
                {"arm": "fr", "task": harness.regex_escape_len.TASK},
                {"tool": "write", "path": "fragment.rs", "text": "{}\n"},
            )
            expected = str((session / "artifacts/fragment.rs").resolve())
            self.assertEqual(result["path"], expected)
            self.assertEqual(result["fr_reference"], expected)

    def test_coordinated_workflow_manifest_and_patch_receipt_are_bounded(self):
        with tempfile.TemporaryDirectory() as tmp:
            session = Path(tmp)
            project = session / "project"
            artifacts = session / "artifacts"
            project.mkdir()
            artifacts.mkdir()
            config = {"task": harness.regex_escape_len.TASK}
            manifest = {
                "schema": 1,
                "transaction": 7,
                "transaction-context-basis": "frtb2:" + "a" * 64,
                "checks": {"basis": "b" * 64, "names": ["upstream", "minimal"]},
                "exercise-reversal": False,
                "patch": {"output": ".fr-agent-change.patch"},
                "check-output-bytes": 2048,
            }
            manifest_path = artifacts / "workflow.json"
            manifest_path.write_text(json.dumps(manifest))
            args = ["workflow", "--from", str(manifest_path)]
            self.assertEqual(
                harness.coordinated_workflow_manifest(session, project, config, args), manifest
            )
            broken = copy.deepcopy(manifest)
            broken["exercise-reversal"] = True
            manifest_path.write_text(json.dumps(broken))
            with self.assertRaisesRegex(ValueError, "disable bundled reversal"):
                harness.coordinated_workflow_manifest(session, project, config, args)

            patch = b"diff --git a/a b/a\n"
            (project / ".fr-agent-change.patch").write_bytes(patch)
            report = {
                "schema": "fr-workflow-1", "executed": True, "passed": True,
                "stages": [{"stage": "deliver-patch", "status": "passed", "result": {
                    "output": ".fr-agent-change.patch", "bytes": len(patch),
                    "sha256": harness.digest(patch),
                }}],
            }
            self.assertEqual(
                harness.retain_workflow_patch(session, project, report), "artifacts/change.patch"
            )
            self.assertEqual((artifacts / "change.patch").read_bytes(), patch)
            self.assertFalse((project / ".fr-agent-change.patch").exists())

    def test_replay_checks_the_second_file_and_reverses_complete_snapshots(self):
        task = harness.regex_escape_len.TASK
        observed = {"checks": [], "undo_exact": True, "redo_exact": True, "workflow_ordered": True}

        def unpack(root, task):
            for name in harness.edit_paths(task):
                (root / name).parent.mkdir(parents=True, exist_ok=True)
                (root / name).write_text("original\n")

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            fixture = root / "fixture"
            unpack(fixture, task)
            harness.initialize(fixture)
            original = harness.snapshot(fixture)
            for name in harness.edit_paths(task):
                (fixture / name).write_text("changed\n")
            final = harness.snapshot(fixture)
            trial = root / "evidence/trial"
            trial.mkdir(parents=True)
            harness.save(trial / "session.json", {"task": task, "arm": "files", "original": original})
            harness.save(trial / "result.json", {"passed": True, **observed})
            (trial / "change.patch").write_bytes(harness.git(fixture, "diff", "--binary").stdout)
            for wrong_second_file in (False, True):
                after = copy.deepcopy(final)
                if wrong_second_file:
                    after["regex-syntax/src/lib.rs"]["sha256"] = "wrong"
                (trial / "events.jsonl").write_text(json.dumps({"after": after}) + "\n")
                harness.save(trial.parent / "manifest.json", {"trials": ["trial"], "files": {
                    str(p.relative_to(trial.parent)): harness.digest(p.read_bytes()) for p in trial.iterdir()}})
                with mock.patch.object(harness, "unpack", side_effect=unpack), \
                     mock.patch.object(harness, "verify", side_effect=[{"passed": False}, {"passed": True}]), \
                     mock.patch.object(harness, "upstream", return_value={"exit_code": 0}), \
                     mock.patch.object(harness, "workflow", return_value=observed):
                    if wrong_second_file:
                        with self.assertRaisesRegex(ValueError, "recorded agent result"):
                            harness.replay(trial.parent)
                    else:
                        self.assertTrue(harness.replay(trial.parent)["passed"])


class BatchMeasurementEvidence(unittest.TestCase):
    def test_failed_or_clipped_commands_cannot_count_as_successful_evidence(self):
        valid = mock.Mock(returncode=0, stdout=b'{"passed": true}\n', stderr=b"")
        report, visible = batch_measurement.checked_payload(valid)
        self.assertEqual(report, {"passed": True})
        self.assertEqual(json.loads(visible)["stdout_omitted_bytes"], 0)
        for field, value in (("returncode", 1), ("stdout", b" " * 20001), ("stderr", b"x" * 4097)):
            broken = copy.copy(valid)
            setattr(broken, field, value)
            with self.subTest(field=field), self.assertRaises(AssertionError):
                batch_measurement.checked_payload(broken)

    def test_review_requires_complete_diff_and_a_saved_unapplied_transaction(self):
        valid = {"diff": "complete patch", "changed": True, "saved": True, "applied": False, "transaction": 1}
        batch_measurement.complete_diff(valid)
        for key, value in (("diff", {"text": "partial", "omitted_bytes": 10}), ("diff", ""), ("changed", False),
                           ("saved", False), ("applied", True), ("transaction", None), ("transaction", 0)):
            with self.subTest(key=key, value=value), self.assertRaises(AssertionError):
                batch_measurement.complete_diff({**valid, key: value})

    def test_source_comparison_rejects_line_ending_and_semantic_changes(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            original = "// π\r\nfn value() -> i32 { 4 }\r\n"
            (root / "main.rs").write_bytes(original.encode())
            batch_measurement.require_sources(root, {"main.rs": original})
            for changed in (original.replace("\r\n", "\n"), original.replace("4", "7")):
                (root / "main.rs").write_bytes(changed.encode())
                with self.assertRaisesRegex(AssertionError, "source bytes"):
                    batch_measurement.require_sources(root, {"main.rs": original})
            (root / "main.rs").write_bytes(original.encode())
            (root / "extra.rs").write_bytes(b"fn unexpected() {}")
            with self.assertRaisesRegex(AssertionError, "source inventory"):
                batch_measurement.require_sources(root, {"main.rs": original})

    def test_selection_refuses_truncated_or_ambiguous_source(self):
        valid = {"page": {"total": 1, "next": None}, "columns": ["path", "name", "handle", "source"],
                 "rows": [["main.rs", "main", "function", {"next_offset": None}]], "root": "file"}
        self.assertEqual(batch_measurement.selection(valid, "main.rs", "main"), ("function", "file"))
        for mutation in ("more", "ambiguous", "truncated", "wrong-path", "wrong-name"):
            broken = copy.deepcopy(valid)
            if mutation == "more":
                broken["page"]["next"] = "cursor"
            elif mutation == "ambiguous":
                broken["page"]["total"] = 2
            elif mutation == "truncated":
                broken["rows"][0][3]["next_offset"] = 10
            elif mutation == "wrong-path":
                broken["rows"][0][0] = "calc.rs"
            else:
                broken["rows"][0][1] = "other"
            with self.subTest(mutation=mutation), self.assertRaises(AssertionError):
                batch_measurement.selection(broken, "main.rs", "main")

    def test_metrics_count_utf8_bytes_and_only_project_commands_as_scans(self):
        events = [{"phase": phase, "args": [command], "stdout": "π\n", "visible": "🙂"} for phase, command in
                  (("selection", "project"), ("authoring", "author"), ("history", "history"), ("checks", "checks"))]
        result = batch_measurement.metrics(events, None)
        self.assertEqual(result["project_commands"], 2)
        self.assertEqual(result["derived_scan_passes"], 4)
        self.assertEqual(result["groups"]["all"]["calls"], 4)
        self.assertEqual(result["groups"]["all"]["stdout"], {"bytes": 12, "tokens": None})
        self.assertEqual(result["groups"]["all"]["visible"], {"bytes": 16, "tokens": None})


class ProjectPhaseEvidence(unittest.TestCase):
    def test_construction_times_must_partition_the_outer_project_phase(self):
        valid = {"construction_seconds": {name: 0.01 for name in construction_measurement.CONSTRUCTION},
                 "phases_seconds": {"project": 0.1}}
        construction_measurement.check_construction(valid)
        for mutation in ("missing", "negative", "overflow"):
            report = copy.deepcopy(valid)
            if mutation == "missing":
                report["construction_seconds"].pop("reference_digest")
            elif mutation == "negative":
                report["construction_seconds"]["reference_digest"] = -0.1
            else:
                report["construction_seconds"]["reference_digest"] = 0.1
            with self.subTest(mutation=mutation), self.assertRaises(AssertionError):
                construction_measurement.check_construction(report)

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
                for filename in harness.CODEX_PROVENANCE_FILES:
                    (trial / filename).write_text(f"retained {filename}\n")
            output = root / "evidence"
            implementation = harness.git(harness.ROOT, "rev-parse", "HEAD").stdout.decode().strip()
            harness.record(
                root,
                output,
                execution_note="Synthetic regression; no agents",
                implementation_commit=implementation,
            )
            self.assertFalse((output / names[0] / "change.patch").exists())
            self.assertFalse(json.loads((output / names[0] / "result.json").read_text())["passed"])
            manifest = json.loads((output / "manifest.json").read_text())
            self.assertEqual(manifest["implementation_commit"], implementation)
            self.assertEqual(
                manifest["acceptance"],
                {"passed": False, "failed_trials": names},
            )
            for filename in harness.CODEX_PROVENANCE_FILES:
                copied = output / names[0] / filename
                self.assertEqual(copied.read_text(), f"retained {filename}\n")
                self.assertEqual(manifest["files"][f"{names[0]}/{filename}"], harness.digest(copied.read_bytes()))
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
