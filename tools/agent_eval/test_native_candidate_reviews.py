"""Offline citation and withheld-input checks for the source-based review collection."""
import base64
import copy
import hashlib
from pathlib import Path
import runpy
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval import native_references as refs, opencode_native as native, native_mcp as mcp
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


if __name__ == "__main__":
    unittest.main()
