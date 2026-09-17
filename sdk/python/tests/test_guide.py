import hashlib
import json
from pathlib import Path
import pytest

from fr_ir.context import merkle_object_digest
from fr_ir.guide import (AgentGoal, AgentGuide, GoalConstraints, GoalLimits, GoalOperation,
                        GoalSelector, GuideFile, GuideInputs, GuideReview, _canonical,
                        _guide_delivery_admitted, compile_guided_intent, complete_guide,
                        follow_guide, guide_goal, review_guide)
from fr_ir.intent_actions import (ApplicationMigrationOperation, AuthorBatchOperation,
                                  CapabilityOperation, FormalPlanOperation, ProofSubmissionOperation,
                                  RecipeOperation, SurfaceEditOperation, TaggedIntentAction)
from fr_ir.intent import CompiledIntent
from fr_ir.ir import TaskDelivery
from fr_ir.runtime import FrClient, FrReport, FrRuntimeError, TaskReview


@pytest.mark.parametrize("value", ["--write", "--save-plan", "--write=true", "--save-plan=true"])
def test_capability_scalar_parameters_cannot_request_execution(value):
    with pytest.raises(FrRuntimeError):
        GoalOperation("capability", {"capability": "rename", "parameters": {"new_name": value}})


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


class AuthoredClient(FakeClient):
    def call(self, *arguments, input_bytes=None):
        if arguments[0] == "guide":
            report = super().call(*arguments, input_bytes=input_bytes)
            value = report._value
            value["actions"] = [{"id": "proof-check",
                                 "arguments": ["spec", "proof-check", "target", "--from", "<tactics-file>"],
                                 "output_schema": "fr-proof-attempt-1", "schema_field": "schema",
                                 "input": None, "author_fields": [{"name": "tactics-file", "shape": "Lean tactics"}],
                                 "writes": False, "ready": False, "max_output_bytes": 16384}]
            identity = {key: item for key, item in value.items()
                        if key not in ("object_root", "basis", "serialized_bytes")}
            value["object_root"] = merkle_object_digest(identity)
            value["serialized_bytes"] = 0
            for _ in range(5):
                value["serialized_bytes"] = len(_canonical(value))
            return report
        self.calls.append((arguments, input_bytes))
        assert arguments[:4] == ("spec", "proof-check", "target", "--from")
        supplied = open(arguments[4], encoding="utf-8").read()
        assert supplied == "rfl\n"
        return FrReport({"schema": "fr-proof-attempt-1", "accepted": True}, arguments)


class DeliveryClient(FakeClient):
    def call(self, *arguments, input_bytes=None):
        if arguments[0] == "guide":
            report = super().call(*arguments, input_bytes=input_bytes)
            value = report._value
            manifest = {"schema": "fr-task-change-1", "requests": [], "targets": []}
            value["route"] = {"id": "semantic-scalar", "admitted": True}
            value["actions"] = [{"id": "preview", "arguments": ["task-change", "--from", "-"],
                                 "output_schema": "fr-task-change-1", "schema_field": "schema",
                                 "input": manifest, "author_fields": [], "writes": False,
                                 "ready": True, "max_output_bytes": 16384}]
            identity = {key: item for key, item in value.items()
                        if key not in ("object_root", "basis", "serialized_bytes")}
            value["object_root"] = merkle_object_digest(identity)
            value["serialized_bytes"] = 0
            for _ in range(5):
                value["serialized_bytes"] = len(_canonical(value))
            return report
        self.calls.append((arguments, input_bytes))
        manifest_sha256 = hashlib.sha256(input_bytes).hexdigest()
        basis = "frtc1:" + "1" * 64
        if "--write" in arguments:
            return FrReport({"schema": "fr-task-change-1", "executed": True,
                             "passed": True, "task_change_basis": basis}, arguments)
        return FrReport({"schema": "fr-task-change-1", "ready": True, "executed": False,
                         "manifest_sha256": manifest_sha256,
                         "task_change_basis": basis}, arguments)


