"""Local tests for the opt-in Codex evaluator; no agent service is called."""

import importlib.util
import contextlib
import io
import json
from pathlib import Path
import stat
import sys
import tempfile
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools"))
spec = importlib.util.spec_from_file_location("agent_eval_codex", ROOT / "tools/agent-eval-codex.py")
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


class PairIsolation(unittest.TestCase):
    def prepare(self, root, settings):
        names = [f"trial-{index}" for index in range(len(settings))]
        (root / "experiment.json").write_text(json.dumps({"trials": names, "preserve_trial_order": True}))
        for name, config in zip(names, settings):
            session = root / name
            session.mkdir()
            (session / "session.json").write_text(json.dumps({"task": "unfamiliar", "repetition": 1, **config}))
        return names

    def test_models_and_modes_form_separate_pairs_in_declared_order(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            settings = [{"arm": arm, "model": model, "mode": mode}
                        for model in ("a", "b") for mode in ("single", "delegated") for arm in ("files", "fr")]
            names = self.prepare(root, settings)
            pairs = runner.selected_pairs(root, list(reversed(names)))
            self.assertEqual(len(pairs), 4)
            self.assertEqual([entry[0] for pair in pairs for entry in pair], names)

    def test_mismatched_inputs_refuse(self):
        for field in ("repository_revision", "requirement_sha256", "grader_sha256", "budgets",
                      "model_settings", "fr", "cache_state", "plan_sha256"):
            with self.subTest(field=field), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                names = self.prepare(root, [{"arm": "fr", field: "changed"}, {"arm": "files"}])
                with self.assertRaisesRegex(ValueError, f"unmatched {field}"):
                    runner.selected_pairs(root, names)

    def test_path_traversal_and_symlink_escape_refuse(self):
        with tempfile.TemporaryDirectory() as tmp, tempfile.TemporaryDirectory() as outside:
            root = Path(tmp)
            for name in ("../escape", outside, "linked"):
                (root / "experiment.json").write_text(json.dumps({"trials": [name]}))
                if name == "linked":
                    (root / name).symlink_to(outside)
                with self.assertRaisesRegex(ValueError, "path"):
                    runner.selected_pairs(root, [name])

    def test_malformed_arm_list_refuses_without_type_error(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "experiment.json").write_text(json.dumps({"trials": ["x"], "pair_arms": [{}, "fr"]}))
            with self.assertRaisesRegex(ValueError, "two distinct"):
                runner.selected_pairs(root, ["x"])


class SettingsIsolation(unittest.TestCase):
    def test_substitution_refuses_before_creating_run_artifacts(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for config in ({"model": "declared-model"}, {"model_settings": {"reasoning_effort": "high", "service_tier": "default"}}):
                with self.assertRaisesRegex(ValueError, "declared"):
                    runner.run_trial(Path("missing-codex"), ("trial", root, config), "different-model", "low", "default", 60)
                self.assertEqual(list(root.iterdir()), [])

    def test_matching_declared_settings_are_required(self):
        for config in ({}, {"model": "chosen"}, {"model_settings": {"reasoning_effort": "low", "service_tier": "default"}}):
            with self.subTest(config=config), self.assertRaisesRegex(ValueError, "declared"):
                runner.validate_settings(config, "chosen", "low", "default")
        runner.validate_settings({"model": "chosen", "model_settings": {"reasoning_effort": "low", "service_tier": "default"}},
                                 "chosen", "low", "default")

    def test_cli_does_not_supply_model_or_resource_defaults(self):
        explicit = {"--codex": "codex", "--model": "chosen", "--effort": "low",
                    "--service-tier": "default", "--timeout": "60"}
        for missing in explicit:
            argv = ["agent-eval-codex.py", "run", "missing-sessions", "--trial", "selected", "--dry-run"]
            for key, value in explicit.items():
                if key != missing:
                    argv.extend([key, value])
            with self.subTest(missing=missing), patch.object(sys, "argv", argv), contextlib.redirect_stderr(io.StringIO()) as errors:
                with self.assertRaises(SystemExit) as stopped:
                    runner.main()
                self.assertEqual(stopped.exception.code, 2)
                self.assertIn(missing, errors.getvalue())


class SettingsPreparation(unittest.TestCase):
    def sessions(self, root):
        names = ["repair-fr", "repair-files"]
        (root / "experiment.json").write_text(json.dumps({"trials": names}))
        for name, arm in zip(names, ("fr", "files")):
            session = root / name
            session.mkdir()
            (session / "session.json").write_text(json.dumps({"task": "repair", "arm": arm, "repetition": 1}))
        return names

    def test_cli_freezes_both_sessions_without_launching_codex_or_changing_prompts(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            names = self.sessions(root)
            for name in names:
                (root / name / "prompt.txt").write_text("Original task requirements")
            argv = ["agent-eval-codex.py", "freeze-settings", str(root), "--model", "chosen",
                    "--effort", "low", "--service-tier", "default"]
            for name in names:
                argv.extend(["--trial", name])
            with patch.object(sys, "argv", argv), patch.object(runner, "run_bounded") as launch, contextlib.redirect_stdout(io.StringIO()):
                runner.main()
            launch.assert_not_called()
            for name in names:
                config = runner.load(root / name / "session.json")
                runner.validate_settings(config, "chosen", "low", "default")
                self.assertEqual((root / name / "prompt.txt").read_text(), "Original task requirements")
                self.assertFalse((root / name / "codex-run.json").exists())
            # The runner independently reopens and validates the frozen pair.
            self.assertEqual(len(runner.selected_pairs(root, names)), 1)

    def test_later_used_session_prevents_changes_to_the_whole_selection(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            names = self.sessions(root)
            (root / names[-1] / "events.jsonl").write_text("first attempt")
            before = [(root / name / "session.json").read_bytes() for name in names]
            with self.assertRaisesRegex(ValueError, "not fresh"):
                runner.freeze_settings(runner.selected_pairs(root, names), "chosen", "low", "default")
            self.assertEqual(before, [(root / name / "session.json").read_bytes() for name in names])

    def test_declared_settings_cannot_be_overwritten_or_filled_in(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            names = self.sessions(root)
            for field in ("model", "model_settings"):
                pairs = runner.selected_pairs(root, names)
                pairs[-1][-1][2][field] = "previous declaration"
                before = [(root / name / "session.json").read_bytes() for name in names]
                with self.subTest(field=field), self.assertRaisesRegex(ValueError, "already declares"):
                    runner.freeze_settings(pairs, "chosen", "low", "default")
                self.assertEqual(before, [(root / name / "session.json").read_bytes() for name in names])

    def test_empty_settings_fail_before_writing_sessions(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            names = self.sessions(root)
            before = [(root / name / "session.json").read_bytes() for name in names]
            with self.assertRaisesRegex(ValueError, "nonempty"):
                runner.freeze_settings(runner.selected_pairs(root, names), "", "low", "default")
            self.assertEqual(before, [(root / name / "session.json").read_bytes() for name in names])

    def test_later_attempt_blocks_the_entire_run_before_codex_is_contacted(self):
        for artifact in runner.ATTEMPT_FILES:
            with self.subTest(artifact=artifact), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                names = self.sessions(root)
                runner.freeze_settings(runner.selected_pairs(root, names), "chosen", "low", "default")
                (root / names[-1] / artifact).write_text("prior attempt")
                argv = ["agent-eval-codex.py", "run", str(root), "--codex", "missing-codex",
                        "--model", "chosen", "--effort", "low", "--service-tier", "default",
                        "--timeout", "60", "--confirm-agent-spend"]
                for name in names:
                    argv.extend(["--trial", name])
                with patch.object(sys, "argv", argv), patch.object(runner, "codex_version") as version, \
                        patch.object(runner, "run_trial") as attempt, contextlib.redirect_stderr(io.StringIO()) as errors:
                    with self.assertRaises(SystemExit) as stopped:
                        runner.main()
                self.assertEqual(stopped.exception.code, 2)
                self.assertIn("not fresh", errors.getvalue())
                version.assert_not_called()
                attempt.assert_not_called()
                self.assertEqual((root / names[-1] / artifact).read_text(), "prior attempt")
                self.assertFalse(any((root / names[0] / filename).exists() for filename in runner.ATTEMPT_FILES))


class CodexRunner(unittest.TestCase):
    def test_version_probe_has_bounded_output(self):
        with tempfile.TemporaryDirectory() as tmp:
            fake = Path(tmp) / "codex"
            fake.write_text("#!/bin/sh\necho fake-version\n")
            fake.chmod(0o700)
            self.assertEqual(runner.codex_version(fake), "fake-version")
            fake.write_text("#!/bin/sh\nhead -c 100000 /dev/zero\n")
            with self.assertRaisesRegex(ValueError, "probe failed"):
                runner.codex_version(fake)

    def sessions(self, root):
        names = ["task-fr-r1", "task-files-r1"]
        (root / "experiment.json").write_text(json.dumps({"trials": names}))
        for name, arm in zip(names, ("fr", "files")):
            session = root / name
            (session / "project").mkdir(parents=True)
            (session / "prompt.txt").write_text(f"prompt for {arm}\n")
            (session / "session.json").write_text(
                json.dumps({"task": "task", "arm": arm, "repetition": 1})
            )
        return names

    def test_selection_requires_complete_fresh_pairs(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            names = self.sessions(root)
            with self.assertRaisesRegex(ValueError, "both fr and files"):
                runner.selected_pairs(root, names[:1])
            pairs = runner.selected_pairs(root, names)
            self.assertEqual([[entry[0] for entry in pair] for pair in pairs], [names])

    def test_selection_accepts_declared_pair_arms(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            names = ["task-direct-r1", "task-explicit-r1"]
            (root / "experiment.json").write_text(
                json.dumps({"trials": names, "pair_arms": ["direct", "explicit"]})
            )
            for name, arm in zip(names, ("direct", "explicit")):
                session = root / name
                session.mkdir()
                (session / "session.json").write_text(
                    json.dumps({"task": "task", "arm": arm, "repetition": 1})
                )
            pairs = runner.selected_pairs(root, names)
            self.assertEqual([[entry[0] for entry in pair] for pair in pairs], [names])

    def test_selection_refuses_invalid_declared_pair_arms(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "experiment.json").write_text(
                json.dumps({"trials": ["task"], "pair_arms": ["direct", "direct"]})
            )
            with self.assertRaisesRegex(ValueError, "two distinct"):
                runner.selected_pairs(root, ["task"])

    def test_command_pins_fresh_low_cost_configuration(self):
        session = Path("/tmp/session")
        command = runner.codex_command(Path("codex"), session, "gpt-5.6-luna", "low", "default")
        for flag in ("--ephemeral", "--ignore-user-config", "--ignore-rules", "--json"):
            self.assertIn(flag, command)
        self.assertIn('model_reasoning_effort="low"', command)
        self.assertIn("--skip-git-repo-check", command)
        self.assertIn('service_tier="default"', command)
        self.assertEqual(command[-1], "-")

    def test_fake_run_retains_jsonl_and_refuses_reuse(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            name = self.sessions(root)[0]
            entry = (name, root / name, {"task": "task", "arm": "fr", "repetition": 1,
                "model": "gpt-5.6-luna", "model_settings": {"reasoning_effort": "low", "service_tier": "default"}})
            fake = root / "fake-codex"
            fake.write_text(
                "#!/bin/sh\n"
                "if [ \"$1\" = \"--version\" ]; then echo fake; exit 0; fi\n"
                "cat >/dev/null\n"
                "echo '{\"type\":\"thread.started\",\"thread_id\":\"fresh\"}'\n"
            )
            fake.chmod(fake.stat().st_mode | stat.S_IXUSR)
            record = runner.run_trial(fake, entry, "gpt-5.6-luna", "low", "default", 60)
            self.assertEqual(record["exit_code"], 0)
            events = (root / name / "codex-events.jsonl").read_text()
            self.assertIn("thread.started", events)
            self.assertEqual(record["events_sha256"], runner.digest(root / name / "codex-events.jsonl"))
            with self.assertRaisesRegex(ValueError, "not fresh"):
                runner.run_trial(fake, entry, "gpt-5.6-luna", "low", "default", 60)

    def test_failed_launch_is_retained_as_an_attempt(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            name = self.sessions(root)[0]
            entry = (name, root / name, {"task": "task", "arm": "fr", "repetition": 1,
                "model": "gpt-5.6-luna", "model_settings": {"reasoning_effort": "low", "service_tier": "default"}})
            record = runner.run_trial(root / "missing-codex", entry, "gpt-5.6-luna", "low", "default", 60)
            self.assertIsNone(record["exit_code"])
            self.assertTrue(record["launch_error"])
            self.assertTrue((root / name / "codex-run.json").is_file())

    def test_final_response_alone_prevents_overwriting_a_previous_attempt(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "codex-final.txt").write_text("prior answer")
            config = {"model": "chosen", "model_settings": {"reasoning_effort": "low", "service_tier": "default"}}
            with patch.object(runner, "run_bounded") as launch, self.assertRaisesRegex(ValueError, "not fresh"):
                runner.run_trial(Path("missing-codex"), ("trial", root, config), "chosen", "low", "default", 60)
            launch.assert_not_called()
            self.assertEqual([path.name for path in root.iterdir()], ["codex-final.txt"])
            self.assertEqual((root / "codex-final.txt").read_text(), "prior answer")


if __name__ == "__main__":
    unittest.main()
