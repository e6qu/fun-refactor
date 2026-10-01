"""Offline controls for repository explanation grades and disclosure evidence."""

import base64
import copy
import hashlib
import io
import json
from pathlib import Path
import sys
import tarfile
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval import explanation_grade, opencode_rehearsal as runner, rehearsal_evidence as evidence


def bundle(content="def f():\n    return 12345\n"):
    return {"module.py": {"data": base64.b64encode(content.encode()).decode(), "executable": False}}


class Archives(unittest.TestCase):
    def archive(self, members):
        stream = io.BytesIO()
        with tarfile.open(fileobj=stream, mode="w:gz") as archive:
            for name, raw, kind in members:
                member = tarfile.TarInfo(name)
                member.type, member.size = kind, len(raw)
                archive.addfile(member, io.BytesIO(raw))
        return stream.getvalue()

    def load(self, members, digest=None):
        with tempfile.TemporaryDirectory() as tmp:
            data = self.archive(members)
            path = Path(tmp) / "source.tar.gz"
            path.write_bytes(data)
            return evidence.source_bundle({"source": path.name, "source_format": "tar.gz",
                                           "archive_sha256": digest or hashlib.sha256(data).hexdigest()}, path.parent)

    def test_archive_preserves_complete_regular_files(self):
        actual = self.load([("project/module.py", b"print('hello')\n", tarfile.REGTYPE),
                            ("project/LICENSE", b"license", tarfile.REGTYPE)])
        self.assertEqual(set(actual), {"module.py", "LICENSE"})
        self.assertEqual(base64.b64decode(actual["LICENSE"]["data"]), b"license")

    def test_changed_archive_and_unsafe_members_refuse(self):
        with self.assertRaisesRegex(ValueError, "changed"):
            self.load([("project/a", b"a", tarfile.REGTYPE)], "0"*64)
        for members in ([('/absolute', b"", tarfile.REGTYPE)],
                        [('project/../escape', b"", tarfile.REGTYPE)],
                        [('project/link', b"", tarfile.SYMTYPE)],
                        [('project/a', b"", tarfile.REGTYPE)]*2,
                        [('a/one', b"", tarfile.REGTYPE), ('b/two', b"", tarfile.REGTYPE)]):
            with self.subTest(members=members), self.assertRaises(ValueError):
                self.load(members)

    def test_archive_budgets_refuse_before_export(self):
        with self.assertRaises(ValueError):
            self.load([(f"project/{i}", b"x", tarfile.REGTYPE) for i in range(101)])
        with self.assertRaises(ValueError):
            self.load([("project/large", b"x"*(evidence.LIMIT+1), tarfile.REGTYPE)])

    def test_binary_search_files_are_reported_without_aborting(self):
        files = {**bundle(), "logo.png": {"data": "/w==", "executable": False}}
        result = evidence.search(files, "return")
        self.assertEqual(result["unreadable_files"], 1)
        self.assertEqual(result["matches"][0]["path"], "module.py")

    def test_search_offsets_preserve_unicode_and_crlf(self):
        files = bundle("# é\r\ndef f():\r\n    return 12345\r\n")
        hit = evidence.search(files, "return")["matches"][0]
        self.assertEqual(hit["offset"], len("# é\r\ndef f():\r\n".encode()))
        result = runner.read_source(files, {"path": hit["path"], "offset": hit["offset"],
                                           "bytes": 100, "sha256": hit["sha256"]}, 4096)
        self.assertEqual(result["text"], "    return 12345\r\n")

    def test_archive_expansion_is_bounded_before_tar_metadata(self):
        import gzip
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "source.tar.gz"
            data = gzip.compress(b"x"*(2*evidence.LIMIT+1))
            path.write_bytes(data)
            with self.assertRaisesRegex(ValueError, "expanded archive"):
                evidence.source_bundle({"source": path.name, "source_format": "tar.gz",
                                        "archive_sha256": hashlib.sha256(data).hexdigest()}, path.parent)


