"""Check admission, credential isolation and ordered collection without model calls."""
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


def remote_models():
    return [{"providerID": name, "modelID": "review", "baseURL": "https://" + name + ".example/v1",
             "context": 8192, "output": 1024} for name in ("alpha", "beta")]


def plan(count=1, remote=False):
    frozen, snapshots = fixtures.frozen(count=count)
    if remote:
        questions = [{"id": task["id"], "question": task["question"], "files": snapshots[task["id"]],
                      "selections": [{k: s[k] for k in ("path", "sha256", "start", "end")}
                                     for s in task["packet"]["spans"]]} for task in frozen["plan"]["tasks"]]
        frozen, snapshots = review.design(questions, remote_models(),
            {k: frozen["plan"][k] for k in ("runtime", "binary_sha256", "opencode_sha256")}, {"kind": "test"})
    return frozen, snapshots


class Collection(unittest.TestCase):
    def test_keys_are_selected_per_provider_and_never_copied_to_config(self):
        keys = {"alpha": "alpha-secret", "beta": "beta-secret"}
        with patch.dict(os.environ, {"GITHUB_ACTIONS": "true", "FR_REVIEW_API_KEYS": json.dumps(keys)}, clear=True):
            for model in remote_models():
                env = runner.environment(Path("/capture"), model, Path("/capture/server.json"))
                self.assertEqual(env["FR_REVIEW_API_KEY"], keys[model["providerID"]])
                self.assertNotIn("FR_REVIEW_API_KEYS", env)
                self.assertNotIn("secret", env["OPENCODE_CONFIG_CONTENT"])
                self.assertNotIn(keys["beta" if model["providerID"] == "alpha" else "alpha"], env.values())

    def test_legacy_single_key_cannot_silently_serve_two_providers(self):
        with patch.dict(os.environ, {"GITHUB_ACTIONS": "true", "FR_REVIEW_API_KEY": "legacy"}, clear=True):
            self.assertEqual(collection.credentials(remote_models()[:1]), {"alpha": "legacy"})
            with self.assertRaisesRegex(ValueError, "every remote provider"):
                collection.credentials(remote_models())

    def test_bad_credential_maps_refuse_without_disclosing_values(self):
        values = ('{"alpha":"do-not-print",', '{"alpha":"do-not-print","alpha":"other"}',
                  '[]', '{"alpha":false}', '{"alpha":"line\\nbreak"}', '{"alpha":"do-not-print"}',
                  '{"alpha":"' + "x" * 16384 + '"}')
        for value in values:
            with self.subTest(value=value[:12]), patch.dict(os.environ,
                    {"GITHUB_ACTIONS": "true", "FR_REVIEW_API_KEYS": value}, clear=True):
                with self.assertRaises(ValueError) as error:
                    collection.credentials(remote_models())
                self.assertNotIn("do-not-print", str(error.exception))

    def test_ambiguous_keys_endpoints_and_local_live_calls_refuse(self):
        settings = {"GITHUB_ACTIONS": "true", "FR_REVIEW_API_KEYS": '{"alpha":"key","beta":"key2"}'}
        for changes, models in (({"FR_REVIEW_API_KEY": "extra"}, remote_models()),
                                ({"GITHUB_ACTIONS": "false"}, remote_models()),
                                ({}, [remote_models()[0], {**remote_models()[0], "baseURL": "https://other.example/v1"}])):
            with patch.dict(os.environ, {**settings, **changes}, clear=True), self.assertRaises(ValueError):
                collection.credentials(models)

    def test_missing_second_key_refuses_before_any_attempt(self):
        frozen, snapshots = plan(remote=True)
        with tempfile.TemporaryDirectory() as temporary, patch.dict(os.environ,
                {"GITHUB_ACTIONS": "true", "FR_REVIEW_API_KEYS": '{"alpha":"key"}'}, clear=True):
            output = Path(temporary) / "attempts"
            with patch.object(runner.bounded_host, "run") as launch:
                for operation in (
                    lambda: collection.collect_all(frozen, snapshots, output, Path(sys.executable),
                        Path(sys.executable), Path(temporary), frozen["sha256"]),
                    lambda: runner.collect(frozen, snapshots, frozen["plan"]["cells"][0]["id"], output,
                        Path(sys.executable), Path(sys.executable), Path(temporary)),
                ):
                    with self.assertRaisesRegex(ValueError, "every remote provider"):
                        operation()
                launch.assert_not_called()
            self.assertFalse(output.exists())

    def test_preflight_is_read_only_and_reports_no_live_verification(self):
        frozen, snapshots = plan()
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "attempts"
            result = collection.preflight(frozen, snapshots, output, Path(sys.executable), Path(sys.executable))
            self.assertEqual(result["status"], "ready")
            self.assertEqual(result["maximum_remaining_seconds"], 120)
            self.assertFalse(result["live_compatibility_verified"])
            self.assertFalse(output.exists())

    def test_changed_plan_runtime_binary_and_oversized_collection_refuse(self):
        frozen, snapshots = plan()
        changed = copy.deepcopy(frozen)
        changed["plan"]["runtime"]["terminal_review_collection.py"] = "0" * 64
        changed["sha256"] = digest(changed["plan"])
        large, large_snapshots = plan(count=7)
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "attempts"
            for f, source, binary in ((changed, snapshots, Path(sys.executable)),
                                      (large, large_snapshots, Path(sys.executable)),
                                      (frozen, snapshots, Path(__file__))):
                with self.assertRaises(ValueError):
                    collection.preflight(f, source, output, binary, Path(sys.executable))
            with self.assertRaisesRegex(ValueError, "reviewed identity"):
                collection.collect_all(frozen, snapshots, output, Path(sys.executable),
                    Path(sys.executable), Path(temporary), "0" * 64)
            self.assertFalse(output.exists())

    def test_serial_collection_resumes_only_unstarted_cells_and_stops_on_two_failures(self):
        for outcomes in ((False, True, False), (True, False, True), (True, True, False)):
            with self.subTest(outcomes=outcomes), tempfile.TemporaryDirectory() as temporary:
                frozen, snapshots = plan(count=3)
                output = Path(temporary) / "attempts"
                calls = []
                def capture(frozen, snapshots, cell_id, output, binary, opencode, inputs):
                    cell = next(c for c in frozen["plan"]["cells"] if c["id"] == cell_id)
                    calls.append(cell_id)
                    folder = output / cell_id
                    folder.mkdir(parents=True)
                    process = fixtures.write_capture(folder, frozen["plan"], snapshots, cell)
                    if outcomes[frozen["plan"]["cells"].index(cell)]:
                        process.update(exit_code=17, process_exit_code=17)
                    return runner.seal(frozen, snapshots, cell, folder, process)
                first = frozen["plan"]["cells"][0]["id"]
                capture(frozen, snapshots, first, output, None, None, None)
                with patch.object(runner, "collect", side_effect=capture):
                    result = collection.collect_all(frozen, snapshots, output, Path(sys.executable),
                        Path(sys.executable), Path(temporary), frozen["sha256"])
                    repeated = collection.collect_all(frozen, snapshots, output, Path(sys.executable),
                        Path(sys.executable), Path(temporary), frozen["sha256"])
                stopped = outcomes[:2] == (True, True)
                self.assertEqual(result["status"], "stopped" if stopped else "finished")
                self.assertEqual(len(calls), 2 if stopped else 3)
                self.assertEqual(calls, [c["id"] for c in frozen["plan"]["cells"][:len(calls)]])
                self.assertEqual(result, repeated)

    def test_incomplete_attempt_is_not_retried(self):
        frozen, snapshots = plan()
        with tempfile.TemporaryDirectory() as temporary, patch.object(runner, "collect") as collect:
            output = Path(temporary)
            (output / frozen["plan"]["cells"][0]["id"]).mkdir()
            with self.assertRaises(ValueError):
                collection.collect_all(frozen, snapshots, output, Path(sys.executable),
                    Path(sys.executable), output, frozen["sha256"])
            collect.assert_not_called()


if __name__ == "__main__":
    unittest.main()
