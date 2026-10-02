"""Public authoring, stale previews, byte replay and failure atomicity."""
import base64
import difflib
import hashlib
import os
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval import native_author as author, native_changes as changes
from agent_eval import opencode_changes as runner
from agent_eval.study import digest, encode, load
from agent_eval.test_native_changes import FILES, edit, fixture, plan

HANDLE = "frp1:" + "a" * 32 + ":1"
ARGS = {"path": "module.py", "handle": HANDLE, "body": "return 43\n"}
BEFORE = base64.b64decode(FILES["module.py"]["data"])
AFTER = BEFORE.replace(b"42", b"43")


def review():
    before, after = BEFORE[17:26], AFTER[17:26]
    return {"schema": "fr-author-batch-1", "query": "batch", "changed": True, "applied": False,
            "saved": False, "postconditions_held": True, "validation": "reparse-strict",
            "plan_context_basis": "frpb1:" + "b" * 64, "files_changed": 1,
            "diff": "".join(difflib.unified_diff(BEFORE.decode().splitlines(True), AFTER.decode().splitlines(True), "a/module.py", "b/module.py")),
            "steps": [{"path": "module.py", "handle": HANDLE, "operation": "replace-body", "changed": True,
                       "before_span": {"start": 17, "end": 26}, "before_bytes": 9, "after_bytes": 9,
                       "before_sha256": hashlib.sha256(before).hexdigest(), "after_sha256": hashlib.sha256(after).hexdigest()}]}


class FakeFr:
    def __init__(self, *, fail=None):
        self.commands, self.fail = [], fail

    def __call__(self, command, data, label):
        self.commands.append(command)
        assert not data and label == "fr"
        root = Path(command[command.index("-C") + 1])
        if "history" in command:
            (root / "module.py").write_bytes(AFTER)
            if self.fail == "after-write":
                raise ValueError("failed after write")
            if self.fail == "wrong-write":
                (root / "module.py").write_bytes(AFTER + b"extra\n")
            return encode({"action": "apply", "applied": True, "transaction": 1, "context_basis": "frtb2:" + "c" * 64,
                           "changes": [{"path": "module.py"}]})
        if "--save-plan" in command:
            if self.fail == "save":
                raise ValueError("save failed")
            return encode({"saved": True, "applied": False, "transaction": 1,
                           "plan_context_basis": "frpb1:" + "b" * 64, "transaction_context_basis": "frtb2:" + "c" * 64})
        return encode(review())


