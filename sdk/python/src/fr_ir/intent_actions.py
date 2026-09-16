"""Tagged intent operations matching ``fr-intent-action-2`` directly."""
from __future__ import annotations
from dataclasses import dataclass
from typing import Any, Mapping

from .ir import AgentProperty, ProjectRequest, TaskChange, TaskDelivery
from .runtime import FrRuntimeError


def _checks(checks: tuple[str, ...], delivery: TaskDelivery) -> None:
    if (not isinstance(checks, tuple) or not checks or len(set(checks)) != len(checks)
            or any(not isinstance(name, str) or not name for name in checks)
            or not isinstance(delivery, TaskDelivery)):
        raise FrRuntimeError("writing intent operations require named checks and TaskDelivery")


@dataclass(frozen=True)
class TaskChangeOperation:
    task_change: TaskChange

    def __post_init__(self) -> None:
        if not isinstance(self.task_change, TaskChange):
            raise FrRuntimeError("task-change operation requires TaskChange")

    def to_data(self) -> dict[str, Any]:
        return {"kind": "task-change", "task_change": self.task_change.to_data()}


@dataclass(frozen=True)
class AuthorBatchOperation:
    author_batch: Mapping[str, Any]
    checks: tuple[str, ...]
    delivery: TaskDelivery = TaskDelivery()

    def __post_init__(self) -> None:
        _checks(self.checks, self.delivery)
        if not isinstance(self.author_batch, Mapping):
            raise FrRuntimeError("author-batch operation requires the public author batch manifest")

    def to_data(self) -> dict[str, Any]:
        return {"kind": "author-batch", "author_batch": dict(self.author_batch),
                "checks": list(self.checks), "delivery": self.delivery.to_data()}


@dataclass(frozen=True)
class RecipeOperation:
    recipe: str
    checks: tuple[str, ...]
    delivery: TaskDelivery = TaskDelivery()

    def __post_init__(self) -> None:
        _checks(self.checks, self.delivery)
        if not isinstance(self.recipe, str) or not self.recipe.strip():
            raise FrRuntimeError("recipe operation requires agent-authored DSL")

    def to_data(self) -> dict[str, Any]:
        return {"kind": "recipe", "recipe": self.recipe,
                "checks": list(self.checks), "delivery": self.delivery.to_data()}


@dataclass(frozen=True)
class FrameworkMigrationOperation:
    feature: str
    to: str
    out: str
    checks: tuple[str, ...]
    delivery: TaskDelivery = TaskDelivery()
    register_with: str | None = None
    dependency_manifest: str | None = None
    dependency_requirement: tuple[str, ...] = ()
    cutover: bool = False

    def __post_init__(self) -> None:
        _checks(self.checks, self.delivery)
        if self.to not in ("fastapi", "nextjs") or not self.feature or not self.out:
            raise FrRuntimeError("migration operation requires a feature, supported framework and output")
        if not isinstance(self.cutover, bool):
            raise FrRuntimeError("migration cutover must be a boolean")

    def to_data(self) -> dict[str, Any]:
        return {"kind": "framework-migration", "feature": self.feature, "to": self.to,
                "out": self.out, "register_with": self.register_with,
                "dependency_manifest": self.dependency_manifest,
                "dependency_requirement": list(self.dependency_requirement), "cutover": self.cutover,
                "checks": list(self.checks), "delivery": self.delivery.to_data()}


@dataclass(frozen=True)
class ApplicationMigrationOperation:
    to: str
    out: str
    checks: tuple[str, ...]
    delivery: TaskDelivery = TaskDelivery()
    register_with: str | None = None
    dependency_manifest: str | None = None
    dependency_requirement: tuple[str, ...] = ()

    def __post_init__(self) -> None:
        _checks(self.checks, self.delivery)
        if self.to not in ("nextjs", "fastapi", "express", "go-net-http", "react"):
            raise FrRuntimeError("application migration requires a supported adapter")
        if not isinstance(self.out, str) or not self.out:
            raise FrRuntimeError("application migration requires an output directory")
        if (self.register_with is not None
                and self.to not in ("fastapi", "express", "go-net-http")):
            raise FrRuntimeError("application registration requires a backend adapter")
        if self.to == "go-net-http" and (self.dependency_manifest is not None
                                           or self.dependency_requirement):
            raise FrRuntimeError("Go standard HTTP has no framework dependency edit")

    def to_data(self) -> dict[str, Any]:
        return {"kind": "application-migration", "to": self.to, "out": self.out,
                "register_with": self.register_with,
                "dependency_manifest": self.dependency_manifest,
                "dependency_requirement": list(self.dependency_requirement),
                "checks": list(self.checks), "delivery": self.delivery.to_data()}


