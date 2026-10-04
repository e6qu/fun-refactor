"""Offline controls for provided source, narrow reviews and collection stop rules."""
import base64
import copy
import hashlib
import io
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval import native_mcp as mcp, source_packets as packets, source_reviews as reviews
from agent_eval.study import digest, encode

RAW = b"def value():\n    return 42\n"
FILES = {"module.py": {"data": base64.b64encode(RAW).decode(), "executable": False}}


def selection(start=0, end=len(RAW), path="module.py", raw=RAW):
    return {"path": path, "sha256": hashlib.sha256(raw).hexdigest(), "start": start, "end": end}


def question(name="identity", selections=None):
    return {"id": name, "question": "Review only preservation of original objects.",
            "files": copy.deepcopy(FILES), "selections": selections or [selection()]}


def frozen(questions=None):
    return reviews.freeze(questions or [question()], ["provider/model"], Path(__file__), Path(__file__), {})


def answer(packet):
    return {"findings": [{"gap": "The assertion checks equality only.", "wrong_repair": "Rebuild equal objects.",
                         "input": "Distinct equal objects.", "expected": "Keep each original object.",
                         "citations": [{"source": packet["source_refs"][0]["source"]}]}],
            "limitations": "This synthetic example does not establish an actual grader flaw."}


def transcript(plan, task, submitted, calls=None):
    log = io.BytesIO()
    server = mcp.Server({"files": task["files"], "arm": "fr", "binary": "fr", "workspace": "/unused",
                         "tools_schema_version": 4}, log)
    batches = calls or [[{"name": "submit_answer", "arguments": {"answer": submitted}}], []]
    events, messages = [], [{"info": {"role": "user"}, "parts": [{"type": "text",
        "text": plan["prompt"] + "\nTask:\n" + task["requirement"]}]}]
    usage = {"input": 10, "output": 20, "reasoning": 0, "cache": {"read": 0, "write": 0}}
    count = 0
    for index, batch in enumerate(batches):
        mid, parts = f"msg_{index}", []
        events.append({"type": "step_start", "sessionID": "ses_review", "part": {"messageID": mid}})
        for call in batch:
            response = server.call(call)
            part = {"type": "tool", "messageID": mid, "sessionID": "ses_review", "id": f"prt_{count}",
                    "callID": f"call_{count}", "tool": "rehearsal_" + call["name"],
                    "state": {"status": "completed", "input": call["arguments"], "output": response["content"][0]["text"]}}
            count += 1
            parts.append(part)
            events.append({"type": "tool_use", "sessionID": "ses_review", "part": part})
        finish = "tool-calls" if batch else "stop"
        events.append({"type": "step_finish", "sessionID": "ses_review",
                       "part": {"messageID": mid, "reason": finish, "tokens": usage, "cost": 0}})
        messages.append({"info": {"id": mid, "role": "assistant", "providerID": "provider", "modelID": "model",
                                  "finish": finish, "tokens": usage, "cost": 0}, "parts": parts})
    return b"\n".join(encode(row) for row in events), {"info": {"id": "ses_review"}, "messages": messages}, log.getvalue()