class PublicEdits(unittest.TestCase):
    def test_retained_public_edit_pilot_replays_with_its_timeout_and_no_fr_adoption(self):
        root = Path(__file__).resolve().parents[2] / "tests/agent-eval/opencode/results/2026-10-02-public-edit-code-changes"
        report = runner.replay(load(root / "plan.json"), root / "attempts")
        self.assertEqual(report, load(root / "collection-report.json"))
        self.assertEqual(report["submitted"], 3)
        self.assertEqual([r["status"] for r in report["attempts"]], ["submitted", "submitted", "submitted", "failed"])
        self.assertTrue(all(r["audit"]["metrics"]["fr_requests"] == 0 for r in report["attempts"] if r["status"] == "submitted"))

    def machine(self, **kwargs):
        machine = changes.Machine(FILES, "fr", execute=FakeFr(), version=2, **kwargs)
        self.addCleanup(machine.close)
        return machine

    def preview(self, machine):
        result = machine.call({"name": "fr_preview_body", "arguments": ARGS})
        self.assertNotIn("error", result)
        return result

    def test_review_then_apply_reconstructs_exact_bytes_without_executing_candidate(self):
        machine = self.machine()
        preview = self.preview(machine)
        self.assertEqual(machine.files, FILES)
        result = machine.call({"name": "fr_apply_preview", "arguments": {"preview_id": preview["preview_id"]}})
        self.assertNotIn("error", result)
        self.assertEqual(base64.b64decode(machine.files["module.py"]["data"]), AFTER)
        self.assertEqual(machine.edits, 1)
        self.assertEqual(len(machine.execute.commands), 3)
        self.assertIn("--plan-basis", machine.execute.commands[1])
        self.assertIn("--context-basis", machine.execute.commands[2])
        replay = self.machine(replay=True)
        self.assertEqual(replay.call({"name": "fr_preview_body", "arguments": ARGS}, preview), preview)
        self.assertEqual(replay.call({"name": "fr_apply_preview", "arguments": {"preview_id": preview["preview_id"]}}, result), result)
        self.assertEqual(replay.files, machine.files)
        self.assertEqual(replay.execute.commands, [])

    def test_edits_and_second_previews_invalidate_old_reviews(self):
        machine = self.machine()
        preview = self.preview(machine)
        machine.call(edit())
        self.assertIn("error", machine.call({"name": "fr_apply_preview", "arguments": {"preview_id": preview["preview_id"]}}))
        machine = self.machine()
        preview = self.preview(machine)
        machine.call({"name": "fr_preview_body", "arguments": {**ARGS, "handle": "stale"}})
        self.assertIn("error", machine.call({"name": "fr_apply_preview", "arguments": {"preview_id": preview["preview_id"]}}))

    def test_wrong_id_no_preview_double_apply_and_post_submit_refuse(self):
        machine = self.machine()
        request = {"name": "fr_apply_preview", "arguments": {"preview_id": "0" * 64}}
        self.assertIn("error", machine.call(request))
        preview = self.preview(machine)
        self.assertIn("error", machine.call(request))
        request["arguments"]["preview_id"] = preview["preview_id"]
        self.assertNotIn("error", machine.call(request))
        self.assertIn("error", machine.call(request))
        machine.call({"name": "submit_patch", "arguments": {"summary": "done"}})
        self.assertIn("error", machine.call({"name": "fr_preview_body", "arguments": ARGS}))

    def test_failed_save_or_apply_never_commits_partial_source(self):
        for failure in ("save", "after-write", "wrong-write"):
            machine = self.machine()
            machine.execute = FakeFr(fail=failure)
            preview = self.preview(machine)
            result = machine.call({"name": "fr_apply_preview", "arguments": {"preview_id": preview["preview_id"]}})
            self.assertIn("error", result)
            self.assertEqual(machine.files, FILES)
            self.assertEqual(machine.edits, 0)
            self.assertEqual((machine.workspace() / "module.py").read_bytes(), BEFORE)
            self.assertIsNone(machine.author.pending)

    def test_schema_versions_and_arm_permissions_are_frozen(self):
        for version, arm in ((1, "fr"), (1, "files"), (2, "files")):
            machine = changes.Machine(FILES, arm, version=version)
            self.assertIn("error", machine.call({"name": "fr_preview_body", "arguments": ARGS}))
        self.assertNotIn(author.GUIDANCE, changes.prompt("files", 2))
        self.assertIn(author.GUIDANCE, changes.prompt("fr", 2))
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            frozen = plan(root)
            original = frozen["plan"]["source_plan"]["plan"]["manifest"]
            frozen = runner.freeze(original, root, root / "fr", public_edits=True)
            runner.checked(frozen)
            frozen["plan"]["edit_guidance"] += " changed"
            frozen["sha256"] = digest(frozen["plan"])
            with self.assertRaisesRegex(ValueError, "guidance"):
                runner.checked(frozen)

    def test_forged_clipped_out_of_scope_and_changed_byte_claims_refuse(self):
        mutations = [lambda r: r.update(diff=r["diff"].replace("return 42", "return 44")),
                     lambda r: r.update(diff=r["diff"].replace("module.py", "other.py")),
                     lambda r: r.update(diff=r["diff"][:-5]),
                     lambda r: r.update(saved=True), lambda r: r.update(postconditions_held=False),
                     lambda r: r["steps"][0].update(after_sha256="0" * 64),
                     lambda r: r["steps"][0].update(before_span={"start": 20, "end": 21}),
                     lambda r: r["steps"][0].update(handle="wrong")]
        for mutate in mutations:
            altered = review()
            mutate(altered)
            with self.assertRaises(ValueError):
                author.proposal(FILES, ARGS, altered)

    def test_unified_diff_multiple_hunks_unicode_and_missing_final_newline(self):
        before = "\u03bb\n" + "a\n" * 12 + "z\n"
        after = "\u00e9\n" + "a\n" * 12 + "last\n"
        diff = "".join(difflib.unified_diff(before.splitlines(True), after.splitlines(True), "a/x.py", "b/x.py"))
        self.assertEqual(author.patched(before.encode(), diff, "x.py"), after.encode())
        diff = "--- a/x.py\n+++ b/x.py\n@@ -1 +1 @@\n-old\n\\ No newline at end of file\n+new\n\\ No newline at end of file\n"
        self.assertEqual(author.patched(b"old", diff, "x.py"), b"new")
        for changed in (diff.replace("@@ -1 +1", "@@ -2 +1"), diff.replace("@@ -1 +1", "@@ -1,2 +1"), diff + "garbage\n"):
            with self.assertRaises(ValueError):
                author.patched(b"old", changed, "x.py")

    def test_transcript_replay_counts_authoring_and_rejects_changed_submission(self):
        preview = self.preview(self.machine())
        calls = [{"name": "fr_preview_body", "arguments": ARGS},
                 {"name": "fr_apply_preview", "arguments": {"preview_id": preview["preview_id"]}},
                 {"name": "submit_patch", "arguments": {"summary": "changed, no tests run"}}]
        raw, exported, rows = fixture(calls, "fr", 2, FakeFr())
        result = changes.audit(raw, exported, rows, {"files": FILES, "requirement": "task"}, {"arm": "fr", "model": "provider/model"}, 2)
        self.assertEqual(base64.b64decode(result["files"]["module.py"]["data"]), AFTER)
        self.assertEqual(result["metrics"]["author_tool_calls"], 2)
        self.assertEqual(result["metrics"]["author_applied"], 1)
        self.assertGreater(result["metrics"]["author_result_bytes"], 500)
        self.assertFalse(result["metrics"]["author_semantics_reexecuted"])
        self.assertFalse(result["metrics"]["source_disclosure_complete"])
        rows[1]["result"]["submission_sha256"] = "0" * 64
        with self.assertRaises(ValueError):
            changes.audit(raw, exported, rows, {"files": FILES, "requirement": "task"}, {"arm": "fr", "model": "provider/model"}, 2)


