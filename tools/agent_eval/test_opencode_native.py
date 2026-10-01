"""Offline MCP, native transcript and evidence controls; no model service calls."""
import base64
import copy
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval import native_mcp as mcp, opencode_native as native
from agent_eval.study import encode, load

ROOT = Path(__file__).resolve().parents[2]
FILES = {"module.py": {"data": base64.b64encode(b"def value():\n    return 42\n").decode(), "executable": False}}


def config(arm="files"):
    return {"files": copy.deepcopy(FILES), "arm": arm, "binary": "fr", "workspace": "/unused"}


def exchange(method, params=None, identity=1):
    row = {"jsonrpc": "2.0", "method": method}
    if params is not None:
        row["params"] = params
    if identity is not None:
        row["id"] = identity
    return row


def fixture(same_step=False):
    log = io.BytesIO()
    server = mcp.Server(config(), log)
    args = {"path": "module.py", "offset": 0, "bytes": 8192, "sha256": ""}
    calls = [{"name": "read_source", "arguments": args}, {"name": "submit_answer", "arguments": {"answer": {}}}]
    rows, events, messages = [], [], []
    tokens = {"input": 10, "output": 20, "reasoning": 0, "cache": {"read": 1, "write": 0}}
    for i in range(2 if same_step else 3):
        mid = f"msg_{i}"
        parts = []
        events.append({"type": "step_start", "sessionID": "ses_abc", "part": {"messageID": mid}})
        selected = calls if same_step and i == 0 else ([] if same_step else calls[i:i+1])
        for params in selected:
            response = server.call(params)
            part = {"type": "tool", "messageID": mid, "sessionID": "ses_abc", "id": f"prt_{len(rows)}",
                    "callID": f"call_{len(rows)}", "tool": "rehearsal_" + params["name"],
                    "state": {"status": "completed", "input": params["arguments"], "output": response["content"][0]["text"]}}
            rows.append(json.loads(log.getvalue().splitlines()[-1]))
            parts.append(part)
            events.append({"type": "tool_use", "sessionID": "ses_abc", "part": part})
        reason = "tool-calls" if selected else "stop"
        events.append({"type": "step_finish", "sessionID": "ses_abc", "part": {"messageID": mid, "reason": reason, "tokens": tokens, "cost": 0}})
        messages.append({"info": {"id": mid, "role": "assistant", "providerID": "provider", "modelID": "model", "finish": reason,
                                  "tokens": tokens, "cost": 0}, "parts": parts})
    return events, {"info": {"id": "ses_abc"}, "messages": messages}, rows