class Packets(unittest.TestCase):
    def test_packet_resolves_exact_source_and_counts_rereads(self):
        packet = packets.build(FILES, [selection()])
        spans = packets.checked(FILES, packet)
        result = packets.finding(answer(packet), spans)
        self.assertEqual(result["findings"][0]["citations"], [{"path": "module.py", "quote": RAW.decode()}])
        self.assertFalse(result["claims_verified"])
        self.assertFalse(result["findings"][0]["verified_counterexample"])
        cost = packets.accounting(packet, spans)
        self.assertEqual((cost["provided_source_bytes"], cost["retrieved_source_bytes"],
                          cost["repeated_source_bytes"], cost["unique_source_bytes"]), (27, 27, 27, 27))
        self.assertGreater(cost["provided_packet_bytes"], 27)

    def test_altered_bytes_id_extent_or_frozen_source_refuse(self):
        packet = packets.build(FILES, [selection()])
        for key, value in [("text", "def value():\n    return 43\n"), ("end", 26), ("sha256", "0" * 64)]:
            altered = copy.deepcopy(packet)
            altered["spans"][0][key] = value
            with self.assertRaises(ValueError):
                packets.checked(FILES, altered)
        altered = copy.deepcopy(packet)
        altered["source_refs"][0]["source"] = "src1:" + "0" * 64
        with self.assertRaises(ValueError):
            packets.checked(FILES, altered)
        files = copy.deepcopy(FILES)
        files["module.py"]["data"] = base64.b64encode(RAW.replace(b"42", b"43")).decode()
        with self.assertRaises(ValueError):
            packets.checked(files, packet)

    def test_overlaps_bad_boundaries_and_source_or_metadata_budgets_refuse(self):
        for slices in ([], [selection()] * 2, [selection(start=True)], [selection(end=100)], [selection(start=-1)]):
            with self.assertRaises(ValueError):
                packets.build(FILES, slices)
        raw = "κ".encode()
        files = {"module.py": {"data": base64.b64encode(raw).decode(), "executable": False}}
        with self.assertRaises(UnicodeError):
            packets.build(files, [selection(end=1, raw=raw)])
        for raw in (b"x" * 8193, b"\x00" * 8192):
            files["module.py"]["data"] = base64.b64encode(raw).decode()
            with self.assertRaises(ValueError):
                packets.build(files, [selection(end=len(raw), raw=raw)])

    def test_empty_review_does_not_accept_the_grader_and_unread_quotes_refuse(self):
        packet = packets.build(FILES, [selection(0, 16)])
        review = packets.finding({"findings": [], "limitations": "Only one invariant inspected."}, packet["spans"])
        self.assertFalse(review["claims_verified"])
        submitted = answer(packet)
        submitted["findings"][0]["citations"] = [{"path": "module.py", "quote": RAW.decode()}]
        with self.assertRaises(ValueError):
            packets.finding(submitted, packet["spans"])
        with self.assertRaises(ValueError):
            packets.finding({"findings": [], "limitations": ""}, packet["spans"])


