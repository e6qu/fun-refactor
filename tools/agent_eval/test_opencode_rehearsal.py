"""Offline OpenCode protocol and private-grader tests; never call a model service."""
import base64
import copy
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval import opencode_rehearsal as runner
from agent_eval.study import encode, load

ROOT = Path(__file__).resolve().parents[2]
FIXTURES = ROOT / "tests/agent-eval/opencode"


def stream(action=None, session="ses_abc", message="msg_abc"):
    tokens = {"input": 20, "output": 10, "reasoning": 5, "total": 42, "cache": {"read": 7, "write": 0}}
    return [
        {"type": "step_start", "sessionID": session, "part": {"messageID": message}},
        {"type": "text", "sessionID": session, "part": {"messageID": message, "text": json.dumps(action or {"action": "finish", "answer": "done"})}},
        {"type": "step_finish", "sessionID": session, "part": {"messageID": message, "reason": "stop", "tokens": tokens, "cost": 0}}]


def serialized(rows):
    return b"\n".join(encode(row) for row in rows)


class Protocol(unittest.TestCase):
    def test_prose_can_wrap_one_action_but_ambiguous_actions_refuse(self):
        rows = stream()
        rows[1]["part"]["text"] = "Here is my action:\n" + rows[1]["part"]["text"]
        result = runner.events(serialized(rows))
        self.assertEqual(result["action_format"], "embedded_json")
        rows[1]["part"]["text"] += '\n{"action":"list"}'
        with self.assertRaisesRegex(ValueError, "exactly one"):
            runner.events(serialized(rows))

    def test_retains_cli_usage_without_relabeling_it_as_billing(self):
        result = runner.events(serialized(stream()))
        self.assertEqual(result["tokens"]["reasoning"], 5)
        self.assertEqual(result["tokens"]["output"], 10)
        self.assertEqual(result["reported_cost"], 0)

    def test_incomplete_native_tool_and_error_streams_refuse(self):
        for rows in (stream()[:-1], stream() + [stream()[-1]],
                     stream() + [{"type": "tool_use", "sessionID": "ses_abc"}],
                     [{"type": "error", "sessionID": "ses_abc"}]):
            with self.subTest(rows=rows), self.assertRaises(ValueError):
                runner.events(serialized(rows))

    def test_session_or_message_switch_refuses(self):
        with self.assertRaises(ValueError):
            runner.events(serialized(stream()), "ses_other")
        rows = stream()
        rows[0]["part"]["messageID"] = "other"
        with self.assertRaises(ValueError):
            runner.events(serialized(rows))

    def test_negative_or_boolean_token_counts_refuse(self):
        for value in (-1, True, 1.5):
            rows = stream()
            rows[-1]["part"]["tokens"]["input"] = value
            with self.assertRaises(ValueError):
                runner.events(serialized(rows))

    def test_duplicate_action_keys_and_out_of_order_events_refuse(self):
        rows = stream()
        rows[1]["part"]["text"] = '{"action":"list","action":"finish","answer":"done"}'
        with self.assertRaises(ValueError):
            runner.events(serialized(rows))
        rows = stream()
        with self.assertRaises(ValueError):
            runner.events(serialized([rows[-1], *rows[:-1]]))

    def test_export_checks_model_all_messages_and_counts(self):
        turn = runner.events(serialized(stream()))
        export = {"info": {"id": "ses_abc"}, "messages": [{"info": {"id": "msg_abc", "role": "assistant", "finish": "stop",
                  "providerID": "provider", "modelID": "model", "tokens": turn["tokens"], "cost": 0}}]}
        self.assertTrue(runner.model_identity(export, "ses_abc", "provider/model", [turn]))
        for field, value in (("modelID", "substitution"), ("cost", 2), ("finish", "length")):
            wrong = copy.deepcopy(export)
            wrong["messages"][0]["info"][field] = value
            with self.assertRaises(ValueError):
                runner.model_identity(wrong, "ses_abc", "provider/model", [turn])
        export["messages"].append(export["messages"][0])
        with self.assertRaises(ValueError):
            runner.model_identity(export, "ses_abc", "provider/model", [turn])


