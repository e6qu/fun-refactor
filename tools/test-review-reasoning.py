"""Offline controls for provider-visible reasoning settings and stopped work."""
import copy
import base64
import gzip
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

from agent_eval import terminal_reviews as review, terminal_review_runner as runner
from agent_eval import test_terminal_reviews as fixtures
from agent_eval.study import encode

spec = importlib.util.spec_from_file_location("reasoning", Path(__file__).with_name("check-review-reasoning.py"))
reasoning = importlib.util.module_from_spec(spec)
spec.loader.exec_module(reasoning)


def fixture(root):
    old, snapshots = fixtures.frozen()
    task = old["plan"]["tasks"][0]
    case = reasoning.CASES[0]
    questions = [{"id": task["id"], "question": task["question"], "files": snapshots[task["id"]],
                  "selections": [{k: s[k] for k in ("path", "sha256", "start", "end")} for s in task["packet"]["spans"]]}]
    model = {"providerID": case["provider"], "modelID": case["model"], "variant": case["variant"],
             "configured": True, "context": 32768, "output": 2048}
    frozen, snapshots = review.freeze(questions, [model], Path(sys.executable), Path(sys.executable), {"case": case})
    (root / "plan.json").write_bytes(encode(frozen))
    import gzip
    (root / "inputs.json.gz").write_bytes(gzip.compress(encode(snapshots), mtime=0))
    cell = frozen["plan"]["cells"][0]
    folder = root / "attempts" / cell["id"]
    folder.mkdir(parents=True)
    def mutate(messages, rows):
        messages[0]["info"].update(model={"providerID": case["provider"], "modelID": case["model"], "variant": case["variant"]})
        for message in messages[1:]:
            message["info"].update(providerID=case["provider"], modelID=case["model"])
    process = fixtures.write_capture(folder, frozen["plan"], snapshots, cell, mutate=mutate)
    identity = json.loads((folder / "identity.json").read_bytes())
    identity["schema"] = frozen["plan"]["schema"]
    (folder / "identity.json").write_bytes(encode(identity))
    runner.seal(frozen, snapshots, cell, folder, process)
    rows = [json.loads(line) for line in (folder / "tools.jsonl").read_bytes().splitlines()]
    request = {"model": case["model"], "reasoning_effort": case["variant"], "max_tokens": 2048,
               "tool_choice": "required", "messages": []}
    requests = [request, {**request, "messages": [{"role": "tool", "content": rows[0]["response"]["content"][0]["text"]}]}]
    (root / "provider.json").write_bytes(encode(requests))
    return requests


