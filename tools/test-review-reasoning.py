"""Offline controls for provider-visible reasoning settings and stopped work."""
import copy
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
        messages[0]["info"].update(variant=case["variant"], model={"providerID": case["provider"], "modelID": case["model"]})
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
