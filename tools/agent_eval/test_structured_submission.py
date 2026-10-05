"""Offline adversarial controls for terminal native submissions."""
import base64
import copy
import io
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval import native_mcp as mcp, structured_probe as probe, structured_submission as protocol
from agent_eval.study import encode


def capture_fixture(root):
    request, terminal, messages, events, rows = fixture()
    tools = [{"type": "function", "function": {"name": "rehearsal_" + t["name"],
              "parameters": t["inputSchema"]}} for t in mcp.schemas("files") if t["name"] != "submit_answer"]
    tools.append({"type": "function", "function": {"name": "StructuredOutput", "parameters": protocol.FORMAT["schema"]}})
    provider = [{"model": "protocol", "tool_choice": "required", "tools": tools, "messages": []},
                {"model": "protocol", "tool_choice": "required", "tools": tools, "messages": [
                 {"role": "tool", "tool_call_id": "call_1_0", "content": rows[0]["response"]["content"][0]["text"]}]}]
    process = {"exit_code": 0, "process_exit_code": 0, "stop_reason": None, "timed_out": False,
               "launch_error": None, "monitor_error": None, "limits": {"wall_seconds": 120, "cpu_seconds": 20,
               "rss_bytes": 768 * 1024**2, "disk_bytes": 16 * 1024**2, "transcript_bytes": 1024**2},
               "elapsed_seconds": 1, "sampled_cpu_seconds": 1, "sampled_aggregate_rss_bytes": 100,
               "sampled_disk_growth_bytes": 100, "transcript_bytes": 100}
    data = {"request.json": request, "terminal.json": terminal, "messages.json": messages,
            "export.json": {"info": {"id": "ses_control"}, "messages": messages}, "provider.json": provider,
            "process.json": process, "identity.json": {"schema": protocol.SCHEMA,
            "opencode_version": protocol.VERSION, "case": "one-answer", "provider_requests": 2}}
    for name, value in data.items():
        (root / name).write_bytes(encode(value))
    for name, values in (("events.jsonl", events), ("tools.jsonl", rows)):
        (root / name).write_bytes(b"".join(encode(row) + b"\n" for row in values))


