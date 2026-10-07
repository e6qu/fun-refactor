"""Offline controls for frozen settings, exact edits and failed change captures."""
import base64
import copy
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval import terminal_changes as changes, terminal_change_runner as runner
from agent_eval import native_changes, source_reviews, terminal_reviews
from agent_eval.study import digest, encode
from agent_eval.test_native_changes import FILES, edit
from agent_eval.test_structured_submission import fixture, events_for, capture_fixture


def task():
    return {"id": "value", "requirement": "Return 43.", "public_feedback": "value() returned 42; expected 43.",
            "files": copy.deepcopy(FILES), "grader": {"schema": "fr-stdio-grader-1", "image": "sha256:" + "1" * 64,
            "command": ["python3", "/workspace/module.py"], "limits": {"wall_seconds": 1, "memory_bytes": 67108864,
            "scratch_bytes": 1048576, "output_bytes": 4096, "candidate_bytes": 1048576},
            "cases": [{"id": "value", "stdin": "", "stdout": "43\n", "exit_code": 0}]}}


def frozen():
    model = {"providerID": "scripted", "modelID": "protocol", "configured": True,
             "context": 32768, "output": 2048, "variant": "low"}
    return changes.freeze([task()], [model], Path(__file__), Path(__file__), {}, {"kind": "offline control"})


def capture(root, plan, snapshots):
    capture_fixture(root)
    process = json.loads((root / "process.json").read_bytes())
    (root / "provider.json").unlink()
    request, terminal, messages, _, _ = fixture()
    cell = plan["plan"]["cells"][0]
    request = changes.request(plan["plan"], plan["plan"]["tasks"][0], cell)
    messages[0]["info"].update(model={**request["model"], "variant": "low"})
    messages[0]["parts"][0].update(request["parts"][0])
    log = io.BytesIO()
    server = native_changes.Server({"files": FILES, "arm": "files", "binary": "unused", "tools_schema_version": 2}, log)
    response = server.call(edit())
    tool = messages[1]["parts"][1]
    tool.update(tool="rehearsal_replace_source")
    tool["state"].update(input=edit()["arguments"], output=response["content"][0]["text"])
    answer = {"answer": {"summary": "Changed value; tests not run."}}
    terminal["info"]["structured"] = answer
    terminal["parts"][1]["state"]["input"] = answer
    messages[-1] = terminal
    identity = {"schema": changes.SCHEMA, "plan_sha256": plan["sha256"], "cell": cell,
                "opencode_version": changes.protocol.VERSION, "binary_sha256": plan["plan"]["binary_sha256"],
                "opencode_sha256": plan["plan"]["opencode_sha256"]}
    for name, value in (("request.json", request), ("terminal.json", terminal), ("messages.json", messages),
                        ("export.json", {"info": {"id": "ses_control"}, "messages": messages}), ("identity.json", identity)):
        (root / name).write_bytes(encode(value))
    (root / "events.jsonl").write_bytes(b"".join(encode(e) + b"\n" for e in events_for(messages)))
    (root / "tools.jsonl").write_bytes(log.getvalue())
    server.machine.close()
    return process


