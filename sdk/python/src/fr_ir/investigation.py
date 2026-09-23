"""Typed, locally persistent investigations; execution stays with reviewed delivery."""
from __future__ import annotations

from dataclasses import asdict, dataclass, field
from enum import Enum
import json
from pathlib import Path
import tempfile
from typing import Any, Mapping, TYPE_CHECKING

from .context import ObjectStore, restore_stored_value, store_merkle_value
from .runtime import FrClient, FrReport, FrRuntimeError, Occurrence

if TYPE_CHECKING:
    from .guide import GuideAction


class StepState(str, Enum):
    PENDING = "pending"
    READY = "ready"
    RUNNING = "running"
    SATISFIED = "satisfied"
    BLOCKED = "blocked"
    STALE = "stale"


class DependencyKind(str, Enum):
    SOURCE = "source"
    CONFIGURATION = "configuration"
    LOOKUP = "lookup"
    ANALYZER = "analyzer"
    WORKSPACE = "workspace"
    CHECK_CONFIGURATION = "check-configuration"
    CHECK_SOURCES = "check-sources"
    CHECK_TOOLCHAIN = "check-toolchain"


class EvidenceKind(str, Enum):
    OBSERVATION = "observation"
    CHECK = "check"
    MODEL_PROOF = "model-proof"
    SOURCE_CORRESPONDENCE = "source-correspondence"


@dataclass(frozen=True)
class Dependency:
    kind: DependencyKind
    key: str
    digest: str | None = None


@dataclass(frozen=True)
class Evidence:
    id: str
    kind: EvidenceKind
    input_digest: str
    passed: bool
    reference: str


@dataclass(frozen=True)
class TaskStep:
    id: str
    question: str
    inputs: tuple[Dependency, ...]
    depends_on: tuple[str, ...] = ()
    state: StepState = StepState.PENDING
    evidence: tuple[Evidence, ...] = ()
    required_checks: tuple[str, ...] = ()
    satisfies: tuple[str, ...] = ()
    action: tuple[str, ...] = ()
    action_input: Mapping[str, Any] | None = None

    @classmethod
    def checked(cls, id: str, question: str, *, checks: tuple[str, ...],
                satisfies: tuple[str, ...] = (), depends_on: tuple[str, ...] = ()) -> TaskStep:
        if not checks or len(set(checks)) != len(checks):
            raise FrRuntimeError("checked step needs distinct required checks")
        return cls(id, question, tuple(Dependency(kind, "selected-project") for kind in (
            DependencyKind.WORKSPACE, DependencyKind.CHECK_CONFIGURATION,
            DependencyKind.CHECK_SOURCES, DependencyKind.CHECK_TOOLCHAIN,
        )), depends_on=depends_on, required_checks=checks, satisfies=satisfies)

    @classmethod
    def from_guide(cls, id: str, question: str, action: GuideAction, *,
                   satisfies: tuple[str, ...] = (), depends_on: tuple[str, ...] = ()) -> TaskStep:
        value = action.to_data()
        if value.get("ready") is not True or value.get("author_fields"):
            raise FrRuntimeError("task action requires authored guide inputs")
        return cls(id, question, (Dependency(DependencyKind.WORKSPACE, "guide-snapshot",
                                            action.guide.report.at("/revision")),),
                   depends_on=depends_on, satisfies=satisfies, action=action.arguments,
                   action_input=value.get("input"))


