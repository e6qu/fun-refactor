"""Offline source, plan and failure-accounting controls for terminal reviews."""
import base64
import copy
import gzip
import hashlib
import os
from pathlib import Path
import runpy
import sys
import subprocess
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval import native_mcp as mcp, source_reviews, structured_probe as probe
from agent_eval import terminal_reviews as review, terminal_review_runner as runner
from agent_eval import test_structured_submission as base
from agent_eval.study import digest, encode


def frozen(count=1, packet=False):
    files = {"module.py": {"data": base64.b64encode(probe.SOURCE).decode(), "executable": False}}
    selection = {"path": "module.py", "sha256": hashlib.sha256(probe.SOURCE).hexdigest(),
                 "start": 0, "end": len(probe.SOURCE) if packet else 16}
    questions = [{"id": f"question-{i}", "question": "Check whether value returns 43.", "files": files,
                  "selections": [selection]} for i in range(count)]
    models = [{"providerID": "scripted", "modelID": "protocol", "baseURL": "http://127.0.0.1:8080/v1",
               "context": 8192, "output": 512}]
    return review.freeze(questions, models, Path(sys.executable), Path(sys.executable), {"kind": "unit fixture"})


def write_capture(root, plan, snapshots, cell, *, packet=False, mutate=None):
    base.capture_fixture(root)
    (root / "provider.json").unlink()
    _, _, messages, _, rows = base.fixture()
    task = next(t for t in plan["tasks"] if t["id"] == cell["task"])
    request = review.request(plan, task, cell)
    messages[0]["parts"][0].update(request["parts"][0])
    answer = {"answer": {"findings": [{"gap": "Returns the wrong value", "wrong_repair": "Keep 42",
              "input": "value()", "expected": "43", "citations": [{"path": "module.py", "quote": probe.SOURCE.decode()}]}],
              "limitations": "One small source fixture only."}}
    messages[-1]["info"]["structured"] = copy.deepcopy(answer)
    messages[-1]["parts"][1]["state"]["input"] = copy.deepcopy(answer)
    if packet:
        messages.pop(1)
        rows = []
    if mutate:
        mutate(messages, rows)
    data = {"request.json": request, "terminal.json": messages[-1], "messages.json": messages,
            "export.json": {"info": {"id": "ses_control"}, "messages": messages},
            "identity.json": {"schema": review.SCHEMA, "plan_sha256": digest(plan), "cell": cell,
                "opencode_version": plan["opencode_version"], "binary_sha256": plan["binary_sha256"],
                "opencode_sha256": plan["opencode_sha256"]}}
    for name, value in data.items():
        (root / name).write_bytes(encode(value))
    (root / "events.jsonl").write_bytes(b"".join(encode(e) + b"\n" for e in base.events_for(messages)))
    (root / "tools.jsonl").write_bytes(b"".join(encode(r) + b"\n" for r in rows))
    return mcp.decode((root / "process.json").read_bytes())


