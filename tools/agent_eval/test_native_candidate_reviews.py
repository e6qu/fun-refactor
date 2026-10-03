"""Offline citation and withheld-input checks for the source-based review collection."""
import base64
import copy
import hashlib
import io
import json
from pathlib import Path
import runpy
import sys
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval import native_references as refs, opencode_native as native, native_mcp as mcp
from agent_eval import native_costs as costs
from agent_eval.study import encode, load
from agent_eval.test_opencode_native import fixture, FILES

ROOT = Path(__file__).resolve().parents[2]
HERE = ROOT / "tests/agent-eval/opencode/reviews/2026-10-04-native"
REVIEW = runpy.run_path(str(HERE / "collect.py"))


class ReviewClaims(unittest.TestCase):
    def setUp(self):
        self.raw = base64.b64decode(FILES["module.py"]["data"])
        self.span = {"path": "module.py", "sha256": hashlib.sha256(self.raw).hexdigest(),
                     "start": 0, "end": len(self.raw), "text": self.raw.decode()}
        self.answer = {"findings": [{"gap": "Example hypothetical gap", "wrong_repair": "Return 41",
                        "input": "value()", "expected": "42",
                        "citations": [{"path": "module.py", "quote": self.raw.decode()}]}],
                       "limitations": "This fixture does not establish a real grader omission."}

    def test_cited_claims_remain_unverified(self):
        result = REVIEW["findings"](self.answer, [self.span])
        self.assertFalse(result["claims_verified"])
        self.assertFalse(result["findings"][0]["verified_counterexample"])
        self.assertEqual(result["findings"][0]["citations"], self.answer["findings"][0]["citations"])
        self.answer["findings"][0]["citations"] = [{"source": next(iter(refs.pieces([self.span])))}]
        self.assertEqual(REVIEW["findings"](self.answer, [self.span]), result)

    def test_unread_guessed_and_wrong_path_citations_refuse(self):
        for citations, spans in [
            (self.answer["findings"][0]["citations"], []),
            ([{"source": "src1:" + "0" * 64}], [self.span]),
            ([{"path": "another.py", "quote": self.raw.decode()}], [self.span]),
            ([{"path": "module.py", "quote": "invented source text"}], [self.span]),
            ([{"path": "module.py", "quote": ""}], [self.span]),
        ]:
            answer = copy.deepcopy(self.answer)
            answer["findings"][0]["citations"] = citations
            with self.assertRaises(ValueError):
                REVIEW["findings"](answer, spans)

    def test_contiguous_reads_work_but_an_unread_gap_does_not(self):
        cut = 14
        spans = [{**self.span, "end": cut, "text": self.raw[:cut].decode()},
                 {**self.span, "start": cut, "text": self.raw[cut:].decode()}]
        REVIEW["findings"](self.answer, spans)
        spans[1].update(start=cut + 1, text=self.raw[cut + 1:].decode())
        with self.assertRaises(ValueError):
            REVIEW["findings"](self.answer, spans)

    def test_empty_findings_require_limitations_and_are_not_acceptance(self):
        result = REVIEW["findings"]({"findings": [], "limitations": "No supported finding in inspected code."}, [])
        self.assertFalse(result["claims_verified"])
        for changes in ({"limitations": ""}, {"findings": self.answer["findings"] * 3}, {"accepted": True}):
            with self.assertRaises(ValueError):
                REVIEW["findings"]({**self.answer, **changes}, [self.span])
        for field in ("gap", "wrong_repair", "input", "expected", "citations"):
            answer = copy.deepcopy(self.answer)
            del answer["findings"][0][field]
            with self.assertRaises(ValueError):
                REVIEW["findings"](answer, [self.span])

    def test_source_in_submission_step_cannot_support_a_finding(self):
        for same_step in (False, True):
            events, exported, rows = fixture(same_step)
            rows[-1]["params"]["arguments"]["answer"] = self.answer
            for message in exported["messages"]:
                for part in message["parts"]:
                    if part.get("tool") == "rehearsal_submit_answer":
                        part["state"]["input"]["answer"] = self.answer
            task = {"files": FILES, "requirement": "Review this example."}
            plan = {"tools_schema_version": 3, "prompt": REVIEW["PROMPT"], "tools": {"files": mcp.schemas("files")}}
            exported["messages"].insert(0, {"info": {"role": "user"}, "parts": [{"type": "text",
                "text": REVIEW["PROMPT"] + "\nTask:\n" + task["requirement"]}]})
            result = native.audit(b"\n".join(encode(e) for e in events), exported, rows, task,
                                  {"arm": "files", "model": "provider/model"}, plan)
            if same_step:
                with self.assertRaises(ValueError):
                    REVIEW["findings"](result["answer"], result["disclosed"])
            else:
                REVIEW["findings"](result["answer"], result["disclosed"])

    def test_review_inputs_exclude_controls_and_reference_repairs(self):
        pack = ROOT / "tests/agent-eval/opencode/candidates"
        for task in load(pack / "manifest.json")["tasks"]:
            files = REVIEW["inputs"](task)
            review = {name for name in files if name.startswith("review/")}
            self.assertEqual(review, {"review/requirement.txt", "review/grader.py", "review/cases.json", "review/public-check.py"})
            self.assertFalse(any("controls" in name or "review-baseline" in name for name in files))
            self.assertEqual(base64.b64decode(files["review/requirement.txt"]["data"]).decode(), task["requirement"])


