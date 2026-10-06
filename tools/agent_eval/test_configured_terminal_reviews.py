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