class ScalarGuideClient(FakeClient):
    def call(self, *arguments, input_bytes=None):
        report = super().call(*arguments, input_bytes=input_bytes)
        if arguments[0] != "guide":
            return report
        goal = json.loads(input_bytes)
        value = report._value
        target = {"handle": "frp1:scalar", "path": "app.rs"}
        manifest = {
            "schema": "fr-task-change-1", "requests": [],
            "targets": [{"id": "goal", "handle": target["handle"],
                         "op": "edit-body-scalar", "scalar": {
                             "operation": "set-int", "from": "7", "to": "9",
                         }}],
            "postconditions": {"files-changed": 1, "edits": 1, "changed-operations": 1,
                               "paths-changed": [target["path"]]},
            "checks": ["syntax"], "delivery": TaskDelivery().to_data(),
        }
        value["route"] = {"id": "semantic-scalar", "admitted": True}
        value["target"] = target
        value["actions"] = [{"id": "preview", "arguments": ["task-change", "--from", "-"],
                             "output_schema": "fr-task-change-1", "schema_field": "schema",
                             "input": manifest, "author_fields": [], "writes": False,
                             "ready": True, "max_output_bytes": 16384}]
        identity = {key: item for key, item in value.items()
                    if key not in ("object_root", "basis", "serialized_bytes")}
        value["object_root"] = merkle_object_digest(identity)
        value["serialized_bytes"] = 0
        for _ in range(5):
            value["serialized_bytes"] = len(_canonical(value))
        return report


class ApplicationGuideClient(FakeClient):
    def call(self, *arguments, input_bytes=None):
        report = super().call(*arguments, input_bytes=input_bytes)
        if arguments[0] == "guide":
            value = report._value
            value["route"] = {"id": "framework-migration", "admitted": True}
            value["target"] = {"handle": "frh1:application"}
            identity = {key: item for key, item in value.items()
                        if key not in ("object_root", "basis", "serialized_bytes")}
            value["object_root"] = merkle_object_digest(identity)
            value["serialized_bytes"] = 0
            for _ in range(5):
                value["serialized_bytes"] = len(_canonical(value))
        return report

    def compile(self, intent, *, store=None):
        self.compiled_intent = intent
        manifest = _canonical(intent.to_data())
        intent_basis = "frai1:" + hashlib.sha256(manifest).hexdigest()
        action_basis = "fraa2:" + "2" * 64
        operation = intent.action.operation.to_data()
        review = {
            "schema": "fr-intent-operation-review-2", "kind": operation["kind"],
            "ready": True, "executed": False, "diff": "diff --git a/a b/a\n",
            "writable": True, "targets": [intent.target],
            "input_sha256": hashlib.sha256(_canonical(intent.action.to_data())).hexdigest(),
            "checks": {"configuration_basis": "checks", "names": ["syntax"]},
            "delivery": TaskDelivery().to_data(),
            "claims": {"implementation_correspondence": False},
            "proof": None, "proof_validation": None,
        }
        packet = {
            "schema": "fr-agent-context-1", "revision": "r",
            "intent": {"basis": intent_basis},
            "target": {"handle": intent.target},
            "action": {"schema": "fr-agent-action-2", "basis": action_basis,
                       "review": review},
        }
        encoded = _canonical(packet)
        return CompiledIntent(intent, FrReport(packet, ("intent",)), (), manifest,
                              hashlib.sha256(encoded).hexdigest(), action_basis)

    def execute_intent(self, compiled):
        review = compiled.at("/action/review")
        return type("Result", (), {"passed": True, "compiled": compiled,
                                    "review": review})()


class RouteGuideClient(ApplicationGuideClient):
    def __init__(self, route):
        super().__init__()
        self.route = route

    def call(self, *arguments, input_bytes=None):
        report = super().call(*arguments, input_bytes=input_bytes)
        if arguments[0] == "guide":
            value = report._value
            value["route"] = {"id": self.route, "admitted": True}
            identity = {key: item for key, item in value.items()
                        if key not in ("object_root", "basis", "serialized_bytes")}
            value["object_root"] = merkle_object_digest(identity)
            value["serialized_bytes"] = 0
            for _ in range(5):
                value["serialized_bytes"] = len(_canonical(value))
        return report


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
    lambda: GoalOperation("capability", {"capability":"inline-call", "range":{"start":True,"end":2}}),
    lambda: GoalOperation("capability", {"capability":"inline-call", "range":{"start":3,"end":2}}),
    lambda: GoalOperation("capability", {"capability":"inline-call", "range":{"start":0,"end":2,"extra":3}}),
])
def test_invalid_goals_refuse_before_subprocess_work(build):
    with pytest.raises(FrRuntimeError):
        build()


def test_guide_verifies_identity_and_follows_ready_preview_actions():
    client = FakeClient()
    goal = AgentGoal("understand", selector=GoalSelector(path="app.rs"))
    guide = guide_goal(client, goal)
    assert isinstance(guide, AgentGuide)
    report = follow_guide(client, guide.actions()[0])
    assert report.schema == "fr-project-1"
    assert len(client.calls) == 3


