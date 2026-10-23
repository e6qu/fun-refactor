"""Fair native inputs, explicit loopback settings and retained client failures."""
import copy
import json
from pathlib import Path
import subprocess
import sys
import tomllib
import unittest
from unittest.mock import MagicMock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval import comparison_clients as clients, comparison_native as native
from agent_eval.comparison_plan import ROOT, prepare, verify


class NativeInputs(unittest.TestCase):
    def test_specialized_server_cannot_change_ordinary_permissions_or_model(self):
        for host in clients.HOSTS:
            ordinary = clients.native_config(host, "files", "http://127.0.0.1:1234")
            self.assertEqual(ordinary, clients.native_config(host, "fr", "http://127.0.0.1:1234"))
            graph = clients.native_config(host, "codegraph", "http://127.0.0.1:1234")
            if host == "codex":
                server = graph.pop("mcp_servers")
                ordinary.pop("mcp_servers")
                self.assertEqual(server["codegraph"]["enabled_tools"], ["codegraph_explore"])
            else:
                server = graph.pop("mcp")
                ordinary.pop("mcp")
                if host == "opencode":
                    self.assertEqual(graph["permission"].pop("codegraph_codegraph_explore"), "allow")
                self.assertTrue(server)
            self.assertEqual(graph, ordinary, host)
            for setup in ("files", "fr"):
                config = clients.native_config(host, setup, "http://127.0.0.1:1234")
                self.assertNotIn("codegraph", json.dumps(clients.launch_command(host, setup, config, clients.CONTROL_PROMPT)))

    def test_rehearsal_cannot_be_pointed_to_a_live_provider(self):
        for endpoint in ("https://api.openai.com", "https://api.anthropic.com", "http://localhost:12",
                         "http://127.0.0.1:1/v1", "http://127.0.0.1:0", "http://127.0.0.1:65536"):
            for host in clients.HOSTS:
                with self.subTest(host=host, endpoint=endpoint), self.assertRaises(ValueError):
                    clients.native_config(host, "files", endpoint)

    def test_native_launches_keep_project_skills_and_avoid_title_inference(self):
        for setup in clients.SETUPS:
            claude = clients.launch_command("claude", setup, clients.native_config("claude", setup, "http://127.0.0.1:1234"), clients.CONTROL_PROMPT)
            self.assertEqual(claude[claude.index("--setting-sources") + 1], "project")
            opencode = clients.launch_command("opencode", setup, clients.native_config("opencode", setup, "http://127.0.0.1:1234"), clients.CONTROL_PROMPT)
            self.assertEqual(opencode[opencode.index("--title") + 1], "Scripted native tool control")
            config = clients.native_config("opencode", setup, "http://127.0.0.1:1234")
            self.assertEqual(config["permission"]["bash"], "allow")
            self.assertEqual(native.call_for("opencode", "files")[0], "bash")

    def test_editing_one_returned_config_cannot_change_later_worker_limits(self):
        graph = clients.native_config("claude", "codegraph", "http://127.0.0.1:1234")
        graph["mcp"]["mcpServers"]["codegraph"]["env"]["CODEGRAPH_PARSE_WORKERS"] = "128"
        fresh = clients.native_config("codex", "codegraph", "http://127.0.0.1:1234")
        self.assertEqual(fresh["mcp_servers"]["codegraph"]["env"]["CODEGRAPH_PARSE_WORKERS"], "1")

    def test_codex_native_toml_retains_every_explicit_setting(self):
        for setup in clients.SETUPS:
            config = clients.native_config("codex", setup, "http://127.0.0.1:4321")
            serialized = "\n".join(k + " = " + native.toml(v) for k, v in config.items())
            self.assertEqual(tomllib.loads(serialized), config)
            self.assertEqual(config["model_provider"], "rehearsal")
            self.assertEqual(config["model_providers"]["rehearsal"]["base_url"], "http://127.0.0.1:4321/v1")

    def test_all_attempts_receive_the_same_task_and_grader(self):
        plan = prepare(ROOT)
        frozen = plan["native_rehearsal_inputs"]
        self.assertFalse(frozen["live_calls_enabled"])
        for task in frozen["tasks"]:
            requirement = plan["tasks"][task]["requirement"]
            prompts = frozen["tasks"][task]["prompts"]
            self.assertEqual(set(prompts), {"discover", "deliver"})
            for prompt in prompts.values():
                self.assertIn(requirement, prompt)
                self.assertNotIn("src/lib.rs", prompt)
                self.assertNotIn("regex-syntax/src", prompt)
            self.assertEqual(len([a for a in plan["attempts"] if a["task"] == task]), 9)
            self.assertIn("--candidate", frozen["tasks"][task]["grader"])
            self.assertIn("independent patch receiver", frozen["tasks"][task]["controls"])
        changed = copy.deepcopy(plan)
        changed["native_rehearsal_inputs"]["applications"]["codex"]["files"]["model"] = "replacement"
        with self.assertRaises(ValueError):
            verify(changed, ROOT)
        changed = copy.deepcopy(plan)
        changed["native_rehearsal_inputs"]["tasks"]["unicode-dice"]["prompts"]["deliver"] += "new instructions"
        with self.assertRaises(ValueError):
            verify(changed, ROOT)

    def test_native_client_error_keeps_both_streams_and_exit_status(self):
        result = subprocess.CompletedProcess(["claude"], 1, b"stdout diagnosis", b"stderr diagnosis")
        with patch.object(native.subprocess, "run", return_value=result), \
             patch.object(native, "trace"), patch.object(native, "DIAGNOSTICS", MagicMock()):
            with self.assertRaises(ValueError) as failed:
                native.command(["claude"], {})
        for text in ("stdout diagnosis", "stderr diagnosis", "1"):
            self.assertIn(text, str(failed.exception))


if __name__ == "__main__":
    unittest.main()