class TerminalReview(unittest.TestCase):
    def test_variant_is_frozen_and_changes_the_request_without_raising_limits(self):
        f, snapshots = frozen()
        plan = f["plan"]
        model = plan["models"][0]
        for configured in (False, True):
            value = {**model, "variant": "low"}
            if configured:
                value.pop("baseURL")
                value["configured"] = True
            self.assertEqual(review.profile(value), value)
        plan["models"][0]["variant"] = "low"
        f["sha256"] = digest(plan)
        checked = review.checked(f, snapshots)
        body = review.request(checked, checked["tasks"][0], checked["cells"][0])
        self.assertEqual(body["variant"], "low")
        self.assertEqual(checked["limits"], frozen()[0]["plan"]["limits"])
        plan["models"][0]["variant"] = "max"
        with self.assertRaisesRegex(ValueError, "plan changed"):
            review.checked(f, snapshots)
        for value in (None, {}, "", "low high", "x" * 65):
            with self.assertRaisesRegex(ValueError, "invalid model variant"):
                review.profile({**model, "variant": value})

    def test_freeze_rejects_inputs_that_cannot_be_replayed(self):
        f, _ = frozen()
        for field, value in (("providerID", "x" * 257), ("modelID", "x" * 257),
                             ("baseURL", "https://example.test/" + "x" * 2048)):
            with self.subTest(field=field), self.assertRaises(ValueError):
                review.profile({**f["plan"]["models"][0], field: value})
        with patch.object(review, "MAX_BYTES", len(encode(f)) - 1):
            with self.assertRaisesRegex(ValueError, "plan exceeds retention budget"):
                frozen()

    def test_oversized_archive_refuses_before_creating_output(self):
        f, snapshots = frozen()
        cli = runpy.run_path(str(Path(__file__).parents[1] / "terminal-reviews.py"))
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "unused"
            args = ["terminal-reviews.py", "freeze", "questions.json", "models.json", str(output),
                    "--fr", sys.executable, "--opencode", sys.executable]
            with patch.object(sys, "argv", args), patch.object(review, "read", return_value=b"[]"), \
                    patch.object(review, "freeze", return_value=(f, snapshots)), \
                    patch.object(gzip, "compress", return_value=b"x" * (review.MAX_BYTES + 1)):
                with self.assertRaisesRegex(ValueError, "archive exceeds replay budget"):
                    cli["main"]()
            self.assertFalse(output.exists())

    def test_capture_refuses_changed_parent_plan_before_starting_client(self):
        f, snapshots = frozen()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "plan.json").write_bytes(encode(f))
            (root / "inputs.json.gz").write_bytes(gzip.compress(encode(snapshots), mtime=0))
            result = subprocess.run([sys.executable, "-B", str(Path(__file__).parents[1] / "terminal-reviews.py"),
                "capture", str(root), f["plan"]["cells"][0]["id"], str(root / "unused"),
                "--fr", "/missing-fr", "--opencode", "/missing-opencode", "--plan-sha", "0" * 64],
                capture_output=True, timeout=5)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn(b"capture plan differs from parent", result.stderr)
            self.assertFalse((root / "unused").exists())

    def test_large_failed_log_is_bounded_and_truncation_is_reported(self):
        f, snapshots = frozen()
        cell = f["plan"]["cells"][0]
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            root = output / cell["id"]
            root.mkdir()
            process = write_capture(root, f["plan"], snapshots, cell)
            (root / "server.stderr").write_bytes(b"x" * (review.MAX_BYTES + 31))
            record = runner.seal(f, snapshots, cell, root, process)
            self.assertEqual(record["status"], "failed")
            self.assertEqual(record["truncated_artifacts"]["server.stderr"]["observed_bytes"], review.MAX_BYTES + 31)
            self.assertEqual((root / "server.stderr").stat().st_size, review.MAX_BYTES)
            row = review.report(f, snapshots, output)["attempts"][0]
            self.assertEqual(row["observed"]["usage"]["finished_steps"], 2)
            self.assertIsNone(row["audit"])

    def test_packet_and_retrieved_citations_replay(self):
        for packet in (False, True):
            f, snapshots = frozen(packet=packet)
            plan, cell = f["plan"], f["plan"]["cells"][0]
            with tempfile.TemporaryDirectory() as temporary:
                output = Path(temporary)
                root = output / cell["id"]
                root.mkdir()
                process = write_capture(root, plan, snapshots, cell, packet=packet)
                record = runner.seal(f, snapshots, cell, root, process)
                self.assertEqual(record["status"], "completed", record["failure"])
                result = review.report(f, snapshots, output)
                row = result["attempts"][0]
                self.assertEqual(row["observed"]["host_calls"], 0 if packet else 1)
                self.assertEqual(row["audit"]["source_context"]["unique_source_bytes"], len(probe.SOURCE))
                self.assertFalse(row["audit"]["review"]["claims_verified"])
                self.assertEqual(row["observed"]["terminal_calls"], 1)

    def test_frozen_source_protocol_and_limits_cannot_drift(self):
        f, snapshots = frozen()
        for field, value in (("limits", {}), ("prompt", "different"), ("format", {}),
                             ("tools", []), ("retries", 1), ("opencode_version", "other")):
            changed = copy.deepcopy(f)
            changed["plan"][field] = value
            changed["sha256"] = digest(changed["plan"])
            with self.subTest(field=field), self.assertRaises(ValueError):
                review.checked(changed, snapshots)
        snapshots["question-0"]["module.py"]["data"] = base64.b64encode(b"unrelated source").decode()
        with self.assertRaises(ValueError):
            review.checked(f, snapshots)

    def test_source_result_is_recomputed_even_when_all_records_agree(self):
        def invented(messages, rows):
            rows[0]["result"]["text"] = "def value():\n    return 43\n"
            raw = encode(rows[0]["result"]).decode()
            rows[0]["response"]["content"][0]["text"] = raw
            messages[1]["parts"][1]["state"]["output"] = raw
        f, snapshots = frozen()
        cell = f["plan"]["cells"][0]
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            write_capture(root, f["plan"], snapshots, cell, mutate=invented)
            task = {**f["plan"]["tasks"][0], "files": snapshots[cell["task"]]}
            with self.assertRaisesRegex(ValueError, "source replay"):
                review.audit(f["plan"], task, cell, root)

    def test_unprovided_citation_refuses(self):
        f, snapshots = frozen(packet=False)
        cell = f["plan"]["cells"][0]
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            process = write_capture(root, f["plan"], snapshots, cell, packet=True)
            record = runner.seal(f, snapshots, cell, root, process)
            self.assertEqual(record["status"], "failed")
            self.assertIn("citation", record["failure"])

    def test_failed_prefix_retains_usage_without_export(self):
        f, snapshots = frozen()
        cell = f["plan"]["cells"][0]
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            root = output / cell["id"]
            root.mkdir()
            process = write_capture(root, f["plan"], snapshots, cell)
            for name in ("export.json", "messages.json", "terminal.json", "identity.json"):
                (root / name).unlink()
            with (root / "events.jsonl").open("ab") as stream:
                stream.write(b'{"type":')
            process.update(exit_code=17, process_exit_code=17)
            self.assertEqual(runner.seal(f, snapshots, cell, root, process)["status"], "failed")
            row = review.report(f, snapshots, output)["attempts"][0]
            self.assertIsNone(row["audit"])
            self.assertEqual(row["observed"]["usage"]["finished_steps"], 2)
            self.assertEqual(row["observed"]["host_calls"], 1)
            self.assertGreater(row["observed"]["unparsed_stream_tail_bytes"], 0)
            self.assertFalse(row["observed"]["usage"]["provider_usage_verified"])

    def test_two_failures_stop_later_cells_and_retries(self):
        f, snapshots = frozen(count=3)
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            for cell in f["plan"]["cells"][:2]:
                root = output / cell["id"]
                root.mkdir()
                process = write_capture(root, f["plan"], snapshots, cell)
                process.update(exit_code=17, process_exit_code=17)
                runner.seal(f, snapshots, cell, root, process)
            result = review.report(f, snapshots, output)
            self.assertEqual((result["failed"], result["not_started"]), (2, 1))
            for cell in (f["plan"]["cells"][0], f["plan"]["cells"][2]):
                with self.assertRaises(ValueError):
                    source_reviews.eligible(f["plan"], cell["id"], output, f["sha256"])
            (output / f["plan"]["cells"][2]["id"]).mkdir()
            with self.assertRaisesRegex(ValueError, "stop or gap"):
                review.report(f, snapshots, output)

    def test_changed_records_symlinks_and_runtime_refuse(self):
        f, snapshots = frozen()
        cell = f["plan"]["cells"][0]
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            root = output / cell["id"]
            root.mkdir()
            process = write_capture(root, f["plan"], snapshots, cell)
            runner.seal(f, snapshots, cell, root, process)
            (root / "request.json").write_bytes(b"{}")
            with self.assertRaisesRegex(ValueError, "artifact changed"):
                review.report(f, snapshots, output)
            (root / "request.json").unlink()
            (root / "request.json").symlink_to(root / "messages.json")
            with self.assertRaisesRegex(ValueError, "invalid review artifact"):
                review.report(f, snapshots, output)
        f["plan"]["runtime"]["terminal_reviews.py"] = "0" * 64
        f["sha256"] = digest(f["plan"])
        with self.assertRaisesRegex(ValueError, "runtime changed"):
            review.checked(f, snapshots, execution=True)

    def test_provider_configuration_never_copies_a_secret_into_the_plan(self):
        f, _ = frozen()
        model = {**f["plan"]["models"][0], "baseURL": "https://provider.example/v1"}
        with patch.dict(os.environ, {"GITHUB_ACTIONS": "true", "FR_REVIEW_API_KEY": "test-only-value"}):
            env = runner.environment(Path("/capture"), model, Path("/capture/config.json"))
        self.assertNotIn("test-only-value", env["OPENCODE_CONFIG_CONTENT"])
        self.assertIn("{env:FR_REVIEW_API_KEY}", env["OPENCODE_CONFIG_CONTENT"])
        with patch.dict(os.environ, {}, clear=True), self.assertRaisesRegex(ValueError, "remote bounded"):
            runner.environment(Path("/capture"), model, Path("/capture/config.json"))
        with self.assertRaises(ValueError):
            review.profile({**model, "apiKey": "not allowed"})


if __name__ == "__main__":
    unittest.main()
