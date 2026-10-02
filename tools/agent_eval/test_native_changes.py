"""Offline edit, transcript replay, submission isolation and runner controls."""
import base64
import copy
import hashlib
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval import native_changes as changes, opencode_changes as runner
from agent_eval import native_mcp as mcp, opencode_rehearsal as legacy
from agent_eval.study import digest, encode, load

ROOT = Path(__file__).resolve().parents[2]
FILES = {"module.py": {"data": base64.b64encode(b"def value():\n    return 42\n").decode(), "executable": False}}


def edit(files=FILES, **kw):
    args = {"path": "module.py", "sha256": hashlib.sha256(base64.b64decode(files["module.py"]["data"])).hexdigest(),
            "old": "return 42", "new": "return 43", **kw}
    return {"name": "replace_source", "arguments": args}


def fixture(calls=None, arm="files", version=1, execute=None):
    log = io.BytesIO()
    server = changes.Server({"files": FILES, "arm": arm, "binary": "fr", "tools_schema_version": version}, log)
    if execute is not None:
        server.machine.execute = execute
    calls = calls or [{"name": "read_source", "arguments": {"path": "module.py", "offset": 0, "bytes": 8192, "sha256": ""}},
                      edit(), {"name": "submit_patch", "arguments": {"summary": "Changed value; tests not run."}}]
    events, messages = [], [{"info": {"role": "user"}, "parts": [{"type": "text", "text": changes.prompt(arm, version) + "\nTask:\ntask"}]}]
    tokens = {"input": 10, "output": 20, "reasoning": 0, "cache": {"read": 1, "write": 0}}
    for index in range(len(calls) + 1):
        mid, parts = f"msg_{index}", []
        events.append({"type": "step_start", "sessionID": "ses_abc", "part": {"messageID": mid}})
        if index < len(calls):
            params = calls[index]
            response = server.call(params)
            state = {"status": "error" if response["isError"] else "completed", "input": params.get("arguments", {})}
            state["error" if response["isError"] else "output"] = response["content"][0]["text"]
            part = {"type": "tool", "messageID": mid, "sessionID": "ses_abc", "callID": f"call_{index}",
                    "tool": "rehearsal_" + params["name"], "state": state}
            events.append({"type": "tool_use", "sessionID": "ses_abc", "part": part})
            parts.append(part)
        reason = "tool-calls" if parts else "stop"
        events.append({"type": "step_finish", "sessionID": "ses_abc", "part": {"messageID": mid, "reason": reason, "tokens": tokens, "cost": 0}})
        messages.append({"info": {"id": mid, "role": "assistant", "providerID": "provider", "modelID": "model", "finish": reason,
                                  "tokens": tokens, "cost": 0}, "parts": parts})
    server.machine.close()
    return b"\n".join(encode(e) for e in events), {"info": {"id": "ses_abc"}, "messages": messages}, [mcp.decode(line) for line in log.getvalue().splitlines()]


def plan(root):
    (root / "source").mkdir()
    (root / "source/module.py").write_bytes(base64.b64decode(FILES["module.py"]["data"]))
    grader = {"schema": "fr-stdio-grader-1", "image": "sha256:" + "1" * 64,
              "command": ["python3", "/workspace/module.py"],
              "limits": {"wall_seconds": 1, "memory_bytes": 67108864, "scratch_bytes": 1048576, "output_bytes": 4096, "candidate_bytes": 1048576},
              "cases": [{"id": "private", "stdin": "", "stdout": "43\n", "exit_code": 0}]}
    (root / "grader.json").write_bytes(encode(grader))
    (root / "fr").write_text("fake binary")
    manifest = {"schema": legacy.SCHEMA, "seed": 416, "models": ["provider/model"], "repetitions": 1,
                "tasks": [{"id": "change", "kind": "fix", "source": "source", "grader": "grader.json", "requirement": "task", "provenance": "synthetic control"}]}
    return runner.freeze(manifest, root, root / "fr")


