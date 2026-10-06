"""Check configured-client access without reading credentials or calling a model."""
import copy
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval import terminal_review_collection as collection, terminal_review_runner as runner
from agent_eval import terminal_reviews as review, test_terminal_reviews as fixtures
from agent_eval.study import digest


def configured():
    frozen, snapshots = fixtures.frozen(packet=True)
    task = frozen["plan"]["tasks"][0]
    questions = [{"id": task["id"], "question": task["question"], "files": snapshots[task["id"]],
                  "selections": [{k: s[k] for k in ("path", "sha256", "start", "end")} for s in task["packet"]["spans"]]}]
    models = [{k: v for k, v in frozen["plan"]["models"][0].items() if k != "baseURL"} | {"configured": True}]
    return review.freeze(questions, models, Path(sys.executable), Path(sys.executable), {"kind": "configured control"})


class Configured(unittest.TestCase):
    def test_hosted_identity_counterexample_matches_reviewed_source(self):
        import base64
        import hashlib
        from agent_eval import change_controls, source_reviews
        base = Path(__file__).resolve().parents[2] / "tests/agent-eval/opencode"
        root = base / "reviews/2026-10-06-configured"
        pack = base / "candidates"
        evidence = json.loads((root / "counterexample.json").read_bytes())
        frozen = json.loads((root / "frozen/plan.json").read_bytes())
        self.assertEqual(evidence["review_plan_sha256"], frozen["sha256"])
        self.assertEqual(evidence["review_cell"], "packaging-one-shot-duplicates-0")
        self.assertEqual(evidence["run"]["job_conclusion"], "success")
        for name, sha in evidence["candidate_inputs"].items():
            self.assertEqual(hashlib.sha256((pack / name).read_bytes()).hexdigest(), sha)
        task = next(t for t in json.loads((pack / "manifest.json").read_bytes())["tasks"]
                    if t["id"] == "packaging-prerelease")
        files, variants, public = change_controls.definitions(pack, task)
        rows = evidence["results"]
        self.assertEqual({r["id"] for r in rows}, {"packaging-prerelease/reference",
                         "packaging-prerelease/reuse-equal-output-object"})
        change_controls.verify(rows)
        current = json.loads((pack / task["grader"]).read_bytes())
        for row in rows:
            variant = next(v for v in variants if row["id"] == task["id"] + "/" + v["id"])
            self.assertEqual(row["submission_sha256"], digest(change_controls.apply(files, variant)))
            self.assertEqual(row["grade"]["grader_sha256"], evidence["candidate_inputs"][task["grader"]])
            self.assertEqual(row["grade"]["image"], current["image"])
            self.assertEqual(row["public_grade"]["candidate"], row["grade"]["candidate"])
            self.assertEqual(row["public_grade"]["grader_sha256"], digest(public))
            self.assertEqual([c["id"] for c in row["grade"]["cases"]], [c["id"] for c in current["cases"]])
            if "baseline_grade" in row:
                baseline = json.loads((pack / variant["baseline"]["grader"]).read_bytes())
                self.assertEqual(row["baseline_grade"]["image"], current["image"])
                self.assertEqual([c["id"] for c in row["baseline_grade"]["cases"]], [c["id"] for c in baseline["cases"]])
                self.assertEqual(row["expected_failed_cases"], ["duplicate-final-identity"])
                snapshots = source_reviews.read_inputs(root / "frozen")
                reviewed = base64.b64decode(snapshots["packaging-one-shot-duplicates"]["review/grader.py"]["data"])
                self.assertEqual(baseline["command"][-1].encode(), reviewed)
        report = json.loads((root / "report.json").read_bytes())
        finding = report["attempts"][2]["audit"]["review"]["findings"][0]
        self.assertFalse(finding["verified_counterexample"])

    def test_retained_reviews_replay_and_resource_stop_stays_closed(self):
        import hashlib
        from agent_eval import source_reviews
        root = Path(__file__).resolve().parents[2] / "tests/agent-eval/opencode/reviews/2026-10-06-configured"
        frozen = json.loads((root / "frozen/plan.json").read_bytes())
        result = review.report(frozen, source_reviews.read_inputs(root / "frozen"), root / "attempts")
        self.assertEqual(result, json.loads((root / "report.json").read_bytes()))
        self.assertEqual([result[k] for k in ("completed", "failed", "not_started")], [3, 1, 2])
        stop = json.loads((root / "stop.json").read_bytes())
        self.assertEqual(stop["plan_sha256"], frozen["sha256"])
        self.assertEqual(stop["reason"], "local_resource_limit")
        self.assertFalse(stop["resume_allowed"])
        self.assertEqual(stop["unstarted_cells"], [row["cell"]["id"] for row in result["attempts"] if row["status"] == "not_started"])
        failed = next(row for row in result["attempts"] if row["cell"]["id"] == stop["cell"])
        self.assertEqual(failed["status"], "failed")
        process = failed["process"]
        self.assertEqual(process["stop_reason"], stop["stop_reason"])
        self.assertEqual(process["stop_reason"], "rss_bytes")
        self.assertEqual(process["exit_code"], 125)
        raw = (root / "attempts" / stop["cell"] / "process.json").read_bytes()
        self.assertEqual(hashlib.sha256(raw).hexdigest(), stop["process_sha256"])
        self.assertEqual(sum(row.get("observed", {}).get("fr_calls", 0) for row in result["attempts"]), 0)
        self.assertFalse(result["efficiency_comparison"])

    def test_admission_uses_no_hosted_key_and_keeps_client_configuration(self):
        frozen, snapshots = configured()
        model = frozen["plan"]["models"][0]
        self.assertEqual(frozen["plan"]["schema"], review.CONFIGURED_SCHEMA)
        with tempfile.TemporaryDirectory() as temporary, patch.dict(os.environ,
                {"HOME": temporary, "XDG_DATA_HOME": temporary + "/client-data", "FR_REVIEW_API_KEYS": "invalid-map"}, clear=True):
            root = Path(temporary)
            with patch.object(Path, "read_bytes", side_effect=AssertionError("must not read credential files")):
                self.assertEqual(collection.credentials([model]), {})
                env = runner.environment(root, model, root / "server.json")
            self.assertEqual(env["XDG_DATA_HOME"], temporary + "/client-data")
            settings = json.loads(env["OPENCODE_CONFIG_CONTENT"])
            self.assertEqual(settings["enabled_providers"], [model["providerID"]])
            self.assertNotIn("options", settings["provider"][model["providerID"]])
            self.assertEqual(settings["permission"]["*"], "deny")
            self.assertEqual(json.loads(env["OPENCODE_PERMISSION"]), settings["permission"])
            self.assertEqual(settings["plugin"], [])
            self.assertEqual(collection.preflight(frozen, snapshots, root / "attempts",
                Path(sys.executable), Path(sys.executable))["status"], "ready")

    def test_auth_mode_cannot_change_after_freezing(self):
        frozen, snapshots = configured()
        for field, value in (("schema", review.SCHEMA), ("provider_transport", "openai-compatible")):
            changed = copy.deepcopy(frozen)
            changed["plan"][field] = value
            changed["sha256"] = digest(changed["plan"])
            with self.assertRaises(ValueError):
                review.checked(changed, snapshots)
        model = frozen["plan"]["models"][0]
        for field in ("apiKey", "baseURL", "auth_path"):
            with self.assertRaises(ValueError):
                review.profile({**model, field: "must-not-be-retained"})

    def test_legacy_frozen_plan_still_replays_unchanged(self):
        frozen, snapshots = fixtures.frozen()
        self.assertEqual(review.checked(frozen, snapshots)["schema"], review.SCHEMA)
        self.assertEqual(frozen["plan"]["provider_transport"], "openai-compatible")


if __name__ == "__main__":
    unittest.main()
