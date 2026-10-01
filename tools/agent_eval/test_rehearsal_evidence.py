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
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval import explanation_grade, opencode_rehearsal as runner, rehearsal_evidence as evidence
from agent_eval.test_opencode_rehearsal import stream, serialized


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

    def test_changed_file_version_is_not_counted_as_repeated_source(self):
        self.task["kind"] = "fix"
        self.task["files"] = copy.deepcopy(self.files)
        self.read()
        request = {"action": "replace", "path": "module.py", "old": "12345", "new": "54321"}
        result = runner.action(self.files, request, "fr", Path("fr"), self.root, None)
        self.add(request, result)
        self.read()
        self.record["submission_sha256"] = runner.digest(self.files)
        self.finish()
        counts, _ = self.audit()
        self.assertEqual(counts["unique_source_bytes"], 2*len(self.source))
        self.assertEqual(counts["repeated_source_bytes"], 0)

    def test_forged_fr_extents_and_source_refuse(self):
        handle = "frp1:" + "a"*32 + ":1"
        request = {"action": "fr", "operation": "show", "handle": handle}
        valid = {"node": {"handle": handle, "path": "module.py", "span": {"start": 0, "end": len(self.source)}},
                 "source": {"offset": 0, "span": {"start": 0, "end": len(self.source)},
                            "returned_bytes": len(self.source), "text": self.source.decode()}}
        for key, value in (("offset", 1), ("returned_bytes", 0), ("text", "invented source")):
            wrong = copy.deepcopy(valid)
            wrong["source"][key] = value
            with self.subTest(key=key), self.assertRaises(ValueError):
                evidence.disclosed(self.files, request, wrong)

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

    def test_result_after_last_allowed_turn_is_not_disclosed_to_agent(self):
        self.read()
        self.record["status"] = "failed"
        counts, spans = self.audit()
        self.assertEqual(counts["exact_source_bytes"], 0)
        self.assertEqual(counts["delivered_tool_result_bytes"], 0)
        self.assertEqual(counts["undelivered_results"], 1)
        self.assertGreater(counts["tool_result_bytes"], 0)
        self.assertEqual(spans, [])

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
    def test_tenacity_normalization_changes_only_the_documentation_link(self):
        base = Path(__file__).resolve().parents[2] / "tests/agent-eval/opencode/repositories"
        provenance = json.loads((base / "provenance.json").read_text())[-1]
        self.assertEqual(runner.identity(base / "tenacity.tar.gz"), provenance["archive_sha256"])
        self.assertEqual(runner.identity(base / "tenacity-regular.tar.gz"), provenance["regular_archive_sha256"])
        with tarfile.open(base / "tenacity.tar.gz") as original, tarfile.open(base / "tenacity-regular.tar.gz") as normalized:
            before, after = original.getmembers(), normalized.getmembers()
            self.assertEqual([m.name for m in before], [m.name for m in after])
            links = []
            for old, new in zip(before, after):
                if old.isfile():
                    self.assertEqual(original.extractfile(old).read(), normalized.extractfile(new).read())
                    self.assertEqual(old.mode, new.mode)
                elif old.issym():
                    links.append(old.name.split("/", 1)[1])
                    raw = normalized.extractfile(new).read()
                    transformation = provenance["transformations"][0]
                    self.assertEqual(raw, original.extractfile(transformation["target"]).read())
                    self.assertEqual(hashlib.sha256(raw).hexdigest(), transformation["sha256"])
            self.assertEqual(links, ["README.rst"])

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


