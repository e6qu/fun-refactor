"""Replay catalog admission and the new whole-task collection without clients."""
import base64
import gzip
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from agent_eval import source_reviews, terminal_reviews

REPO = Path(__file__).resolve().parents[1]
EVIDENCE = REPO / "tests/agent-eval/opencode/memory/2026-10-07-catalog"
COLLECTION = REPO / "tests/agent-eval/opencode/reviews/2026-10-07-packaging"


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class CatalogReview(unittest.TestCase):
    def test_public_catalog_reconstructs_without_config_or_network(self):
        prepare = load("catalog_prepare", REPO / "tests/agent-eval/opencode/catalog/prepare.py")
        path = prepare.HERE / "catalog.json"
        self.assertEqual(path.read_bytes(), prepare.prepare())
        catalog = json.loads(path.read_bytes())
        for provider, model_id in prepare.PAIRS.items():
            model = catalog[provider]["models"][model_id]
            self.assertTrue(catalog[provider]["api"].startswith("https://"))
            self.assertEqual(catalog[provider]["npm"], "@ai-sdk/openai-compatible")
            self.assertEqual(model["interleaved"], {"field": "reasoning_content"})
            self.assertTrue(model["tool_call"] and model["reasoning"])

    def test_model_resolution_probe_refuses_local_execution(self):
        probe = load("catalog_resolution", REPO / "tools/check-client-catalog.py")
        with tempfile.TemporaryDirectory() as temporary, patch.dict("os.environ", {}, clear=True):
            root = Path(temporary) / "unstarted"
            with self.assertRaisesRegex(ValueError, "on GitHub"):
                probe.check(root, Path("unused"), Path("unused"))
            self.assertFalse(root.exists())

    def test_verbose_models_require_unique_complete_metadata(self):
        probe = load("catalog_resolution_parser", REPO / "tools/check-client-catalog.py")
        self.assertEqual(probe.models(b'one/model\n{\n"api": {}\n}\ntwo/model\n{"api":{}}\n'),
                         {"one/model": {"api": {}}, "two/model": {"api": {}}})
        for raw in (b'one/model\n{}\none/model\n{}', b'one/model\n{', b'one/model'):
            with self.assertRaises(ValueError):
                probe.models(raw)

    def test_complete_admission_evidence_replays(self):
        manifest = json.loads((EVIDENCE / "manifest.json").read_bytes())
        expected = json.loads((EVIDENCE / "report.json").read_bytes())
        for name, digest in manifest["sources"].items():
            self.assertEqual(source_reviews.identity(EVIDENCE / name), digest)
        checker = load("catalog_checker", EVIDENCE / "check-client-memory.py")
        checker.client_memory_profile = load("agent_eval.catalog_profile", EVIDENCE / "client_memory_profile.py")
        control = load("catalog_control", REPO / "tools/check-terminal-reviews.py")
        for name, identity in manifest["platforms"].items():
            archive = EVIDENCE / (name + ".json.gz")
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
                if name == "workstation":
                    result = control.report(root / "control/configured", "configured")
                    self.assertEqual([result], expected[name]["control"])
                    attempt = result["report"]["attempts"][0]
                    self.assertEqual(attempt["status"], "completed")
                    self.assertLessEqual(attempt["process"]["sampled_aggregate_rss_bytes"], checker.HEADROOM_RSS)
                    invocation = json.loads((root / "invocation.json").read_bytes())
                    self.assertEqual(source_reviews.identity(root / "catalog.json"), invocation["catalog_sha256"])
                    frozen = json.loads((root / "control/configured/plan.json").read_bytes())
                    self.assertEqual(frozen["plan"]["opencode_sha256"], invocation["opencode_sha256"])
                    listing = json.loads((root / "model-list/result.json").read_bytes())
                    self.assertEqual(listing, expected[name]["model_list"])
                    self.assertEqual(listing["models"], {"kimi-code-plan-global/k3": True, "zai-coding-plan/glm-5.3-flash": True})
                    self.assertFalse(listing["live_compatibility_verified"])
                else:
                    result = checker.report(root)
                    self.assertEqual(result, expected[name])
                    self.assertTrue(result["admitted"])
                    self.assertEqual(result["experiment"], "catalog")
                    for cell in result["cases"]:
                        if cell["mode"] == "catalog":
                            self.assertTrue(cell["completed"])
                            self.assertLessEqual(cell["process"]["sampled_aggregate_rss_bytes"], checker.HEADROOM_RSS)
                        elif name == "macos-14":
                            self.assertEqual(cell["process"]["stop_reason"], "rss_bytes")

    def test_review_freeze_binds_catalog_evidence_and_full_task(self):
        collector = load("catalog_collector", COLLECTION / "collect.py")
        frozen = json.loads((COLLECTION / "plan.json").read_bytes())
        snapshots = source_reviews.read_inputs(COLLECTION)
        plan = terminal_reviews.checked(frozen, snapshots, execution=False)
        self.assertEqual(plan["provenance"]["bindings"], collector.bindings())
        self.assertEqual((COLLECTION / "catalog.json").read_bytes(), b"{}\n")
        self.assertEqual(len(plan["cells"]), 2)
        self.assertEqual(plan["limits"]["rss_bytes"], 768 * 1024**2)
        result = terminal_reviews.report(frozen, snapshots, COLLECTION / "attempts")
        if (COLLECTION / "report.json").exists():
            self.assertEqual(result, json.loads((COLLECTION / "report.json").read_bytes()))

    def test_stopped_collection_refuses_before_client_or_git(self):
        collector = load("stopped_collector", COLLECTION / "collect.py")
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "stop.json").write_text("{}")
            with patch.object(collector, "HERE", root), patch.object(collector.runner, "collect") as run:
                with self.assertRaisesRegex(ValueError, "permanently stopped"):
                    collector.collect("unused", Path("unused"), Path("unused"))
                run.assert_not_called()


if __name__ == "__main__":
    unittest.main()
