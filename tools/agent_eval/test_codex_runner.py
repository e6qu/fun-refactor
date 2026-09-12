"""Local tests for the opt-in Codex evaluator; no agent service is called."""

import importlib.util
import json
from pathlib import Path
import stat
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("agent_eval_codex", ROOT / "tools/agent-eval-codex.py")
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


class CodexRunner(unittest.TestCase):
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
            entry = (name, root / name, {"task": "task", "arm": "fr", "repetition": 1})
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
            entry = (name, root / name, {"task": "task", "arm": "fr", "repetition": 1})
            record = runner.run_trial(root / "missing-codex", entry, "gpt-5.6-luna", "low", "default", 60)
            self.assertIsNone(record["exit_code"])
            self.assertTrue(record["launch_error"])
            self.assertTrue((root / name / "codex-run.json").is_file())


if __name__ == "__main__":
    unittest.main()