class Collection(unittest.TestCase):
    def test_frozen_reference_sources_replay_exact_edits_without_candidate_execution(self):
        from agent_eval import change_controls
        root = Path(__file__).resolve().parents[2]
        directory = root / "tests/agent-eval/opencode/reviews/2026-10-05-references"
        frozen_plan = reviews.load(directory / "plan.json")
        snapshots = reviews.read_inputs(directory)
        plan = reviews.checked(frozen_plan, snapshots)
        self.assertTrue(plan["reference_repairs_disclosed"])
        self.assertFalse(plan["candidate_execution"])
        self.assertEqual(len(plan["cells"]), 6)
        self.assertEqual(plan["stop_after_consecutive_failures"], 2)
        self.assertEqual(plan["retries"], 0)
        for task in plan["tasks"]:
            files = snapshots[task["id"]]
            baseline = {p.removeprefix("baseline/"): row for p, row in files.items() if p.startswith("baseline/")}
            repaired = {p: row for p, row in files.items() if not p.startswith(("baseline/", "review/"))}
            reference = mcp.decode(base64.b64decode(files["review/reference.json"]["data"]))
            bindings = plan["provenance"]["references"][task["id"]]
            self.assertEqual(bindings["baseline_sha256"], digest(baseline))
            self.assertEqual(bindings["reference_sha256"], digest(repaired))
            self.assertEqual(bindings["control_sha256"], digest(reference))
            self.assertEqual(change_controls.apply(baseline, reference), repaired)
            self.assertTrue(all(s["path"].startswith(("src/", "review/")) for s in task["packet"]["spans"]))
            self.assertIn("proposed reference repair, not a proven answer", task["question"])

    def test_reference_disclosure_is_versioned_and_does_not_change_old_reports(self):
        old, snapshots = frozen()
        reference, revised = reviews.freeze_reference(
            [question()], ["provider/model"], Path(__file__), Path(__file__), {})
        self.assertEqual(snapshots, revised)
        self.assertEqual(old["plan"]["limits"], reference["plan"]["limits"])
        for plan in (old, reference):
            reviews.checked(plan, snapshots, execution=True)
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "attempts"
            original = reviews.report(old, snapshots, output)
            disclosed = reviews.report(reference, snapshots, output)
        self.assertEqual(original["schema"], "fr-source-review-report-1")
        self.assertNotIn("reference_repairs_disclosed", original)
        self.assertEqual(disclosed["schema"], "fr-source-review-report-2")
        self.assertIs(disclosed["reference_repairs_disclosed"], True)
        self.assertFalse(disclosed["claims_verified"])
        self.assertEqual(disclosed["not_started"], 1)
        for plan, values in ((old, (True, 0, None)), (reference, (False, 1, "true"))):
            for value in values:
                altered = copy.deepcopy(plan)
                altered["plan"]["reference_repairs_disclosed"] = value
                altered["sha256"] = digest(altered["plan"])
                with self.assertRaisesRegex(ValueError, "reference disclosure"):
                    reviews.checked(altered, snapshots)

    def test_github_counterexample_binds_review_inputs_and_the_same_wrong_repair(self):
        from agent_eval import change_controls as controls
        root = Path(__file__).resolve().parents[2]
        directory = root / "tests/agent-eval/opencode/reviews/2026-10-04-packets"
        pack = root / "tests/agent-eval/opencode/candidates"
        evidence = reviews.load(directory / "counterexample.json")
        self.assertEqual(evidence["review_plan_sha256"], reviews.load(directory / "plan.json")["sha256"])
        self.assertEqual(evidence["review_cell"], "dotenv-flat-grammar-0")
        for name, sha in evidence["candidate_inputs"].items():
            self.assertEqual(hashlib.sha256((pack / name).read_bytes()).hexdigest(), sha)
        task = reviews.load(pack / "manifest.json")["tasks"][0]
        files, variants, public = controls.definitions(pack, task)
        rows = evidence["results"]
        self.assertEqual({r["id"] for r in rows}, {"dotenv-alternate/reference",
                         "dotenv-alternate/recursively-expand-alternate-word"})
        controls.verify(rows)
        current = reviews.load(pack / task["grader"])
        for row in rows:
            variant = next(v for v in variants if row["id"] == task["id"] + "/" + v["id"])
            self.assertEqual(row["submission_sha256"], digest(controls.apply(files, variant)))
            self.assertEqual(row["grade"]["grader_sha256"], evidence["candidate_inputs"][task["grader"]])
            self.assertEqual(row["grade"]["image"], current["image"])
            self.assertEqual(row["public_grade"]["candidate"], row["grade"]["candidate"])
            self.assertEqual(row["public_grade"]["grader_sha256"], digest(public))
            self.assertEqual([c["id"] for c in row["grade"]["cases"]], [c["id"] for c in current["cases"]])
            if "baseline_grade" in row:
                old = reviews.load(pack / variant["baseline"]["grader"])
                self.assertEqual(row["baseline_grade"]["image"], current["image"])
                self.assertEqual([c["id"] for c in row["baseline_grade"]["cases"]], [c["id"] for c in old["cases"]])

    def test_retained_attempts_replay_without_promoting_timeout_submissions(self):
        root = Path(__file__).resolve().parents[2]
        directory = root / "tests/agent-eval/opencode/reviews/2026-10-04-packets"
        plan = reviews.load(directory / "plan.json")
        snapshots = reviews.read_inputs(directory)
        result = reviews.report(plan, snapshots, directory / "attempts")
        self.assertEqual(result, reviews.load(directory / "report.json"))
        self.assertEqual((result["completed"], result["failed"], result["not_started"]), (1, 2, 3))
        complete, submitted_timeout, unfinished = result["attempts"][:3]
        self.assertEqual(len(complete["review"]["findings"]), 1)
        self.assertTrue(complete["initial_packet_export_verified"])
        self.assertEqual(complete["combined_source_before_answer"]["retrieved_source_bytes"], 0)
        self.assertTrue(submitted_timeout["costs"]["observed"]["submission_observed"])
        for row in (submitted_timeout, unfinished):
            self.assertIsNone(row["review"])
            self.assertIsNone(row["actual_usd"])
            self.assertIsNone(row["combined_source_before_answer"])
            self.assertFalse(row["initial_packet_export_verified"])

    def test_counterexample_preserves_the_grader_given_to_the_reviewer(self):
        root = Path(__file__).resolve().parents[2]
        directory = root / "tests/agent-eval/opencode/reviews/2026-10-04-packets"
        pack = root / "tests/agent-eval/opencode/candidates"
        plan = reviews.load(directory / "plan.json")["plan"]
        snapshots = reviews.read_inputs(directory)
        variant = next(row for row in reviews.load(pack / "controls.json")["dotenv-alternate"]
                       if row["id"] == "recursively-expand-alternate-word")
        baseline = pack / variant["baseline"]["grader"]
        self.assertEqual(hashlib.sha256(baseline.read_bytes()).hexdigest(),
                         plan["provenance"]["public_inputs"]["dotenv-alternate-grader.json"])
        old = reviews.load(baseline)
        source = base64.b64decode(snapshots["dotenv-flat-grammar"]["review/grader.py"]["data"])
        self.assertEqual(old["command"][-1].encode(), source)
        current = reviews.load(pack / "dotenv-alternate-grader.json")
        self.assertEqual(len(current["cases"]), len(old["cases"]) + 1)
        failures = reviews.load(pack / "control-failures.json")["dotenv-alternate"]
        self.assertEqual(failures[variant["id"]], ["flat-braces"])

    def test_retained_plan_binds_narrow_questions_and_withholds_reference_repairs(self):
        root = Path(__file__).resolve().parents[2]
        directory = root / "tests/agent-eval/opencode/reviews/2026-10-04-packets"
        plan = reviews.load(directory / "plan.json")
        snapshots = reviews.read_inputs(directory)
        checked = reviews.checked(plan, snapshots)
        self.assertEqual(len(checked["cells"]), 6)
        self.assertEqual(checked["models"], ["kimi-code-plan-global/k3", "zai-coding-plan/glm-5.3-flash"])
        for task in checked["tasks"]:
            files = snapshots[task["id"]]
            self.assertEqual({name for name in files if name.startswith("review/")},
                             {"review/requirement.txt", "review/grader.py", "review/cases.json", "review/public-check.py"})
            self.assertFalse(any("controls" in name or "review-baseline" in name for name in files))
            self.assertLessEqual(len(encode(task["packet"])), packets.MAX_PACKET)
            self.assertEqual(task["requirement"], packets.requirement(task["question"], task["packet"]))

    def audit(self, frozen_plan, snapshots, submitted, calls=None, altered_prompt=False):
        plan = frozen_plan["plan"]
        task = {**plan["tasks"][0], "files": snapshots[plan["tasks"][0]["id"]]}
        raw, exported, log = transcript(plan, task, submitted, calls)
        if altered_prompt:
            exported["messages"][0]["parts"][0]["text"] += "unrecorded context"
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            (directory / "opencode.stdout").write_bytes(raw)
            (directory / "export.stdout").write_bytes(encode(exported))
            (directory / "tools.jsonl").write_bytes(log)
            return reviews.audit(plan, task, plan["cells"][0], directory)

    def test_initial_packet_can_support_a_submission_without_a_read_call(self):
        plan, snapshots = frozen()
        submitted = answer(plan["plan"]["tasks"][0]["packet"])
        audited = self.audit(plan, snapshots, submitted)
        self.assertEqual(audited["metrics"]["tool_calls"], 1)
        self.assertEqual(audited["source_context"]["provided_source_bytes"], 27)
        self.assertEqual(audited["source_context"]["retrieved_source_bytes"], 0)
        self.assertEqual(len(audited["review"]["findings"]), 1)
        with self.assertRaisesRegex(ValueError, "prompt differs"):
            self.audit(plan, snapshots, submitted, altered_prompt=True)

    def test_same_step_read_cannot_fill_an_omitted_packet_gap(self):
        plan, snapshots = frozen([question(selections=[selection(0, 16)])])
        submitted = answer(plan["plan"]["tasks"][0]["packet"])
        submitted["findings"][0]["citations"] = [{"path": "module.py", "quote": RAW.decode()}]
        read = {"name": "read_source", "arguments": {"path": "module.py", "offset": 0, "bytes": 8192, "sha256": ""}}
        submit = {"name": "submit_answer", "arguments": {"answer": submitted}}
        with self.assertRaisesRegex(ValueError, "citation was not provided"):
            self.audit(plan, snapshots, submitted, [[read, submit], []])
        result = self.audit(plan, snapshots, submitted, [[read], [submit], []])
        self.assertEqual(result["source_context"]["repeated_source_bytes"], 16)

    def test_frozen_packet_question_runtime_and_stop_rule_are_checked(self):
        plan, snapshots = frozen()
        reviews.checked(plan, snapshots, execution=True)
        for key, value in (("limits", {**plan["plan"]["limits"], "wall_seconds": 121}),
                           ("retries", 1), ("stop_after_consecutive_failures", 3)):
            altered = copy.deepcopy(plan)
            altered["plan"][key] = value
            altered["sha256"] = digest(altered["plan"])
            with self.assertRaises(ValueError):
                reviews.checked(altered, snapshots)
        altered = copy.deepcopy(plan)
        altered["plan"]["tasks"][0]["requirement"] += "other question"
        altered["sha256"] = digest(altered["plan"])
        with self.assertRaises(ValueError):
            reviews.checked(altered, snapshots)
        with patch.object(reviews, "implementation", return_value={}):
            reviews.checked(plan, snapshots)
            with self.assertRaises(ValueError):
                reviews.checked(plan, snapshots, execution=True)

    def test_two_failures_stop_later_cells_and_a_cell_cannot_be_retried(self):
        plan, _ = frozen([question("one"), question("two"), question("three")])
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            cells = plan["plan"]["cells"]
            self.assertEqual(reviews.eligible(plan["plan"], cells[0]["id"], output, plan["sha256"]), cells[0])
            for cell in cells[:2]:
                folder = output / cell["id"]
                folder.mkdir()
                (folder / "record.json").write_bytes(encode({"cell": cell, "plan_sha256": plan["sha256"], "status": "failed"}))
            with self.assertRaisesRegex(ValueError, "already attempted"):
                reviews.eligible(plan["plan"], cells[0]["id"], output, plan["sha256"])
            with self.assertRaisesRegex(ValueError, "stop rule"):
                reviews.eligible(plan["plan"], cells[2]["id"], output, plan["sha256"])

    def test_collection_retains_partial_reads_and_enforces_the_stop_without_a_model(self):
        plan, snapshots = frozen([question("one"), question("two"), question("three")])
        calls = []

        def timeout(command, data, out, err, directory, **kwargs):
            calls.append(command)
            version = command[-1] == "--version"
            if version:
                out.write(b"test-version")
            else:
                with (directory / "tools.jsonl").open("wb") as log:
                    server = mcp.Server({"files": FILES, "arm": "fr", "binary": "fr", "workspace": "/unused",
                                         "tools_schema_version": 4}, log)
                    server.call({"name": "read_source", "arguments": {
                        "path": "module.py", "offset": 0, "bytes": 8192, "sha256": ""}})
            return {"exit_code": 0 if version else -15, "stop_reason": None if version else "wall",
                    "sampled_cpu_seconds": 0.01, "sampled_aggregate_rss_bytes": 4096}

        with tempfile.TemporaryDirectory() as temporary, patch.object(reviews.bounded_host, "run", side_effect=timeout):
            output = Path(temporary)
            for cell in plan["plan"]["cells"][:2]:
                record = reviews.collect(plan, snapshots, cell["id"], output, Path(__file__), Path(__file__))
                self.assertEqual(record["status"], "failed")
            with self.assertRaisesRegex(ValueError, "stop rule"):
                reviews.collect(plan, snapshots, plan["plan"]["cells"][2]["id"], output, Path(__file__), Path(__file__))
            self.assertEqual(len(calls), 4)
            result = reviews.report(plan, snapshots, output)
            self.assertEqual((result["completed"], result["failed"], result["not_started"]), (0, 2, 1))
            for row in result["attempts"][:2]:
                self.assertEqual(row["costs"]["observed"]["source_page_bytes"], 27)
                self.assertEqual(row["costs"]["observed"]["native_confirmed_source_page_bytes"], 0)
                self.assertFalse(row["initial_packet_export_verified"])
                self.assertIsNone(row["combined_source_before_answer"])
                self.assertIsNone(row["actual_usd"])
            cell = plan["plan"]["cells"][0]
            with (output / cell["id"] / "opencode.stdout").open("ab") as stream:
                stream.write(b"changed")
            with self.assertRaisesRegex(ValueError, "artifact changed"):
                reviews.report(plan, snapshots, output)


if __name__ == "__main__":
    unittest.main()