def test_authored_guide_files_are_exact_bounded_and_temporary():
    client = AuthoredClient()
    goal = AgentGoal("prove")
    guide = guide_goal(client, goal)
    action = guide.actions()[0]
    with pytest.raises(FrRuntimeError, match="exactly match"):
        follow_guide(client, action)
    with pytest.raises(FrRuntimeError, match="exactly match"):
        follow_guide(client, action, GuideInputs({"wrong": "rfl"}))
    report = follow_guide(client, action, GuideInputs({
        "tactics-file": GuideFile("proof.lean", "rfl\n"),
    }))
    assert report.schema == "fr-proof-attempt-1"
    assert not Path(client.calls[-1][0][-1]).exists()


def test_complete_guide_keeps_intermediate_reports_inside_one_sdk_call():
    client = AuthoredClient()
    result = complete_guide(client, AgentGoal("prove"), {0: GuideInputs({
        "tactics-file": GuideFile("proof.lean", "rfl\n"),
    })})
    assert result.guide.at("/schema") == "fr-agent-guide-1"
    assert [report.schema for report in result.reports] == ["fr-proof-attempt-1"]


def test_complete_guide_exposes_one_task_preview_for_inspection():
    client = DeliveryClient()
    run = complete_guide(client, AgentGoal("change"))
    assert isinstance(run.review(), TaskReview)


def test_complete_scalar_guide_returns_its_exact_typed_action_without_reauthoring():
    goal = AgentGoal(
        "change", selector=GoalSelector(name="calculate"),
        operation=GoalOperation("semantic-scalar", {
            "operation": "set-int", "from": "7", "to": "9",
        }), checks=("syntax",), delivery=TaskDelivery(),
    )
    guide = guide_goal(ScalarGuideClient(), goal)
    action = guide.semantic_scalar_action()
    assert action.operation.to_data() == {
        "kind": "task-change", "task_change": guide.at("/actions/0/input"),
    }
    guide.report._value["actions"][0]["input"]["checks"] = ["other"]
    with pytest.raises(FrRuntimeError, match="changed after receipt"):
        guide.semantic_scalar_action()


def test_guided_application_migration_accepts_the_operation_advertised_by_the_guide():
    client = ApplicationGuideClient()
    goal = AgentGoal(
        "migrate",
        selector=GoalSelector(path="api.ts"),
        operation=GoalOperation("framework-migration", {"to": "go-net-http"}),
        checks=("syntax",),
        delivery=TaskDelivery(),
    )
    guide = guide_goal(client, goal)
    compiled = compile_guided_intent(
        client,
        guide,
        TaggedIntentAction(ApplicationMigrationOperation(
            "go-net-http", "generated", ("syntax",),
        )),
    )
    assert compiled.at("/revision") == "r"
    assert client.compiled_intent.action.operation.to_data()["kind"] == "application-migration"


def test_guided_review_requires_checks_and_delivery_in_the_goal_before_compilation():
    client = ApplicationGuideClient()
    goal = AgentGoal(
        "migrate", selector=GoalSelector(path="api.ts"),
        operation=GoalOperation("framework-migration", {"to": "go-net-http"}),
        checks=("syntax",),
    )
    guide = guide_goal(client, goal)
    with pytest.raises(FrRuntimeError, match="delivery differs"):
        review_guide(client, guide, TaggedIntentAction(ApplicationMigrationOperation(
            "go-net-http", "generated", ("syntax",),
        )))
    assert not hasattr(client, "compiled_intent")


def test_common_guide_review_executes_the_unchanged_native_action():
    client = ApplicationGuideClient()
    goal = AgentGoal(
        "migrate",
        selector=GoalSelector(path="api.ts"),
        operation=GoalOperation("framework-migration", {"to": "go-net-http"}),
        checks=("syntax",),
        delivery=TaskDelivery(),
    )
    guide = guide_goal(client, goal)
    action = TaggedIntentAction(ApplicationMigrationOperation(
        "go-net-http", "generated", ("syntax",),
    ))
    review = review_guide(client, guide, action)
    assert isinstance(review, GuideReview)
    assert review.basis == "fraa2:" + "2" * 64
    assert review.at("/kind") == "application-migration"
    result = FrClient.execute_guide(client, review)
    assert result.passed


