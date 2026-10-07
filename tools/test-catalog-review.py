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
    def test_low_effort_failures_and_rejected_partial_claim_replay(self):
        from agent_eval import change_controls
        from agent_eval.study import digest
        here = COLLECTION.with_name("2026-10-07-packaging-low")
        collector = load("low_collector", here / "collect.py")
        frozen = json.loads((here / "plan.json").read_bytes())
        snapshots = source_reviews.read_inputs(here)
        result = terminal_reviews.report(frozen, snapshots, here / "attempts")
        self.assertEqual(result, json.loads((here / "report.json").read_bytes()))
        self.assertEqual(frozen["plan"]["provenance"]["bindings"], collector.bindings())
        self.assertEqual((result["completed"], result["failed"], result["not_started"]), (0, 2, 0))
        self.assertFalse(json.loads((here / "stop.json").read_bytes())["resume_allowed"])
        with self.assertRaisesRegex(ValueError, "permanently stopped"):
            collector.collect("unused", Path("unused"), Path("unused"))
        design = json.loads((here / "design.json").read_bytes())
        previous = json.loads((COLLECTION.with_name("2026-10-06-packaging-inputs") / "design.json").read_bytes())
        self.assertEqual(design, {**previous, "models": [{**m, "variant": "low"} for m in previous["models"]]})
        for attempt in result["attempts"]:
            folder = here / "attempts" / attempt["cell"]["id"]
            terminal = json.loads((folder / "terminal.json").read_bytes())["info"]
            self.assertEqual(terminal["error"]["name"], "StructuredOutputError")
            self.assertEqual(terminal["finish"], "length")
            self.assertEqual(terminal["tokens"]["output"] + terminal["tokens"]["reasoning"], 2048)
            self.assertEqual(json.loads((folder / "request.json").read_bytes())["variant"], "low")
            self.assertIsNone(attempt["process"]["stop_reason"])
            self.assertEqual(attempt["observed"]["fr_calls"], 0)
            self.assertFalse(result["claims_verified"])
        verification = json.loads((here / "claim-verification.json").read_bytes())
        self.assertEqual(verification["review_plan_sha256"], frozen["sha256"])
        self.assertEqual(verification["decision"], "rejected")
        archive = here / "github-controls.json.gz"
        self.assertEqual(source_reviews.identity(archive), verification["archive_sha256"])
        with gzip.open(archive, "rb") as stream:
            raw = stream.read(1024**2 + 1)
        self.assertLessEqual(len(raw), 1024**2)
        self.assertEqual(hashlib.sha256(raw).hexdigest(), verification["controls_sha256"])
        controls = json.loads(raw)
        change_controls.verify(controls["results"])
        reference = next(r for r in controls["results"] if r["id"] == "packaging-prerelease/reference")
        pack = REPO / "tests/agent-eval/opencode/candidates"
        task = next(t for t in json.loads((pack / "manifest.json").read_bytes())["tasks"] if t["id"] == "packaging-prerelease")
        files, definitions, _ = change_controls.definitions(pack, task)
        candidate = change_controls.apply(files, next(c for c in definitions if c["id"] == "reference"))
        self.assertEqual(digest(candidate), reference["submission_sha256"])
        reviewed = snapshots["packaging-whole-task"]
        self.assertEqual(candidate, {p: reviewed[p] for p in candidate})
        grader = json.loads((pack / task["grader"]).read_bytes())
        self.assertEqual(source_reviews.identity(pack / task["grader"]), reference["grade"]["grader_sha256"])
        self.assertEqual(base64.b64decode(reviewed["review/grader.py"]["data"]).decode(), grader["command"][-1])
        self.assertIn("check('>1.0,<2', ['1.0.post1', '1.1a1'], ['1.1a1'])", grader["command"][-1])
        self.assertTrue(next(c for c in reference["grade"]["cases"] if c["id"] == "bounds-and-exclusions")["passed"])

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
        self.replay_admission_evidence(EVIDENCE)

    def test_provider_preserving_admission_evidence_replays(self):
        self.replay_admission_evidence(EVIDENCE.with_name("2026-10-07-providers"))

    def replay_admission_evidence(self, evidence):
        EVIDENCE = evidence
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
                    measurement = root / "client-memory" if (root / "client-memory").exists() else root
                    result = checker.report(measurement)
                    self.assertEqual(result, {k: v for k, v in expected[name].items() if k != "resolution"})
                    if "resolution" in expected[name]:
                        self.replay_resolution(root / "client-catalog", expected[name]["resolution"], EVIDENCE)
                    self.assertTrue(result["admitted"])
                    self.assertEqual(result["experiment"], "catalog")
                    for cell in result["cases"]:
                        if cell["mode"] == "catalog":
                            self.assertTrue(cell["completed"])
                            self.assertLessEqual(cell["process"]["sampled_aggregate_rss_bytes"], checker.HEADROOM_RSS)
                        elif name == "macos-14":
                            self.assertEqual(cell["process"]["stop_reason"], "rss_bytes")

    def replay_resolution(self, root, expected, evidence):
        from agent_eval import structured_probe
        probe = load("retained_resolution", evidence / "check-client-catalog.py")
        result = json.loads((root / "result.json").read_bytes())
        self.assertEqual(result, expected)
        public = json.loads((root / "providers/catalog.json").read_bytes())
        self.assertEqual([row["mode"] for row in result["cases"]], ["empty", "providers"])
        for row in result["cases"]:
            folder = root / row["mode"]
            self.assertEqual(row["catalog_sha256"], source_reviews.identity(folder / "catalog.json"))
            self.assertEqual(row["models_sha256"], source_reviews.identity(folder / "models.stdout"))
            self.assertEqual(row["process"], json.loads((folder / "process.json").read_bytes()))
            structured_probe.checked_process(row["process"])
            models = probe.models((folder / "models.stdout").read_bytes())
            for provider, metadata in public.items():
                for model_id in metadata["models"]:
                    model = models[provider + "/" + model_id]
                    if row["mode"] == "empty":
                        self.assertFalse(model["api"].get("url"))
                    else:
                        self.assertEqual(model["api"], {"id": model_id, "url": metadata["api"], "npm": metadata["npm"]})
                        self.assertTrue(model["capabilities"]["reasoning"] and model["capabilities"]["toolcall"])
                        self.assertEqual(model["capabilities"]["interleaved"], {"field": "reasoning_content"})
                    self.assertEqual((model["limit"]["context"], model["limit"]["output"]), (32768, 2048))

    def test_provider_reviews_preserve_output_exhaustion_without_acceptance(self):
        here = COLLECTION.with_name("2026-10-07-packaging-providers")
        collector = load("provider_collector", here / "collect.py")
        frozen = json.loads((here / "plan.json").read_bytes())
        result = terminal_reviews.report(frozen, source_reviews.read_inputs(here), here / "attempts")
        self.assertEqual(frozen["plan"]["provenance"]["bindings"], collector.bindings())
        self.assertEqual(result, json.loads((here / "report.json").read_bytes()))
        self.assertEqual((result["completed"], result["failed"], result["not_started"]), (0, 2, 0))
        self.assertFalse(result["claims_verified"])
        self.assertFalse(json.loads((here / "stop.json").read_bytes())["resume_allowed"])
        for attempt in result["attempts"]:
            folder = here / "attempts" / attempt["cell"]["id"]
            terminal = json.loads((folder / "terminal.json").read_bytes())
            info = terminal["info"]
            self.assertEqual(info["error"]["name"], "StructuredOutputError")
            self.assertEqual(info["finish"], "length")
            self.assertEqual(info["tokens"]["reasoning"] + info["tokens"]["output"], 2048)
            self.assertEqual((folder / "tools.jsonl").read_bytes(), b"")
            self.assertIsNone(attempt["process"]["stop_reason"])

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