@unittest.skipUnless(os.environ.get("FR_NATIVE_AUTHOR_BINARY"), "real CLI control runs in the native CI shard")
class RealAuthor(unittest.TestCase):
    def test_public_python_and_rust_body_edits_preserve_surrounding_code(self):
        examples = [("module.py", "def value():\n    return 42\n\ndef other():\n    return 9\n", "return 43\n"),
                    ("lib.rs", "pub fn value() -> i32 { 42 }\npub fn other() -> i32 { 9 }\n", "{ 43 }"),
                    ("nested.py", "class Demo:\n    @decorate\n    async def value(self):\n        return 42\n", "return 43\n")]
        for path, source, body in examples:
            files = {path: {"data": base64.b64encode(source.encode()).decode(), "executable": False}}
            machine = changes.Machine(files, "fr", Path(os.environ["FR_NATIVE_AUTHOR_BINARY"]).resolve(), version=2)
            self.addCleanup(machine.close)
            found = machine.call({"name": "fr_explore", "arguments": {"term": "value", "path": path}})
            self.assertNotIn("error", found)
            handle = found["rows"][0]["handle"]
            args = {"path": path, "handle": handle, "body": body}
            preview = machine.call({"name": "fr_preview_body", "arguments": args})
            self.assertNotIn("error", preview)
            self.assertEqual(machine.files, files)
            applied = machine.call({"name": "fr_apply_preview", "arguments": {"preview_id": preview["preview_id"]}})
            self.assertNotIn("error", applied)
            self.assertEqual(base64.b64decode(machine.files[path]["data"]).decode(), source.replace("42", "43"))
            replay = changes.Machine(files, "fr", replay=True, version=2)
            self.assertEqual(replay.call({"name": "fr_preview_body", "arguments": args}, preview), preview)
            self.assertEqual(replay.call({"name": "fr_apply_preview", "arguments": {"preview_id": preview["preview_id"]}}, applied), applied)
            self.assertEqual(replay.files, machine.files)
            refused = machine.call({"name": "fr_preview_body", "arguments": args})
            self.assertIn("error", refused, "old handles must not edit new source")
            self.assertEqual(replay.files, machine.files)


if __name__ == "__main__":
    unittest.main()