class Edits(unittest.TestCase):
    def test_hash_guard_preserves_original_and_read_continuations(self):
        machine = changes.Machine(FILES, "files")
        result = machine.call(edit())
        self.assertEqual(machine.original, FILES)
        self.assertEqual(result["sha256"], hashlib.sha256(b"def value():\n    return 43\n").hexdigest())
        self.assertIn("error", machine.call(edit()))
        self.assertEqual(machine.edits, 1)
        stale = machine.call({"name": "read_source", "arguments": {"path": "module.py", "offset": 0, "bytes": 8192, "sha256": edit()["arguments"]["sha256"]}})
        self.assertIn("error", stale)
        good = machine.call({"name": "read_source", "arguments": {"path": "module.py", "offset": 0, "bytes": 8192, "sha256": result["sha256"]}})
        self.assertIn("return 43", good["text"])

    def test_refused_edits_are_atomic(self):
        for fields in ({"path": "../private"}, {"path": "new.py"}, {"sha256": ""}, {"old": ""},
                       {"old": "missing"}, {"old": "e"}, {"new": "return 42"}, {"new": "a" * 8193}):
            machine = changes.Machine(FILES, "files")
            self.assertIn("error", machine.call(edit(**fields)), fields)
            self.assertEqual(machine.files, FILES)

    def test_binary_and_workspace_growth_refuse_without_mutation(self):
        binary = {"module.py": {"data": base64.b64encode(b"\xff").decode(), "executable": False}}
        machine = changes.Machine(binary, "files")
        self.assertIn("error", machine.call(edit(binary)))
        large = {"module.py": {"data": base64.b64encode(b"x" * (legacy.MAX_WORKSPACE - 1) + b"y").decode(), "executable": True}}
        machine = changes.Machine(large, "files")
        self.assertIn("error", machine.call(edit(large, old="y", new="yy")))
        self.assertEqual(machine.files, large)

    def test_submit_binds_files_and_stops_mutations(self):
        machine = changes.Machine(FILES, "files")
        machine.call(edit())
        result = machine.call({"name": "submit_patch", "arguments": {"summary": "done"}})
        self.assertEqual(result["submission_sha256"], digest(machine.files))
        self.assertEqual(result["changed_paths"], ["module.py"])
        self.assertIn("error", machine.call(edit()))
        machine.calls = mcp.MAX_CALLS
        with self.assertRaisesRegex(ValueError, "budget"):
            machine.call({"name": "list_files"})

    def test_ordinary_arm_cannot_run_fr_and_no_arm_has_shell(self):
        for arm in ("files", "fr"):
            machine = changes.Machine(FILES, arm, execute=lambda *_: self.fail("no execution"))
            self.assertIn("error", machine.call({"name": "bash", "arguments": {}}))
        self.assertIn("error", changes.Machine(FILES, "files").call({"name": "fr_explore", "arguments": {"term": "value"}}))

    def test_fr_sees_edited_bytes_in_fresh_snapshot(self):
        roots = []
        def execute(command, *_):
            root = Path(command[command.index("-C") + 1])
            roots.append(root)
            self.assertEqual((root / "module.py").read_text(), "def value():\n    return 43\n")
            return encode({"mode": "names", "profile": {"name": "compact", "source_bytes": 2048}, "rows": []})
        machine = changes.Machine(FILES, "fr", execute=execute)
        machine.call(edit())
        self.assertNotIn("error", machine.call({"name": "fr_explore", "arguments": {"term": "value"}}))
        self.assertNotIn("error", machine.call({"name": "fr_explore", "arguments": {"term": "value"}}))
        self.assertEqual(roots[0], roots[1])
        machine.close()
        self.assertFalse(roots[0].exists())