class ReadOnlyCosts(unittest.TestCase):
    def observe(self, raw, rows, version=2, arm="files"):
        return costs.observed(raw, b"".join(encode(r) + b"\n" for r in rows),
                              {"files": FILES}, arm, version, read_only=True)

    def test_finished_steps_and_readonly_submission_are_accounted_separately(self):
        events, _, rows = fixture()
        result = self.observe(b"\n".join(encode(e) for e in events), rows)
        self.assertEqual(result["observed"]["host_calls"], 2)
        self.assertEqual(result["observed"]["native_confirmed_results"], 2)
        self.assertEqual(result["observed"]["accepted_edits"], 0)
        self.assertEqual(result["observed"]["source_page_bytes"], 27)
        self.assertTrue(result["observed"]["submission_observed"])
        self.assertEqual(result["usage"]["finished_steps"], 3)
        self.assertFalse(result["coverage"]["export_and_model_identity_verified"])

    def test_source_produced_after_last_native_event_is_not_confirmed_delivery(self):
        events, _, rows = fixture()
        result = self.observe(encode(events[0]) + b"\n", rows[:1])
        self.assertEqual(result["observed"]["produced_only_results"], 1)
        self.assertEqual(result["observed"]["native_confirmed_source_page_bytes"], 0)
        self.assertEqual(result["observed"]["source_page_bytes"], 27)
        self.assertFalse(result["observed"]["submission_observed"])
        self.assertEqual(result["usage"]["finished_steps"], 0)

    def test_forged_source_and_reference_ids_fail_without_model_execution(self):
        log = io.BytesIO()
        server = mcp.Server({"files": FILES, "arm": "files", "binary": "fr", "workspace": "/unused",
                             "tools_schema_version": 4}, log)
        server.call({"name": "read_source", "arguments": {"path": "module.py", "offset": 0, "bytes": 8192, "sha256": ""}})
        row = json.loads(log.getvalue())
        for field, value in (("text", "def value():\n    return 43\n"), ("source_refs", [])):
            altered = copy.deepcopy(row)
            altered["result"][field] = value
            altered["response"]["content"][0]["text"] = encode(altered["result"]).decode()
            with self.assertRaisesRegex(ValueError, "replay differs"):
                self.observe(b"", [altered], version=4)

    def test_readonly_replay_refuses_edits_and_counts_refusals(self):
        log = io.BytesIO()
        server = mcp.Server({"files": FILES, "arm": "files", "binary": "fr", "workspace": "/unused",
                             "tools_schema_version": 4}, log)
        server.call({"name": "replace_source", "arguments": {"path": "module.py"}})
        row = json.loads(log.getvalue())
        result = self.observe(b"", [row], version=4)
        self.assertEqual(result["observed"]["refused_calls"], 1)
        self.assertEqual(result["observed"]["accepted_edits"], 0)
        self.assertFalse(result["observed"]["submission_observed"])

    def test_fr_source_is_rebuilt_and_overlap_is_shared_with_ordinary_reads(self):
        handle = "frp1:" + "a" * 32 + ":1"
        behavior = {"mode": "behavior", "profile": {"name": "compact", "source_bytes": 2048}, "rows": [],
                    "declaration": {"node": {"handle": handle, "path": "module.py", "span": {"start": 0, "end": 27}},
                    "source": {"offset": 0, "returned_bytes": 27, "span": {"start": 0, "end": 27},
                               "text": "def value():\n    return 42\n"}}}
        log = io.BytesIO()
        server = mcp.Server({"files": FILES, "arm": "fr", "binary": "fr", "workspace": "/unused",
                             "tools_schema_version": 4}, log, lambda *_: encode(behavior))
        server.call({"name": "fr_explore", "arguments": {"term": "value", "mode": "behavior", "target": handle}})
        server.call({"name": "read_source", "arguments": {"path": "module.py", "offset": 0, "bytes": 8192, "sha256": ""}})
        rows = [json.loads(line) for line in log.getvalue().splitlines()]
        result = self.observe(b"", rows, version=4, arm="fr")
        self.assertEqual(result["observed"]["fr_calls"], 1)
        self.assertEqual(result["observed"]["source_page_bytes"], 54)
        self.assertEqual(result["observed"]["repeated_source_page_bytes"], 27)
        self.assertEqual(result["observed"]["native_confirmed_results"], 0)
        rows[0]["result"]["declaration"]["source"]["text"] = "def value():\n    return 43\n"
        rows[0]["response"]["content"][0]["text"] = encode(rows[0]["result"]).decode()
        with self.assertRaises(ValueError):
            self.observe(b"", rows, version=4, arm="fr")

    def test_retained_failures_replay_with_frozen_sources(self):
        module = runpy.run_path(str(HERE / "report.py"))
        collector = module["report"].__globals__["COLLECT"]
        with patch.dict(collector["task_inputs"].__globals__, {"PACK": Path("/absent-candidate-pack")}):
            result = module["report"]()
        self.assertEqual(result, load(HERE / "report.json"))
        self.assertEqual((result["planned"], result["completed"], result["failed"], result["not_started"]), (6, 0, 2, 4))
        attempted = result["attempts"][:2]
        self.assertEqual([r["costs"]["observed"]["host_calls"] for r in attempted], [8, 9])
        self.assertEqual([r["costs"]["usage"]["finished_steps"] for r in attempted], [4, 7])
        for row in attempted:
            self.assertIsNone(row["review"])
            self.assertIsNone(row["actual_usd"])
            self.assertEqual(row["costs"]["observed"]["fr_calls"], 0)
            self.assertFalse(row["costs"]["coverage"]["reported_step_usage_complete"])


if __name__ == "__main__":
    unittest.main()
