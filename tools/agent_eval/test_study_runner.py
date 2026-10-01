"""Offline conversations and adversarial workspace tests; real containers run in CI."""
import base64
import copy
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval.request_gateway import prepare
from agent_eval.study import digest, plan
from agent_eval.study_budget import Budget
from agent_eval.study_report import report
from agent_eval.study_runner import Loop, profile, response_items, run_attempt, tool_definitions
from agent_eval.study_workspace import ContainerTools, git_snapshot
from agent_eval.workspace_bundle import pack, unpack, validate
from agent_eval.test_gateway import api_manifest


def files(text="before"):
    return {"main.py": {"data": base64.b64encode(text.encode()).decode(), "executable": False}}


def call(name="command", arguments=None, identity="tool-1"):
    return {"id": identity, "name": name, "arguments": arguments or {"argv": ["cat", "main.py"], "stdin": ""}}


class FakeProvider:
    def __init__(self, turns):
        self.turns = iter(turns)
        self.requests = []

    def __call__(self, provider, path, payload, timeout):
        if path.endswith(("input_tokens", "count_tokens")):
            return {"object": "response.input_tokens", "input_tokens": 100}
        self.requests.append(copy.deepcopy(payload))
        turn = next(self.turns)
        if isinstance(turn, Exception):
            raise turn
        if provider == "openai":
            output = [{"type": "reasoning", "id": "rs-" + str(len(self.requests)), "summary": [], "encrypted_content": "opaque-fixture"}]
            output += ([{"type": "message", "id": "msg", "role": "assistant", "status": "completed", "phase": "final_answer",
                         "content": [{"type": "output_text", "text": turn, "annotations": [], "logprobs": []}]}] if isinstance(turn, str) else
                       [{"type": "function_call", "call_id": row["id"], "name": row["name"], "arguments": json.dumps(row["arguments"])} for row in turn])
            return {"object": "response", "model": payload["model"], "id": f"response-{len(self.requests)}", "status": "completed",
                    "output": output, "usage": {"input_tokens": 100, "output_tokens": 10,
                    "input_tokens_details": {"cached_tokens": 0, "cache_write_tokens": 0}, "output_tokens_details": {"reasoning_tokens": 2}}}
        return {"type": "message", "model": payload["model"], "id": f"response-{len(self.requests)}",
                "stop_reason": "end_turn" if isinstance(turn, str) else "tool_use",
                "content": [{"type": "text", "text": turn}] if isinstance(turn, str) else
                           [{"type": "tool_use", "id": row["id"], "name": row["name"], "input": row["arguments"]} for row in turn],
                "usage": {"input_tokens": 100, "output_tokens": 10, "cache_read_input_tokens": 0, "cache_creation_input_tokens": 0}}


class FakeTools:
    def __init__(self, *args):
        self.calls = []

    def command(self, state, arguments, timeout):
        self.calls.append(arguments)
        return files(arguments["argv"][-1]), {"stdout": "observed", "stderr": "", "exit_code": 0, "stop_reason": None}


