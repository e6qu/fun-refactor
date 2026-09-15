import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from fr_ir import (
    Disclosure,
    FrClient,
    FrRuntimeError,
    TaskChange,
    TaskDelivery,
    TaskTarget,
)
from fr_ir.runtime import _session_step


def completed(value, code=0, stderr=b""):
    return subprocess.CompletedProcess([], code, json.dumps(value).encode(), stderr)


class RuntimeTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.client = FrClient(self.root, executable="/opt/fr", timeout=7, max_output_bytes=8192)

    def tearDown(self):
        self.temp.cleanup()

    @patch("fr_ir.runtime.subprocess.run")
    def test_client_returns_structured_values_without_a_shell(self, run):
        run.return_value = completed({"schema": "example-1", "rows": [["handle"]]})
        report = self.client.project("find", "render")
        self.assertEqual(report.schema, "example-1")
        self.assertEqual(report.at("/rows/0/0"), "handle")
        command = run.call_args.args[0]
        self.assertEqual(command[:4], ["/opt/fr", "--json", "-C", str(self.client.root)])
        self.assertEqual(command[4:], ["project", "find", "render"])
        self.assertNotIn("shell", run.call_args.kwargs)
        with self.assertRaisesRegex(FrRuntimeError, "absent"):
            report.at("/rows/1")

    @patch("fr_ir.runtime.subprocess.run")
    def test_disclosure_follows_only_exact_server_actions(self, run):
        action = ["project", "disclose", "frp1:rev:1", "--reveal", "frh1:hole",
                  "--token-limit", "4096"]
        initial = {
            "schema": "fr-progressive-disclosure-1",
            "token_budget": {"limit": 4096, "used_upper_bound": 900},
            "frontier": [{
                "domain": "project-evidence", "address": "basis#/model",
                "object_digest": "a" * 64, "reveal": {"arguments": action},
            }],
        }
        revealed = {
            "schema": "fr-progressive-disclosure-1",
            "token_budget": {"limit": 4096, "used_upper_bound": 1000},
            "status": "revealed", "revealed": {"domain": "project-evidence"},
        }
        run.side_effect = [completed(initial), completed(revealed)]
        disclosure = self.client.disclose("frp1:rev:1", view="evidence")
        actions = disclosure.actions(domain="project-evidence")
        self.assertEqual(len(actions), 1)
        self.assertEqual(actions[0].object_digest, "a" * 64)
        self.assertEqual(self.client.follow(actions[0]).at("/status"), "revealed")
        self.assertEqual(run.call_args.args[0][4:], action)
        with self.assertRaisesRegex(FrRuntimeError, "exact project disclose"):
            from fr_ir import DisclosureAction
            DisclosureAction.from_data({"arguments": ["history", "undo", "1"]})
        with self.assertRaisesRegex(FrRuntimeError, "token budget"):
            Disclosure({**initial, "token_budget": {"limit": 10, "used_upper_bound": 11}}, ())

    def test_disclosure_exposes_exact_page_continuations(self):
        arguments = ["project", "disclose", "frp1:rev:1", "--reveal", "frh1:hole",
                     "--token-limit", "4096", "--cursor", "frdc1:page:1"]
        disclosure = Disclosure({
            "schema": "fr-progressive-disclosure-1",
            "token_budget": {"limit": 4096, "used_upper_bound": 900},
            "revealed": {
                "domain": "project-evidence", "address": "basis#/model",
                "object_digest": "a" * 64,
            },
            "continuation": {"arguments": arguments, "reason": "more-children"},
        }, ())
        actions = disclosure.actions(domain="project-evidence")
        self.assertEqual(len(actions), 1)
        self.assertEqual(actions[0].arguments, tuple(arguments))
        self.assertEqual(actions[0].kind, "continuation")
        self.assertEqual(actions[0].reason, "more-children")
        with self.assertRaisesRegex(FrRuntimeError, "unsupported shape"):
            from fr_ir import DisclosureAction
            DisclosureAction.from_continuation({"arguments": arguments, "extra": True})

    @patch("fr_ir.runtime.subprocess.run")
    def test_review_and_execute_reuse_exact_manifest_and_basis(self, run):
        basis = "frtc1:" + "b" * 64

        def response(command, **kwargs):
            manifest = kwargs["input"]
            digest = hashlib.sha256(manifest).hexdigest()
            if "--write" not in command:
                return completed({
                    "schema": "fr-task-change-1", "ready": True, "executed": False,
                    "manifest_sha256": digest, "task_change_basis": basis,
                })
            self.assertEqual(command[-2:], ["--basis", basis])
            return completed({
                "schema": "fr-task-change-1", "executed": True, "passed": True,
                "task_change_basis": basis,
            })

        run.side_effect = response
        change = TaskChange(
            [], [TaskTarget("body", "frp1:rev:1", "replace-body", fragment="{ 2 }")],
            {"files-changed": 1}, ["unit"], TaskDelivery(),
        )
        review = self.client.review(change)
        result = self.client.execute(review)
        self.assertTrue(result.passed)
        self.assertEqual(run.call_count, 2)
        self.assertEqual(run.call_args.kwargs["input"], review.manifest)
        object.__setattr__(review, "manifest", review.manifest + b" ")
        with self.assertRaisesRegex(FrRuntimeError, "changed before"):
            self.client.execute(review)

    @patch("fr_ir.runtime.subprocess.run")
    def test_failures_and_local_limits_remain_bounded(self, run):
        run.return_value = completed({"error": "stale handle"}, 2, b"more detail")
        with self.assertRaises(FrRuntimeError) as failure:
            self.client.project("show", "stale")
        self.assertEqual(failure.exception.exit_code, 2)
        self.assertEqual(str(failure.exception), "stale handle")
        with self.assertRaisesRegex(FrRuntimeError, "string-argument"):
            self.client.project("x" * 20_000)
        with self.assertRaisesRegex(FrRuntimeError, "64 KiB"):
            self.client.call("task-change", "--from", "-", input_bytes=b"x" * 65_537)

    def test_session_kernel_covers_all_finite_inputs(self):
        accepted = []
        for state in range(3):
            for action in range(2):
                for preview in (False, True):
                    for manifest in (False, True):
                        for basis in (False, True):
                            outcome = _session_step(state, action, preview, manifest, basis)
                            if outcome != 3:
                                accepted.append((state, action, preview, manifest, basis, outcome))
        self.assertEqual(accepted, [
            (0, 0, True, False, False, 1),
            (0, 0, True, False, True, 1),
            (0, 0, True, True, False, 1),
            (0, 0, True, True, True, 1),
            (1, 1, True, True, True, 2),
        ])


if __name__ == "__main__":
    unittest.main()
