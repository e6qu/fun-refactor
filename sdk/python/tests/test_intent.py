from fr_ir.context import MemoryObjectStore, merkle_object_digest, restore_stored_value
from fr_ir.intent import (AgentIntent, IntentAction, IntentNeed, compile_intent,
                          execute_intent, prepare_intent)
from fr_ir.ir import ProjectRequest, TaskChange, TaskDelivery, TaskTarget
from fr_ir.runtime import FrReport, FrRuntimeError
from fr_ir.intent import _intent_admitted, _intent_section_allowed
import hashlib
import json
import pytest


IDENTITY = ("r", "v", "o", "target", "evidence", "compact")


class FakeDisclosure:
    def session_identity(self):
        return IDENTITY


class FakeSession:
    def __init__(self):
        self.calls = 1
        self.sections = []
        self.latest = FakeDisclosure()
        self.reports = (FakeDisclosure(),)

    def materialize_section(self, section, *, max_calls):
        if max_calls < 1:
            raise AssertionError("invalid test call bound")
        self.sections.append((section, max_calls))
        self.calls += 1
        return {"section": section, "rows": [1]}

    def packet(self, selected, *, include_actions, max_bytes):
        assert not include_actions
        value = {"schema": "fr-agent-context-1", "target": {"handle": "target"},
                 "selected": {name: pointer for name, pointer in selected.items()},
                 "serialized_bytes": min(max_bytes, 1200)}
        return FrReport(value, ())

    def session_identity(self):
        return IDENTITY


class FakeClient:
    def __init__(self):
        self.session = FakeSession()
        self.arguments = None

    def context(self, target, **kwargs):
        self.arguments = (target, kwargs)
        return self.session


class FakeNativeClient:
    def __init__(self, mutate=None):
        self.input = None
        self.mutate = mutate

    def call(self, *arguments, input_bytes=None):
        self.input = input_bytes
        intent = json.loads(input_bytes)
        if "--write" in arguments:
            basis = arguments[arguments.index("--basis") + 1]
            digest = hashlib.sha256(input_bytes).hexdigest()
            return FrReport({
                "schema": "fr-agent-action-result-1",
                "intent_basis": f"frai1:{digest}",
                "action_basis": basis,
                "reviewed_context_omitted": True,
                "executed": True,
                "passed": True,
                "action": {"schema": "fr-task-change-1", "executed": True,
                           "passed": True},
            }, arguments)
        selected = {need["name"]: {"section": need["section"]} for need in intent["needs"]}
        digest = hashlib.sha256(input_bytes).hexdigest()
        value = {
            "schema": "fr-agent-context-1", "revision": "r", "context_basis": "c",
            "view_basis": "v", "object_root": "o", "view": "evidence", "profile": "compact",
            "target": {"handle": intent["target"]}, "calls": 0,
            "intent": {"schema": "fr-agent-intent-1", "purpose": intent["purpose"],
                       "manifest_sha256": digest, "basis": f"frai1:{digest}"},
            "selected": selected,
            "object_digests": {name: merkle_object_digest(item) for name, item in selected.items()},
            "cached_objects": [], "serialized_bytes": 0,
            "execution": {"engine": "native", "project_snapshots": 1,
                          "progressive_disclosure_calls": 0},
            "limits": {"packet_bytes": intent["packet_limit"],
                       "progressive_disclosure_calls": intent["call_limit"]},
        }
        action = intent.get("action")
        if action is not None:
            task_bytes = json.dumps(action["task_change"], ensure_ascii=False, sort_keys=True,
                                    separators=(",", ":"), allow_nan=False).encode()
            value["action"] = {
                "schema": "fr-agent-action-1", "basis": f"fraa1:{'0' * 64}",
                "review": {
                    "schema": "fr-task-change-1",
                    "manifest_sha256": hashlib.sha256(task_bytes).hexdigest(),
                    "targets": [{"handle": intent["target"]}],
                    "ready": True, "executed": False,
                },
            }
        if self.mutate is not None:
            self.mutate(value)
        for _ in range(4):
            encoded = json.dumps(value, ensure_ascii=False, sort_keys=True,
                                 separators=(",", ":"), allow_nan=False).encode()
            value["serialized_bytes"] = len(encoded)
        return FrReport(value, arguments)


