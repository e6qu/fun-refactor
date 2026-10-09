"""Body input preparation binds exact targets without doing subprocess work."""
import hashlib

import pytest

from fr_ir.context import merkle_object_digest
from fr_ir.guide import (AgentGoal, AgentGuide, GoalConstraints, GoalOperation, GoalSelector,
                        _canonical)
from fr_ir.ir import IrError, TaskDelivery
from fr_ir.runtime import FrReport, FrRuntimeError


def retained_guide(*, multiple=False, mutate=None, **goal_changes):
    goal = AgentGoal(**{
        "purpose": "change", "selector": GoalSelector(name="same", scope="first.py"),
        "operation": (GoalOperation("source-bodies", {"additional": [{"name": "same", "scope": "second.py"}]})
                      if multiple else GoalOperation("source-body")),
        "constraints": GoalConstraints(allow_source=True), "checks": ("behavior",),
        "delivery": TaskDelivery(patch="change.patch"), **goal_changes,
    })
    targets = [{"handle": "frp1:first", "name": "same", "path": "first.py"}]
    if multiple:
        targets.append({"handle": "frp1:second", "name": "same", "path": "second.py"})
    data = {"schema": "fr-agent-guide-1", "goal_sha256": hashlib.sha256(_canonical(goal.to_data())).hexdigest(),
            "purpose": goal.purpose, "revision": "r", "execution": {"admitted": False},
            "route": {"id": goal.operation.kind, "admitted": True}, "state": "ready", "refusals": [],
            "actions": [], "limits": {"reveal_token_upper_bound": goal.context.token_limit,
                                       "packet_bytes": goal.context.packet_limit},
            "target": targets[0], "targets": targets}
    if mutate:
        mutate(data)
    data["object_root"] = merkle_object_digest(data)
    data["basis"] = "frag1:" + "0" * 64
    data["serialized_bytes"] = 0
    for _ in range(5):
        data["serialized_bytes"] = len(_canonical(data))
    return AgentGuide(goal, FrReport(data, ("guide",)))


def test_body_action_uses_exact_handles_even_when_names_match():
    guide = retained_guide(multiple=True)
    bodies = {"frp1:second": "return 2\n", "frp1:first": "return 1\n"}
    action = guide.source_body_action(bodies)
    bodies["frp1:first"] = "return 99\n"
    change = action.operation.to_data()["task_change"]
    assert [(t["handle"], t["fragment"]) for t in change["targets"]] == [
        ("frp1:first", "return 1\n"), ("frp1:second", "return 2\n")]
    assert change["checks"] == ["behavior"]
    assert change["delivery"] == guide.goal.delivery.to_data()
    assert change["postconditions"] == {"files-changed": 2, "edits": 2,
                                       "changed-operations": 2, "paths-changed": ["first.py", "second.py"]}
    assert action.diff_bytes == guide.goal.context.token_limit
    assert action.report_bytes == guide.goal.context.packet_limit
    assert action.proof_expectation == guide.goal.proof


@pytest.mark.parametrize("bodies", [None, "return 1", {}, {"same": "return 1"},
    {"frp1:first": 1}, {"frp1:first": "return 1", "frp1:extra": "return 2"}])
def test_body_action_rejects_missing_ambiguous_extra_or_untyped_inputs(bodies):
    with pytest.raises(FrRuntimeError, match="every exact guided handle"):
        retained_guide().source_body_action(bodies)


@pytest.mark.parametrize("mutate", [
    lambda d: d.update(state="clipped"), lambda d: d.update(refusals=["ambiguous selection"]),
    lambda d: d["route"].update(admitted=False), lambda d: d["route"].update(id="semantic-body"),
    lambda d: d.update(targets=d["targets"][:1]),
    lambda d: d.update(targets=[d["target"], d["target"]]),
    lambda d: d["targets"][1].pop("path"),
])
def test_body_action_rejects_incomplete_or_refused_guides(mutate):
    with pytest.raises(FrRuntimeError):
        retained_guide(multiple=True, mutate=mutate).source_body_action(
            {"frp1:first": "return 1", "frp1:second": "return 2"})


@pytest.mark.parametrize("changes", [{"checks": ()}, {"delivery": None},
    {"constraints": GoalConstraints()}, {"purpose": "understand"},
    {"operation": GoalOperation("semantic-body")}])
def test_body_action_requires_a_complete_body_goal(changes):
    with pytest.raises(FrRuntimeError, match="complete admitted source-body"):
        retained_guide(**changes).source_body_action({"frp1:first": "return 1"})


@pytest.mark.parametrize("field", ["goal", "report"])
def test_body_action_rejects_changed_receipts(field):
    guide = retained_guide(multiple=True)
    if field == "goal":
        guide.goal.operation.fields["additional"][0]["name"] = "other"
    else:
        guide.report._value["target"]["path"] = "other.py"
    with pytest.raises(FrRuntimeError, match="changed after receipt"):
        guide.source_body_action({"frp1:first": "return 1", "frp1:second": "return 2"})


@pytest.mark.parametrize("body", ["\0", "x" * 65537])
def test_body_action_retains_fragment_size_and_content_limits(body):
    with pytest.raises(IrError, match="inline fragment"):
        retained_guide().source_body_action({"frp1:first": body})
