"""Offline cgroup protocol tests; real kernel/container checks run only on GitHub."""
import copy
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import unittest
from unittest.mock import patch
import uuid

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval.container_resources import ContainerResources, audit, limits, slice_name, unit
from agent_eval.isolated_grade import DOCKER
from agent_eval.study import digest, plan
from agent_eval.study_budget import Budget
from agent_eval.study_report import report
from agent_eval.study_runner import profile, run_attempt, tool_definitions
from agent_eval.workspace_bundle import pack
from agent_eval import test_study_runner as fixtures
from agent_eval.test_gateway import api_manifest
from agent_eval.test_isolated_grade import rubric


class Resources(unittest.TestCase):
    def test_missing_first_monitor_sample_retains_auditable_incomplete_evidence(self):
        scope = self.scope()
        (self.path / "cpu.stat").unlink()
        result = scope.finish()
        self.assertEqual(result["samples"], 1)
        self.assertFalse(result["complete"])
        self.assertTrue(audit(result, self.profile)["stopped"])

    def test_stalled_monitor_shutdown_does_not_wait_again_on_its_sample_lock(self):
        from unittest.mock import Mock
        scope = self.scope()
        scope.thread = Mock()
        scope.thread.is_alive.return_value = True
        with patch.object(scope, "sample", side_effect=AssertionError("would wait on stalled lock")):
            result = scope.finish()
        self.assertEqual(result["monitor_error"], "ThreadTimeout")
        self.assertFalse(result["complete"])
        self.assertTrue(audit(result, self.profile)["stopped"])

    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.name = "frstudy" + uuid.uuid4().hex + ".slice"
        self.path = self.root / self.name
        self.path.mkdir()
        self.profile = limits()
        for name, value in {"memory.max": str(self.profile["memory_bytes"]), "memory.swap.max": "0", "pids.max": "64",
                            "cpu.max": "50000 100000", "cpu.stat": "usage_usec 0\nuser_usec 0\nsystem_usec 0\n",
                            "memory.peak": "0", "memory.events": "max 0\noom_kill 0\n", "pids.events": "max 0\n",
                            "cgroup.events": "populated 0\n", "cgroup.kill": ""}.items():
            (self.path / name).write_text(value)
        self.calls, self.parent, self.driver = [], self.name, "systemd"
        original = Path.read_text
        for manager in (patch("agent_eval.container_resources.environment"),
                        patch.object(Path, "read_text", lambda path, *a, **kw: "0::/host\n" if str(path) == "/proc/self/cgroup" else original(path, *a, **kw))):
            manager.start()
            self.addCleanup(manager.stop)
        self.addCleanup(lambda: (Path(tempfile.gettempdir()) / (self.name + ".lock")).unlink(missing_ok=True))

    def execute(self, command, data, directory, timeout, cap):
        self.calls.append(command)
        payload = None
        if command[3] == "info":
            payload = {"CgroupDriver": self.driver, "CgroupVersion": "2", "SecurityOptions": []}
        elif command[3] == "context":
            payload = "unix:///var/run/docker.sock"
        elif command[3] == "inspect":
            payload = self.parent
        return {"exit_code": 0, "stop_reason": None}, json.dumps(payload).encode(), b""

    def scope(self):
        result = ContainerResources(self.profile, self.name, self.root, execute=self.execute, root=self.root)
        self.addCleanup(result.finish)
        return result

    def test_unit_and_profile_reject_shared_names_and_unbounded_limits(self):
        self.assertIn("CPUQuota=50%", unit(self.name, self.profile))
        for name in ("system.slice", "../escape.slice", "frstudy" + "a" * 32 + "-other.slice"):
            with self.subTest(name=name), self.assertRaises(ValueError):
                slice_name(name)
        for field, amount in (("cpu_seconds", 601), ("cpu_seconds", True), ("memory_bytes", 2**40),
                              ("memory_bytes", 17000000), ("pids", 65)):
            with self.subTest(field=field, amount=amount), self.assertRaises(ValueError):
                unit(self.name, {**self.profile, field: amount})

    def test_preflight_refuses_limit_drift_used_scopes_and_wrong_docker_driver(self):
        for filename, value in (("memory.max", "max"), ("memory.swap.max", "1"), ("pids.max", "max"),
                                ("cpu.max", "max 100000"), ("memory.peak", "1"), ("cpu.stat", "usage_usec 1\n"),
                                ("cgroup.events", "populated 1\n")):
            path, old = self.path / filename, (self.path / filename).read_text()
            path.write_text(value)
            with self.subTest(filename=filename), self.assertRaises(ValueError):
                self.scope()
            path.write_text(old)
        self.driver = "cgroupfs"
        with self.assertRaisesRegex(ValueError, "systemd cgroup v2"):
            self.scope()

    def test_two_hosts_cannot_admit_the_same_empty_slice(self):
        first = self.scope()
        with self.assertRaises(BlockingIOError):
            self.scope()
        first.finish()

    def test_container_parent_is_injected_and_verified_before_start(self):
        scope = self.scope()
        scope.invoke(DOCKER + ["create", "--name", "fr-study-fixture", "image"], b"", self.root, 2, 4096)
        self.assertIn("--cgroup-parent=" + self.name, self.calls[-2])
        self.assertEqual(self.calls[-1][3], "inspect")
        self.parent = "other.slice"
        with self.assertRaisesRegex(ValueError, "required container parent"):
            scope.invoke(DOCKER + ["create", "--name", "fr-grade-fixture", "image"], b"", self.root, 2, 4096)
        with self.assertRaisesRegex(ValueError, "overridden"):
            scope.invoke(DOCKER + ["create", "--cgroup-parent=other.slice"], b"", self.root, 2, 4096)

    def test_cpu_budget_is_shared_and_cleanup_still_runs_after_a_stop(self):
        scope = self.scope()
        (self.path / "cpu.stat").write_text("usage_usec 30000000\n")
        scope.check()
        (self.path / "cpu.stat").write_text("usage_usec 60000000\n")
        with self.assertRaisesRegex(ValueError, "budget stopped"):
            scope.check()
        self.assertEqual((self.path / "cgroup.kill").read_text(), "1")
        scope.invoke(DOCKER + ["rm", "--force", "fr-study-fixture"], b"", self.root, 2, 4096)
        self.assertEqual(self.calls[-1][3:5], ["rm", "--force"])
        retained = scope.finish()
        self.assertEqual(retained["stop_reason"], "cpu_seconds")
        self.assertEqual(audit(retained, self.profile)["cpu_seconds"], 60)

    def test_watchdog_samples_while_the_command_client_is_blocked(self):
        scope = self.scope()
        killed = threading.Event()
        with patch.object(scope, "kill", side_effect=killed.set):
            scope.start()
            (self.path / "cpu.stat").write_text("usage_usec 60000000\n")
            self.assertTrue(killed.wait(timeout=2))
            scope.finish()
        self.assertEqual(scope.reason, "cpu_seconds")

    def test_memory_and_process_limit_events_stop_the_scope(self):
        scope = self.scope()
        (self.path / "memory.events").write_text("max 1\noom_kill 1\n")
        with self.assertRaises(ValueError):
            scope.check()
        self.assertEqual(scope.finish()["stop_reason"], "memory_bytes")

    def test_process_limit_stop_is_distinct(self):
        scope = self.scope()
        (self.path / "pids.events").write_text("max 1\n")
        with self.assertRaises(ValueError):
            scope.check()
        self.assertEqual(scope.finish()["stop_reason"], "pids")

    def test_missing_or_regressing_counters_fail_closed(self):
        scope = self.scope()
        (self.path / "cpu.stat").write_text("usage_usec 50\n")
        scope.check()
        (self.path / "cpu.stat").write_text("usage_usec 40\n")
        with self.assertRaises(ValueError):
            scope.check()
        evidence = scope.finish()
        self.assertEqual(evidence["stop_reason"], "monitor_error")
        self.assertFalse(evidence["complete"])
        self.assertTrue(audit(evidence, self.profile)["stopped"])

    def test_cleanup_kills_stragglers_and_refuses_a_clean_result(self):
        scope = self.scope()
        (self.path / "cgroup.events").write_text("populated 1\n")
        result = scope.finish()
        self.assertEqual(result["stop_reason"], "cleanup_failed")
        self.assertEqual((self.path / "cgroup.kill").read_text(), "1")
        self.assertTrue(audit(result, self.profile)["stopped"])

    def test_failed_kill_remains_visible(self):
        scope = self.scope()
        (self.path / "cgroup.events").write_text("populated 1\n")
        original = Path.write_text
        def denied(path, *args, **kwargs):
            if path.name == "cgroup.kill":
                raise PermissionError("fixture")
            return original(path, *args, **kwargs)
        with patch.object(Path, "write_text", denied):
            result = scope.finish()
        self.assertEqual(result["kill_error"], "PermissionError")
        self.assertFalse(result["complete"])

    def test_auditor_rejects_changed_limits_hidden_stops_and_invented_completeness(self):
        result = self.scope().finish()
        self.assertFalse(audit(result, self.profile)["stopped"])
        for field, value in (("limits", {**self.profile, "cpu_seconds": 1}),
                             ("complete", False), ("scope", "whole machine"), ("samples", 0)):
            with self.subTest(field=field), self.assertRaises(ValueError):
                audit({**result, field: value}, self.profile)
        changed = copy.deepcopy(result)
        changed["counters"]["cpu_usec"] = 60000001
        with self.assertRaisesRegex(ValueError, "no retained stop"):
            audit(changed, self.profile)


