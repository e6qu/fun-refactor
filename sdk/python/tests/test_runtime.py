import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
from unittest.mock import patch

from fr_ir.context import (
    DirectoryObjectStore,
    MemoryObjectStore,
    restore_stored_value,
    store_merkle_value,
)
from fr_ir.ir import (
    TaskChange,
    TaskDelivery,
    TaskTarget,
    merkle_object_digest,
)
from fr_ir.runtime import Disclosure, DisclosureAction, FrClient, FrRuntimeError
from fr_ir.context import _context_materialization_admitted, _object_store_admitted
from fr_ir.runtime import _session_step
import pytest


def completed(value, code=0, stderr=b""):
    return subprocess.CompletedProcess([], code, json.dumps(value).encode(), stderr)


class TestRuntime:
    def setup_method(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.client = FrClient(self.root, executable="/opt/fr", timeout=7, max_output_bytes=8192)

    def teardown_method(self):
        self.temp.cleanup()

    @patch("fr_ir.runtime.subprocess.run")
    def test_client_returns_structured_values_without_a_shell(self, run):
        run.return_value = completed({"schema": "example-1", "rows": [["handle"]]})
        report = self.client.project("find", "render")
        assert report.schema == "example-1"
        assert report.at("/rows/0/0") == "handle"
        command = run.call_args.args[0]
        assert command[:4] == ["/opt/fr", "--json", "-C", str(self.client.root)]
        assert command[4:] == ["project", "find", "render"]
        assert "shell" not in run.call_args.kwargs
        with pytest.raises(FrRuntimeError, match="absent"):
            report.at("/rows/1")
        with pytest.raises(FrRuntimeError, match="non-canonical escape"):
            report.at("/rows~")

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
        assert len(actions) == 1
        assert actions[0].object_digest == "a" * 64
        assert self.client.follow(actions[0]).at("/status") == "revealed"
        assert run.call_args.args[0][4:] == action
        with pytest.raises(FrRuntimeError, match="exact project disclose"):
            DisclosureAction.from_data({"arguments": ["history", "undo", "1"]})
        with pytest.raises(FrRuntimeError, match="token budget"):
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
        assert len(actions) == 1
        assert actions[0].arguments == tuple(arguments)
        assert actions[0].kind == "continuation"
        assert actions[0].reason == "more-children"
        with pytest.raises(FrRuntimeError, match="unsupported shape"):
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
            assert command[-2:] == ["--basis", basis]
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
        assert result.passed
        assert run.call_count == 2
        assert run.call_args.kwargs["input"] == review.manifest
        object.__setattr__(review, "manifest", review.manifest + b" ")
        with pytest.raises(FrRuntimeError, match="changed before"):
            self.client.execute(review)

    @patch("fr_ir.runtime.subprocess.run")
    def test_failures_and_local_limits_remain_bounded(self, run):
        run.return_value = completed({"error": "stale handle"}, 2, b"more detail")
        with pytest.raises(FrRuntimeError) as failure:
            self.client.project("show", "stale")
        assert failure.value.exit_code == 2
        assert str(failure.value) == "stale handle"
        with pytest.raises(FrRuntimeError, match="string-argument"):
            self.client.project("x" * 20_000)
        with pytest.raises(FrRuntimeError, match="64 KiB"):
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
        assert accepted == [
            (0, 0, True, False, False, 1),
            (0, 0, True, False, True, 1),
            (0, 0, True, True, False, 1),
            (0, 0, True, True, True, 1),
            (1, 1, True, True, True, 2),
        ]

    def test_context_admission_kernels_cover_their_boundaries(self):
        assert _context_materialization_admitted(64, 64, True, True, True)
        assert not _context_materialization_admitted(0, 0, True, True, True)
        assert not _context_materialization_admitted(65, 64, True, True, True)
        for missing in range(3):
            evidence = [True, True, True]
            evidence[missing] = False
            assert not _context_materialization_admitted(1, 1, *evidence)

        assert _object_store_admitted(
            65_536, 67_108_864, True, True, True,
        )
        assert not _object_store_admitted(0, 0, True, True, True)
        assert not _object_store_admitted(
            65_537, 67_108_864, True, True, True,
        )
        assert not _object_store_admitted(
            1, 67_108_865, True, True, True,
        )
        for missing in range(3):
            evidence = [True, True, True]
            evidence[missing] = False
            assert not _object_store_admitted(1, 0, *evidence)

    @patch("fr_ir.runtime.subprocess.run")
    def test_context_reaches_a_later_section_and_caches_its_value(self, run):
        revision = "1" * 64
        view_basis = "frdv1:" + "2" * 64
        object_root = "3" * 64
        handle = "frp1:rev:1"

        def base(**values):
            return {
                "schema": "fr-progressive-disclosure-1",
                "token_budget": {"limit": 4096, "used_upper_bound": 900},
                "revision": revision,
                "view_basis": view_basis,
                "view": "evidence",
                "profile": "compact",
                "context_basis": "frcb1:" + "4" * 64,
                "commitment": {"object_root": object_root},
                "target": {"handle": handle, "kind": "function", "name": "render"},
                **values,
            }

        root_action = ["project", "disclose", handle, "--reveal", "frh1:root"]
        next_action = root_action + ["--cursor", "frdc1:next"]
        code_action = ["project", "disclose", handle, "--reveal", "frh1:code"]
        calls_action = ["project", "disclose", handle, "--reveal", "frh1:calls"]
        code_map = {"nodes": [{"name": "render", "kind": "function"}], "edges": []}
        call_traces = [{"caller": "caller", "callee": "render"}]
        run.side_effect = [
            completed(base(frontier=[{
                "domain": "project-evidence", "address": f"basis#/model",
                "object_digest": object_root, "reveal": {"arguments": root_action},
            }])),
            completed(base(revealed={
                "domain": "project-evidence", "address": "basis#/model",
                "object_digest": object_root,
                "children": [{"key": "call_traces", "hole": {
                    "domain": "project-evidence", "address": "basis#/model/call_traces",
                    "object_digest": "5" * 64,
                    "reveal": {"arguments": calls_action},
                }}],
            }, continuation={"arguments": next_action, "reason": "more-children"})),
            completed(base(revealed={
                "domain": "project-evidence", "address": "basis#/model",
                "object_digest": object_root,
                "children": [{"key": "code_map", "hole": {
                    "domain": "project-evidence", "address": "basis#/model/code_map",
                    "object_digest": merkle_object_digest(code_map),
                    "reveal": {"arguments": code_action},
                }}],
            })),
            completed(base(revealed={
                "domain": "project-evidence", "address": "basis#/model/code_map",
                "object_digest": merkle_object_digest(code_map), "value": code_map,
            })),
            completed(base(revealed={
                "domain": "project-evidence", "address": "basis#/model/call_traces",
                "object_digest": merkle_object_digest(call_traces), "value": call_traces,
            })),
        ]
        store = MemoryObjectStore()
        session = self.client.context(handle, view="evidence", store=store)
        code_map_value = session.materialize_section("code_map")
        assert code_map_value["nodes"][0]["name"] == "render"
        assert session.calls == 4
        digest = merkle_object_digest(code_map)
        assert digest in session.cached_digests
        assert restore_stored_value(store, digest) == code_map
        assert session.materialize_section("call_traces") == call_traces
        packet = session.packet(
            {"node": "/model/code_map/nodes/0"}, include_actions=False, max_bytes=1024,
        )
        assert packet.at("/selected/node/name") == "render"
        assert packet.at("/serialized_bytes") <= 1024
        assert run.call_count == 5

    @patch("fr_ir.runtime.subprocess.run")
    def test_follow_refuses_a_response_from_another_disclosure_session(self, run):
        def report(revision, **values):
            return {
                "schema": "fr-progressive-disclosure-1",
                "token_budget": {"limit": 4096, "used_upper_bound": 900},
                "revision": revision, "view_basis": "frdv1:" + "2" * 64,
                "view": "evidence", "profile": "compact",
                "commitment": {"object_root": "3" * 64},
                "target": {"handle": "frp1:rev:1"}, **values,
            }
        action = ["project", "disclose", "frp1:rev:1", "--reveal", "frh1:root"]
        run.side_effect = [
            completed(report("1" * 64, frontier=[{
                "domain": "project-evidence", "address": "basis#/model",
                "reveal": {"arguments": action},
            }])),
            completed(report("9" * 64, status="revealed")),
        ]
        initial = self.client.disclose("frp1:rev:1", view="evidence")
        with pytest.raises(FrRuntimeError, match="crossed its bound session"):
            self.client.follow(initial.actions()[0])

    def test_directory_object_store_round_trips_and_rejects_conflicts(self):
        store = DirectoryObjectStore(self.root / "objects")
        value = {"service": {"routes": ["GET /items", "POST /items"]}}
        stored = store_merkle_value(store, value)
        assert stored.objects > 1
        assert restore_stored_value(store, stored.digest) == value
        store_merkle_value(store, value, expected_digest=stored.digest)
        with pytest.raises(FrRuntimeError, match="advertised object digest"):
            store_merkle_value(store, value, expected_digest="0" * 64)
        with pytest.raises(FrRuntimeError, match="conflicting immutable"):
            store.put(stored.digest, {"schema": "wrong"})

        class DroppingStore:
            def get(self, digest):
                return None

            def put(self, digest, record):
                pass

        with pytest.raises(FrRuntimeError, match="immutable write verification"):
            store_merkle_value(DroppingStore(), value)

    @patch("fr_ir.runtime.subprocess.run")
    def test_materialization_reconstructs_pages_empty_arrays_and_unicode_strings(self, run):
        revision = "1" * 64
        view_basis = "frdv1:" + "2" * 64
        object_root = "3" * 64
        handle = "frp1:rev:1"
        large = "é" * 100
        value = {"empty": [], "large": large, "name": "service"}
        value_digest = merkle_object_digest(value)
        empty_digest = merkle_object_digest([])
        large_digest = merkle_object_digest(large)

        def base(**values):
            return {
                "schema": "fr-progressive-disclosure-1",
                "token_budget": {"limit": 4096, "used_upper_bound": 1000},
                "revision": revision, "view_basis": view_basis,
                "view": "evidence", "profile": "compact",
                "commitment": {"object_root": object_root},
                "target": {"handle": handle}, **values,
            }

        def command(hole, *suffix):
            return ["project", "disclose", handle, "--reveal", hole, *suffix]

        root = command("frh1:root")
        section = command("frh1:section")
        empty = command("frh1:empty")
        page = command("frh1:section", "--cursor", "frdc1:section:2")
        string = command("frh1:string")
        string_page = command("frh1:string", "--cursor", "frdc1:string:100")
        run.side_effect = [
            completed(base(frontier=[{
                "domain": "project-evidence", "address": "basis#/model",
                "object_digest": object_root, "reveal": {"arguments": root},
            }])),
            completed(base(revealed={
                "domain": "project-evidence", "address": "basis#/model",
                "object_digest": object_root,
                "children": [{"key": "code_map", "hole": {
                    "domain": "project-evidence", "address": "basis#/model/code_map",
                    "object_digest": value_digest, "reveal": {"arguments": section},
                }}],
            })),
            completed(base(revealed={
                "domain": "project-evidence", "address": "basis#/model/code_map",
                "object_digest": value_digest, "value_kind": "object",
                "children": [
                    {"key": "empty", "hole": {
                        "domain": "project-evidence",
                        "address": "basis#/model/code_map/empty",
                        "object_digest": empty_digest, "reveal": {"arguments": empty},
                    }},
                    {"key": "name", "object_digest": merkle_object_digest("service"),
                     "value": "service"},
                ],
            }, continuation={"arguments": page, "reason": "more-children"})),
            completed(base(revealed={
                "domain": "project-evidence", "address": "basis#/model/code_map/empty",
                "object_digest": empty_digest, "children": [],
            })),
            completed(base(revealed={
                "domain": "project-evidence", "address": "basis#/model/code_map",
                "object_digest": value_digest, "value_kind": "object",
                "children": [{"key": "large", "hole": {
                    "domain": "project-evidence",
                    "address": "basis#/model/code_map/large",
                    "object_digest": large_digest, "reveal": {"arguments": string},
                }}],
            })),
            completed(base(revealed={
                "domain": "project-evidence", "address": "basis#/model/code_map/large",
                "object_digest": large_digest, "value_fragment": "é" * 50,
                "offset": 0, "returned_bytes": 100, "total_bytes": 200,
            }, continuation={"arguments": string_page, "reason": "more-string"})),
            completed(base(revealed={
                "domain": "project-evidence", "address": "basis#/model/code_map/large",
                "object_digest": large_digest, "value_fragment": "é" * 50,
                "offset": 100, "returned_bytes": 100, "total_bytes": 200,
            })),
        ]
        store = MemoryObjectStore()
        session = self.client.context(handle, view="evidence", store=store)
        assert session.materialize_section("code_map") == value
        assert session.calls == 7
        assert restore_stored_value(store, value_digest) == value
        assert session.packet(
            {"large": "/model/code_map/large"}, include_actions=False, max_bytes=2048,
        ).at("/selected/large") == large

    def test_context_packet_and_traversal_bounds_refuse_locally(self):
        with pytest.raises(FrRuntimeError, match="canonical RFC 6901"):
            from fr_ir.context import ContextSession
            ContextSession.reveal(object(), "/bad~")
        store = MemoryObjectStore()
        with pytest.raises(FrRuntimeError, match="lowercase SHA-256"):
            store.get("ABC")

    @patch("fr_ir.runtime.subprocess.run")
    def test_materialization_never_issues_a_call_beyond_its_bound(self, run):
        handle = "frp1:rev:1"
        identity = {
            "schema": "fr-progressive-disclosure-1",
            "token_budget": {"limit": 4096, "used_upper_bound": 900},
            "revision": "1" * 64, "view_basis": "frdv1:" + "2" * 64,
            "view": "evidence", "profile": "compact",
            "commitment": {"object_root": "3" * 64},
            "target": {"handle": handle},
        }
        child_action = ["project", "disclose", handle, "--reveal", "frh1:child"]
        nested_action = ["project", "disclose", handle, "--reveal", "frh1:nested"]
        run.side_effect = [
            completed({**identity, "revealed": {
                "address": "basis#/model", "object_digest": "3" * 64,
                "value_kind": "object", "children": [{"key": "code_map", "hole": {
                    "address": "basis#/model/code_map", "object_digest": "4" * 64,
                    "reveal": {"arguments": child_action},
                }}],
            }}),
            completed({**identity, "revealed": {
                "address": "basis#/model/code_map", "object_digest": "4" * 64,
                "value_kind": "object", "children": [{"key": "nodes", "hole": {
                    "address": "basis#/model/code_map/nodes", "object_digest": "5" * 64,
                    "reveal": {"arguments": nested_action},
                }}],
            }}),
        ]
        session = self.client.context(handle, view="evidence")
        with pytest.raises(FrRuntimeError, match="call bound"):
            session.materialize_section("code_map", max_calls=1)
        assert run.call_count == 2