@dataclass(frozen=True)
class TaskPlan:
    goal: str
    acceptance: tuple[str, ...]
    steps: tuple[TaskStep, ...]
    hypotheses: tuple[str, ...] = ()
    questions: tuple[str, ...] = ()
    schema: str = field(default="fr-investigation-plan-1", init=False)

    def to_data(self) -> dict[str, Any]:
        return json.loads(json.dumps(asdict(self)))

    @classmethod
    def from_data(cls, value: Any) -> TaskPlan:
        if not isinstance(value, Mapping) or value.get("schema") != "fr-investigation-plan-1":
            raise FrRuntimeError("unsupported investigation plan")
        _fields(value, {"schema", "goal", "acceptance", "steps", "hypotheses", "questions"})
        try:
            steps = []
            for step in value["steps"]:
                _fields(step, {"id", "question", "inputs", "depends_on", "state", "evidence",
                               "required_checks", "satisfies", "action", "action_input"})
                if step.get("action_input") is not None and not isinstance(step["action_input"], Mapping):
                    raise FrRuntimeError("task action input must be an object")
                inputs = []
                for dep in step["inputs"]:
                    _fields(dep, {"kind", "key", "digest"})
                    inputs.append(Dependency(DependencyKind(dep["kind"]), _text(dep["key"]), dep.get("digest")))
                evidence = []
                for item in step.get("evidence", []):
                    _fields(item, {"id", "kind", "input_digest", "passed", "reference"})
                    if type(item["passed"]) is not bool:
                        raise FrRuntimeError("evidence passed must be boolean")
                    evidence.append(Evidence(_text(item["id"]), EvidenceKind(item["kind"]),
                                             _text(item["input_digest"]), item["passed"], _text(item["reference"])))
                steps.append(TaskStep(_text(step["id"]), _text(step["question"]), tuple(inputs),
                                      _texts(step.get("depends_on", [])), StepState(step.get("state", "pending")),
                                      tuple(evidence), _texts(step.get("required_checks", [])),
                                      _texts(step.get("satisfies", [])), _texts(step.get("action", [])), step.get("action_input")))
            return cls(_text(value["goal"]), _texts(value["acceptance"]), tuple(steps),
                       _texts(value.get("hypotheses", [])), _texts(value.get("questions", [])))
        except (KeyError, TypeError, ValueError) as error:
            raise FrRuntimeError("malformed investigation plan") from error

    def store(self, store: ObjectStore) -> str:
        """Persist through the existing canonical Merkle store, returning an immutable root."""
        return store_merkle_value(store, self.to_data()).digest

    @classmethod
    def restore(cls, store: ObjectStore, digest: str) -> TaskPlan:
        return cls.from_data(restore_stored_value(store, digest))

    def resume(self, client: FrClient, *, transition: str | None = None) -> ResumedPlan:
        """Revalidate input dependencies before reporting any retained step as satisfied."""
        with tempfile.TemporaryDirectory(prefix="fr-plan-") as directory:
            path = Path(directory) / "plan.json"
            path.write_text(json.dumps(self.to_data()), encoding="utf-8")
            arguments = ["investigate", "--from", str(path)]
            if transition is not None:
                arguments.extend(["--transition", transition])
            report = client.project(*arguments)
        if report.schema != "fr-investigation-resume-1":
            raise FrRuntimeError("unsupported investigation resume report")
        data = report.to_data()
        return ResumedPlan(TaskPlan.from_data(data["plan"]), data["input_digests"],
                           _texts(data["invalidated"]), data["complete"], report)


@dataclass(frozen=True)
class ResumedPlan:
    plan: TaskPlan
    input_digests: Mapping[str, str]
    invalidated: tuple[str, ...]
    complete: bool
    report: FrReport


@dataclass(frozen=True)
class FlowWitness:
    sink: str
    context: str
    rules: str
    origin: str
    occurrences: tuple[Occurrence, ...]
    claim: str

    @classmethod
    def from_data(cls, value: Any) -> FlowWitness:
        try:
            return cls(_text(value["sink"]), _text(value["context"]), _text(value["rules"]),
                       _text(value["trace"]["origin"]),
                       tuple(Occurrence.from_data(o) for o in value["trace"]["occurrences"]),
                       _text(value["claim"]))
        except (KeyError, TypeError, ValueError) as error:
            raise FrRuntimeError("malformed flow witness") from error


def flow_witnesses(report: FrReport) -> tuple[FlowWitness, ...]:
    if report.schema != "fr-dataflow-1":
        raise FrRuntimeError("report is not a scalar dataflow analysis")
    data = report.to_data()
    witnesses = tuple(FlowWitness.from_data(item) for item in data["witnesses"])
    if any(o.revision != data["revision"] for w in witnesses for o in w.occurrences):
        raise FrRuntimeError("flow occurrence belongs to another revision")
    return witnesses


def _fields(value: Any, allowed: set[str]) -> None:
    if not isinstance(value, Mapping) or set(value) - allowed:
        raise FrRuntimeError("unknown investigation fields")


def _text(value: Any) -> str:
    if not isinstance(value, str) or not value:
        raise FrRuntimeError("investigation text must be nonempty")
    return value


def _texts(value: Any) -> tuple[str, ...]:
    if not isinstance(value, list):
        raise FrRuntimeError("investigation list expected")
    return tuple(_text(item) for item in value)
