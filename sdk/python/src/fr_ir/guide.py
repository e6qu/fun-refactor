"""IR-shaped structured goals and deterministic, read/preview-only workflow guides."""

from __future__ import annotations
from dataclasses import dataclass, field, replace
import hashlib
import json
from pathlib import Path
import re
import tempfile
from typing import Any, Mapping, TYPE_CHECKING

from .context import merkle_object_digest
from .runtime import FrReport, FrRuntimeError
from .ir import TaskDelivery
from .intent_actions import TaggedIntentAction
from .context import ObjectStore
from .intent import CompiledIntent

if TYPE_CHECKING:
    from .runtime import FrClient, TaskReview

_PURPOSES = ("understand", "trace", "change", "migrate", "prove")
_PROOFS = ("none", "model", "implementation")
_KINDS = ("automatic", "capability", "recipe", "semantic-scalar", "semantic-change",
          "semantic-body", "surface-edit", "framework-migration", "formalize", "proof")


def _canonical(value: Any) -> bytes:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"),
                      allow_nan=False).encode("utf-8")


def _bounded(value: Any, label: str, maximum: int = 512) -> None:
    if (not isinstance(value, str) or not value or "\0" in value
            or len(value.encode("utf-8")) > maximum):
        raise FrRuntimeError(f"{label} must be a bounded nonempty string")


def _requests_execution(value: str) -> bool:
    return value.split("=", 1)[0] in ("--write", "--save-plan")


@dataclass(frozen=True)
class GoalSelector:
    name: str | None = None
    path: str | None = None
    scope: str = "."
    kind: str | None = None
    language: str | None = None
    locals: bool = False

    def __post_init__(self) -> None:
        if (self.name is None) == (self.path is None):
            raise FrRuntimeError("goal selector needs exactly one name or path")
        for label, value in (("name", self.name), ("path", self.path),
                             ("scope", self.scope), ("kind", self.kind)):
            if value is not None:
                _bounded(value, label)
        if not isinstance(self.locals, bool):
            raise FrRuntimeError("selector locals must be Boolean")

    def to_data(self) -> dict[str, Any]:
        return {"name": self.name, "path": self.path, "scope": self.scope,
                "kind": self.kind, "language": self.language, "locals": self.locals}


@dataclass(frozen=True)
class GoalOperation:
    """A tagged operation; its fields have the same names as the public JSON union."""
    kind: str = "automatic"
    fields: Mapping[str, Any] = field(default_factory=dict)

    def __post_init__(self) -> None:
        required = {
            "automatic": (), "capability": ("capability",), "recipe": ("verb",),
            "semantic-scalar": ("operation",), "semantic-change": (), "semantic-body": (),
            "surface-edit": ("surface",), "framework-migration": ("to",),
            "formalize": (), "proof": ("obligation",),
        }
        optional = {"capability": ("parameters", "range"), "semantic-scalar": ("from", "to")}
        if (self.kind not in _KINDS or not isinstance(self.fields, Mapping)
                or not set(required[self.kind]) <= set(self.fields)
                or not set(self.fields) <= set(required[self.kind] + optional.get(self.kind, ()))):
            raise FrRuntimeError("goal operation fields do not match its tagged kind")
        value = json.loads(_canonical(dict(self.fields)))
        for name in required[self.kind]:
            _bounded(value[name], f"operation {name}", 4096)
        if self.kind == "capability":
            span = value.get("range")
            if "range" in value and (not isinstance(span, dict) or set(span) != {"start", "end"}
                    or any(isinstance(item, bool) or not isinstance(item, int) for item in span.values())
                    or not 0 <= span["start"] < span["end"]):
                raise FrRuntimeError("capability goal range must be a nonempty byte span")
            parameters = value.setdefault("parameters", {})
            if not isinstance(parameters, dict):
                raise FrRuntimeError("capability parameters must be a string map")
            for name, item in parameters.items():
                _bounded(name, "parameter name")
                _bounded(item, "parameter value", 4096)
                if item.split("=", 1)[0] in ("--write", "--save-plan"):
                    raise FrRuntimeError("capability parameters cannot request execution or plan persistence")
        if self.kind == "semantic-scalar":
            value.setdefault("from", None)
            value.setdefault("to", None)
            if value["to"] is not None and value["from"] is None:
                raise FrRuntimeError("scalar to needs an exact from value")
            for name in ("from", "to"):
                if value[name] is not None and not isinstance(value[name], str):
                    raise FrRuntimeError("scalar values must use text")
        object.__setattr__(self, "fields", value)

    def to_data(self) -> dict[str, Any]:
        return {"kind": self.kind, **json.loads(_canonical(self.fields))}