class RetainedPipeline(unittest.TestCase):
    def test_review_preserves_outcomes_and_binds_both_reports(self):
        base = Path(__file__).resolve().parents[2] / "tests/agent-eval/opencode/results/2026-10-01-explanations"
        review = runner.load(base / "audit-review.json")
        for name, field in (("original-report.json", "original_report_sha256"),
                            ("report.json", "corrected_report_sha256"),
                            ("reviewed-disclosure.py", "reviewed_disclosure_sha256")):
            self.assertEqual(runner.identity(base / name), review[field])
        original, current = runner.load(base / "original-report.json"), runner.load(base / "report.json")
        for before, after in zip(original["attempts"], current["attempts"]):
            self.assertEqual({k: v for k, v in before.items() if k != "disclosure"},
                             {k: v for k, v in after.items() if k != "disclosure"})
            self.assertLessEqual(after["disclosure"]["exact_source_bytes"], before["disclosure"]["exact_source_bytes"])
        self.assertEqual(len(review["changes"]), 3)

    def test_recovered_export_does_not_replace_the_failed_attempt(self):
        base = Path(__file__).resolve().parents[2] / "tests/agent-eval/opencode/results/2026-10-01-explanations"
        diagnostic = runner.load(base / "export-diagnostic.json")
        folder = base / "attempts" / diagnostic["cell"]
        record = runner.load(folder / "record.json")
        self.assertEqual(record["status"], "failed")
        original = next(folder.glob("*-export.stdout"))
        self.assertEqual(runner.identity(original), diagnostic["original_export_sha256"])
        self.assertEqual(original.stat().st_size, 65536)
        self.assertEqual(runner.identity(base / "export-recovered.json"), diagnostic["recovered_export_sha256"])
        self.assertEqual(runner.identity(base / "reviewed-export.py"), diagnostic["helper_sha256"])
        self.assertTrue((base / "export-recovered.json").read_bytes().startswith(original.read_bytes()))
        self.assertTrue(runner.model_identity(runner.load(base / "export-recovered.json"), record["turns"][0]["session"],
                                             record["cell"]["model"], record["turns"]))
        self.assertEqual(diagnostic["process"]["exit_code"], 0)
        self.assertNotEqual(runner.load(base / "export-wrapper-refusal.json")["process"]["exit_code"], 0)

    def test_boolean_protocol_version_refuses_even_with_matching_hash(self):
        plan = {"schema": runner.SCHEMA, "protocol": True,
                "limits": {"turns": runner.MAX_TURNS, "wall_seconds": 120, "workspace_bytes": runner.MAX_WORKSPACE}}
        frozen = {"plan": plan, "sha256": runner.digest(plan)}
        with self.assertRaisesRegex(ValueError, "unsupported rehearsal protocol"):
            runner.checked(frozen)

    def test_live_explanation_records_replay_and_regrade_offline(self):
        base = Path(__file__).resolve().parents[2] / "tests/agent-eval/opencode/results/2026-10-01-explanations"
        frozen = runner.load(base / "plan.json")
        report = runner.report(frozen, base / "attempts")
        self.assertEqual(report, runner.load(base / "report.json"))
        self.assertEqual(report["planned"], 12)
        self.assertTrue(all(row["status"] in ("completed", "failed") for row in report["attempts"]))
        names = ("opencode_rehearsal.py", "bounded_host.py", "source_disclosure.py",
                 "workspace_bundle.py", "study.py", "rehearsal_evidence.py")
        self.assertEqual(runner.digest({name: runner.identity(base / "frozen-runner" / name)
                                       for name in names}), frozen["plan"]["implementation_sha256"])
        for row in report["attempts"]:
            folder = base / "attempts" / row["cell"]["id"]
            record = runner.load(folder / "record.json")
            task = next(task for task in frozen["plan"]["tasks"] if task["id"] == row["cell"]["task"])
            self.assertEqual(runner.identity(base / "frozen-runner/explanation_grade.py"), task["grader_sha256"])
            if record["status"] != "completed":
                self.assertFalse(row["passed"])
                continue
            _, spans = runner.audit_disclosure(frozen["plan"], task, record, folder)
            with tempfile.TemporaryDirectory() as tmp:
                source = Path(tmp) / "source"
                runner.unpack(task["files"], source, runner.MAX_WORKSPACE)
                grade = explanation_grade.grade(source, {"answer": record["answer"],
                                                        "criteria": task["private_criteria"], "disclosed": spans})
            self.assertEqual(grade, record["grade"])

    def test_explanation_runs_grades_and_replays_without_executing_upstream_code(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "source"
            source.mkdir()
            content = "def f():\n    return 12345\n"
            (source / "module.py").write_text(content)
            criteria = {"result": {"value": 12345, "evidence": [[{"path": "module.py", "contains": content}]]}}
            (root / "criteria.json").write_text(json.dumps(criteria))
            binary = root / "fr"
            binary.write_text("fake binary")
            manifest = {"schema": runner.SCHEMA, "models": ["test/model"], "seed": 1, "repetitions": 1,
                        "tasks": [{"id": "explain", "kind": "explain", "source": "source",
                                   "criteria": "criteria.json", "grader": str(Path(explanation_grade.__file__).resolve()),
                                   "requirement": "Explain f", "provenance": "offline control"}]}
            frozen = runner.freeze(manifest, root, binary)
            cell = frozen["plan"]["cells"][0]
            actions = [{"action": "read", "path": "module.py", "offset": 0, "bytes": 4096, "sha256": ""},
                       {"action": "finish", "answer": {"result": {"value": 12345,
                        "citations": [{"path": "module.py", "quote": content}]}}}]
            messages = []
            def execute(command, prompt, stdout, stderr, *args, **kwargs):
                if "--version" in command:
                    raw = b"fake-opencode"
                elif "run" in command:
                    index = len(messages)
                    message = "msg_" + str(index)
                    raw = serialized(stream(actions[index], message=message))
                    turn = runner.events(raw)
                    messages.append({"info": {"id": message, "role": "assistant", "finish": "stop",
                                              "providerID": "test", "modelID": "model", "tokens": turn["tokens"], "cost": 0}})
                elif "export" in command:
                    raw = runner.encode({"info": {"id": "ses_abc"}, "messages": messages})
                else:
                    raw = runner.encode(explanation_grade.grade(Path(command[-1]), json.loads(prompt)))
                stdout.write(raw)
                return {"exit_code": 0, "stop_reason": None, "sampled_aggregate_rss_bytes": 100}
            output = root / "attempts"
            with patch.object(runner, "bounded_run", side_effect=execute):
                record = runner.run_attempt(frozen, cell["id"], root, output, binary, Path("opencode"))
            self.assertEqual(record["status"], "completed", record["failure"])
            result = runner.report(frozen, output)
            self.assertEqual(result["passed"], 1)
            self.assertEqual(result["attempts"][0]["disclosure"]["unique_source_bytes"], len(content))
            self.assertEqual(json.loads((output / cell["id"] / "submission.json").read_text()),
                             {"source_sha256": frozen["plan"]["tasks"][0]["source_sha256"]})
            (output / cell["id"] / "unexpected.txt").write_text("unhashed data")
            with self.assertRaisesRegex(ValueError, "unplanned retained artifact"):
                runner.report(frozen, output)


if __name__ == "__main__":
    unittest.main()
