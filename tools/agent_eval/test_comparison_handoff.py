"""Reject stale repairs and lost handoffs before any resumed source write."""
from contextlib import chdir
import importlib.util
import json
from pathlib import Path
import tempfile
import sys
import unittest
from unittest.mock import MagicMock, patch
import urllib.error
import urllib.request

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from agent_eval import comparison_handoff_actions as actions
from agent_eval import comparison_native as native
from agent_eval.comparison_clients import CONTROL_PROMPT, FR_SOURCE_BYTES, MODEL, inputs, launch_command, native_config

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("handoff_controller", ROOT / "tools/agent-comparison-handoff.py")
controller = importlib.util.module_from_spec(spec)
spec.loader.exec_module(controller)


class Handoff(unittest.TestCase):
    def test_recipes_preserve_the_pinned_project_and_use_shared_phase_prompts(self):
        frozen = inputs(ROOT)
        for task in controller.TASKS:
            with self.subTest(task=task), tempfile.TemporaryDirectory() as temporary:
                project = Path(temporary) / "project"
                controller.BASE.unpack(project, task)
                before = {name: (project / name).read_bytes() for name in controller.BASE.edit_paths(task)}
                recipe = controller.scenario(task, project)
                self.assertEqual(before, {name: (project / name).read_bytes() for name in before})
                self.assertNotEqual(recipe["original_edits"], recipe["resumed_edits"])
                self.assertIn(recipe["preserved_name"], recipe["resumed_edits"][0]["old"])
                self.assertTrue(all((project / name).stat().st_size <= FR_SOURCE_BYTES for name in before))
                self.assertEqual(recipe["prompts"], frozen["tasks"][task]["prompts"])
                for host in ("claude", "opencode"):
                    config = native_config(host, "files", "http://127.0.0.1:1234")
                    for prompt in recipe["prompts"].values():
                        self.assertIn(prompt, launch_command(host, "files", config, prompt))
                        self.assertNotEqual(prompt, CONTROL_PROMPT)

    def test_intervening_edits_reject_the_old_patch_but_survive_the_new_patch(self):
        for task in controller.TASKS:
            with self.subTest(task=task), tempfile.TemporaryDirectory() as temporary:
                project = Path(temporary) / "project"
                controller.BASE.unpack(project, task)
                controller.BASE.initialize(project)
                recipe = controller.scenario(task, project)
                with chdir(project):
                    old = actions.patch(recipe["original_edits"]).encode()
                    edit = recipe["intervention"]
                    source = Path(edit["path"])
                    source.write_text(source.read_text().replace(edit["old"], edit["new"], 1))
                    before = actions.snapshot()
                    refused = controller.BASE.git(project, "apply", "--check", "-", data=old, check=False)
                    self.assertEqual(refused.returncode, 1)
                    self.assertIn(b"patch failed:", refused.stderr)
                    self.assertEqual(before, actions.snapshot())
                    fresh = actions.patch(recipe["resumed_edits"]).encode()
                    controller.BASE.git(project, "apply", "--check", "-", data=fresh)
                    controller.BASE.git(project, "apply", "-", data=fresh)
                    self.assertIn(recipe["preserved_name"], source.read_text())
                    controller.BASE.git(project, "apply", "--reverse", "-", data=fresh)
                    self.assertEqual(before, actions.snapshot())

    def test_changed_handoff_refuses_delivery_without_writing_source(self):
        with tempfile.TemporaryDirectory() as temporary:
            project, artifacts = Path(temporary) / "project", Path(temporary) / "artifacts"
            controller.BASE.unpack(project, "unicode-dice")
            controller.BASE.initialize(project)
            recipe = controller.scenario("unicode-dice", project)
            with chdir(project), patch.object(actions, "ARTIFACTS", artifacts):
                actions.discover(recipe, "files")
                actions.intervene(recipe)
                Path("HANDOFF.md").write_text("A different diagnosis")
                before = actions.snapshot()
                with self.assertRaisesRegex(ValueError, "handoff changed"):
                    actions.deliver(recipe, "files")
                self.assertEqual(before, actions.snapshot())
                self.assertFalse(Path("change.patch").exists())


class ScriptedProvider(unittest.TestCase):
    def exercise(self, last_result):
        steps = [{"kind": "mcp", "name": "mcp__codegraph__codegraph_explore", "arguments": {}, "expected": ["first result"]},
                 {"kind": "command", "name": "Bash", "arguments": {"command": "true"}, "expected": ["second result"]}]
        messages = []
        with patch.object(native, "trace"), patch.object(native, "DIAGNOSTICS", MagicMock()), \
             native.provider("claude", "codegraph", steps) as (endpoint, requests, errors):
            def request():
                data = {"model": MODEL, "tools": [{"name": step["name"]} for step in steps], "messages": messages}
                with urllib.request.urlopen(urllib.request.Request(endpoint + "/v1/messages", data=json.dumps(data).encode(),
                                             headers={"Content-Type": "application/json"}), timeout=5) as response:
                    return response.read().decode()
            self.assertIn("call_control_0", request())
            messages.append({"role": "user", "content": [{"type": "tool_result", "content": "first result"}]})
            self.assertIn("call_control_1", request())
            messages.append({"role": "user", "content": [{"type": "tool_result", "content": last_result}]})
            if last_result == "second result":
                self.assertIn("end_turn", request())
                self.assertFalse(errors)
            else:
                with self.assertRaises(urllib.error.HTTPError) as refused:
                    request()
                self.assertEqual(refused.exception.code, 400)
                self.assertIn("the native tool result was not returned", errors[0])
            self.assertEqual(len(requests), 3)

    def test_two_distinct_tool_results_complete_the_script(self):
        self.exercise("second result")

    def test_repeating_the_first_result_cannot_satisfy_the_second_step(self):
        self.exercise("first result")


if __name__ == "__main__":
    unittest.main()