class RunnerTests(unittest.TestCase):
    def test_tool_batch_limit_and_unknown_names_execute_nothing(self):
        loop = self.loop(max_tool_calls=1)
        loop.transport = FakeProvider([[call(identity="a"), call(identity="b")]])
        with self.assertRaisesRegex(ValueError, "tool call limit"):
            loop.agent(files(), "task")
        self.assertEqual(loop.backend.calls, [])

    def test_invalid_tool_in_batch_refuses_before_first_tool(self):
        loop = self.loop()
        loop.transport = FakeProvider([[call(identity="a"), call(name="host_shell", identity="b")]])
        with self.assertRaisesRegex(ValueError, "unsupported tool"):
            loop.agent(files(), "task")
        self.assertEqual(loop.backend.calls, [])

    def test_evidence_limit_stops_before_network(self):
        loop = self.loop()
        loop.transport = FakeProvider(["never"])
        with patch("agent_eval.study_runner.disk_size", return_value=128 * 1024**2), self.assertRaisesRegex(ValueError, "evidence allowance"):
            loop.agent(files(), "task")
        self.assertEqual(loop.transport.requests, [])

    def test_tool_container_options_and_cleanup_are_verified_without_docker(self):
        commands = []
        def execute(command, data, root, timeout, cap):
            commands.append(command)
            result = {"exit_code": 0, "stop_reason": None}
            if command[3] == "image":
                return result, b"null", b""
            if command[3] == "start":
                return result, json.dumps({"files": files("changed"), "result": {"stdout": "ok"}}).encode(), b""
            return result, b"", b""
        settings = profile("sha256:" + "a" * 64, "b" * 64)
        backend = ContainerTools(settings["image"], settings, self.root, execute=execute)
        state, _ = backend.command(files(), {"argv": ["cat", "main.py"], "stdin": ""}, 2)
        self.assertEqual(state, files("changed"))
        command = commands[1]
        for flag in ("--network=none", "--read-only", "--memory=256m", "--memory-swap=256m",
                     "--cpus=0.5", "--pids-limit=32", "--cap-drop=ALL", "--user=65534:65534"):
            self.assertIn(flag, command)
        self.assertEqual(commands[-1][3], "rm")
        self.assertNotIn("--env", command)
        with patch.object(backend, "execute", return_value=({"exit_code": 1, "stop_reason": None}, b"", b"")), self.assertRaisesRegex(ValueError, "cleanup failed"):
            backend.command(files(), {"argv": ["true"], "stdin": ""}, 2)

    def test_run_cli_requires_acknowledgement_before_accessing_inputs(self):
        result = subprocess.run([sys.executable, "tools/agent-eval-host.py", "run", "missing", "missing", "cell",
                                 "missing", "missing", "missing", "--binary", "missing", "--skill", "missing"],
                                capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 2)
        self.assertIn("confirm-agent-spend", result.stderr)
        self.assertNotIn("No such file", result.stderr)

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)

    def loop(self, provider="openai", mode="single", **limits):
        value = api_manifest(provider)
        value["runner"] = {**profile("sha256:" + "a" * 64, "b" * 64), **limits}
        for model in value["models"]:
            model["settings"]["request"]["tools"] = tool_definitions(provider)
        frozen = plan(value)
        ledger = Budget(self.root / f"{provider}.db", frozen)
        cell = next(row for row in frozen["cells"] if row["mode"] == mode)
        ledger.begin(cell["id"])
        directory = self.root / provider / "artifacts" / cell["id"]
        directory.mkdir(parents=True)
        return Loop(ledger, cell, directory, FakeTools(), "Test instructions")

    def test_both_protocols_replay_tool_results_and_preserve_reasoning(self):
        for provider in ("openai", "anthropic"):
            with self.subTest(provider=provider):
                loop = self.loop(provider)
                fake = FakeProvider([[call()], "Finished"])
                loop.transport = fake
                state, answer = loop.agent(files(), "Find and fix the bug")
                self.assertEqual(answer, "Finished")
                self.assertEqual(len(loop.agents[0]["invocations"]), 2)
                self.assertEqual(loop.agents[0]["status"], "completed")
                self.assertEqual(loop.measurements["tool_calls"], 1)
                self.assertIsNone(loop.measurements["source_read_bytes"])
                replay = fake.requests[1]
                if provider == "openai":
                    self.assertEqual(replay["input"][1]["encrypted_content"], "opaque-fixture")
                    self.assertEqual(replay["input"][-1]["type"], "function_call_output")
                    self.assertEqual(loop.measurements["peak_context_tokens"], 110)
                else:
                    self.assertEqual(replay["messages"][-1]["content"][0]["type"], "tool_result")
                    self.assertIsNone(loop.measurements["peak_context_tokens"])

    def test_child_gets_focused_context_and_edits_are_discarded(self):
        loop = self.loop(mode="delegated")
        fake = FakeProvider([[call("delegate", {"question": "Inspect only the boundary case"})],
                             [call(arguments={"argv": ["edit", "child-edit"], "stdin": ""})], "See main.py:1", "Integrated"])
        loop.transport = fake
        state, _ = loop.agent(files(), "Private parent context")
        self.assertEqual(state, files())
        self.assertEqual(len(loop.agents), 2)
        self.assertEqual(loop.agents[0]["children"], [loop.agents[1]["id"]])
        self.assertEqual(loop.agents[1]["parent"], loop.agents[0]["id"])
        self.assertNotIn("Private parent context", json.dumps(fake.requests[1]))
        self.assertIn("See main.py:1", json.dumps(fake.requests[-1]))
        self.assertEqual(len(loop.ledger.snapshot()["calls"]), 4)
        self.assertGreater(loop.measurements["handoff_bytes"], 0)

    def test_single_agent_delegation_refuses_without_spawning(self):
        loop = self.loop()
        fake = FakeProvider([[call("delegate", {"question": "help"})]])
        loop.transport = fake
        with self.assertRaisesRegex(ValueError, "child admission"):
            loop.agent(files(), "task")
        self.assertEqual(len(loop.agents), 1)
        self.assertEqual(len(fake.requests), 1)

    def test_repeated_tool_id_never_reexecutes(self):
        loop = self.loop()
        loop.transport = FakeProvider([[call()], [call()]])
        with self.assertRaisesRegex(ValueError, "identity replayed"):
            loop.agent(files(), "task")
        self.assertEqual(len(loop.backend.calls), 1)

    def test_unknown_charge_halts_all_tools_and_retains_failed_agent(self):
        loop = self.loop()
        loop.transport = FakeProvider([TimeoutError("private provider detail")])
        with self.assertRaises(TimeoutError):
            loop.agent(files(), "task")
        self.assertEqual(loop.ledger.snapshot()["calls"][0]["state"], "unknown")
        self.assertFalse(loop.agents[0]["usage_complete"])
        self.assertEqual(loop.backend.calls, [])

    def test_turn_and_tool_limits_stop_before_additional_work(self):
        loop = self.loop(max_turns=1)
        fake = FakeProvider([[call()], "Never send"])
        loop.transport = fake
        with self.assertRaisesRegex(ValueError, "turn limit"):
            loop.agent(files(), "task")
        self.assertEqual(len(fake.requests), 1)

    def test_whole_response_batch_validated_before_dispatch(self):
        for raw in ({"status": "incomplete", "output": []},
                    {"status": "completed", "output": [{"type": "reasoning", "summary": []}]},
                    {"status": "completed", "output": [{"type": "web_search_call"}]},
                    {"status": "completed", "output": [{"type": "message", "role": "assistant", "phase": "commentary",
                                                           "content": [{"type": "output_text", "text": "Working"}]}]}):
            with self.subTest(raw=raw), self.assertRaises(ValueError):
                response_items("openai", raw)

    def test_reasoning_replay_cannot_use_server_references(self):
        loop = self.loop()
        for item in ({"type": "reasoning", "id": "server-reference", "summary": []},
                     {"type": "item_reference", "id": "server-reference"}):
            with self.assertRaises(ValueError):
                prepare(loop.model, {"input": [item]})

    def test_expired_deadline_never_contacts_provider(self):
        loop = self.loop()
        loop.clock = lambda: loop.started + 1000
        loop.transport = FakeProvider(["never"])
        with self.assertRaisesRegex(ValueError, "time budget"):
            loop.agent(files(), "task")
        self.assertEqual(loop.transport.requests, [])

    def test_bundle_rejects_escape_links_duplicates_and_large_files(self):
        for name in ("../outside", "/absolute", "a/../b", "a//b", "a\\b", "."):
            with self.subTest(name=name), self.assertRaises(ValueError):
                validate({name: files()["main.py"]})
        with self.assertRaisesRegex(ValueError, "collision"):
            validate({"a": files()["main.py"], "a/b": files()["main.py"]})
        with self.assertRaises(ValueError):
            validate(files("longer than limit"), 2)
        (self.root / "link").symlink_to("/etc/passwd")
        with self.assertRaisesRegex(ValueError, "symlink"):
            pack(self.root)

    def repository(self):
        repo = self.root / "repo"
        repo.mkdir()
        (repo / "main.py").write_text("print('before')\n")
        for args in (["init", "-q"], ["add", "main.py"], ["-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid",
                                                         "-c", "commit.gpgsign=false", "commit", "-qm", "fixture"]):
            subprocess.run(["git", "-C", str(repo), *args], check=True, capture_output=True)
        revision = subprocess.check_output(["git", "-C", str(repo), "rev-parse", "HEAD"], text=True).strip()
        return repo, revision

    def test_pinned_export_ignores_uncommitted_work(self):
        repo, revision = self.repository()
        (repo / "main.py").write_text("changed")
        (repo / "secret").write_text("untracked")
        result = git_snapshot(repo, revision, self.root)
        self.assertEqual(base64.b64decode(result["main.py"]["data"]), b"print('before')\n")
        self.assertNotIn("secret", result)
        with self.assertRaisesRegex(ValueError, "unavailable"):
            git_snapshot(repo, "0" * 40, self.root)

    def test_complete_attempt_is_auditable_and_incomplete_measurements_stay_unknown(self):
        repo, revision = self.repository()
        skill = self.root / "skill"
        skill.mkdir()
        (skill / "SKILL.md").write_text("Fixture skill")
        binary = self.root / "fr"
        binary.write_text("Fixture binary")
        private = self.root / "grader.json"
        from agent_eval.test_isolated_grade import rubric
        private.write_text(json.dumps(rubric()))
        value = api_manifest()
        value["runner"] = profile("sha256:" + "a" * 64, digest(pack(skill)))
        value["fr"].update(binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
                           skill_sha256=hashlib.sha256((skill / "SKILL.md").read_bytes()).hexdigest())
        for task in value["tasks"]:
            task.update(revision=revision, grader_sha256=hashlib.sha256(private.read_bytes()).hexdigest())
        for model in value["models"]:
            model["settings"]["request"]["tools"] = tool_definitions("openai")
        frozen = plan(value)
        ledger = Budget(self.root / "run.db", frozen)
        cell = next(row["id"] for row in frozen["cells"] if row["task"] == "fix" and row["mode"] == "single")
        attempts = self.root / "attempts"
        record = run_attempt(ledger, cell, repo, private, binary, skill, attempts,
                             transport=FakeProvider([[call()], "Done"]), backend_factory=FakeTools,
                             grader=lambda *args: {"outcome": "passed", "scope": "synthetic test"})
        self.assertEqual(record["status"], "completed")
        audit = report(frozen, attempts)
        observed = next(row for row in audit["attempts"] if row["cell"]["id"] == cell)
        self.assertTrue(observed["provider_usage_verified"])
        self.assertTrue(observed["usage_complete"])
        self.assertIn("disk_bytes", observed["unmeasured_budgets"])
        self.assertFalse(audit["audit_complete"])
        self.assertEqual(observed["source_disclosure"]["opaque_command_calls"], 1)
        record_path = attempts / f"{cell}.json"
        for field in ("tool_calls", "tool_result_bytes", "instruction_bytes", "handoff_bytes", "source_read_bytes"):
            changed = copy.deepcopy(record)
            changed["measurements"][field] = 999
            record_path.write_text(json.dumps(changed))
            with self.subTest(field=field), self.assertRaisesRegex(ValueError, "trace measurement differs"):
                report(frozen, attempts)
        record_path.write_text(json.dumps(record))
        self.assertEqual(ledger.snapshot()["attempts"][0]["state"], "closed")
        second = next(row["id"] for row in frozen["cells"] if row["task"] == "fix" and row["mode"] == "single" and row["id"] != cell)
        failed = run_attempt(ledger, second, repo, private, binary, skill, attempts,
                             transport=FakeProvider([TimeoutError("private-error-detail")]), backend_factory=FakeTools)
        self.assertEqual(failed["status"], "failed")
        self.assertEqual(failed["grade"]["outcome"], "inconclusive")
        self.assertFalse(failed["agents"][0]["usage_complete"])
        self.assertTrue(any(row["state"] == "unknown" for row in ledger.snapshot()["calls"]))
        self.assertNotIn("private-error-detail", (attempts / failed["trace"]["path"]).read_text())
        self.assertFalse(report(frozen, attempts)["spend"]["complete"])
        with self.assertRaisesRegex(ValueError, "already retained"):
            run_attempt(ledger, cell, repo, private, binary, skill, attempts)