class Evidence(unittest.TestCase):
    def test_retained_eight_attempts_replay_without_models_or_candidate_execution(self):
        root = ROOT / "tests/agent-eval/opencode/results/2026-10-02-code-changes"
        with patch.object(runner, "bounded_run", side_effect=AssertionError("offline replay only")):
            report = runner.replay(load(root / "plan.json"), root / "attempts")
        self.assertEqual(report, load(root / "collection-report.json"))
        self.assertEqual(report["submitted"], 7)
        self.assertEqual(sum(row["status"] == "failed" for row in report["attempts"]), 1)
        self.assertTrue(all(row["audit"]["metrics"]["fr_requests"] == 0
                            for row in report["attempts"] if row["status"] == "submitted"))

    def audit(self, data):
        return changes.audit(*data, {"files": FILES, "requirement": "task"}, {"arm": "files", "model": "provider/model"})

    def test_replay_reconstructs_patch_and_counts_edit_costs(self):
        result = self.audit(fixture())
        self.assertEqual(base64.b64decode(result["files"]["module.py"]["data"]), b"def value():\n    return 43\n")
        self.assertEqual(result["metrics"]["accepted_edits"], 1)
        self.assertEqual(result["metrics"]["source_available_before_submission_bytes"], 27)
        self.assertGreater(result["metrics"]["edit_argument_bytes"], 64)
        self.assertFalse(result["metrics"]["complete_context_accounting"])

    def test_refused_unknown_tools_remain_visible(self):
        result = self.audit(fixture([{"name": "bash", "arguments": {}}, {"name": "submit_patch", "arguments": {"summary": "no change"}}]))
        self.assertEqual(result["metrics"]["refused_calls"], 1)
        self.assertEqual(result["changed_paths"], [])

    def test_forged_result_missing_call_and_post_submission_call_refuse(self):
        for kind in ("forged", "missing", "late"):
            raw, exported, rows = fixture()
            if kind == "forged":
                rows[1]["result"]["sha256"] = "0" * 64
            elif kind == "missing":
                rows.pop(1)
            else:
                raw, exported, rows = fixture([{"name": "submit_patch", "arguments": {"summary": "done"}}, edit()])
            with self.assertRaises(ValueError):
                self.audit((raw, exported, rows))

    def test_real_mcp_process_edits_only_its_in_memory_snapshot(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "config").write_bytes(encode({"files": FILES, "arm": "files", "binary": "fr"}))
            messages = [{"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": mcp.PROTOCOL}},
                        {"jsonrpc": "2.0", "method": "notifications/initialized"},
                        {"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": edit()}]
            result = subprocess.run([sys.executable, "-B", str(ROOT / "tools/native-changes.py"), "serve", str(root / "config"), str(root / "log")],
                                    input=b"".join(encode(m) + b"\n" for m in messages), capture_output=True, timeout=5)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertFalse(json.loads(result.stdout.splitlines()[-1])["result"]["isError"])
            self.assertEqual(load(root / "config")["files"], FILES)

    def test_freeze_checks_allocation_graders_and_permissions(self):
        with tempfile.TemporaryDirectory() as tmp:
            frozen = plan(Path(tmp))
            self.assertEqual(len(runner.checked(frozen)["cells"]), 2)
            for change in (lambda p: p["graders"].update(change="{}"), lambda p: p["tools"]["files"].pop(),
                           lambda p: p["limits"].update(wall_seconds=999)):
                altered = copy.deepcopy(frozen)
                change(altered["plan"])
                altered["sha256"] = digest(altered["plan"])
                with self.assertRaises(ValueError):
                    runner.checked(altered)
            env = runner.environment(Path("/config"), Path("/log"))
            config = json.loads(env["OPENCODE_CONFIG_CONTENT"])
            self.assertEqual(config["permission"], {"*": "deny", "rehearsal_*": "allow"})
            self.assertTrue(config["mcp"]["rehearsal"]["command"][2].endswith("native-changes.py"))

    def test_collection_never_grades_and_replay_refuses_forged_submission(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            frozen = plan(root)
            cell = next(c for c in runner.checked(frozen)["cells"] if c["arm"] == "files")
            raw, exported, rows = fixture()
            def execute(command, data, out, err, directory, **kwargs):
                if command[-1] == "--version":
                    out.write(b"test-version\n")
                elif "export" in command:
                    out.write(encode(exported))
                else:
                    out.write(raw)
                    (directory / "tools.jsonl").write_bytes(b"".join(encode(r) + b"\n" for r in rows))
                return {"exit_code": 0, "stop_reason": None, "sampled_cpu_seconds": 0}
            with patch.object(runner, "bounded_run", side_effect=execute), patch.object(runner.isolated_grade, "grade", side_effect=AssertionError("no local grading")):
                record = runner.run_attempt(frozen, cell["id"], root / "attempts", root / "fr", Path("opencode"))
            self.assertEqual(record["status"], "submitted", record)
            self.assertEqual(runner.replay(frozen, root / "attempts")["submitted"], 1)
            def grade(candidate, grader, expected):
                self.assertFalse(grader.is_relative_to(candidate))
                self.assertEqual(legacy.identity(grader), expected)
                self.assertEqual((candidate / "module.py").read_text(), "def value():\n    return 43\n")
                return {"outcome": "passed"}
            self.assertEqual(runner.grade_attempts(frozen, root / "attempts", grader=grade)["passed"], 1)
            folder = root / "attempts" / cell["id"]
            (folder / "submission.json").write_bytes(encode(FILES))
            manifest = load(folder / "manifest.json")
            manifest["files"]["submission.json"] = legacy.identity(folder / "submission.json")
            (folder / "manifest.json").write_bytes(encode(manifest))
            with self.assertRaisesRegex(ValueError, "submitted files differ"):
                runner.replay(frozen, root / "attempts")
            with self.assertRaises(FileExistsError):
                runner.run_attempt(frozen, cell["id"], root / "attempts", root / "fr", Path("opencode"))

    def test_frozen_runner_replays_without_current_checkout_on_import_path(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            frozen = plan(root)
            runner.retain_runner(frozen, root / "runner")
            (root / "plan.json").write_bytes(encode(frozen))
            result = subprocess.run([sys.executable, "-B", str(root / "runner/native-changes.py"), "replay",
                str(root / "plan.json"), str(root / "missing-attempts")], cwd=root, capture_output=True, timeout=5)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(result.stdout)["submitted"], 0)


if __name__ == "__main__":
    unittest.main()