class Transport(unittest.TestCase):
    def test_real_stdio_process_handshake_and_source(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "config").write_bytes(encode(config()))
            requests = [exchange("initialize", {"protocolVersion": "2025-11-25"}),
                        exchange("notifications/initialized", identity=None), exchange("tools/list", identity=2),
                        exchange("tools/call", {"name": "read_source", "arguments": {"path": "module.py", "offset": 0, "bytes": 30, "sha256": ""}}, 3)]
            process = subprocess.run([sys.executable, "-B", str(ROOT / "tools/native-rehearsal.py"), "serve", str(root / "config"), str(root / "log")],
                                     input=b"".join(encode(row) + b"\n" for row in requests), capture_output=True, timeout=5)
            self.assertEqual(process.returncode, 0, process.stderr)
            responses = [json.loads(row) for row in process.stdout.splitlines()]
            self.assertEqual(responses[0]["result"]["protocolVersion"], mcp.PROTOCOL)
            self.assertEqual(len(responses[1]["result"]["tools"]), 4)
            self.assertIn("return 42", responses[2]["result"]["content"][0]["text"])
            self.assertEqual(len((root / "log").read_bytes().splitlines()), 1)

    def test_no_tools_before_initialization_and_unknown_methods(self):
        server = mcp.Server(config(), io.BytesIO())
        with self.assertRaises(ValueError):
            server.dispatch(exchange("tools/list"))
        self.assertEqual(server.dispatch(exchange("resources/read"))["error"]["code"], -32601)

    def test_argument_and_arm_refusals_are_logged(self):
        log = io.BytesIO()
        server = mcp.Server(config(), log)
        for params in [{"name": "fr_map", "arguments": {}}, {"name": "bash", "arguments": {}},
                       {"name": "list_files", "arguments": {"path": "/etc/passwd"}},
                       {"name": "read_source", "arguments": {"path": "../secret", "offset": 0, "bytes": 1, "sha256": ""}},
                       {"name": "read_source", "arguments": {"path": "module.py", "offset": True, "bytes": 1, "sha256": ""}},
                       {"name": "read_source", "arguments": {"path": "module.py", "offset": 0, "bytes": 8193, "sha256": ""}}]:
            self.assertTrue(server.call(params)["isError"])
        self.assertEqual(len(log.getvalue().splitlines()), 6)

    def test_calls_after_submission_and_budget_cannot_execute(self):
        server = mcp.Server(config(), io.BytesIO())
        self.assertFalse(server.call({"name": "submit_answer", "arguments": {"answer": {}}})["isError"])
        self.assertTrue(server.call({"name": "list_files", "arguments": {}})["isError"])
        server.calls = mcp.MAX_CALLS
        with self.assertRaisesRegex(ValueError, "budget"):
            server.call({"name": "list_files", "arguments": {}})

    def test_oversized_and_duplicate_wire_messages_refuse(self):
        for raw in (b"x" * (mcp.MAX_LINE + 1), b'{"a":1,"a":2}\n', b'{"a":NaN}\n'):
            with self.assertRaises(ValueError):
                mcp.serve(config(), io.BytesIO(), io.BytesIO(raw), io.BytesIO())

    def test_public_fr_argv_and_metadata_only_tools(self):
        calls = []
        server = mcp.Server(config("fr"), io.BytesIO(), lambda *args: calls.append(args) or b'{"rows":[]}')
        result = server.call({"name": "fr_find", "arguments": {"name": "value"}})
        self.assertFalse(result["isError"])
        self.assertEqual(calls[0][0][-6:], ["project", "find", "value", "--signature", "--limit", "12"])
        self.assertTrue(server.call({"name": "fr_find", "arguments": {"name": "--help"}})["isError"])

    def test_native_configuration_denies_unlisted_tools(self):
        env = native.environment(Path("/config"), Path("/log"))
        settings = json.loads(env["OPENCODE_CONFIG_CONTENT"])
        self.assertEqual(settings["permission"], {"*": "deny", "rehearsal_*": "allow"})
        self.assertEqual(settings["agent"][native.AGENT]["steps"], 12)
        self.assertFalse(settings["compaction"]["auto"])
        self.assertEqual(env["OPENCODE_DISABLE_PROJECT_CONFIG"], "1")