@dataclass(frozen=True)
class FormalPlanOperation:
    properties: tuple[str, ...]
    agent_properties: tuple[AgentProperty, ...] = ()
    package: str | None = None
    checks: tuple[str, ...] = ()
    delivery: TaskDelivery | None = None

    def __post_init__(self) -> None:
        if (not isinstance(self.properties, tuple) or not isinstance(self.agent_properties, tuple)
                or len(self.properties) + len(self.agent_properties) > 16
                or len(set(self.properties)) != len(self.properties)
                or any(not isinstance(item, AgentProperty) for item in self.agent_properties)):
            raise FrRuntimeError("formal plan operation requires bounded typed properties")
        if self.package is None:
            if self.checks or self.delivery is not None:
                raise FrRuntimeError("read-only formal plans cannot declare source delivery")
        elif self.delivery is None:
            raise FrRuntimeError("formal scaffold operation requires delivery")
        else:
            _checks(self.checks, self.delivery)

    def to_data(self) -> dict[str, Any]:
        return {"kind": "formal-plan", "properties": list(self.properties),
                "agent_properties": [prop.to_data() for prop in self.agent_properties],
                "package": self.package, "checks": list(self.checks),
                "delivery": None if self.delivery is None else self.delivery.to_data()}


@dataclass(frozen=True)
class ProofSubmissionOperation:
    obligation: str
    tactics: str
    checks: tuple[str, ...]
    delivery: TaskDelivery = TaskDelivery()

    def __post_init__(self) -> None:
        _checks(self.checks, self.delivery)
        if (not isinstance(self.obligation, str) or not self.obligation
                or not isinstance(self.tactics, str) or not self.tactics.strip()
                or len(self.tactics.encode()) > 65_536):
            raise FrRuntimeError("proof submission requires an obligation and bounded agent-authored tactics")

    def to_data(self) -> dict[str, Any]:
        return {"kind": "proof-submission", "obligation": self.obligation, "tactics": self.tactics,
                "checks": list(self.checks), "delivery": self.delivery.to_data()}


@dataclass(frozen=True)
class ProjectQueryOperation:
    requests: tuple[ProjectRequest, ...]

    def __post_init__(self) -> None:
        if (not isinstance(self.requests, tuple) or not 1 <= len(self.requests) <= 16
                or any(not isinstance(request, ProjectRequest) for request in self.requests)):
            raise FrRuntimeError("project query operation requires bounded typed requests")

    def to_data(self) -> dict[str, Any]:
        return {"kind": "project-query", "requests": [request.to_data() for request in self.requests]}


@dataclass(frozen=True)
class SurfaceEditOperation:
    edit: str
    to: str
    checks: tuple[str, ...]
    delivery: TaskDelivery = TaskDelivery()

    def __post_init__(self) -> None:
        _checks(self.checks, self.delivery)
        if not isinstance(self.edit, str) or not self.edit.startswith("frse1:"):
            # Native surface IDs use the public frse1 capability namespace.
            raise FrRuntimeError("surface edit operation requires a returned edit capability")
        if not isinstance(self.to, str) or len(self.to.encode()) > 256:
            raise FrRuntimeError("surface edit operation requires a bounded scalar")

    def to_data(self) -> dict[str, Any]:
        return {"kind": "surface-edit", "edit": self.edit, "to": self.to,
                "checks": list(self.checks), "delivery": self.delivery.to_data()}


@dataclass(frozen=True)
class PropertyTaskOperation:
    def to_data(self) -> dict[str, Any]:
        return {"kind": "property-task"}


@dataclass(frozen=True)
class ProofTaskOperation:
    obligation: str

    def __post_init__(self) -> None:
        if not isinstance(self.obligation, str) or not self.obligation:
            raise FrRuntimeError("proof task operation requires an exact obligation")

    def to_data(self) -> dict[str, Any]:
        return {"kind": "proof-task", "obligation": self.obligation}


_CAPABILITY_WRITES = frozenset({"rename", "safe-delete", "restructure", "extract-variable",
    "extract-function", "inline-variable", "inline-call", "change-signature", "micro-rewrites",
    "organize-imports", "remove-flag", "move-to-file", "translate"})
_CAPABILITIES = _CAPABILITY_WRITES | frozenset({"symbols", "impact", "call-graph", "flow", "provenance",
    "entry-points", "stitch", "duplicates", "dead-code", "openapi", "declared-type"})


@dataclass(frozen=True)
class CapabilityOperation:
    capability: str
    parameters: Mapping[str, str] | None = None
    range: Mapping[str, int] | None = None
    checks: tuple[str, ...] = ()
    delivery: TaskDelivery | None = None

    def __post_init__(self) -> None:
        if self.capability not in _CAPABILITIES:
            raise FrRuntimeError("capability operation requires an advertised capability")
        if self.parameters is not None and (not isinstance(self.parameters, Mapping) or any(
                not isinstance(key, str) or not key or not isinstance(value, str) or not value
                or len(value.encode()) > 4096 or "\0" in value for key,value in self.parameters.items())):
            raise FrRuntimeError("capability parameters must be bounded text fields")
        if self.range is not None and (not isinstance(self.range, Mapping) or set(self.range) != {"start","end"}
                or any(isinstance(value,bool) or not isinstance(value,int) for value in self.range.values())
                or not 0 <= self.range["start"] < self.range["end"]):
            raise FrRuntimeError("capability range must be a nonempty byte span")
        if self.capability in _CAPABILITY_WRITES:
            if self.delivery is None:
                raise FrRuntimeError("writing capability requires delivery")
            _checks(self.checks,self.delivery)
        elif self.checks or self.delivery is not None:
            raise FrRuntimeError("read-only capability cannot declare source delivery")

    def to_data(self) -> dict[str, Any]:
        return {"kind":"capability","capability":self.capability,
                "parameters":dict(self.parameters or {}),"range":None if self.range is None else dict(self.range),
                "checks":list(self.checks),"delivery":None if self.delivery is None else self.delivery.to_data()}