@unittest.skipUnless(os.environ.get("FR_STUDY_TEST_IMAGE"), "real tool containers run on GitHub only")
class ContainerTests(unittest.TestCase):
    def test_real_tools_and_private_grader_complete_an_auditable_attempt(self):
        from agent_eval.test_isolated_grade import rubric
        from agent_eval.isolated_grade import grade
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            helper = RunnerTests()
            helper.root = root
            repository, revision = helper.repository()
            skill = root / "skill"
            skill.mkdir()
            (skill / "SKILL.md").write_text("Use fr --version to check this synthetic executable.")
            binary = root / "fr"
            binary.write_text("#!/bin/sh\necho fixture-fr\n")
            binary.chmod(0o555)
            private = root / "private-grader.json"
            private.write_text(json.dumps(rubric(os.environ["FR_STUDY_TEST_IMAGE"])))
            value = api_manifest()
            value["runner"] = profile(os.environ["FR_STUDY_TEST_IMAGE"], digest(pack(skill)))
            value["fr"].update(binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
                               skill_sha256=hashlib.sha256((skill / "SKILL.md").read_bytes()).hexdigest())
            for task in value["tasks"]:
                task.update(revision=revision, grader_sha256=hashlib.sha256(private.read_bytes()).hexdigest())
            for model in value["models"]:
                model["settings"]["request"]["tools"] = tool_definitions("openai")
            frozen = plan(value)
            ledger = Budget(root / "ledger", frozen)
            cell = next(row["id"] for row in frozen["cells"] if row["arm"] == "fr" and row["task"] == "fix" and row["mode"] == "single")
            provider = FakeProvider([[call(arguments={"argv": ["python3", "-c",
                "import pathlib,subprocess; assert subprocess.check_output(['fr','--version']).strip()==b'fixture-fr'; "
                "assert pathlib.Path('/opt/fr-skill/SKILL.md').is_file(); "
                "assert not pathlib.Path('/private-grader.json').exists(); "
                "pathlib.Path('answer.py').write_text('print(int(input())*7)\\n')"], "stdin": ""})], "Finished"])
            attempts = root / "attempts"
            record = run_attempt(ledger, cell, repository, private, binary, skill, attempts, transport=provider)
            self.assertEqual(record["grade"]["outcome"], "passed")
            audit = next(row for row in report(frozen, attempts)["attempts"] if row["cell"]["id"] == cell)
            self.assertTrue(audit["provider_usage_verified"])
            submission = attempts / "artifacts" / cell / "submission"
            (submission / "answer.py").write_text("print('wrong')\n")
            self.assertEqual(grade(submission, private, value["tasks"][0]["grader_sha256"])["outcome"], "failed")

    def test_command_edits_persist_without_exposing_host_or_provider_credentials(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            limits = profile(os.environ["FR_STUDY_TEST_IMAGE"], "b" * 64)
            tool = ContainerTools(limits["image"], limits, root)
            with patch.dict(os.environ, {"OPENAI_API_KEY": "host-only-fixture"}):
                state, result = tool.command(files(), {"argv": ["python3", "-c",
                    "import os,pathlib; assert 'OPENAI_API_KEY' not in os.environ; "
                    "assert not pathlib.Path('/var/run/docker.sock').exists(); "
                    "pathlib.Path('main.py').write_text('after'); print('ok')"], "stdin": ""}, 5)
            self.assertEqual(result["stdout"], "ok\n")
            _, result = tool.command(state, {"argv": ["cat", "main.py"], "stdin": ""}, 5)
            self.assertEqual(result["stdout"], "after")

    def test_timeout_output_and_symlink_exports_are_bounded(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            limits = {**profile(os.environ["FR_STUDY_TEST_IMAGE"], "b" * 64), "output_bytes": 100}
            tool = ContainerTools(limits["image"], limits, root)
            _, result = tool.command(files(), {"argv": ["python3", "-c", "import time; time.sleep(20)"], "stdin": ""}, 0.2)
            self.assertEqual(result["stop_reason"], "wall_seconds")
            _, result = tool.command(files(), {"argv": ["python3", "-c", "print('x'*10000)"], "stdin": ""}, 5)
            self.assertEqual(result["stop_reason"], "output_bytes")
            with self.assertRaises(ValueError):
                tool.command(files(), {"argv": ["ln", "-s", "/etc/passwd", "leak"], "stdin": ""}, 5)
            live = subprocess.check_output(["docker", "--context", "default", "ps", "-aq", "--filter", "name=fr-study-"], text=True)
            self.assertEqual(live.strip(), "")


if __name__ == "__main__":
    unittest.main()