class Reasoning(unittest.TestCase):
    def test_hosted_and_workstation_transport_evidence_replays(self):
        from agent_eval import source_reviews
        repo = Path(__file__).resolve().parents[1]
        here = repo / "tests/agent-eval/opencode/reasoning/2026-10-07-low"
        manifest = json.loads((here / "manifest.json").read_bytes())
        expected = json.loads((here / "report.json").read_bytes())
        for name, digest in manifest["sources"].items():
            self.assertEqual(source_reviews.identity(here / name), digest)
        spec = importlib.util.spec_from_file_location("retained_reasoning", here / "check-review-reasoning.py")
        control = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(control)
        memory_spec = importlib.util.spec_from_file_location("memory", repo / "tools/check-client-memory.py")
        memory = importlib.util.module_from_spec(memory_spec)
        memory_spec.loader.exec_module(memory)
        self.assertEqual(set(manifest["platforms"]), {"macos-14", "ubuntu-latest", "workstation"})
        for name, identity in manifest["platforms"].items():
            archive = here / (name + ".json.gz")
            self.assertEqual(source_reviews.identity(archive), identity["archive_sha256"])
            with gzip.open(archive, "rb") as stream:
                raw = stream.read(8 * 1024**2 + 1)
            self.assertLessEqual(len(raw), 8 * 1024**2)
            files = json.loads(raw)
            self.assertEqual(len(files), identity["files"])
            with tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                for path, data in files.items():
                    relative = Path(path)
                    self.assertFalse(relative.is_absolute() or ".." in relative.parts)
                    destination = root / relative
                    destination.parent.mkdir(parents=True, exist_ok=True)
                    destination.write_bytes(base64.b64decode(data, validate=True))
                folders = [root] if name == "workstation" else [root / "review-reasoning" / str(i) for i in range(4)]
                results = [control.report(folder) for folder in folders]
                self.assertEqual(results, expected[name]["controls"])
                self.assertEqual([r["case"] for r in results], control.CASES[:1] if name == "workstation" else control.CASES)
                for folder, result in zip(folders, results):
                    frozen = json.loads((folder / "plan.json").read_bytes())
                    self.assertEqual(frozen["plan"]["provenance"]["script_sha256"], manifest["sources"]["check-review-reasoning.py"])
                    process = result["report"]["attempts"][0]["process"]
                    self.assertLessEqual(process["sampled_aggregate_rss_bytes"], 640 * 1024**2)
                    self.assertIsNone(process["stop_reason"])
                if name == "workstation":
                    invocation = json.loads((root / "invocation.json").read_bytes())
                    self.assertEqual(invocation["helper_sha256"], manifest["sources"]["fr-local-reasoning-control-fixed.py"])
                    self.assertEqual(invocation["script_sha256"], manifest["sources"]["check-review-reasoning.py"])
                else:
                    measured = memory.report(root / "client-memory")
                    self.assertEqual(measured, expected[name]["memory"])
                    self.assertTrue(measured["admitted"])

    def test_saved_client_variant_regression_keeps_original_failure(self):
        from agent_eval import structured_submission as protocol
        here = Path(__file__).resolve().parents[1] / "tests/agent-eval/opencode/reasoning/2026-10-07-variant-shape"
        manifest = json.loads((here / "manifest.json").read_bytes())
        for name, key in (("capture.json.gz", "capture_sha256"), ("structured_submission.py", "old_validator_sha256"),
                          ("local-control.py", "helper_sha256")):
            self.assertEqual(hashlib.sha256((here / name).read_bytes()).hexdigest(), manifest[key])
        old_spec = importlib.util.spec_from_file_location("agent_eval.old_variant_validator", here / "structured_submission.py")
        old = importlib.util.module_from_spec(old_spec)
        old_spec.loader.exec_module(old)
        with gzip.open(here / "capture.json.gz", "rb") as stream:
            raw = stream.read(8 * 1024**2 + 1)
        self.assertLessEqual(len(raw), 8 * 1024**2)
        files = json.loads(raw)
        self.assertEqual(len(files), manifest["files"])
        def read(name):
            return base64.b64decode(files["attempts/value-review-0/" + name], validate=True)
        request, terminal, messages = [json.loads(read(name)) for name in ("request.json", "terminal.json", "messages.json")]
        events = [json.loads(line) for line in read("events.jsonl").splitlines()]
        rows = [json.loads(line) for line in read("tools.jsonl").splitlines()]
        self.assertEqual(json.loads(read("record.json"))["failure"], "user format differs")
        with self.assertRaisesRegex(ValueError, "user format differs"):
            old.audit(request, terminal, messages, events, rows)
        audit = protocol.audit(request, terminal, messages, events, rows)
        self.assertEqual(audit["assistant_responses"], 2)
        self.assertEqual(json.loads(read("export.json"))["messages"], messages)
        requests = json.loads(base64.b64decode(files["provider.json"], validate=True))
        self.assertEqual([(r["reasoning_effort"], r["max_tokens"]) for r in requests], [("low", 2048)] * 2)
        messages[0]["info"]["model"]["variant"] = "max"
        with self.assertRaisesRegex(ValueError, "user variant differs"):
            protocol.audit(request, terminal, messages, events, rows)

    def test_provider_setting_and_source_delivery_replay(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            fixture(root)
            result = reasoning.report(root)
            self.assertEqual(result["provider_requests"], 2)
            self.assertEqual(result["report"]["completed"], 1)

    def test_changed_or_missing_provider_settings_and_source_refuse(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            requests = fixture(root)
            for index in (0, 1):
                for field, value in (("reasoning_effort", "max"), ("max_tokens", 4096),
                                     ("model", "other"), ("tool_choice", "auto")):
                    changed = copy.deepcopy(requests)
                    changed[index][field] = value
                    (root / "provider.json").write_bytes(encode(changed))
                    with self.assertRaises(ValueError):
                        reasoning.report(root)
            changed = copy.deepcopy(requests)
            del changed[0]["reasoning_effort"]
            (root / "provider.json").write_bytes(encode(changed))
            with self.assertRaisesRegex(ValueError, "did not reach"):
                reasoning.report(root)
            changed = copy.deepcopy(requests)
            changed[1]["messages"][0]["content"] = "invented source"
            (root / "provider.json").write_bytes(encode(changed))
            with self.assertRaisesRegex(ValueError, "source did not reach"):
                reasoning.report(root)

    def test_hosted_controls_refuse_local_execution(self):
        with tempfile.TemporaryDirectory() as temporary, patch.dict(os.environ, {}, clear=True):
            root = Path(temporary) / "unstarted"
            with self.assertRaisesRegex(ValueError, "on GitHub"):
                reasoning.check(root, Path("unused"), Path("unused"))
            self.assertFalse(root.exists())


if __name__ == "__main__":
    unittest.main()