class ExplanationGrades(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.text = "def f():\n    return 12345\n"
        (self.root / "module.py").write_text(self.text)
        self.criteria = {"result": {"value": 12345, "evidence": [[{"path": "module.py", "contains": self.text}]]}}
        self.payload = {"criteria": self.criteria,
                        "answer": {"result": {"value": 12345, "citations": [{"path": "module.py", "quote": self.text}]}},
                        "disclosed": [{"path": "module.py", "sha256": hashlib.sha256(self.text.encode()).hexdigest(),
                                       "text": self.text, "start": 0, "end": len(self.text), "via": "read"}]}

    def test_correct_fact_with_observed_source_passes(self):
        self.assertEqual(evidence.criteria(self.criteria, bundle()), self.criteria)
        result = explanation_grade.grade(self.root, self.payload)
        self.assertTrue(result["passed"])
        self.assertEqual(result["checks"], 2)

    def test_correct_guess_without_disclosure_fails(self):
        self.payload["disclosed"] = []
        self.assertFalse(explanation_grade.grade(self.root, self.payload)["passed"])

    def test_incorrect_fact_cannot_be_rescued_by_valid_source(self):
        for value in (12346, True, "12345"):
            self.payload["answer"]["result"]["value"] = value
            self.assertFalse(explanation_grade.grade(self.root, self.payload)["passed"])

    def test_unrelated_quote_and_changed_source_fail(self):
        self.payload["criteria"]["result"]["evidence"][0][0]["contains"] = "not in the quoted source"
        self.assertFalse(explanation_grade.grade(self.root, self.payload)["passed"])
        self.payload["criteria"]["result"]["evidence"][0][0]["contains"] = self.text
        (self.root / "module.py").write_text(self.text + "# changed\n")
        self.assertFalse(explanation_grade.grade(self.root, self.payload)["passed"])

    def test_empty_rubric_unsupported_claim_and_unsafe_reference_fail(self):
        for field, value in (("criteria", {}), ("answer", {})):
            payload = {**self.payload, field: value}
            self.assertFalse(explanation_grade.grade(self.root, payload)["passed"])
        self.payload["answer"]["extra"] = {"value": "proved", "citations": []}
        self.assertFalse(explanation_grade.grade(self.root, self.payload)["passed"])
        del self.payload["answer"]["extra"]
        self.payload["answer"]["result"]["citations"][0]["path"] = "../outside.py"
        self.assertFalse(explanation_grade.grade(self.root, self.payload)["passed"])

    def test_missing_source_anchors_refuse_at_freeze(self):
        self.criteria["result"]["evidence"][0][0]["contains"] = "this source does not exist"
        with self.assertRaises(ValueError):
            evidence.criteria(self.criteria, bundle())


class Disclosure(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.files = bundle()
        self.source = base64.b64decode(self.files["module.py"]["data"])
        self.task = {"files": self.files, "kind": "explain", "requirement": "Explain f."}
        self.plan = {"instructions": runner.INSTRUCTIONS, "fr_instructions": runner.FR_INSTRUCTIONS}
        self.record = {"cell": {"arm": "fr"}, "status": "completed", "turns": [], "trace": [],
                       "submission_sha256": runner.digest(self.files)}
        self.prompt = runner.initial_prompt(self.plan, self.task, self.record["cell"])

    def add(self, request, result):
        self.record["trace"].extend([{"prompt": self.prompt}, {"action": request, "result": result}])
        self.record["turns"].append({"action": request})
        self.prompt = "Action result:\n" + runner.encode(result).decode() + "\nReturn the next JSON action."

    def finish(self):
        self.record["trace"].append({"prompt": self.prompt})
        self.record["turns"].append({"action": {"action": "finish", "answer": {}}})

    def audit(self):
        return runner.audit_disclosure(self.plan, self.task, self.record, self.root)

    def read(self):
        request = {"action": "read", "path": "module.py", "offset": 0, "bytes": 4096, "sha256": ""}
        result = runner.action(self.files, request, "fr", Path("fr"), self.root, None)
        self.add(request, result)

    def test_overlap_across_read_search_and_fr_is_counted_once(self):
        self.read()
        request = {"action": "search", "text": "return"}
        self.add(request, evidence.search(self.files, "return"))
        handle = "frp1:" + "a"*32 + ":1"
        raw = {"node": {"path": "module.py", "handle": handle, "span": {"start": 0, "end": len(self.source)}},
               "source": {"offset": 0, "span": {"start": 0, "end": len(self.source)},
                          "returned_bytes": len(self.source), "text": self.source.decode()}}
        (self.root / "000-fr.stdout").write_bytes(runner.encode(raw))
        self.add({"action": "fr", "operation": "show", "handle": handle}, raw)
        self.finish()
        counts, spans = self.audit()
        line_bytes = len("    return 12345")
        self.assertEqual(counts["unique_source_bytes"], len(self.source))
        self.assertEqual(counts["repeated_source_bytes"], len(self.source) + line_bytes)
        self.assertEqual(counts["exact_source_bytes"], len(self.source)*2 + line_bytes)
        self.assertEqual(counts["fr_source_bytes"], len(self.source))
        self.assertEqual(len(spans), 3)
        self.assertFalse(counts["complete_context_accounting"])

    def test_tampered_read_and_model_request_refuse(self):
        self.read()
        self.finish()
        original = copy.deepcopy(self.record)
        self.record["trace"][1]["result"]["text"] = "invented source"
        with self.assertRaisesRegex(ValueError, "tool result differs"):
            self.audit()
        self.record = original
        self.record["turns"][0]["action"] = {"action": "list"}
        with self.assertRaisesRegex(ValueError, "trace action differs"):
            self.audit()

    def test_changed_prompt_and_unaccounted_rows_refuse(self):
        self.read()
        self.finish()
        self.record["trace"][0]["prompt"] += " secret grader answer"
        with self.assertRaisesRegex(ValueError, "initial instructions differ"):
            self.audit()

    def test_failed_response_retains_delivered_source(self):
        self.read()
        self.record["status"] = "failed"
        self.record["trace"].append({"prompt": self.prompt})
        counts, _ = self.audit()
        self.assertEqual(counts["exact_source_bytes"], len(self.source))
        self.assertEqual(counts["tool_calls"], 1)

    def test_unfinished_tool_is_not_successful_zero_disclosure(self):
        self.record["status"] = "failed"
        self.record["trace"] = [{"prompt": self.prompt}]
        self.record["turns"] = [{"action": {"action": "fr", "operation": "map"}}]
        counts, _ = self.audit()
        self.assertEqual(counts["unfinished_calls"], 1)

    def test_explanation_edits_refuse_and_preserve_source(self):
        with self.assertRaisesRegex(ValueError, "read-only"):
            runner.action(self.files, {"action": "replace", "path": "module.py", "old": "12345", "new": "0"},
                          "fr", Path("fr"), self.root, None, read_only=True)
        self.assertEqual(self.files, bundle())

    def test_fr_continuations_and_file_scope_reach_public_cli(self):
        calls = []
        execute = lambda *args: calls.append(args[0]) or b"{}"
        runner.action(self.files, {"action": "fr", "operation": "find", "name": "f", "path": "module.py",
                                  "contains": True, "cursor": "opaque-cursor"}, "fr", Path("fr"), self.root, execute)
        self.assertEqual(calls[0][-5:], ["--contains", "--in", "module.py", "--cursor", "opaque-cursor"])
        runner.action(self.files, {"action": "fr", "operation": "show", "handle": "frp1:"+"a"*32+":1", "offset": 4096},
                      "fr", Path("fr"), self.root, execute)
        self.assertEqual(calls[1][-2:], ["--offset", "4096"])


class RepositoryRubrics(unittest.TestCase):
    def test_frozen_rubrics_accept_controls_and_reject_each_wrong_fact(self):
        base = Path(__file__).resolve().parents[2] / "tests/agent-eval/opencode"
        manifest = json.loads((base / "explanations.json").read_text())
        for task in manifest["tasks"]:
            with self.subTest(task=task["id"]), tempfile.TemporaryDirectory() as tmp:
                files = evidence.source_bundle(task, base)
                criteria = evidence.criteria(json.loads((base / task["criteria"]).read_text()), files)
                source = Path(tmp) / "source"
                runner.unpack(files, source, runner.MAX_WORKSPACE)
                answer, spans = {}, []
                for name, claim in criteria.items():
                    citations = []
                    for group in claim["evidence"]:
                        anchor = group[0]
                        path, quote = anchor["path"], anchor["contains"]
                        citations.append({"path": path, "quote": quote})
                        raw = base64.b64decode(files[path]["data"])
                        spans.append({"path": path, "text": quote, "sha256": hashlib.sha256(raw).hexdigest()})
                    answer[name] = {"value": claim["value"], "citations": citations}
                payload = {"answer": answer, "criteria": criteria, "disclosed": spans}
                self.assertTrue(explanation_grade.grade(source, payload)["passed"])
                for name in criteria:
                    wrong = copy.deepcopy(payload)
                    value = wrong["answer"][name]["value"]
                    wrong["answer"][name]["value"] = not value if type(value) is bool else "wrong"
                    self.assertFalse(explanation_grade.grade(source, wrong)["passed"])


if __name__ == "__main__":
    unittest.main()