@dataclass(frozen=True)
class GoalConstraints:
    allow_source: bool = False

    def __post_init__(self) -> None:
        if not isinstance(self.allow_source, bool):
            raise FrRuntimeError("allow_source must be Boolean")

    def to_data(self) -> dict[str, Any]:
        return {"allow_source": self.allow_source}


@dataclass(frozen=True)
class GoalLimits:
    token_limit: int = 4096
    packet_limit: int = 16384

    def __post_init__(self) -> None:
        for value, low, high in ((self.token_limit, 1024, 4096),
                                 (self.packet_limit, 2048, 65536)):
            if isinstance(value, bool) or not isinstance(value, int) or not low <= value <= high:
                raise FrRuntimeError("goal context limit is invalid")

    def to_data(self) -> dict[str, Any]:
        return {"token_limit": self.token_limit, "packet_limit": self.packet_limit}


@dataclass(frozen=True)
class AgentGoal:
    purpose: str
    target: str | None = None
    selector: GoalSelector | None = None
    operation: GoalOperation = field(default_factory=GoalOperation)
    constraints: GoalConstraints = field(default_factory=GoalConstraints)
    checks: tuple[str, ...] = ()
    proof: str = "none"
    context: GoalLimits = field(default_factory=GoalLimits)
    delivery: TaskDelivery | None = None

    def __post_init__(self) -> None:
        if self.purpose not in _PURPOSES or self.proof not in _PROOFS:
            raise FrRuntimeError("goal purpose or proof expectation is invalid")
        if self.target is not None:
            _bounded(self.target, "goal target", 16384)
            if not self.target.startswith("frp1:") or self.selector is not None:
                raise FrRuntimeError("choose one full target handle or selector")
        if self.selector is not None and not isinstance(self.selector, GoalSelector):
            raise FrRuntimeError("goal selector requires GoalSelector")
        if (not isinstance(self.operation, GoalOperation)
                or not isinstance(self.constraints, GoalConstraints)
                or not isinstance(self.context, GoalLimits)):
            raise FrRuntimeError("goal fields require their public IR mirrors")
        checks = tuple(self.checks)
        if len(checks) > 32 or len(set(checks)) != len(checks):
            raise FrRuntimeError("goal checks must be at most 32 unique names")
        for name in checks:
            _bounded(name, "check name", 64)
        object.__setattr__(self, "checks", checks)
        if self.delivery is not None and not isinstance(self.delivery, TaskDelivery):
            raise FrRuntimeError("goal delivery requires the public TaskDelivery IR")

    def to_data(self) -> dict[str, Any]:
        return {"schema": "fr-agent-goal-1", "purpose": self.purpose, "target": self.target,
                "selector": self.selector.to_data() if self.selector else None,
                "operation": self.operation.to_data(), "constraints": self.constraints.to_data(),
                "checks": list(self.checks), "proof": self.proof, "context": self.context.to_data(),
                "delivery": self.delivery.to_data() if self.delivery else None}


def _guide_route_admitted(purpose: int, language_class: int, target_kind: int, route: int,
                         supported: bool, source_required: bool, source_allowed: bool,
                         proof_expectation: int) -> bool:
    purpose_matches = ((route == 0 and purpose <= 4)
                       or (route == 1 and purpose <= 2)
                       or (2 <= route <= 6 and purpose == 2)
                       or (route == 7 and purpose == 3)
                       or (route in (8, 9) and purpose == 4))
    return (purpose_matches and language_class <= 1 and target_kind <= 2 and supported
            and (not source_required or source_allowed) and proof_expectation <= 1)


def _guide_step(state: int, action: int, ready: bool, complete_review: bool,
                basis_matches: bool) -> int:
    if state == 0 and action == 0 and ready:
        return 1
    if state == 1 and action == 1 and ready:
        return 2
    if state == 2 and action == 2 and ready and complete_review:
        return 3
    if state == 3 and action == 3 and ready and complete_review and basis_matches:
        return 4
    return 5


