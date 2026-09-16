import hashlib
import json
import pytest

from fr_ir.context import merkle_object_digest
from fr_ir.guide import (AgentGoal, AgentGuide, GoalConstraints, GoalLimits, GoalOperation,
                        GoalSelector, _canonical, follow_guide, guide_goal)
from fr_ir.runtime import FrReport, FrRuntimeError


class FakeClient:
    def __init__(self, mutate=None):
        self.mutate = mutate
        self.calls = []

    def call(self, *arguments, input_bytes=None):
        self.calls.append((arguments, input_bytes))
        if arguments[0] != "guide":
            return FrReport({"schema": "fr-project-1"}, arguments)
        goal = json.loads(input_bytes)
        report = {"schema": "fr-agent-guide-1", "purpose": goal["purpose"], "revision": "r",
                  "goal_sha256": hashlib.sha256(input_bytes).hexdigest(),
                  "execution": {"admitted": False},
                  "limits": {"reveal_token_upper_bound": goal["context"]["token_limit"],
                             "packet_bytes": goal["context"]["packet_limit"]},
                  "actions": [{"id": "structure", "arguments": ["project", "map"],
                               "output_schema": "fr-project-1", "schema_field": "schema",
                               "input": None, "author_fields": [], "writes": False,
                               "ready": True, "max_output_bytes":16384}]}
        report["object_root"] = merkle_object_digest(report)
        report["basis"] = "frag1:" + "0" * 64
        if self.mutate:
            self.mutate(report)
        report["serialized_bytes"] = 0
        for _ in range(5):
            report["serialized_bytes"] = len(_canonical(report))
        return FrReport(report, arguments)


def test_goal_wire_shape_preserves_tagged_ir_and_explicit_limits():
    goal = AgentGoal("change", selector=GoalSelector(name="calculate"),
                     operation=GoalOperation("semantic-scalar", {"operation": "set-int",
                                                                 "from": "7", "to": "9"}),
                     checks=("unit",), context=GoalLimits(packet_limit=8192))
    data = goal.to_data()
    assert data["schema"] == "fr-agent-goal-1"
    assert data["operation"] == {"kind": "semantic-scalar", "operation": "set-int",
                                 "from": "7", "to": "9"}
    assert data["selector"]["name"] == "calculate"
    assert data["constraints"] == {"allow_source": False}


@pytest.mark.parametrize("build", [
    lambda: AgentGoal("invent"), lambda: AgentGoal("change", target="bare-name"),
    lambda: GoalSelector(name="x", path="app.rs"), lambda: GoalLimits(token_limit=4097),
    lambda: GoalConstraints(allow_source=1), lambda: GoalOperation("invent"),
    lambda: GoalOperation("capability", {"capability": "rename", "extra": 1}),
    lambda: GoalOperation("semantic-scalar", {"operation": "set-int", "to": "9"}),
    lambda: AgentGoal("trace", checks=("x", "x")),
])
def test_invalid_goals_refuse_before_subprocess_work(build):
    with pytest.raises(FrRuntimeError):
        build()


def test_guide_verifies_identity_and_follows_only_ready_preview_actions():
    client = FakeClient()
    goal = AgentGoal("understand", selector=GoalSelector(path="app.rs"))
    guide = guide_goal(client, goal)
    assert isinstance(guide, AgentGuide)
    report = follow_guide(client, guide.actions()[0])
    assert report.schema == "fr-project-1"
    assert len(client.calls) == 3


@pytest.mark.parametrize("mutate", [
    lambda value: value.__setitem__("goal_sha256", "0" * 64),
    lambda value: value.__setitem__("object_root", "0" * 64),
    lambda value: value["execution"].__setitem__("admitted", True),
    lambda value: value["actions"][0]["arguments"].append("--write"),
    lambda value: value["actions"][0].__setitem__("writes", True),
])
def test_tampered_or_writing_guides_refuse(mutate):
    with pytest.raises(FrRuntimeError):
        guide_goal(FakeClient(mutate), AgentGoal("understand"))


def test_changed_retained_guide_cannot_be_followed():
    client = FakeClient()
    guide = guide_goal(client, AgentGoal("understand"))
    guide.report._value["actions"][0]["arguments"].append("--write")
    with pytest.raises(FrRuntimeError):
        follow_guide(client, guide.actions()[0])