class Rehearsal(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.binary = self.root / "fr"
        self.binary.write_text("fake binary")
        self.manifest = load(FIXTURES / "manifest.json")
        self.frozen = runner.freeze(self.manifest, FIXTURES, self.binary)

    def test_freezes_sources_graders_binary_and_randomized_pairs(self):
        self.assertEqual(self.frozen, runner.freeze(self.manifest, FIXTURES, self.binary))
        self.assertEqual(len(self.frozen["plan"]["cells"]), 8)
        for task in self.frozen["plan"]["tasks"]:
            self.assertTrue(task["source_sha256"])
            self.assertNotIn(task["grader"], task["files"])

    def test_changed_plan_and_unknown_attempt_refuse(self):
        changed = copy.deepcopy(self.frozen)
        changed["plan"]["limits"]["turns"] = 99
        with self.assertRaises(ValueError):
            runner.checked(changed)
        with self.assertRaises(ValueError):
            runner.run_attempt(self.frozen, "unknown", FIXTURES, self.root / "out", self.binary, Path("opencode"))

    def test_missing_and_interrupted_cells_remain_visible(self):
        output = self.root / "out"
        output.mkdir()
        (output / self.frozen["plan"]["cells"][0]["id"]).mkdir()
        report = runner.report(self.frozen, output)
        self.assertEqual(report["attempts"][0]["status"], "interrupted")
        self.assertEqual(report["attempts"][1]["status"], "pending")
        self.assertFalse(report["audit_complete"])

    def files(self, source="a = 1\n"):
        return {"module.py": {"data": base64.b64encode(source.encode()).decode(), "executable": False}}

    def action(self, request, files=None, arm="files", execute=None):
        return runner.action(files if files is not None else self.files(), request, arm, self.binary,
                             self.root / "workspace", execute or (lambda *args: b"{}"))

    def test_read_continuation_and_stale_edits(self):
        files = self.files("a = 'é'\n")
        first = self.action({"action": "read", "path": "module.py", "offset": 0, "bytes": 5, "sha256": ""}, files)
        self.action({"action": "replace", "path": "module.py", "old": "é", "new": "x"}, files)
        next_page = self.action({"action": "read", "path": "module.py", "offset": first["next_offset"], "bytes": 5,
                                 "sha256": first["sha256"]}, files)
        self.assertEqual(next_page["error"], "stale_source")

    def test_paths_and_ambiguous_replacements_refuse(self):
        for path in ("../secret", "/tmp/secret"):
            with self.assertRaises(ValueError):
                self.action({"action": "read", "path": path, "offset": 0, "bytes": 10, "sha256": ""})
            with self.assertRaises(ValueError):
                self.action({"action": "replace", "path": path, "old": "a", "new": "b"})
        with self.assertRaises(ValueError):
            self.action({"action": "replace", "path": "module.py", "old": "a", "new": "b"}, self.files("aa"))

    def test_search_returns_explicit_omissions(self):
        result = self.action({"action": "search", "text": "value"}, self.files("value\n" * 30))
        self.assertEqual(len(result["matches"]), 20)
        self.assertEqual(result["omitted_matches"], 10)

    def test_fr_arm_uses_only_fixed_public_read_commands(self):
        calls = []
        self.action({"action": "fr", "operation": "find", "name": "a"}, arm="fr",
                    execute=lambda *args: calls.append(args) or b'{"rows":[]}')
        self.assertEqual(calls[0][0][-6:], ["project", "find", "a", "--signature", "--limit", "12"])
        for request in ({"action": "fr", "operation": "find", "name": "a"}, {"action": "command", "argv": ["sh"]}):
            with self.assertRaises(ValueError):
                self.action(request)
        with self.assertRaises(ValueError):
            self.action({"action": "fr", "operation": "find", "name": "--help"}, arm="fr")

    def test_native_permissions_and_background_features_disabled(self):
        settings = runner.settings()
        self.assertEqual(settings["agent"][runner.AGENT]["permission"], {"*": "deny"})
        self.assertEqual(settings["share"], "disabled")
        self.assertFalse(settings["snapshot"])

    def test_show_accepts_only_full_handles_and_never_path_selectors(self):
        for handle in ("/etc/passwd", "../secret", "--help", "greet", "frp1:bad:1"):
            with self.subTest(handle=handle), self.assertRaises(ValueError):
                self.action({"action": "fr", "operation": "show", "handle": handle}, arm="fr")
        self.assertEqual(self.action({"action": "fr", "operation": "show", "handle": "frp1:" + "a"*32 + ":1"}, arm="fr"), {})

    def test_run_retains_failure_and_cannot_retry_same_cell(self):
        cell = self.frozen["plan"]["cells"][0]["id"]
        def failed(*args, **kwargs):
            return {"exit_code": 125, "stop_reason": "rss_bytes", "sampled_aggregate_rss_bytes": 900000000}
        with patch.object(runner, "bounded_run", side_effect=failed):
            result = runner.run_attempt(self.frozen, cell, FIXTURES, self.root / "out", self.binary, Path("opencode"))
        self.assertEqual(result["status"], "failed")
        self.assertIn("rss_bytes", result["failure"])
        self.assertEqual(runner.report(self.frozen, self.root / "out")["passed"], 0)
        with self.assertRaises(FileExistsError):
            runner.run_attempt(self.frozen, cell, FIXTURES, self.root / "out", self.binary, Path("opencode"))

    def test_success_reaudits_stream_export_grade_and_submission(self):
        cell = self.frozen["plan"]["cells"][0]
        rows = stream()
        turn = runner.events(serialized(rows))
        provider, model = cell["model"].split("/", 1)
        export = {"info": {"id": "ses_abc"}, "messages": [{"info": {"id": "msg_abc", "role": "assistant", "finish": "stop",
                  "providerID": provider, "modelID": model, "tokens": turn["tokens"], "cost": 0}}]}
        grade = {"passed": False, "checks": 5, "scope": "fake independent failing grade"}
        def execute(command, prompt, stdout, stderr, *args, **kwargs):
            if "--version" in command:
                result = b"1.18.34"
            elif "run" in command:
                result = serialized(rows)
            elif "export" in command:
                result = encode(export)
            else:
                result = encode(grade)
            stdout.write(result)
            return {"exit_code": 0, "stop_reason": None, "sampled_aggregate_rss_bytes": 100}
        output = self.root / "out"
        with patch.object(runner, "bounded_run", side_effect=execute):
            record = runner.run_attempt(self.frozen, cell["id"], FIXTURES, output, self.binary, Path("opencode"))
        self.assertEqual(record["status"], "completed")
        self.assertTrue(record["model_observed_by_harness"])
        self.assertEqual(runner.report(self.frozen, output)["passed"], 0)
        self.assertIsNone(record["actual_usd"])
        folder = output / cell["id"]
        record["grade"]["passed"] = True
        (folder / "record.json").write_bytes(encode(record))
        manifest = load(folder / "manifest.json")
        manifest["record_sha256"] = runner.digest(record)
        manifest["files"]["record.json"] = runner.identity(folder / "record.json")
        (folder / "manifest.json").write_bytes(encode(manifest))
        with self.assertRaisesRegex(ValueError, "grade differs"):
            runner.report(self.frozen, output)

    def test_local_graders_refuse_imports_and_dunder_access(self):
        for task in self.frozen["plan"]["tasks"]:
            submission = self.root / task["id"]
            runner.unpack(task["files"], submission, runner.MAX_WORKSPACE)
            source = next(submission.glob("*.py"))
            source.write_text("import os\nos.environ.clear()\n")
            grade = json.loads(subprocess.check_output([sys.executable, "-I", str(FIXTURES / task["grader"]), str(submission)], timeout=5))
            self.assertFalse(grade["passed"])

    def test_real_private_graders_reject_originals_and_accept_independent_solutions(self):
        replacements = {"interval-boundaries": ("<=", "<"),
                        "stable-deduplication": ('raise NotImplementedError("stable_unique")',
                                                 'result = []\n    for value in values:\n        if value not in result:\n            result.append(value)\n    return result')}
        for task in self.frozen["plan"]["tasks"]:
            submission = self.root / task["id"]
            runner.unpack(task["files"], submission, runner.MAX_WORKSPACE)
            def grade():
                return json.loads(subprocess.check_output([sys.executable, "-I", str(FIXTURES / task["grader"]), str(submission)], timeout=5))
            self.assertFalse(grade()["passed"])
            source = next(submission.glob("*.py"))
            source.write_text(source.read_text().replace(*replacements[task["id"]]))
            self.assertTrue(grade()["passed"])


class RetainedRehearsal(unittest.TestCase):
    def test_preflights_retain_ready_exchange_and_both_protocol_failures(self):
        directory = FIXTURES / "results/2026-10-01/preflights"
        ready = directory / "ready"
        turn = runner.events((ready / "events.jsonl").read_bytes())
        self.assertTrue(runner.model_identity(load(ready / "export.json"), turn["session"], "kimi-code-plan-global/k3", [turn]))
        for name in ("forced-summary", "prose-before-json"):
            folder = directory / name
            manifest = load(folder / "manifest.json")
            self.assertTrue(all(runner.identity(folder / path) == sha for path, sha in manifest["files"].items()))
            record = load(folder / "record.json")
            self.assertEqual(record["status"], "failed")
            self.assertEqual(runner.digest(record), manifest["record_sha256"])
            self.assertEqual(record["plan_sha256"], load(folder / "plan.json")["sha256"])

    def test_all_frozen_attempts_remain_auditable_without_model_calls(self):
        directory = FIXTURES / "results/2026-10-01"
        frozen = load(directory / "plan.json")
        observed = runner.report(frozen, directory / "attempts")
        self.assertEqual(observed, load(directory / "report.json"))
        self.assertEqual(len(observed["attempts"]), 8)
        self.assertEqual(sum(row["status"] == "failed" for row in observed["attempts"]), 3)
        self.assertFalse(observed["audit_complete"])
        self.assertTrue(all(row["actual_usd"] is None and row["fr_requests"] == 0 for row in observed["attempts"]))
        names = ("opencode_rehearsal.py", "bounded_host.py", "source_disclosure.py", "workspace_bundle.py", "study.py")
        self.assertEqual(runner.digest({name: runner.identity(directory / "frozen-runner" / name) for name in names}),
                         frozen["plan"]["implementation_sha256"])

    def test_posthoc_grader_review_replays_without_replacing_original_outcomes(self):
        directory = FIXTURES / "results/2026-10-01"
        review = load(directory / "grader-review.json")
        for row in review["attempts"]:
            folder = directory / "attempts" / row["cell"]
            record = load(folder / "record.json")
            if row["status"] == "not_regraded":
                self.assertEqual(record["status"], "failed")
                continue
            grader = directory / "reviewed-graders" / row["grader"]
            self.assertEqual(runner.identity(grader), row["grader_sha256"])
            self.assertEqual(record["submission_sha256"], row["submission_sha256"])
            with tempfile.TemporaryDirectory() as temporary:
                submission = Path(temporary) / "submission"
                runner.unpack(load(folder / "submission.json"), submission, runner.MAX_WORKSPACE)
                observed = json.loads(subprocess.check_output([sys.executable, "-I", "-B", str(grader), str(submission)], timeout=5))
            self.assertEqual(observed, row["grade"])
            if record["grade"]["checks"] == 0:
                self.assertEqual(observed["checks"], 24)
                self.assertFalse(observed["passed"])


if __name__ == "__main__":
    unittest.main()