def _guide_binding_admitted(expected_fields: int, supplied_fields: int, names_match: bool,
                            values_bounded: bool, execution_disabled: bool,
                            basis_matches: bool) -> bool:
    return (expected_fields <= 32 and expected_fields == supplied_fields and names_match
            and values_bounded and execution_disabled and basis_matches)


def _guide_delivery_admitted(purpose: int, action_count: int, report_count: int,
                             review_count: int, route_admitted: bool, guide_matches: bool,
                             review_complete: bool) -> bool:
    return (purpose == 2 and 1 <= action_count <= 16 and report_count == action_count
            and review_count == 1 and route_admitted and guide_matches and review_complete)


@dataclass(frozen=True)
class GuideAction:
    guide: AgentGuide = field(repr=False)
    index: int

    def to_data(self) -> dict[str, Any]:
        actions = self.guide.report.at("/actions")
        if isinstance(self.index, bool) or not isinstance(self.index, int) or not 0 <= self.index < len(actions):
            raise FrRuntimeError("guide action index is invalid")
        return actions[self.index]

    @property
    def arguments(self) -> tuple[str, ...]:
        return tuple(self.to_data()["arguments"])


@dataclass(frozen=True)
class GuideFile:
    """One bounded agent-authored file supplied to a guide placeholder."""

    name: str
    text: str

    def __post_init__(self) -> None:
        if (not isinstance(self.name, str) or Path(self.name).name != self.name
                or self.name in ("", ".", "..") or "\0" in self.name
                or len(self.name.encode("utf-8")) > 255):
            raise FrRuntimeError("guide file needs one bounded plain filename")
        if (not isinstance(self.text, str) or "\0" in self.text
                or len(self.text.encode("utf-8")) > 65_536):
            raise FrRuntimeError("guide file content must be at most 65536 UTF-8 bytes")


@dataclass(frozen=True)
class GuideInputs:
    """Exact values for the named author fields of one guide action."""

    values: Mapping[str, str | GuideFile] = field(default_factory=dict)

    def __post_init__(self) -> None:
        if not isinstance(self.values, Mapping):
            raise FrRuntimeError("guide inputs must be a field map")
        copied: dict[str, str | GuideFile] = {}
        for name, value in self.values.items():
            _bounded(name, "guide input name")
            if not isinstance(value, (str, GuideFile)):
                raise FrRuntimeError("guide input values must be strings or GuideFile values")
            if isinstance(value, str):
                _bounded(value, "guide input value", 4096)
                if _requests_execution(value):
                    raise FrRuntimeError("guide inputs cannot request execution or plan persistence")
            copied[name] = value
        object.__setattr__(self, "values", copied)


@dataclass(frozen=True)
class GuideRun:
    """A retained guide and every read/preview report followed from it."""

    guide: AgentGuide
    reports: tuple[FrReport, ...]

    def __post_init__(self) -> None:
        if (not isinstance(self.guide, AgentGuide)
                or len(self.reports) != len(self.guide.actions())):
            raise FrRuntimeError("guide run does not cover every retained action")

    def review(self) -> TaskReview:
        """Return the sole immutable task review, refusing routes that need a tagged intent."""
        from .runtime import TaskReview
        reviews = tuple(report for report in self.reports if isinstance(report, TaskReview))
        if len(reviews) != 1:
            raise FrRuntimeError(
                "guide run must retain exactly one task review; use compile_guided_intent "
                "for this route"
            )
        return reviews[0]