class Changes(unittest.TestCase):
    def test_two_failed_captures_block_later_work_and_keep_observed_usage(self):
        first, _ = frozen()
        models = copy.deepcopy(first["plan"]["models"])
        models.append({**models[0], "modelID": "second"})
        plan, snapshots = changes.freeze([task()], models, Path(__file__), Path(__file__), {}, {})
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            for cell in plan["plan"]["cells"][:2]:
                root = output / cell["id"]
                root.mkdir()
                process = capture(root, plan, snapshots)
                process.update(exit_code=1, process_exit_code=1)
                runner.seal(plan, snapshots, cell, root, process)
            result = changes.report(plan, snapshots, output)
            self.assertEqual((result["failed"], result["not_started"]), (2, 2))
            self.assertEqual(result["attempts"][0]["observed"]["assistant_messages"], 2)
            self.assertEqual(result["attempts"][0]["observed"]["host_calls"], 1)
            with patch.object(runner.bounded_host, "run") as execute:
                with self.assertRaisesRegex(ValueError, "consecutive-failure"):
                    runner.collect(plan, snapshots, plan["plan"]["cells"][2]["id"], output,
                                   output, Path(__file__), Path(__file__))
                execute.assert_not_called()

    def test_retry_and_out_of_order_collection_refuse_before_launch(self):
        plan, snapshots = frozen()
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            first, second = plan["plan"]["cells"]
            with patch.object(runner.bounded_host, "run") as execute:
                with self.assertRaises((OSError, ValueError)):
                    runner.collect(plan, snapshots, second["id"], output, output, Path(__file__), Path(__file__))
                self.assertFalse((output / second["id"]).exists())
                root = output / first["id"]
                root.mkdir()
                process = capture(root, plan, snapshots)
                runner.seal(plan, snapshots, first, root, process)
                with self.assertRaisesRegex(ValueError, "already attempted"):
                    runner.collect(plan, snapshots, first["id"], output, output, Path(__file__), Path(__file__))
                execute.assert_not_called()

    def test_frozen_profiles_feedback_tools_and_source_refuse_drift(self):
        plan, snapshots = frozen()
        self.assertEqual(changes.checked(plan, snapshots)["limits"], terminal_reviews.native.LIMITS)
        for arm in changes.ARMS:
            names = {t["name"] for t in plan["plan"]["tools"][arm]}
            self.assertIn("replace_source", names)
            self.assertNotIn("submit_patch", names)
            self.assertNotIn("run_checks", names)
            self.assertEqual("fr_preview_body" in names, arm == "fr")
        for mutate in (lambda p: p["tools"]["files"].pop(), lambda p: p["limits"].update(rss_bytes=2**30),
                       lambda p: p["models"][0].update(output=4096), lambda p: p["cells"].reverse()):
            altered = copy.deepcopy(plan)
            mutate(altered["plan"])
            altered["sha256"] = digest(altered["plan"])
            with self.assertRaises(ValueError):
                changes.checked(altered, snapshots)
        changed = copy.deepcopy(snapshots)
        changed["value"]["module.py"]["data"] = base64.b64encode(b"different").decode()
        with self.assertRaises(ValueError):
            changes.checked(plan, changed)

    def test_complete_capture_replays_exact_submission_and_refuses_forgery(self):
        plan, snapshots = frozen()
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            root = output / plan["plan"]["cells"][0]["id"]
            root.mkdir()
            process = capture(root, plan, snapshots)
            record = runner.seal(plan, snapshots, plan["plan"]["cells"][0], root, process)
            self.assertEqual(record["status"], "completed", record)
            report = changes.report(plan, snapshots, output)
            self.assertEqual((report["completed"], report["not_started"]), (1, 1))
            self.assertEqual(record["audit"]["changed_paths"], ["module.py"])
            self.assertFalse(record["audit"]["behavior_verified"])
            (root / "submission.json").write_bytes(encode(FILES))
            manifest = json.loads((root / "manifest.json").read_bytes())
            manifest["submission.json"] = source_reviews.identity(root / "submission.json")
            (root / "manifest.json").write_bytes(encode(manifest))
            with self.assertRaisesRegex(ValueError, "submission replay"):
                changes.report(plan, snapshots, output)

    def test_resource_failure_retains_evidence_and_blocks_next_cell(self):
        plan, snapshots = frozen()
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            first, second = plan["plan"]["cells"]
            root = output / first["id"]
            root.mkdir()
            process = capture(root, plan, snapshots)
            process.update(stop_reason="rss limit", exit_code=1)
            record = runner.seal(plan, snapshots, first, root, process)
            self.assertEqual(record["status"], "failed")
            self.assertFalse((root / "submission.json").exists())
            self.assertEqual(changes.report(plan, snapshots, output)["failed"], 1)
            with patch.object(runner.bounded_host, "run") as execute:
                with self.assertRaisesRegex(ValueError, "resource stop"):
                    runner.collect(plan, snapshots, second["id"], output, output, Path(__file__), Path(__file__))
                execute.assert_not_called()
            self.assertFalse((output / second["id"]).exists())

    def test_configured_environment_uses_frozen_catalog_and_no_credential_copy(self):
        plan, _ = frozen()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with patch.object(runner.reviews.source_reviews.legacy, "environment", return_value={"PATH": "test"}):
                env = runner.environment(root, plan["plan"]["models"][0], root / "config.json", {"provider": {}})
            settings = json.loads(env["OPENCODE_CONFIG_CONTENT"])
            profile = settings["provider"]["scripted"]["models"]["protocol"]
            self.assertEqual(profile["limit"], {"context": 32768, "output": 2048})
            self.assertEqual(json.loads(Path(env["OPENCODE_MODELS_PATH"]).read_bytes()), {"provider": {}})
            self.assertEqual(env["BUN_OPTIONS"], "")
            self.assertTrue(settings["mcp"]["rehearsal"]["command"][2].endswith("terminal-changes.py"))

    def test_legacy_submission_and_stale_edits_cannot_be_promoted(self):
        machine = native_changes.Machine(FILES, "files", version=2)
        first = machine.call(edit())
        stale = machine.call(edit())
        rows = [{"params": edit(), "result": first}, {"params": edit(), "result": stale}]
        result = changes.replay_edits(FILES, "files", rows, {"summary": "One edit, one stale refusal."})
        self.assertEqual(result["accepted_edits"], 1)
        self.assertEqual(result["metrics"]["refused_calls"], 1)
        with self.assertRaisesRegex(ValueError, "replay differs"):
            changes.replay_edits(FILES, "files", [{"params": {"name": "submit_patch", "arguments": {"summary": "x"}},
                "result": {"submitted": True}}], {"summary": "x"})
        machine.close()


if __name__ == "__main__":
    unittest.main()