IntentOperation = (TaskChangeOperation | AuthorBatchOperation | RecipeOperation
                   | FrameworkMigrationOperation | ApplicationMigrationOperation
                   | FormalPlanOperation | ProofSubmissionOperation
                   | ProjectQueryOperation | SurfaceEditOperation | PropertyTaskOperation | ProofTaskOperation | CapabilityOperation)
_OPERATION_TYPES = (TaskChangeOperation, AuthorBatchOperation, RecipeOperation,
                    FrameworkMigrationOperation, ApplicationMigrationOperation,
                    FormalPlanOperation, ProofSubmissionOperation,
                    ProjectQueryOperation, SurfaceEditOperation, PropertyTaskOperation, ProofTaskOperation, CapabilityOperation)
_OPERATION_CODES = {"task-change": 0, "author-batch": 1, "recipe": 2,
                    "framework-migration": 3, "application-migration": 3,
                    "formal-plan": 4, "proof-submission": 5,
                    "project-query": 6, "surface-edit": 7, "property-task": 8, "proof-task": 9}


def _operation_code(operation: Mapping[str, Any]) -> int:
    if operation["kind"] == "capability":
        return 11 if operation["capability"] in _CAPABILITY_WRITES else 10
    return _OPERATION_CODES[operation["kind"]]


def _action_purpose_allowed(purpose: int, operation: int) -> bool:
    return ((purpose == 2 and (0 <= operation <= 2 or operation in (7,11))) or (purpose == 3 and operation == 3)
            or (purpose == 4 and (4 <= operation <= 5 or 8 <= operation <= 9))
            or (0 <= purpose <= 4 and operation in (6,10)))


def _review_mode(purpose: int, operation: int, complete: bool, writable: bool,
                 write: bool, basis_supplied: bool, basis_matches: bool) -> int:
    if not _action_purpose_allowed(purpose, operation) or not complete:
        return 2
    if not write and not basis_supplied:
        return 0
    if writable and write and basis_supplied and basis_matches:
        return 1
    return 2


@dataclass(frozen=True)
class IntentGuideBinding:
    goal: Mapping[str, Any]
    basis: str

    def __post_init__(self) -> None:
        if (not isinstance(self.goal, Mapping) or self.goal.get("schema") != "fr-agent-goal-1"
                or not isinstance(self.basis, str) or not self.basis.startswith("frag1:")):
            raise FrRuntimeError("intent guide binding requires a goal and its retained basis")

    def to_data(self) -> dict[str, Any]:
        return {"goal":dict(self.goal),"basis":self.basis}


@dataclass(frozen=True)
class TaggedIntentAction:
    operation: IntentOperation
    diff_bytes: int = 4_096
    report_bytes: int = 65_536
    proof_expectation: str = "none"
    guide: IntentGuideBinding | None = None

    def __post_init__(self) -> None:
        if self.guide is not None and not isinstance(self.guide, IntentGuideBinding):
            raise FrRuntimeError("intent guide binding requires IntentGuideBinding")
        if self.proof_expectation not in ("none", "model"):
            raise FrRuntimeError("implementation proof expectations require unavailable separate evidence")
        if not isinstance(self.operation, _OPERATION_TYPES):
            raise FrRuntimeError("tagged intent action requires a typed operation")
        for value, low, high in ((self.diff_bytes, 0, 65_536),
                                (self.report_bytes, 256, 1_048_576)):
            if isinstance(value, bool) or not isinstance(value, int) or not low <= value <= high:
                raise FrRuntimeError("tagged intent action bound is invalid")

    def to_data(self) -> dict[str, Any]:
        return {"schema": "fr-intent-action-2", "operation": self.operation.to_data(),
                "diff_bytes": self.diff_bytes, "report_bytes": self.report_bytes,
                "proof_expectation": self.proof_expectation,
                "guide":None if self.guide is None else self.guide.to_data()}


def _review_complete(target_count: int, evidence_count: int, checks_declared: bool,
                     writable: bool, proof_required: bool, proof_checked: bool,
                     implementation_requested: bool, implementation_evidence: bool,
                     diff_complete: bool) -> bool:
    return (1 <= target_count <= 32 and evidence_count == target_count - 1
            and (not writable or checks_declared) and (not proof_required or proof_checked)
            and (not implementation_requested or implementation_evidence) and diff_complete)