@dataclass(frozen=True)
class AgentGuide:
    goal: AgentGoal
    report: FrReport
    report_sha256: str = field(init=False, repr=False)

    def __post_init__(self) -> None:
        value = self.report.to_data()
        encoded = _canonical(value)
        identity = {key: item for key, item in value.items()
                    if key not in ("object_root", "basis", "serialized_bytes")}
        if (value.get("schema") != "fr-agent-guide-1"
                or value.get("goal_sha256") != hashlib.sha256(_canonical(self.goal.to_data())).hexdigest()
                or value.get("purpose") != self.goal.purpose
                or not isinstance(value.get("revision"), str)
                or not re.fullmatch(r"frag1:[0-9a-f]{64}", value.get("basis", ""))
                or value.get("object_root") != merkle_object_digest(identity)
                or value.get("serialized_bytes") != len(encoded)
                or len(encoded) > self.goal.context.packet_limit
                or value.get("execution", {}).get("admitted") is not False
                or not isinstance(value.get("actions"), list)
                or value.get("limits") != {"reveal_token_upper_bound": self.goal.context.token_limit,
                                           "packet_bytes": self.goal.context.packet_limit}):
            raise FrRuntimeError("agent guide failed identity or bounded preview admission")
        for action in value["actions"]:
            fields = action.get("author_fields") if isinstance(action, dict) else None
            names = ([item.get("name") for item in fields]
                     if isinstance(fields, list) and all(isinstance(item, dict) for item in fields)
                     else [])
            placeholders = ({item[1:-1] for item in action.get("arguments", [])
                             if isinstance(item, str) and item.startswith("<") and item.endswith(">")}
                            if isinstance(action, dict) else set())
            if (not isinstance(action, dict) or action.get("writes") is not False
                    or not isinstance(action.get("arguments"), list)
                    or not action["arguments"]
                    or any(not isinstance(item, str) or _requests_execution(item)
                           for item in action["arguments"])
                    or not isinstance(fields, list) or not all(isinstance(name, str) for name in names)
                    or any(not name or "\0" in name or len(name.encode("utf-8")) > 64
                           for name in names)
                    or len(names) > 32 or len(names) != len(set(names))
                    or placeholders != set(names)
                    or action.get("ready") is not (len(fields) == 0)):
                raise FrRuntimeError("guide recommended an invalid or writing action")
        object.__setattr__(self, "report_sha256", hashlib.sha256(encoded).hexdigest())

    def at(self, pointer: str = "") -> Any:
        return self.report.at(pointer)

    def to_data(self) -> dict[str, Any]:
        return self.report.to_data()

    def actions(self) -> tuple[GuideAction, ...]:
        return tuple(GuideAction(self, index) for index in range(len(self.at("/actions"))))


def guide_goal(client: FrClient, goal: AgentGoal) -> AgentGuide:
    if not isinstance(goal, AgentGoal):
        raise FrRuntimeError("guide requires an AgentGoal")
    report = client.call("guide", "--from", "-", input_bytes=_canonical(goal.to_data()))
    return AgentGuide(goal, report)


def follow_guide(
    client: FrClient,
    action: GuideAction,
    inputs: GuideInputs | None = None,
) -> FrReport:
    if not isinstance(action, GuideAction):
        raise FrRuntimeError("follow_guide requires an action retained by a guide")
    if hashlib.sha256(_canonical(action.guide.to_data())).hexdigest() != action.guide.report_sha256:
        raise FrRuntimeError("guide changed after receipt")
    value = action.to_data()
    expected_fields = tuple(field["name"] for field in value["author_fields"])
    supplied = {} if inputs is None else inputs.values
    if set(supplied) != set(expected_fields):
        raise FrRuntimeError("guide inputs must exactly match the action's named author fields")
    if value["ready"] is not (len(expected_fields) == 0):
        raise FrRuntimeError("guide action readiness differs from its author fields")
    current = guide_goal(client, action.guide.goal)
    basis_matches = current.at("/basis") == action.guide.at("/basis")
    if not _guide_binding_admitted(
        len(expected_fields), len(supplied), set(supplied) == set(expected_fields),
        True, True, basis_matches,
    ):
        raise FrRuntimeError("guide basis changed before its read/preview action")
    data = value.get("input")
    with tempfile.TemporaryDirectory(prefix="fr-guide-") as directory:
        replacements: dict[str, str] = {}
        for name, supplied_value in supplied.items():
            if isinstance(supplied_value, GuideFile):
                path = Path(directory) / supplied_value.name
                path.write_text(supplied_value.text, encoding="utf-8")
                replacements[f"<{name}>"] = str(path)
            else:
                replacements[f"<{name}>"] = supplied_value
        arguments = tuple(replacements.get(item, item) for item in action.arguments)
        if any(item.startswith("<") and item.endswith(">") for item in arguments):
            raise FrRuntimeError("guide action retains an unbound author placeholder")
        report = client.call(*arguments, input_bytes=_canonical(data) if data is not None else None)
    expected = value.get("output_schema")
    field_name = value.get("schema_field", "schema")
    if report.to_data().get(field_name) != expected:
        raise FrRuntimeError("guide action returned a different output schema")
    if len(_canonical(report.to_data())) > value["max_output_bytes"]:
        raise FrRuntimeError("guide action response exceeds its explicit output ceiling")
    if expected == "fr-task-change-1" and data is not None:
        from .runtime import TaskReview
        manifest = _canonical(data)
        basis = report.to_data().get("task_change_basis")
        if not isinstance(basis, str):
            raise FrRuntimeError("guided task preview has no reviewed basis")
        return TaskReview(report.to_data(), report.arguments, manifest,
                          hashlib.sha256(manifest).hexdigest(), basis)
    return report


