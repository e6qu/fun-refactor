from fr_ir.intent import AgentIntent, IntentNeed, prepare_intent
from fr_ir.runtime import FrReport, FrRuntimeError
from fr_ir.intent import _intent_admitted
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


class TestIntent:
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
            lambda: AgentIntent("target", "trace", call_limit=0),
            lambda: AgentIntent("target", "trace", call_limit=513),
            lambda: AgentIntent("target", "trace", packet_limit=1000),
        ]
        for build in invalid:
            with pytest.raises(FrRuntimeError):
                build()

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