@pytest.mark.parametrize(("route", "purpose", "goal_operation", "operation"), [
    ("direct-capability", "change",
     GoalOperation("capability", {"capability": "rename", "parameters": {"new_name": "next"}}),
     CapabilityOperation("rename", {"new_name": "next"}, checks=("syntax",),
                         delivery=TaskDelivery())),
    ("recipe", "change", GoalOperation("recipe", {"verb": "rename"}),
     RecipeOperation("schema 1\nrecipe r { rename to \"next\" where name=\"old\" }\n",
                     ("syntax",), TaskDelivery())),
    ("semantic-change", "change", GoalOperation("semantic-change"),
     AuthorBatchOperation({"schema": "fr-author-batch-1"}, ("syntax",), TaskDelivery())),
    ("surface-edit", "change", GoalOperation("surface-edit", {"surface": "styles"}),
     SurfaceEditOperation("frse1:edit", "blue", ("syntax",), TaskDelivery())),
    ("framework-migration", "migrate",
     GoalOperation("framework-migration", {"to": "go-net-http"}),
     ApplicationMigrationOperation("go-net-http", "generated", ("syntax",), TaskDelivery())),
    ("formalization", "prove", GoalOperation("formalize"),
     FormalPlanOperation((), package="proofs", checks=("syntax",), delivery=TaskDelivery())),
    ("proof", "prove", GoalOperation("proof", {"obligation": "goal"}),
     ProofSubmissionOperation("goal", "rfl\n", ("syntax",), TaskDelivery())),
])
def test_common_review_type_covers_each_writable_guide_route(
        route, purpose, goal_operation, operation):
    client = RouteGuideClient(route)
    goal = AgentGoal(purpose, target="frp1:route", operation=goal_operation,
                     checks=("syntax",), delivery=TaskDelivery())
    review = review_guide(client, guide_goal(client, goal), TaggedIntentAction(operation))
    assert isinstance(review, GuideReview)
    assert review.at("/kind") == operation.to_data()["kind"]


def test_common_guide_review_refuses_mutated_review_content():
    client = ApplicationGuideClient()
    goal = AgentGoal(
        "migrate", selector=GoalSelector(path="api.ts"),
        operation=GoalOperation("framework-migration", {"to": "go-net-http"}),
        checks=("syntax",), delivery=TaskDelivery(),
    )
    review = review_guide(client, guide_goal(client, goal), TaggedIntentAction(
        ApplicationMigrationOperation("go-net-http", "generated", ("syntax",)),
    ))
    review.compiled.packet._value["action"]["review"]["diff"] = "changed"
    with pytest.raises(FrRuntimeError, match="changed before execution"):
        FrClient.execute_guide(client, review)


def test_uniform_delivery_refuses_route_specific_runs_and_missing_task_previews():
    client = DeliveryClient()
    run = complete_guide(client, AgentGoal("change"))
    with pytest.raises(FrRuntimeError, match="guide review"):
        FrClient.execute_guide(client, run)
    with pytest.raises(FrRuntimeError, match="exactly one"):
        complete_guide(AuthoredClient(), AgentGoal("prove"), {0: GuideInputs({
            "tactics-file": GuideFile("proof.lean", "rfl\n"),
        })}).review()


def test_guide_delivery_kernel_covers_every_writable_route_and_identity_field():
    for purpose, route, operation in (
        (2, 1, 11), (2, 2, 2), (2, 3, 0), (2, 4, 1), (2, 5, 0),
        (2, 6, 7), (3, 7, 3), (4, 8, 4), (4, 9, 5),
    ):
        case = [purpose, route, operation, *([True] * 11)]
        assert _guide_delivery_admitted(*case)
        for index in range(3, len(case)):
            refused = list(case)
            refused[index] = False
            assert not _guide_delivery_admitted(*refused)
    assert not _guide_delivery_admitted(0, 3, 0, *([True] * 11))
    assert not _guide_delivery_admitted(2, 3, 3, *([True] * 11))


@pytest.mark.parametrize("build", [
    lambda: GuideFile("../proof.lean", "rfl"),
    lambda: GuideFile("proof.lean", "x" * 65_537),
    lambda: GuideInputs({"value": "--write"}),
    lambda: GuideInputs({"value": object()}),
])
def test_invalid_guide_inputs_refuse_before_subprocess_work(build):
    with pytest.raises(FrRuntimeError):
        build()


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


def test_capability_byte_range_matches_the_public_span_shape():
    span = {"start":17,"end":22}
    goal = AgentGoal("change", selector=GoalSelector(name="caller"),
        operation=GoalOperation("capability", {"capability":"inline-call", "range":span}))
    assert goal.to_data()["operation"]["range"] == span
