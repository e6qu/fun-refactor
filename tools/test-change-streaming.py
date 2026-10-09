#!/usr/bin/env python3
"""Offline invariants for the hosted streaming CPU experiment."""
import copy
import importlib.util
import json
from pathlib import Path
import unittest

from agent_eval import scripted_stream

spec = importlib.util.spec_from_file_location("streaming", Path(__file__).with_name("check-change-streaming.py"))
streaming = importlib.util.module_from_spec(spec)
spec.loader.exec_module(streaming)


def assemble(frames):
    text, calls, roles, finishes, usage = "", {}, [], [], []
    for frame in frames[:-1]:
        value = json.loads(frame.removeprefix(b"data: "))
        choice = value["choices"][0]
        delta = choice["delta"]
        text += delta.get("content", "")
        if "role" in delta:
            roles.append(delta["role"])
        for call in delta.get("tool_calls", []):
            retained = calls.setdefault(call["index"], {"function": {"arguments": ""}})
            for key in ("id", "type"):
                if key in call:
                    retained[key] = call[key]
            for key, part in call["function"].items():
                if key == "arguments":
                    retained["function"][key] += part
                else:
                    retained["function"][key] = part
        if choice["finish_reason"] is not None:
            finishes.append(choice["finish_reason"])
        if "usage" in value:
            usage.append(value["usage"])
    assert frames[-1] == b"data: [DONE]\n\n"
    return {"message": {"role": roles[0], "content": text, "tool_calls": list(calls.values())},
            "roles": roles, "finishes": finishes, "usage": usage}


class StreamingTests(unittest.TestCase):
    def test_fragmentation_preserves_text_calls_and_usage(self):
        reply = streaming.ORIGINAL_RESPONSE(1, "one-answer")
        reply["choices"][0]["message"]["content"] = "α/日本語/\\n" * 50
        extra = copy.deepcopy(reply["choices"][0]["message"]["tool_calls"][0])
        extra["id"] = "second"
        extra["function"]["arguments"] = json.dumps({"text": "quote\" and newline\n and 日本語"}, ensure_ascii=False)
        reply["choices"][0]["message"]["tool_calls"].append(extra)
        before = copy.deepcopy(reply)
        for mode in scripted_stream.MODES:
            rebuilt = assemble(scripted_stream.frames(reply, mode))
            self.assertEqual(rebuilt, {"message": reply["choices"][0]["message"], "roles": ["assistant"],
                "finishes": ["tool_calls"], "usage": [reply["usage"]]})
        self.assertEqual(reply, before)

    def test_only_workspace_references_are_normalized(self):
        reply = streaming.ORIGINAL_RESPONSE(1, "one-answer")
        other = copy.deepcopy(reply)
        call = other["choices"][0]["message"]["tool_calls"][0]
        call["function"]["arguments"] = json.dumps({"handle": "first", "body": "return 1"})
        first = streaming.reply_identity(other)
        call["function"]["arguments"] = json.dumps({"handle": "second", "body": "return 1"})
        self.assertEqual(first, streaming.reply_identity(other))
        call["function"]["arguments"] = json.dumps({"handle": "second", "body": "return 2"})
        self.assertNotEqual(first, streaming.reply_identity(other))

    def test_failure_and_unstarted_work_cannot_admit_or_compare(self):
        failed = {"status": "failed", "mode": "whole", "process": {"stop_reason": "cpu_seconds"}}
        remaining = [{"status": "not_started", "mode": mode, "reason": "earlier capture failed"}
                     for mode in streaming.ORDER[1:]]
        report = streaming.compare([failed, *remaining])
        self.assertFalse(report["admitted"])
        self.assertFalse(report["complete_comparison"])
        self.assertIsNone(report["chunked_minus_whole_mean_cpu_seconds"])
        self.assertEqual(report["attempts"], [failed, *remaining])

    def test_comparison_refuses_different_repairs(self):
        rows = [{"status": "completed", "mode": mode, "submission_sha256": "same", "reply_identities": ["reply"],
                 "runtime": {}, "binary_sha256": "fr", "opencode_sha256": "client", "admitted": True,
                 "process": {"sampled_cpu_seconds": 5}, "text_delta_events": 20 if mode == "chunked" else 2}
                for mode in streaming.ORDER]
        self.assertTrue(streaming.compare(rows)["admitted"])
        rows[-1]["submission_sha256"] = "different"
        with self.assertRaisesRegex(ValueError, "submission_sha256"):
            streaming.compare(rows)


if __name__ == "__main__":
    unittest.main()