def complete_guide(
    client: FrClient,
    goal: AgentGoal,
    inputs: Mapping[int, GuideInputs] | None = None,
) -> GuideRun:
    """Select one route and follow all its read/preview actions inside the SDK."""
    if inputs is not None and not isinstance(inputs, Mapping):
        raise FrRuntimeError("guide run inputs must be an action-index map")
    authored = {} if inputs is None else dict(inputs)
    if (any(isinstance(index, bool) or not isinstance(index, int) or index < 0
            or not isinstance(value, GuideInputs) for index, value in authored.items())):
        raise FrRuntimeError("guide run inputs must map nonnegative action indexes to GuideInputs")
    guide = guide_goal(client, goal)
    if set(authored) - set(range(len(guide.actions()))):
        raise FrRuntimeError("guide run inputs name an absent action")
    reports = tuple(
        follow_guide(client, action, authored.get(action.index))
        for action in guide.actions()
    )
    return GuideRun(guide, reports)


def compile_guided_intent(client: FrClient, guide: AgentGuide, action: TaggedIntentAction,
                          *, store: ObjectStore | None = None) -> CompiledIntent:
    """Bind an authored operation to a fresh navigator target and its snapshot."""
    from .intent import AgentIntent, IntentNeed
    from .intent_actions import IntentGuideBinding, TaggedIntentAction
    if not isinstance(guide, AgentGuide) or not isinstance(action, TaggedIntentAction):
        raise FrRuntimeError("guided intent requires a retained guide and TaggedIntentAction")
    if hashlib.sha256(_canonical(guide.to_data())).hexdigest() != guide.report_sha256:
        raise FrRuntimeError("guide changed after receipt")
    route = guide.at("/route")
    allowed = {
        "evidence": ("project-query",),
        "direct-capability": ("capability",),
        "recipe": ("recipe",),
        "semantic-scalar": ("task-change", "author-batch"),
        "semantic-change": ("task-change", "author-batch"),
        "semantic-body": ("task-change", "author-batch"),
        "surface-edit": ("surface-edit",),
        "framework-migration": ("framework-migration", "application-migration"),
        "formalization": ("property-task", "formal-plan"),
        "proof": ("proof-task", "proof-submission"),
    }
    operation = action.operation.to_data()
    if route.get("admitted") is not True or operation["kind"] not in allowed.get(route.get("id"), ()):
        raise FrRuntimeError("intent operation does not match its admitted navigator route")
    action = replace(action, guide=IntentGuideBinding(guide.goal.to_data(),guide.at("/basis")))
    if action.proof_expectation != guide.goal.proof:
        raise FrRuntimeError("intent proof expectation differs from its goal")
    checks = operation.get("checks", ())
    delivery = operation.get("delivery")
    if operation["kind"] == "task-change":
        checks, delivery = operation["task_change"]["checks"], operation["task_change"]["delivery"]
    if checks and guide.goal.checks and set(checks) != set(guide.goal.checks):
        raise FrRuntimeError("intent checks differ from its goal")
    if delivery is not None and guide.goal.delivery is not None and delivery != guide.goal.delivery.to_data():
        raise FrRuntimeError("intent delivery differs from its goal")
    compiled = client.compile(AgentIntent(guide.at("/target/handle"), guide.goal.purpose,
        needs=(IntentNeed("map", "code_map", "/target"),),
        token_limit=guide.goal.context.token_limit, packet_limit=guide.goal.context.packet_limit,
        action=action), store=store)
    if compiled.at("/revision") != guide.at("/revision"):
        raise FrRuntimeError("project changed between navigator and intent snapshots")
    return compiled
