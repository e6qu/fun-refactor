"""Offline adversarial controls for terminal native submissions."""
import base64
import copy
import io
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval import native_mcp as mcp, structured_probe as probe, structured_submission as protocol


def fixture():
    request = {"model": {"providerID": "scripted", "modelID": "protocol"}, "agent": "fr-submission",
               "format": copy.deepcopy(protocol.FORMAT), "parts": [{"type": "text", "text": protocol.PROMPT}]}
    session = "ses_control"
    log = io.BytesIO()
    server = mcp.Server({"files": {"module.py": {"data": base64.b64encode(probe.SOURCE).decode(), "executable": False}},
                        "arm": "files", "binary": "unused", "workspace": "/unused", "tools_schema_version": 6}, log)
    params = {"name": "read_source", "arguments": {"path": "module.py", "offset": 0, "bytes": 128, "sha256": ""}}
    response = server.call(params)
    rows = [mcp.decode(line) for line in log.getvalue().splitlines()]
    messages = [{"info": {"id": "msg_user", "sessionID": session, "role": "user", "format": request["format"]},
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
