"""Public feedback, replay, shared budgets and GitHub-only execution controls."""
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
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval import native_changes as changes, native_checks as checks, native_costs
from agent_eval import opencode_changes as runner, container_resources as resources
from agent_eval import test_native_changes as fixtures
from agent_eval.study import digest, encode, load

SLICE = "frstudy" + "a" * 32 + ".slice"


def profile(image=None):
    return {"schema": "fr-stdio-grader-1", "image": image or "sha256:" + "1" * 64,
            "command": ["python3", "-B", "-c", "import module; print(module.value())"],
            "limits": {"wall_seconds": 2, "memory_bytes": 64 * 1024**2,
                       "scratch_bytes": 1024**2, "output_bytes": 2048, "candidate_bytes": 1024**2},
            "cases": [{"id": "public-value", "stdin": "", "stdout": "43\n", "exit_code": 0}]}


def fake_grade(files, spec):
    # Inspect fixture text; no candidate Python is executed on the workstation.
    output = b"43\n" if b"return 43" in base64.b64decode(files["module.py"]["data"]) else b"42\n"
    passed = output.decode() == spec["cases"][0]["stdout"]
    case = {"id": spec["cases"][0]["id"], "passed": passed, "failure": None if passed else "wrong output",
            "execution": {"exit_code": 0, "stop_reason": None},
            "container_state": {"Running": False, "OOMKilled": False, "Error": "", "ExitCode": 0}}
    for channel, raw in (("stdout", output), ("stderr", b"")):
        case.update({channel + "_base64": base64.b64encode(raw).decode(), channel + "_bytes": len(raw),
                     channel + "_sha256": hashlib.sha256(raw).hexdigest()})
    return {"schema": "fr-isolated-grade-1", "grader_sha256": hashlib.sha256(encode(spec)).hexdigest(),
            "candidate": checks.candidate_identity(files), "image": spec["image"],
            "outcome": "passed" if passed else "failed", "cases": [case]}


def calls():
    return [{"name": "describe_checks"}, {"name": "run_checks"}, fixtures.edit(),
            {"name": "run_checks"}, {"name": "submit_patch", "arguments": {"summary": "Public check passed; private grading pending."}}]


def frozen(root):
    initial = fixtures.plan(root)
    return runner.freeze(runner.checked(initial)["manifest"], root, root / "fr", public_checks={"change": profile()})


def evidence(stop=False):
    return {"schema": resources.SCHEMA, "limits": checks.RESOURCES, "slice": SLICE, "scope": resources.SCOPE,
            "samples": 3, "counters": {"cpu_usec": 10000001 if stop else 12000, "memory_peak_bytes": 100000,
                "memory_limit_events": 0, "oom_kills": 0, "pids_limit_events": 0, "populated": 0},
            "stop_reason": "cpu_seconds" if stop else None, "monitor_error": None, "kill_error": None, "complete": True}