class Replay(unittest.TestCase):
    def test_missing_answer_keeps_both_responses_in_observed_work(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            capture_fixture(root)
            messages = mcp.decode((root / "messages.json").read_bytes())
            final = messages[-1]
            final["info"].pop("structured")
            final["info"]["error"] = {"name": "StructuredOutputError"}
            final["parts"].pop(1)
            for name, value in (("messages.json", messages), ("terminal.json", final),
                                ("export.json", {"info": {"id": "ses_control"}, "messages": messages})):
                (root / name).write_bytes(encode(value))
            (root / "events.jsonl").write_bytes(b"".join(encode(e) + b"\n" for e in events_for(messages)))
            identity = mcp.decode((root / "identity.json").read_bytes())
            identity["case"] = "missing-answer"
            (root / "identity.json").write_bytes(encode(identity))
            result = probe.review(root, "missing-answer")
            self.assertFalse(result["accepted"])
            self.assertEqual(result["observed_work"]["assistant_responses"], 2)
            self.assertEqual(result["observed_work"]["provider_requests"], 2)
            self.assertEqual(len(result["observed_work"]["tokens"]), 2)
            self.assertEqual(result["observed_work"]["tool_calls"], 1)

    def test_capture_replays_without_starting_a_client(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            capture_fixture(root)
            result = probe.review(root, "one-answer")
            self.assertTrue(result["accepted"])
            self.assertEqual(len(result["evidence_sha256"]), 9)

    def test_independent_export_provider_and_resource_changes_refuse(self):
        mutations = [("export.json", lambda d: d["messages"].pop()),
                     ("provider.json", lambda d: d.append(d[0])),
                     ("provider.json", lambda d: d[1]["messages"][0].update(content="invented")),
                     ("provider.json", lambda d: d[0]["tools"][-1]["function"].update(parameters={})),
                     ("process.json", lambda d: d["limits"].update(rss_bytes=2**30)),
                     ("process.json", lambda d: d.update(sampled_cpu_seconds=21)),
                     ("process.json", lambda d: d.update(timed_out=True)),
                     ("identity.json", lambda d: d.update(opencode_version="different"))]
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name, mutate in mutations:
                capture_fixture(root)
                path = root / name
                data = mcp.decode(path.read_bytes())
                mutate(data)
                path.write_bytes(encode(data))
                with self.subTest(name=name), self.assertRaises(ValueError):
                    probe.review(root, "one-answer")

    def test_oversized_and_symlinked_artifacts_refuse(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            capture_fixture(root)
            path = root / "terminal.json"
            path.write_bytes(b"x" * (probe.MAX_BYTES + 1))
            with self.assertRaisesRegex(ValueError, "oversized"):
                probe.review(root, "one-answer")
            path.unlink()
            path.symlink_to(root / "messages.json")
            with self.assertRaisesRegex(ValueError, "missing or oversized"):
                probe.review(root, "one-answer")


def fixture():
    request = {"model": {"providerID": "scripted", "modelID": "protocol"}, "agent": "fr-submission",
               "format": copy.deepcopy(protocol.FORMAT), "parts": [{"type": "text", "text": protocol.PROMPT}]}
    session = "ses_control"
    log = io.BytesIO()
    server = mcp.Server({"files": {"module.py": {"data": base64.b64encode(probe.SOURCE).decode(), "executable": False}},
                        "arm": "files", "binary": "unused", "workspace": "/unused", "tools_schema_version": 6}, log)
    params = {"name": "read_source", "_meta": {"progressToken": 2}, "arguments": {"path": "module.py", "offset": 0, "bytes": 128, "sha256": ""}}
    response = server.call(params)
    rows = [mcp.decode(line) for line in log.getvalue().splitlines()]
    messages = [{"info": {"id": "msg_user", "sessionID": session, "role": "user", "format": request["format"],
                         "model": copy.deepcopy(request["model"]), "agent": request["agent"]},
                 "parts": [{"id": "prt_user", "sessionID": session, "messageID": "msg_user", **request["parts"][0]}]}]
    tokens = {"input": 100, "output": 10, "reasoning": 0, "cache": {"read": 0, "write": 0}}
    for index in (1, 2):
        mid = f"msg_{index}"
        info = {"id": mid, "sessionID": session, "role": "assistant", "parentID": "msg_user",
                "providerID": "scripted", "modelID": "protocol", "finish": "tool-calls",
                "tokens": copy.deepcopy(tokens), "cost": 0, "time": {"created": index, "completed": index + 1}}
        if index == 2:
            info["structured"] = copy.deepcopy(probe.ANSWER)
        part = {"id": f"prt_{index}_tool", "sessionID": session, "messageID": mid,
                "callID": f"call_{index}", "type": "tool",
                "tool": "StructuredOutput" if index == 2 else "rehearsal_read_source",
                "state": {"status": "completed", "input": copy.deepcopy(probe.ANSWER) if index == 2 else params["arguments"],
                          "output": "Structured output captured successfully." if index == 2 else response["content"][0]["text"]}}
        parts = [{"id": f"prt_{index}_start", "sessionID": session, "messageID": mid, "type": "step-start"}, part,
                 {"id": f"prt_{index}_finish", "sessionID": session, "messageID": mid, "type": "step-finish",
                  "reason": "tool-calls", "tokens": copy.deepcopy(tokens), "cost": 0}]
        messages.append({"info": info, "parts": parts})
    return request, copy.deepcopy(messages[-1]), messages, events_for(messages), rows


def events_for(messages):
    events = []
    for message in messages:
        events.append({"type": "message.updated", "properties": {"info": copy.deepcopy(message["info"])}})
        events.extend({"type": "message.part.updated", "properties": {"part": copy.deepcopy(part)}} for part in message["parts"])
    events.append({"type": "session.status", "properties": {"sessionID": "ses_control", "status": {"type": "idle"}}})
    return events


class Submission(unittest.TestCase):
    def test_model_identity_uses_separate_provider_and_model_fields(self):
        request, _, messages, _, rows = fixture()
        request["model"] = {"providerID": "vendor", "modelID": "family/model"}
        messages[0]["info"]["model"] = copy.deepcopy(request["model"])
        for message in messages[1:]:
            message["info"].update(request["model"])
        protocol.audit(request, copy.deepcopy(messages[-1]), messages, events_for(messages), rows)
        messages[1]["info"].update(providerID="vendor/family", modelID="model")
        with self.assertRaisesRegex(ValueError, "model differs"):
            protocol.audit(request, copy.deepcopy(messages[-1]), messages, events_for(messages), rows)

    def test_one_terminal_answer_accounts_for_both_responses(self):
        result = protocol.audit(*fixture())
        self.assertEqual(result["answer"], probe.ANSWER["answer"])
        self.assertEqual(result["assistant_responses"], 2)
        self.assertEqual(result["tool_calls"], 2)
        self.assertEqual(len(result["tokens"]), 2)
        self.assertFalse(result["provider_usage_verified"])

    def refuse(self, mutate, *, synchronize=False):
        request, terminal, messages, events, rows = fixture()
        mutate(request, terminal, messages, events, rows)
        if synchronize:
            terminal = copy.deepcopy(messages[-1])
            events = events_for(messages)
        with self.assertRaises((ValueError, KeyError, TypeError)):
            protocol.audit(request, terminal, messages, events, rows)

    def test_missing_terminal_answer_and_duplicate_submissions_refuse(self):
        def missing(q, t, m, e, r):
            m[-1]["parts"].pop(1)
            del m[-1]["info"]["structured"]
        def duplicate(q, t, m, e, r):
            extra = copy.deepcopy(m[-1]["parts"][1])
            extra.update(id="prt_extra", callID="call_extra")
            m[-1]["parts"].insert(2, extra)
        for mutate in (missing, duplicate):
            self.refuse(mutate, synchronize=True)

    def test_other_tools_in_the_submission_response_refuse(self):
        def mutate(q, t, m, e, r):
            part = m[1]["parts"].pop(1)
            part["messageID"] = m[-1]["info"]["id"]
            m[-1]["parts"].insert(1, part)
        self.refuse(mutate, synchronize=True)

    def test_continuation_after_submission_refuses(self):
        def mutate(q, t, m, e, r):
            extra = copy.deepcopy(m[-1])
            extra["info"].update(id="msg_extra", finish="stop")
            del extra["info"]["structured"]
            extra["parts"].pop(1)
            for p in extra["parts"]:
                p.update(id=p["id"] + "x", messageID="msg_extra")
                if p["type"] == "step-finish":
                    p["reason"] = "stop"
            m.append(extra)
        self.refuse(mutate, synchronize=True)

    def test_answer_request_and_event_mismatches_refuse(self):
        mutations = [lambda q, t, m, e, r: t["info"]["structured"].update(answer={}),
                     lambda q, t, m, e, r: q["format"].update(retryCount=1),
                     lambda q, t, m, e, r: q["model"].update(modelID="different"),
                     lambda q, t, m, e, r: e.pop(),
                     lambda q, t, m, e, r: e[0]["properties"]["info"].update(sessionID="ses_other"),
                     lambda q, t, m, e, r: e.pop(2),
                     lambda q, t, m, e, r: r.clear(),
                     lambda q, t, m, e, r: r[0]["result"].update(text="invented")]
        for mutate in mutations:
            self.refuse(mutate)

    def test_incomplete_usage_model_and_identity_refuse_even_with_matching_events(self):
        mutations = [lambda q, t, m, e, r: m[-1]["info"].update(modelID="other"),
                     lambda q, t, m, e, r: m[-1]["info"].update(error={"name": "Aborted"}),
                     lambda q, t, m, e, r: m[-1]["info"]["time"].pop("completed"),
                     lambda q, t, m, e, r: m[-1]["info"]["tokens"].update(input=-1),
                     lambda q, t, m, e, r: m[-1]["parts"][-1]["tokens"].update(output=99),
                     lambda q, t, m, e, r: m[-1]["parts"][1].update(callID="call_1"),
                     lambda q, t, m, e, r: m[-1]["info"]["structured"].update(answer={})]
        for mutate in mutations:
            self.refuse(mutate, synchronize=True)

    def test_scripted_provider_refuses_extra_calls_and_has_no_live_credentials(self):
        with self.assertRaisesRegex(ValueError, "extra provider request"):
            probe.response(3, "one-answer")
        env = probe.environment(Path("/isolated"), "http://127.0.0.1:1234/v1")
        settings = mcp.decode(env["OPENCODE_CONFIG_CONTENT"])
        self.assertEqual(settings["enabled_providers"], ["scripted"])
        self.assertEqual(settings["permission"]["StructuredOutput"], "allow")
        self.assertEqual(env["XDG_DATA_HOME"], "/isolated/data")
        self.assertNotIn("OPENAI_API_KEY", env)


if __name__ == "__main__":
    unittest.main()