class Evidence(unittest.TestCase):
    def test_review_recovers_two_auditor_failures_and_preserves_originals(self):
        root = ROOT / "tests/agent-eval/opencode/results/2026-10-02-native/repositories"
        paths = sorted((root / "attempts").glob("*/record.json"))
        before = [p.read_bytes() for p in paths]
        with patch.object(native, "bounded_run", side_effect=AssertionError("review must stay offline")):
            result = native.review(load(root / "plan.json"), root / "attempts")
        self.assertEqual(result["original_passed"], 2)
        self.assertEqual(result["reviewed_passed"], 4)
        self.assertEqual(before, [p.read_bytes() for p in paths])
        stopped = [row for row in result["attempts"] if row["reviewed_status"] == "failed"]
        self.assertEqual(len(stopped), 1)
        self.assertIn("wall_seconds", stopped[0]["reviewed_failure"])

    def test_tool_part_cannot_claim_another_message(self):
        data = fixture()
        data[1]["messages"][0]["parts"][0]["messageID"] = "msg_1"
        with self.assertRaisesRegex(ValueError, "another message"):
            self.audit(data)

    def test_host_refusal_matches_native_error_without_disclosing_source(self):
        events, exported, rows = fixture()
        row = rows[0]
        row["params"]["arguments"]["sha256"] = "0"
        row["result"] = {"error": "invalid source SHA-256"}
        text = encode(row["result"]).decode()
        row["response"] = {"content": [{"type": "text", "text": text}], "isError": True}
        state = exported["messages"][0]["parts"][0]["state"]
        state["input"]["sha256"] = "0"
        state.update(status="error", error=text)
        del state["output"]
        result = self.audit((events, exported, rows))
        self.assertEqual(result["disclosed"], [])
        state["error"] = "unrelated failure"
        with self.assertRaises(ValueError):
            self.audit((events, exported, rows))

    def test_retained_preflights_replay_without_a_model_or_existing_workspace(self):
        root = ROOT / "tests/agent-eval/opencode/results/2026-10-02-native/preflight"
        result = native.report(load(root / "plan.json"), root / "attempts")
        self.assertEqual(result["passed"], 4)
        self.assertEqual(result["planned"], 4)

    def test_disabled_assertions_cannot_pass_the_rubric(self):
        code = "from agent_eval.opencode_native import grade; from pathlib import Path; grade(Path('.'), {})"
        result = subprocess.run([sys.executable, "-O", "-B", "-c", code], cwd=ROOT / "tools", capture_output=True, timeout=5)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(b"requires Python assertions", result.stderr)

    def test_different_prompt_and_duplicate_stream_call_refuse(self):
        for kind in ("prompt", "duplicate"):
            data = copy.deepcopy(fixture())
            events, exported, rows = data
            user = {"info": {"role": "user"}, "parts": [{"type": "text", "text": native.PROMPT + "\nTask:\ntask"}]}
            exported["messages"].insert(0, user)
            if kind == "prompt":
                user["parts"][0]["text"] += " changed"
            else:
                calls = [e for e in events if e["type"] == "tool_use"]
                calls[1]["part"] = calls[0]["part"]
            with self.assertRaises(ValueError):
                native.audit(b"\n".join(encode(e) for e in events), exported, rows,
                             {"files": FILES, "requirement": "task"}, {"arm": "files", "model": "provider/model"})

    def audit(self, data):
        events, exported, rows = copy.deepcopy(data)
        exported["messages"].insert(0, {"info": {"role": "user"}, "parts": [{"type": "text", "text": native.PROMPT + "\nTask:\ntask"}]})
        return native.audit(b"\n".join(encode(e) for e in events), exported, rows, {"files": FILES, "requirement": "task"}, {"arm": "files", "model": "provider/model"})

    def test_completed_source_and_usage_are_reconstructed(self):
        result = self.audit(fixture())
        self.assertEqual(result["metrics"]["unique_source_bytes"], 27)
        self.assertEqual(len(result["tokens"]), 3)
        self.assertFalse(result["metrics"]["complete_context_accounting"])

    def test_same_response_read_does_not_support_submitted_answer(self):
        result = self.audit(fixture(same_step=True))
        self.assertEqual(result["disclosed"], [])
        self.assertEqual(result["metrics"]["results_not_available_before_answer"], 1)

    def test_missing_stream_export_host_and_wrong_model_refuse(self):
        for mutation in (lambda d: d[0].pop(), lambda d: d[1]["messages"].pop(), lambda d: d[2].pop(),
                         lambda d: d[1]["messages"][0]["info"].update(modelID="wrong"),
                         lambda d: d[0][0].update(sessionID="ses_other"),
                         lambda d: d[2][0]["result"].update(text="fake")):
            data = copy.deepcopy(fixture())
            mutation(data)
            with self.assertRaises(ValueError):
                self.audit(data)

    def test_even_rehashed_invented_source_is_rejected(self):
        data = copy.deepcopy(fixture())
        row = data[2][0]
        row["result"]["text"] = "different source"
        output = encode(row["result"]).decode()
        row["response"]["content"][0]["text"] = output
        for event in data[0]:
            if event["type"] == "tool_use" and event["part"]["tool"] == "rehearsal_read_source":
                event["part"]["state"]["output"] = output
        data[1]["messages"][0]["parts"][0]["state"]["output"] = output
        with self.assertRaisesRegex(ValueError, "replay"):
            self.audit(data)

    def test_frozen_tasks_pending_and_interrupted_are_visible(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            binary = root / "fr"
            binary.write_text("fake binary")
            manifest = ROOT / "tests/agent-eval/opencode/explanations.json"
            frozen = native.freeze(load(manifest), manifest.parent, binary)
            cells = native.checked(frozen)["cells"]
            output = root / "attempts"
            (output / cells[0]["id"]).mkdir(parents=True)
            report = native.report(frozen, output)
            self.assertEqual(report["attempts"][0]["status"], "interrupted")
            self.assertEqual(report["attempts"][1]["status"], "pending")
            self.assertEqual(report["planned"], 12)
            frozen["plan"]["limits"] = {**native.LIMITS, "steps": 99}
            with self.assertRaises(ValueError):
                native.checked(frozen)


if __name__ == "__main__":
    unittest.main()