class Feedback(unittest.TestCase):
    def test_check_edit_check_submit_replays_in_both_arms(self):
        for arm in ("files", "fr"):
            data = fixtures.fixture(calls(), arm=arm, version=3, public_check=profile(), check_execute=fake_grade)
            raw, exported, rows = data
            self.assertEqual([r["result"]["status"] for r in rows if r["params"]["name"] == "run_checks"], ["failed", "passed"])
            result = changes.audit(*data, {"files": fixtures.FILES, "requirement": "task", "public_check": profile()},
                                   {"arm": arm, "model": "provider/model"}, 3)
            self.assertEqual(result["metrics"]["public_checks"]["runs"], 2)
            self.assertTrue(result["metrics"]["public_checks"]["current"])
            self.assertFalse(result["metrics"]["public_checks"]["offline_semantics_reexecuted"])
            cost = native_costs.observed(raw, b"".join(encode(r) + b"\n" for r in rows),
                                         {"files": fixtures.FILES, "public_check": profile()}, arm, 3)
            self.assertEqual(cost["observed"]["public_checks"]["runs"], 2)
            self.assertGreater(cost["observed"]["produced_result_bytes"], 1000)
            self.assertEqual(cost["observed"]["source_page_bytes"], 0)

    def test_edit_makes_previous_feedback_stale_and_quota_cannot_reset(self):
        machine = changes.Machine(fixtures.FILES, "files", version=3, public_check=profile(), check_execute=fake_grade)
        self.assertEqual(machine.call({"name": "run_checks"})["status"], "failed")
        machine.call(fixtures.edit())
        self.assertFalse(machine.checks.summary()["current"])
        self.assertEqual(machine.call({"name": "run_checks"})["status"], "passed")
        self.assertTrue(machine.checks.summary()["current"])
        self.assertIn("budget", machine.call({"name": "run_checks"})["error"])
        submitted = machine.call({"name": "submit_patch", "arguments": {"summary": "done"}})
        self.assertEqual(submitted["public_checks"]["runs"], 2)
        self.assertIn("already submitted", machine.call({"name": "run_checks"})["error"])

    def test_unavailable_backend_is_retained_as_unavailable_not_passed(self):
        machine = changes.Machine(fixtures.FILES, "files", version=3, public_check=profile())
        result = machine.call({"name": "run_checks"})
        self.assertEqual(result["status"], "unavailable")
        replay = changes.Machine(fixtures.FILES, "files", version=3, public_check=profile(), replay=True)
        self.assertEqual(replay.call({"name": "run_checks"}, result), result)
        self.assertEqual(replay.checks.runs, 1)

    def test_forged_identity_output_case_and_verdict_refuse(self):
        source = changes.Machine(fixtures.FILES, "files", version=3, public_check=profile(), check_execute=fake_grade)
        original = source.call({"name": "run_checks"})
        for corrupt in ("source", "profile", "candidate", "stdout", "case", "verdict", "oom", "running", "extra"):
            with self.subTest(corrupt=corrupt):
                result = copy.deepcopy(original)
                case = result["grade"]["cases"][0]
                if corrupt in ("source", "profile"):
                    result[corrupt + "_sha256"] = "0" * 64
                elif corrupt == "candidate":
                    result["grade"]["candidate"]["sha256"] = "0" * 64
                elif corrupt == "stdout":
                    case["stdout_base64"] = base64.b64encode(b"forged").decode()
                elif corrupt == "case":
                    case["id"] = "other"
                elif corrupt == "verdict":
                    case["passed"] = True
                elif corrupt in ("oom", "running"):
                    case["container_state"]["OOMKilled" if corrupt == "oom" else "Running"] = "false"
                else:
                    result["unfrozen"] = True
                replay = changes.Machine(fixtures.FILES, "files", version=3, public_check=profile(), replay=True)
                self.assertIn("error", replay.call({"name": "run_checks"}, result))

    def test_old_protocols_have_no_check_tools_and_new_profiles_are_bounded(self):
        for version in (1, 2):
            self.assertNotIn("run_checks", [t["name"] for t in changes.schemas("fr", version)])
            self.assertIn("There is no local test execution", changes.prompt("files", version))
        for field, value in (("image", "python:latest"), ("cases", profile()["cases"] * 2),
                             ("command", ["python3", "x" * 9000])):
            spec = {**profile(), field: value}
            with self.assertRaises(ValueError):
                checks.validate(spec)
        for field in profile()["limits"]:
            spec = profile()
            spec["limits"][field] = 2**40
            with self.assertRaises(ValueError):
                checks.validate(spec)