class AttemptResources(unittest.TestCase):
    setUp = fixtures.RunnerTests.setUp
    repository = fixtures.RunnerTests.repository

    def test_offline_reporting_does_not_require_linux_runtime_imports(self):
        source = """import builtins,sys
sys.path.insert(0, 'tools')
original = builtins.__import__
def guarded(name, *args, **kwargs):
    if name == 'fcntl':
        raise ImportError('simulated non-POSIX report consumer')
    return original(name, *args, **kwargs)
builtins.__import__ = guarded
from agent_eval.study_report import report
from agent_eval.container_resources import limits, unit
assert 'MemoryMax=' in unit('frstudy' + 'a'*32 + '.slice', limits())
"""
        subprocess.run([sys.executable, "-c", source], check=True, capture_output=True, timeout=10)

    def attempt(self, stop=False):
        repository, revision = self.repository()
        skill = self.root / "skill"
        skill.mkdir()
        (skill / "SKILL.md").write_text("Fixture skill")
        binary = self.root / "fr"
        binary.write_text("Fixture binary")
        grader = self.root / "grader.json"
        grader.write_text(json.dumps(rubric()))
        value = api_manifest()
        value["runner"] = {**profile("sha256:" + "a" * 64, digest(pack(skill))), "container_resources": limits()}
        value["fr"].update(binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
                           skill_sha256=hashlib.sha256((skill / "SKILL.md").read_bytes()).hexdigest())
        for task in value["tasks"]:
            task.update(revision=revision, grader_sha256=hashlib.sha256(grader.read_bytes()).hexdigest())
        for model in value["models"]:
            model["settings"]["request"]["tools"] = tool_definitions("openai")
        frozen = plan(value)
        ledger = Budget(self.root / "ledger", frozen)
        cell = next(row["id"] for row in frozen["cells"] if row["task"] == "fix" and row["mode"] == "single")
        from agent_eval.container_resources import SCHEMA, SCOPE
        evidence = {"schema": SCHEMA, "limits": limits(), "slice": "frstudy" + "a" * 32 + ".slice", "scope": SCOPE,
                    "samples": 3, "counters": {"cpu_usec": 60000001 if stop else 12000, "memory_peak_bytes": 100000,
                    "memory_limit_events": 0, "oom_kills": 0, "pids_limit_events": 0, "populated": 0},
                    "stop_reason": "cpu_seconds" if stop else None, "monitor_error": None, "kill_error": None, "complete": True}
        from unittest.mock import Mock
        resource = Mock()
        resource.start.return_value = resource
        resource.finish.return_value = evidence
        backend = Mock(side_effect=lambda *args, **kwargs: fixtures.FakeTools())
        grading = Mock(return_value={"outcome": "passed"})
        attempts = self.root / "attempts"
        with patch("agent_eval.study_runner.ContainerResources", return_value=resource):
            record = run_attempt(ledger, cell, repository, grader, binary, skill, attempts,
                                 transport=fixtures.FakeProvider([[fixtures.call()], "Done"]), backend_factory=backend,
                                 grader=grading, container_slice=evidence["slice"])
        self.assertEqual(backend.call_args.kwargs["execute"], resource.invoke)
        self.assertEqual(grading.call_args.kwargs["execute"], resource.invoke)
        self.assertIsNone(record["measurements"]["sampled_aggregate_rss_bytes"])
        self.assertIsNone(record["measurements"]["tool_cpu_seconds"])
        observed = next(row for row in report(frozen, attempts)["attempts"] if row["cell"]["id"] == cell)
        self.assertEqual(observed["container_resources"]["stopped"], stop)
        return record, frozen, attempts, cell

    def test_same_resource_adapter_covers_tools_and_grading_without_claiming_host_totals(self):
        record, _, _, _ = self.attempt()
        self.assertEqual(record["status"], "completed")

    def test_late_resource_stop_overrides_success_and_cannot_be_relabelled(self):
        record, frozen, attempts, cell = self.attempt(stop=True)
        self.assertEqual(record["status"], "failed")
        self.assertEqual(record["grade"]["outcome"], "inconclusive")
        record["status"] = "completed"
        (attempts / f"{cell}.json").write_text(json.dumps(record))
        with self.assertRaisesRegex(ValueError, "resource-stopped"):
            report(frozen, attempts)

    def test_profile_and_runtime_slice_must_match_before_source_or_provider_access(self):
        value = api_manifest()
        value["runner"] = profile("sha256:" + "a" * 64, "b" * 64)
        for model in value["models"]:
            model["settings"]["request"]["tools"] = tool_definitions("openai")
        ledger = Budget(self.root / "mismatch", plan(value))
        with self.assertRaisesRegex(ValueError, "supplied together"):
            run_attempt(ledger, next(iter(ledger.cells)), self.root, self.root, self.root, self.root, self.root,
                        container_slice="frstudy" + "a" * 32 + ".slice")
        self.assertEqual(ledger.snapshot()["attempts"], [])

    def test_profile_cli_prints_resources_without_running_containers(self):
        skill = self.root / "skill"
        skill.mkdir()
        (skill / "SKILL.md").write_text("Fixture")
        result = subprocess.run([sys.executable, "tools/agent-eval-host.py", "loop-profile", "openai",
                                 "sha256:" + "a" * 64, str(skill), "--container-resources"],
                                check=True, capture_output=True, text=True, timeout=10)
        self.assertEqual(json.loads(result.stdout)["runner"]["container_resources"], limits())