class TestIntent:
    @staticmethod
    def change(target="target", *, requests=()):
        return TaskChange(
            requests,
            [TaskTarget("render", target, "replace-body", fragment="{ 1 }")],
            {"files-changed": 1, "edits": 1, "changed-operations": 1,
             "paths-changed": ["src/lib.rs"]},
            ["unit"],
            TaskDelivery(patch="artifacts/change.patch"),
        )

    def test_purposes_expand_to_stable_high_level_evidence(self):
        expected = {
            "understand": ("code_map",),
            "trace": ("code_map", "call_traces", "sources_and_sinks"),
            "change": ("code_map", "impact"),
            "migrate": ("code_map", "impact", "sources_and_sinks"),
            "prove": ("code_map", "impact"),
        }
        for purpose, sections in expected.items():
            intent = AgentIntent("target", purpose)
            assert tuple(need.section for need in intent.needs) == sections
            assert intent.to_data()["schema"] == "fr-agent-intent-1"

    def test_prepare_materializes_each_section_once_and_returns_only_selections(self):
        client = FakeClient()
        intent = AgentIntent("target", "trace", (
            IntentNeed("entry", "code_map", "/nodes/0"),
            IntentNeed("edges", "code_map", "/edges"),
            IntentNeed("flows", "sources_and_sinks"),
        ), call_limit=7, packet_limit=2048)
        prepared = prepare_intent(client, intent)
        assert [row[0] for row in client.session.sections] == \
                         ["code_map", "sources_and_sinks"]
        assert client.session.sections[1][1] == 6
        assert prepared.at("/selected/entry") == "/model/code_map/nodes/0"
        assert set(prepared.at("/selected")) == {"entry", "edges", "flows"}

    def test_invalid_intents_refuse_before_any_subprocess_work(self):
        invalid = [
            lambda: AgentIntent("", "trace"),
            lambda: AgentIntent("target", "invent"),
            lambda: AgentIntent("target", "trace", (IntentNeed("x", "code_map", "/bad~"),)),
            lambda: AgentIntent("target", "trace", (IntentNeed("x", "bad-name"),)),
            lambda: AgentIntent("target", "trace", (IntentNeed("x", "invented"),)),
            lambda: AgentIntent("target", "trace", (IntentNeed("same", "code_map"),
                                                     IntentNeed("same", "impact"))),
            lambda: AgentIntent("target", "understand", (IntentNeed("x", "impact"),)),
            lambda: AgentIntent("target", "trace", call_limit=0),
            lambda: AgentIntent("target", "trace", call_limit=513),
            lambda: AgentIntent("target", "trace", packet_limit=1000),
        ]
        for build in invalid:
            with pytest.raises(FrRuntimeError):
                build()

    def test_native_compile_uses_one_call_and_verifies_object_storage(self):
        client = FakeNativeClient()
        store = MemoryObjectStore()
        intent = AgentIntent("target", "trace")
        compiled = compile_intent(client, intent, store=store)
        assert json.loads(client.input) == intent.to_data()
        assert compiled.at("/calls") == 0
        assert set(compiled.at("/selected")) == {"code_map", "call_traces", "sources_and_sinks"}
        assert len(compiled.stored_digests) == 3
        for name, digest in compiled.at("/object_digests").items():
            assert restore_stored_value(store, digest) == compiled.at(f"/selected/{name}")

    def test_native_compile_refuses_changed_digest_and_identity(self):
        for mutate in (
            lambda value: value["object_digests"].__setitem__("code_map", "0" * 64),
            lambda value: value["target"].__setitem__("handle", "other"),
            lambda value: value["intent"].__setitem__("purpose", "change"),
            lambda value: value["execution"].__setitem__("project_snapshots", 2),
            lambda value: value["limits"].__setitem__("packet_bytes", 65_536),
            lambda value: value.__setitem__("view_basis", None),
        ):
            with pytest.raises(FrRuntimeError):
                compile_intent(FakeNativeClient(mutate), AgentIntent("target", "understand"))

    def test_native_change_intent_compiles_and_executes_one_retained_review(self):
        client = FakeNativeClient()
        intent = AgentIntent(
            "target", "change", packet_limit=65_536,
            action=IntentAction(self.change()),
        )
        compiled = compile_intent(client, intent)
        assert compiled.action_basis == f"fraa1:{'0' * 64}"
        assert compiled.at("/action/review/targets/0/handle") == "target"
        result = execute_intent(client, compiled)
        assert result.passed
        assert result.at("/action_basis") == compiled.action_basis
        assert json.loads(client.input) == intent.to_data()

    def test_invalid_change_actions_refuse_before_subprocess_work(self):
        with pytest.raises(FrRuntimeError):
            AgentIntent("target", "trace", action=IntentAction(self.change()))
        with pytest.raises(FrRuntimeError):
            AgentIntent("target", "change", action=IntentAction(self.change("other")))
        request = ProjectRequest("find", ["find", "render"])
        referenced = TaskChange(
            [request],
            [TaskTarget("render", "target", "replace-body", fragment="{ 1 }")],
            {"files-changed": 1}, ["unit"],
        )
        with pytest.raises(FrRuntimeError):
            AgentIntent("target", "change", action=IntentAction(referenced))
        with pytest.raises(FrRuntimeError):
            IntentAction(self.change(), diff_bytes=True)
        with pytest.raises(FrRuntimeError):
            execute_intent(FakeNativeClient(), compile_intent(
                FakeNativeClient(), AgentIntent("target", "change")
            ))

    def test_purpose_section_policy_is_total_over_public_codes(self):
        expected = {
            0: {0}, 1: {0, 1, 3}, 2: {0, 2}, 3: {0, 2, 3}, 4: {0, 2},
        }
        for purpose in range(7):
            for section in range(6):
                assert _intent_section_allowed(purpose, section) == \
                    (section in expected.get(purpose, set()))

    def test_admission_policy_requires_every_bound_and_identity(self):
        assert _intent_admitted(2, 1, 512, 512, 65_536, 65_536,
                                         True, True, True)
        assert not _intent_admitted(0, 1, 0, 1, 1024, 1024,
                                          True, True, True)
        assert not _intent_admitted(1, 9, 0, 1, 1024, 1024,
                                          True, True, True)
        assert not _intent_admitted(1, 1, 513, 512, 1024, 1024,
                                          True, True, True)
        assert not _intent_admitted(1, 1, 0, 1, 1025, 1024,
                                          True, True, True)
        for missing in range(3):
            evidence = [True, True, True]
            evidence[missing] = False
            assert not _intent_admitted(1, 1, 0, 1, 1024, 1024, *evidence)