class Collection(unittest.TestCase):
    def collect(self, root, stop=False, cleanup=False, cpu=1):
        plan = frozen(root)
        cell = next(c for c in runner.checked(plan)["cells"] if c["arm"] == "files")
        data = fixtures.fixture(calls(), version=3, public_check=profile(), check_execute=fake_grade)
        allowance = []
        def execute(command, data_in, out, err, directory, **kw):
            allowance.append(kw["cpu_limit_seconds"])
            if command[-1] == "--version":
                out.write(b"fixture-version\n")
            elif "export" in command:
                out.write(encode(data[1]))
            else:
                settings = load(directory / "server.json")
                self.assertEqual(set(settings), {"files", "arm", "binary", "tools_schema_version", "public_check", "container_slice"})
                self.assertEqual(settings["public_check"], profile())
                out.write(data[0])
                (directory / "tools.jsonl").write_bytes(b"".join(encode(r) + b"\n" for r in data[2]))
            return {"exit_code": 0, "stop_reason": None, "sampled_cpu_seconds": cpu,
                    "sampled_aggregate_rss_bytes": 1024, "sampled_disk_growth_bytes": 1024}
        resource = Mock()
        resource.start.return_value = resource
        resource.finish.return_value = evidence(stop)
        backend = Mock()
        if cleanup:
            backend.cleanup.side_effect = ValueError("cleanup failed")
        with patch.object(checks, "require_runner"), patch.object(runner.container_resources, "ContainerResources", return_value=resource), \
                patch.object(checks, "DockerChecks", return_value=backend), patch.object(runner, "bounded_run", side_effect=execute):
            record = runner.run_attempt(plan, cell["id"], root / "attempts", root / "fr", Path("opencode"), container_slice=SLICE)
        backend.cleanup.assert_called_once()
        resource.finish.assert_called_once()
        return plan, cell, record, allowance

    def test_shared_budget_resources_private_separation_and_offline_replay(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            plan, cell, record, allowance = self.collect(root)
            self.assertEqual(record["status"], "submitted", record)
            self.assertEqual(allowance, [10, 9, 8])
            self.assertEqual(runner.replay(plan, root / "attempts")["submitted"], 1)
            from agent_eval.native_outcomes import cohort
            (root / "plan.json").write_bytes(encode(plan))
            costs = next(r["costs"] for r in cohort(root)["attempts"] if r["cell"] == cell)
            self.assertEqual(costs["public_check_containers"]["cpu_seconds"], 0.012)
            self.assertEqual(costs["observed"]["public_checks"]["runs"], 2)

    def test_late_resource_stop_cleanup_failure_and_cpu_exhaustion_cannot_submit(self):
        for stop, cleanup, cpu in ((True, False, 1), (False, True, 1), (False, False, 10)):
            with self.subTest(stop=stop, cleanup=cleanup, cpu=cpu), tempfile.TemporaryDirectory() as temp:
                root = Path(temp)
                plan, cell, record, allowance = self.collect(root, stop, cleanup, cpu)
                self.assertEqual(record["status"], "failed")
                self.assertEqual(runner.replay(plan, root / "attempts")["submitted"], 0)
                if cpu == 10:
                    self.assertEqual(allowance, [10])
                else:
                    folder = root / "attempts" / cell["id"]
                    record["status"] = "submitted"
                    (folder / "record.json").write_bytes(encode(record))
                    manifest = load(folder / "manifest.json")
                    manifest["files"]["record.json"] = runner.legacy.identity(folder / "record.json")
                    (folder / "manifest.json").write_bytes(encode(manifest))
                    with self.assertRaisesRegex(ValueError, "resource-stopped"):
                        runner.replay(plan, root / "attempts")

    def test_missing_slice_and_local_execution_refuse_before_provider_access(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            plan = frozen(root)
            cell = runner.checked(plan)["cells"][0]["id"]
            with patch.object(runner, "bounded_run", side_effect=AssertionError("no model calls")):
                with self.assertRaisesRegex(ValueError, "supplied together"):
                    runner.run_attempt(plan, cell, root / "attempts", root / "fr", Path("opencode"))
                with patch.dict(os.environ, {"GITHUB_ACTIONS": "false"}):
                    with self.assertRaisesRegex(ValueError, "GitHub runner"):
                        runner.run_attempt(plan, cell, root / "attempts", root / "fr", Path("opencode"), container_slice=SLICE)
            self.assertFalse((root / "attempts").exists())

    def test_frozen_runner_and_resource_unit_remain_offline(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            plan = frozen(root)
            runner.retain_runner(plan, root / "runner")
            (root / "plan.json").write_bytes(encode(plan))
            cli = [sys.executable, "-B", str(root / "runner/native-changes.py")]
            completed = subprocess.run(cli + ["replay", str(root / "plan.json"), str(root / "absent")],
                                       cwd=root, capture_output=True, text=True, check=True, timeout=5)
            self.assertEqual(json.loads(completed.stdout)["submitted"], 0)
            completed = subprocess.run(cli + ["scope-unit", str(root / "plan.json"), SLICE],
                                       cwd=root, capture_output=True, text=True, check=True, timeout=5)
            self.assertIn("MemoryMax=134217728", completed.stdout)
            altered = copy.deepcopy(plan)
            altered["plan"]["public_check_resources"]["cpu_seconds"] = 11
            altered["sha256"] = digest(altered["plan"])
            with self.assertRaisesRegex(ValueError, "resource profile"):
                runner.checked(altered)


class DockerContract(unittest.TestCase):
    def test_required_parent_label_verification_and_owned_cleanup(self):
        seen = []
        def execute(command, *args):
            seen.append(command)
            output = encode(SLICE) if command[3] == "inspect" else b"a" * 64 + b"\n" if command[3] == "ps" else b""
            return {"exit_code": 0, "stop_reason": None}, output, b""
        with patch.object(checks, "require_runner"):
            backend = checks.DockerChecks(SLICE, Path("."), execute=execute)
        command = checks.isolated_grade.create_command(profile(), Path("/source"), "container-name")
        backend.invoke(command, b"", Path("."), 1, 100)
        self.assertIn("--cgroup-parent=" + SLICE, seen[0])
        self.assertIn("--label=fr.native-check=" + SLICE, seen[0])
        self.assertEqual(seen[1][-1], "container-name")
        backend.cleanup()
        self.assertEqual(seen[-2][-1], "label=fr.native-check=" + SLICE)
        self.assertEqual(seen[-1][-1], "a" * 64)
        with self.assertRaisesRegex(ValueError, "overridden"):
            backend.invoke(command + ["--cgroup-parent=other"], b"", Path("."), 1, 100)

    def test_bad_parent_and_cleanup_inventory_refuse(self):
        for output in (b"foreign\n", b"a" * 64 + b"\n" + b"a" * 64 + b"\n", (b"a" * 64 + b"\n") * 3):
            execute = Mock(return_value=({"exit_code": 0, "stop_reason": None}, output, b""))
            with patch.object(checks, "require_runner"):
                backend = checks.DockerChecks(SLICE, Path("."), execute=execute)
            with self.assertRaisesRegex(ValueError, "inventory"):
                backend.cleanup()
            self.assertEqual(execute.call_count, 1)
        execute = Mock(side_effect=[({"exit_code": 0, "stop_reason": None}, b"", b""),
                                    ({"exit_code": 0, "stop_reason": None}, encode("wrong.slice"), b"")])
        with patch.object(checks, "require_runner"):
            backend = checks.DockerChecks(SLICE, Path("."), execute=execute)
        with self.assertRaisesRegex(ValueError, "parent differs"):
            backend.invoke(checks.isolated_grade.create_command(profile(), Path("/source"), "name"), b"", Path("."), 1, 100)


@unittest.skipUnless(sys.platform == "linux" and os.environ.get("GITHUB_ACTIONS") == "true"
                     and os.environ.get("FR_STUDY_CGROUP_TESTS") == "1" and os.environ.get("FR_STUDY_TEST_IMAGE"),
                     "real native public checks run on GitHub only")
class GitHubChecks(unittest.TestCase):
    def test_real_mcp_child_runs_checks_in_the_parent_attempt_slice(self):
        from agent_eval.test_container_resources import prepared_slice
        from agent_eval import native_mcp
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with prepared_slice(checks.RESOURCES, root) as name:
                scope = resources.ContainerResources(checks.RESOURCES, name, root).start()
                backend = checks.DockerChecks(name, root)
                (root / "config.json").write_bytes(encode({"files": fixtures.FILES, "arm": "files", "binary": "fr",
                    "tools_schema_version": 3, "public_check": profile(os.environ["FR_STUDY_TEST_IMAGE"]), "container_slice": name}))
                messages = [{"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": native_mcp.PROTOCOL}},
                            {"jsonrpc": "2.0", "method": "notifications/initialized"}]
                messages.extend({"jsonrpc": "2.0", "id": index + 2, "method": "tools/call", "params": call}
                                for index, call in enumerate(calls()))
                try:
                    completed = subprocess.run([sys.executable, "-B", str(fixtures.ROOT / "tools/native-changes.py"),
                        "serve", str(root / "config.json"), str(root / "tools.jsonl")],
                        input=b"".join(encode(m) + b"\n" for m in messages), capture_output=True, timeout=30)
                finally:
                    retained = scope.finish()
                    backend.cleanup()
                self.assertEqual(completed.returncode, 0, completed.stderr)
                rows = [json.loads(line) for line in (root / "tools.jsonl").read_bytes().splitlines()]
                self.assertEqual([r["result"].get("status", r["result"]) for r in rows if r["params"]["name"] == "run_checks"],
                                 ["failed", "passed"])
                self.assertTrue(rows[-1]["result"]["public_checks"]["current"])
                self.assertFalse(resources.audit(retained, checks.RESOURCES)["stopped"])

    def test_real_public_failure_repair_pass_and_separate_private_grade(self):
        from agent_eval.test_container_resources import prepared_slice
        from agent_eval.workspace_bundle import unpack
        for arm in ("files", "fr"):
            with self.subTest(arm=arm), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                with prepared_slice(checks.RESOURCES, root) as name:
                    scope = resources.ContainerResources(checks.RESOURCES, name, root).start()
                    backend = checks.DockerChecks(name, root)
                    public = profile(os.environ["FR_STUDY_TEST_IMAGE"])
                    try:
                        data = fixtures.fixture(calls(), arm=arm, version=3, public_check=public, check_execute=backend.run)
                    finally:
                        retained = scope.finish()
                        backend.cleanup()
                    rows = data[2]
                    self.assertEqual([r["result"].get("status", r["result"]) for r in rows if r["params"]["name"] == "run_checks"],
                                     ["failed", "passed"])
                    self.assertFalse(resources.audit(retained, checks.RESOURCES)["stopped"])
                    self.assertGreater(retained["counters"]["cpu_usec"], 0)
                    with patch.object(checks.DockerChecks, "run", side_effect=AssertionError("replay cannot execute code")):
                        replayed = changes.audit(*data, {"files": fixtures.FILES, "requirement": "task", "public_check": public},
                                                 {"arm": arm, "model": "provider/model"}, 3)
                    unpack(replayed["files"], root / "submitted")
                    private = profile(public["image"])
                    private["command"][-1] = "import module; print(module.value() * 2)"
                    private["cases"] = [{"id": "private-double", "stdin": "", "stdout": "86\n", "exit_code": 0}]
                    private_path = root / "private.json"
                    private_path.write_bytes(encode(private))
                    result = checks.isolated_grade.grade(root / "submitted", private_path, hashlib.sha256(encode(private)).hexdigest())
                    self.assertEqual(result["outcome"], "passed")
                    self.assertNotIn("private-double", encode(rows).decode())

    def test_container_cannot_write_source_read_private_file_or_use_network(self):
        from agent_eval.test_container_resources import prepared_slice
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            secret = root / "private-expected.txt"
            secret.write_text("unexposed expectation")
            source = """def value():
    from pathlib import Path
    import socket
    assert not Path(%r).exists()
    try:
        Path('/workspace/injected').touch()
    except OSError:
        pass
    else:
        raise AssertionError('source is writable')
    connection = socket.socket()
    connection.settimeout(0.1)
    try:
        connection.connect(('1.1.1.1', 443))
    except OSError:
        pass
    else:
        raise AssertionError('network available')
    return 43
""" % str(secret)
            files = {"module.py": {"data": base64.b64encode(source.encode()).decode(), "executable": False}}
            with prepared_slice(checks.RESOURCES, root) as name:
                scope = resources.ContainerResources(checks.RESOURCES, name, root).start()
                backend = checks.DockerChecks(name, root)
                try:
                    machine = changes.Machine(files, "files", version=3,
                        public_check=profile(os.environ["FR_STUDY_TEST_IMAGE"]), check_execute=backend.run)
                    result = machine.call({"name": "run_checks"})
                finally:
                    retained = scope.finish()
                    backend.cleanup()
                self.assertEqual(result.get("status"), "passed", result)
                self.assertFalse(resources.audit(retained, checks.RESOURCES)["stopped"])
                self.assertEqual(machine.files, files)

    def test_stranded_container_is_stopped_and_owned_metadata_is_removed(self):
        import uuid
        from agent_eval.test_container_resources import prepared_slice
        from agent_eval.workspace_bundle import unpack
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            unpack(fixtures.FILES, root / "source")
            spec = profile(os.environ["FR_STUDY_TEST_IMAGE"])
            spec["command"][-1] = "import time; time.sleep(60)"
            with prepared_slice(checks.RESOURCES, root) as name:
                scope = resources.ContainerResources(checks.RESOURCES, name, root).start()
                backend = checks.DockerChecks(name, root)
                container = "fr-native-check-test-" + uuid.uuid4().hex
                try:
                    created, _, _ = backend.invoke(checks.isolated_grade.create_command(spec, root / "source", container), b"", root, 5, 4096)
                    self.assertEqual(created["exit_code"], 0)
                    started, _, _ = backend.invoke(checks.isolated_grade.DOCKER + ["start", container], b"", root, 5, 4096)
                    self.assertEqual(started["exit_code"], 0)
                    self.assertTrue(scope.read()["populated"])
                finally:
                    retained = scope.finish()
                    backend.cleanup()
                self.assertTrue(resources.audit(retained, checks.RESOURCES)["stopped"])
                state, names, _ = backend.execute(checks.isolated_grade.DOCKER + ["ps", "-aq", "--filter", "label=fr.native-check=" + name],
                                                  b"", root, 5, 4096)
                self.assertEqual(state["exit_code"], 0)
                self.assertEqual(names.strip(), b"")


if __name__ == "__main__":
    unittest.main()