@contextmanager
def prepared_slice(profile, directory):
    """CI-only administrative setup, restricted to a newly generated unit name."""
    name = "frstudy" + uuid.uuid4().hex + ".slice"
    local = directory / name
    local.write_text(unit(name, profile))
    destination = "/run/systemd/system/" + name
    def command(*argv):
        return subprocess.run(argv, check=True, capture_output=True, text=True, timeout=15)
    try:
        command("sudo", "install", "-m", "0644", str(local), destination)
        command("sudo", "systemctl", "daemon-reload")
        command("sudo", "systemctl", "start", name)
        command("sudo", "chown", str(os.getuid()), "/sys/fs/cgroup/" + name + "/cgroup.kill")
        yield name
    finally:
        command("sudo", "systemctl", "stop", name)
        command("sudo", "rm", "--", destination)
        command("sudo", "systemctl", "daemon-reload")
        (Path(tempfile.gettempdir()) / (name + ".lock")).unlink(missing_ok=True)


@unittest.skipUnless(os.environ.get("FR_STUDY_CGROUP_TESTS") == "1" and os.environ.get("FR_STUDY_TEST_IMAGE"),
                     "real cgroup and Docker checks run on GitHub only")
class KernelResources(unittest.TestCase):
    def test_commands_and_grader_share_real_kernel_counters(self):
        from agent_eval.isolated_grade import grade
        from agent_eval.study_workspace import ContainerTools
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with prepared_slice(limits(), root) as name:
                scope = ContainerResources(limits(), name, root).start()
                try:
                    settings = profile(os.environ["FR_STUDY_TEST_IMAGE"], "b" * 64)
                    tool = ContainerTools(settings["image"], settings, root, execute=scope.invoke)
                    state, result = tool.command(fixtures.files(), {"argv": ["python3", "-c",
                        "from pathlib import Path; Path('answer.py').write_text('print(int(input())*7)\\n'); "
                        "allocation=bytearray(20*1024**2); print('ok')"], "stdin": ""}, 5)
                    self.assertEqual(result["stdout"], "ok\n")
                    before = scope.last["cpu_usec"]
                    from agent_eval.workspace_bundle import unpack
                    unpack(state, root / "candidate")
                    grader = root / "grader.json"
                    grader.write_text(json.dumps(rubric(settings["image"])))
                    result = grade(root / "candidate", grader, hashlib.sha256(grader.read_bytes()).hexdigest(), execute=scope.invoke)
                    self.assertEqual(result["outcome"], "passed")
                    self.assertGreater(scope.last["cpu_usec"], before)
                finally:
                    retained = scope.finish()
                self.assertIsNone(retained["stop_reason"])
                self.assertGreater(retained["counters"]["memory_peak_bytes"], 20 * 1024**2)
                self.assertEqual(retained["counters"]["populated"], 0)
                self.assertFalse(audit(retained, limits())["stopped"])

    def test_shared_cpu_budget_kills_descendants_that_leave_the_process_group(self):
        from agent_eval.study_workspace import ContainerTools
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            budget = {**limits(), "cpu_seconds": 0.2}
            with prepared_slice(budget, root) as name:
                scope = ContainerResources(budget, name, root).start()
                try:
                    settings = profile(os.environ["FR_STUDY_TEST_IMAGE"], "b" * 64)
                    tool = ContainerTools(settings["image"], settings, root, execute=scope.invoke)
                    script = "import subprocess,sys; subprocess.Popen([sys.executable,'-c','import os; os.setsid()\\nwhile True: pass'])\nwhile True: pass"
                    with self.assertRaises(ValueError):
                        tool.command(fixtures.files(), {"argv": ["python3", "-c", script], "stdin": ""}, 8)
                finally:
                    retained = scope.finish()
                self.assertEqual(retained["stop_reason"], "cpu_seconds")
                self.assertEqual(retained["counters"]["populated"], 0)
                self.assertTrue(audit(retained, budget)["stopped"])

    def test_kernel_memory_limit_stops_large_allocations_and_retains_evidence(self):
        from agent_eval.study_workspace import ContainerTools
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            budget = {**limits(), "memory_bytes": 64 * 1024**2}
            with prepared_slice(budget, root) as name:
                scope = ContainerResources(budget, name, root).start()
                try:
                    settings = profile(os.environ["FR_STUDY_TEST_IMAGE"], "b" * 64)
                    tool = ContainerTools(settings["image"], settings, root, execute=scope.invoke)
                    with self.assertRaises(ValueError):
                        tool.command(fixtures.files(), {"argv": ["python3", "-c", "allocation=bytearray(128*1024**2)"], "stdin": ""}, 8)
                finally:
                    retained = scope.finish()
                self.assertEqual(retained["stop_reason"], "memory_bytes")
                self.assertGreater(retained["counters"]["memory_limit_events"], 0)
                self.assertEqual(retained["counters"]["populated"], 0)


if __name__ == "__main__":
    unittest.main()
