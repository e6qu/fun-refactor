"""Declarative bounded context requests for agent workflows."""

from __future__ import annotations
from dataclasses import dataclass, field
import hashlib
import json
import re
from typing import Any, Mapping, TYPE_CHECKING

from .context import ContextSession, ObjectStore, merkle_object_digest, store_merkle_value
from .ir import TaskChange
from .intent_actions import TaggedIntentAction, _operation_code, _action_purpose_allowed, _review_complete, _CAPABILITY_WRITES
from .runtime import FrReport, FrRuntimeError

if TYPE_CHECKING:
    from .runtime import FrClient

_NAME = re.compile(r"^[A-Za-z][A-Za-z0-9_-]{0,63}$")
_SECTION = re.compile(r"^[A-Za-z][A-Za-z0-9_]{0,63}$")
_PURPOSES = {
    "understand": ("code_map",),
    "trace": ("code_map", "call_traces", "sources_and_sinks"),
    "change": ("code_map", "impact"),
    "migrate": ("code_map", "impact", "sources_and_sinks"),
    "prove": ("code_map", "impact"),
}
_EVIDENCE_SECTIONS = frozenset(
    {section for sections in _PURPOSES.values() for section in sections}
)
_SECTION_CODES = {
    "code_map": 0,
    "call_traces": 1,
    "impact": 2,
    "sources_and_sinks": 3,
}
_PURPOSE_CODES = {
    "understand": 0,
    "trace": 1,
    "change": 2,
    "migrate": 3,
    "prove": 4,
}


def _intent_section_allowed(purpose: int, section: int) -> bool:
    return ((purpose, section) in {
        (0, 0),
        (1, 0), (1, 1), (1, 3),
        (2, 0), (2, 2),
        (3, 0), (3, 2), (3, 3),
        (4, 0), (4, 2),
    })


def _intent_admitted(needs: int, sections: int, calls: int, call_limit: int,
                     packet_bytes: int, packet_limit: int, target_matches: bool,
                     session_matches: bool, complete: bool) -> bool:
    return (1 <= needs <= 32 and 1 <= sections <= 8 and sections <= needs
            and 1 <= call_limit <= 512 and 0 <= calls <= call_limit
            and 1_024 <= packet_limit <= 65_536 and 0 <= packet_bytes <= packet_limit
            and target_matches and session_matches and complete)


@dataclass(frozen=True)
class IntentNeed:
    """One named projection from a top-level progressive evidence section."""
    name: str
    section: str
    pointer: str = ""

    def __post_init__(self) -> None:
        if not isinstance(self.name, str) or not _NAME.fullmatch(self.name):
            raise FrRuntimeError("intent need name is invalid")
        if not isinstance(self.section, str) or not _SECTION.fullmatch(self.section):
            raise FrRuntimeError("intent need section is invalid")
        if self.section not in _EVIDENCE_SECTIONS:
            raise FrRuntimeError("intent need section is unsupported")
        if not isinstance(self.pointer, str) or (self.pointer and (
                not self.pointer.startswith("/") or re.search(r"~(?:[^01]|$)", self.pointer))):
            raise FrRuntimeError("intent need pointer must be a canonical relative JSON Pointer")

    @property
    def absolute_pointer(self) -> str:
        return f"/model/{self.section}{self.pointer}"


@dataclass(frozen=True)
class AgentIntent:
    """A high-level goal expanded to deterministic progressive evidence reads."""
    target: str
    purpose: str
    needs: tuple[IntentNeed, ...] = ()
    token_limit: int = 4_096
    call_limit: int = 192
    packet_limit: int = 8_192
    action: IntentAction | TaggedIntentAction | None = None

    def __post_init__(self) -> None:
        if not isinstance(self.target, str) or not self.target or len(self.target.encode()) > 16_384:
            raise FrRuntimeError("agent intent target is invalid")
        if self.purpose not in _PURPOSES:
            raise FrRuntimeError("agent intent purpose is unsupported")
        needs = self.needs or tuple(IntentNeed(section, section) for section in _PURPOSES[self.purpose])
        if not isinstance(needs, tuple) or not 1 <= len(needs) <= 32:
            raise FrRuntimeError("agent intent needs 1 through 32 projections")
        if any(not isinstance(need, IntentNeed) for need in needs):
            raise FrRuntimeError("agent intent needs typed IntentNeed values")
        if len({need.name for need in needs}) != len(needs):
            raise FrRuntimeError("agent intent projection names must be unique")
        if len({need.section for need in needs}) > 8:
            raise FrRuntimeError("agent intent reaches at most eight evidence sections")
        if any(not _intent_section_allowed(_PURPOSE_CODES[self.purpose],
                                           _SECTION_CODES[need.section]) for need in needs):
            raise FrRuntimeError("agent intent need section is outside its declared purpose")
        for value, low, high, label in ((self.token_limit, 1_024, 4_096, "token"),
                                        (self.call_limit, 1, 512, "call"),
                                        (self.packet_limit, 1_024, 65_536, "packet")):
            if isinstance(value, bool) or not isinstance(value, int) or not low <= value <= high:
                raise FrRuntimeError(f"agent intent {label} bound is invalid")
        if isinstance(self.action, TaggedIntentAction):
            operation = self.action.operation.to_data()
            if (self.action.proof_expectation == "model" and self.purpose != "prove") or not _action_purpose_allowed(_PURPOSE_CODES[self.purpose], _operation_code(operation)):
                raise FrRuntimeError("intent action kind is outside its declared purpose")
        elif self.action is not None:
            if not isinstance(self.action, IntentAction):
                raise FrRuntimeError("agent intent action must be an IntentAction")
            if self.purpose != "change":
                raise FrRuntimeError("agent intent actions require purpose 'change'")
            change = self.action.task_change
            if change.requests or len(change.targets) != 1:
                raise FrRuntimeError(
                    "agent intent action requires one direct task-change target and no project requests"
                )
            if change.targets[0].handle != self.target:
                raise FrRuntimeError("agent intent action target must equal the intent target")
        object.__setattr__(self, "needs", needs)

    def to_data(self) -> dict[str, Any]:
        value = {"schema": "fr-agent-intent-1", "target": self.target,
                 "purpose": self.purpose,
                 "needs": [{"name": n.name, "section": n.section, "pointer": n.pointer}
                           for n in self.needs],
                 "token_limit": self.token_limit, "call_limit": self.call_limit,
                 "packet_limit": self.packet_limit}
        if self.action is not None:
            value["action"] = self.action.to_data()
        return value


@dataclass(frozen=True)
class IntentAction:
    """One direct reviewed task change compiled with its evidence intent."""
    task_change: TaskChange
    diff_bytes: int = 4_096
    report_bytes: int = 65_536

    def __post_init__(self) -> None:
        if not isinstance(self.task_change, TaskChange):
            raise FrRuntimeError("intent action requires a TaskChange")
        for value, low, high, label in (
            (self.diff_bytes, 0, 65_536, "diff"),
            (self.report_bytes, 256, 1_048_576, "report"),
        ):
            if isinstance(value, bool) or not isinstance(value, int) or not low <= value <= high:
                raise FrRuntimeError(f"intent action {label} bound is invalid")

    def to_data(self) -> dict[str, Any]:
        return {"task_change": self.task_change.to_data(),
                "diff_bytes": self.diff_bytes, "report_bytes": self.report_bytes}


@dataclass(frozen=True)
class PreparedIntent:
    intent: AgentIntent
    session: ContextSession
    packet: FrReport

    def at(self, pointer: str = "") -> Any:
        return self.packet.at(pointer)

    def to_data(self) -> Mapping[str, Any]:
        return self.packet.to_data()


@dataclass(frozen=True)
class CompiledIntent:
    """One native single-snapshot intent packet and any verified stored roots."""
    intent: AgentIntent
    packet: FrReport
    stored_digests: tuple[str, ...] = ()
    manifest: bytes = field(default=b"", repr=False)
    preview_sha256: str = field(default="", repr=False)
    action_basis: str | None = None

    def at(self, pointer: str = "") -> Any:
        return self.packet.at(pointer)

    def to_data(self) -> Mapping[str, Any]:
        return self.packet.to_data()


@dataclass(frozen=True)
class IntentResult:
    """The checked result of one unchanged intent-bound action review."""
    compiled: CompiledIntent
    report: FrReport

    def __post_init__(self) -> None:
        value = self.report.to_data()
        if (value.get("schema") != ("fr-agent-action-result-2" if isinstance(
                self.compiled.intent.action, TaggedIntentAction) else "fr-agent-action-result-1")
                or value.get("intent_basis") != self.compiled.at("/intent/basis")
                or value.get("action_basis") != self.compiled.action_basis
                or value.get("reviewed_context_omitted") is not True
                or value.get("executed") is not True
                or value.get("passed") is not True):
            raise FrRuntimeError("intent action result does not match its reviewed intent")

        if isinstance(self.compiled.intent.action, TaggedIntentAction):
            review = self.compiled.at("/action/review")
            if (value.get("kind") != review["kind"] or value.get("claims") != review["claims"]
                    or value.get("proof") != review.get("proof")
                    or value.get("proof_validation") != review.get("proof_validation")):
                raise FrRuntimeError("intent action result changed its reviewed claims or proof evidence")

    @property
    def passed(self) -> bool:
        return True

    def at(self, pointer: str = "") -> Any:
        return self.report.at(pointer)

    def to_data(self) -> Mapping[str, Any]:
        return self.report.to_data()


def _action_complete(intent: AgentIntent, action: Any) -> bool:
    if intent.action is None:
        return action is None
    if not isinstance(action, Mapping):
        return False
    review = action.get("review")
    if not isinstance(review, Mapping):
        return False
    if isinstance(intent.action, TaggedIntentAction):
        expected = intent.action.to_data()
        targets = review.get("targets")
        return (
            action.get("schema") == "fr-agent-action-2"
            and isinstance(action.get("basis"), str)
            and re.fullmatch(r"fraa2:[0-9a-f]{64}", action["basis"]) is not None
            and review.get("schema") == "fr-intent-operation-review-2"
            and len(_wire(review)) <= intent.action.report_bytes
            and review.get("kind") == expected["operation"]["kind"]
            and review.get("input_sha256") == hashlib.sha256(_wire(expected)).hexdigest()
            and review.get("ready") is True and review.get("executed") is False
            and isinstance(targets, list) and 1 <= len(targets) <= 32
            and all(isinstance(target, str) for target in targets)
            and len(set(targets)) == len(targets) and intent.target in targets
            and isinstance(review.get("diff"), str)
            and len(review["diff"].encode("utf-8")) <= intent.action.diff_bytes
            and isinstance(review.get("claims"), Mapping)
            and review["claims"].get("implementation_correspondence") is False
        )
    task = intent.action.task_change
    task_bytes = json.dumps(task.to_data(), ensure_ascii=False, sort_keys=True,
                            separators=(",", ":"), allow_nan=False).encode("utf-8")
    targets = review.get("targets")
    return (
        action.get("schema") == "fr-agent-action-1"
        and isinstance(action.get("basis"), str)
        and action.get("basis", "").startswith("fraa1:")
        and review.get("schema") == "fr-task-change-1"
        and review.get("manifest_sha256") == hashlib.sha256(task_bytes).hexdigest()
        and review.get("ready") is True
        and review.get("executed") is False
        and isinstance(targets, list)
        and len(targets) == 1
        and isinstance(targets[0], Mapping)
        and targets[0].get("handle") == intent.target
    )


def _wire(value: Any) -> bytes:
    return json.dumps(value, ensure_ascii=False, sort_keys=True,
                      separators=(",", ":"), allow_nan=False).encode("utf-8")


def _tagged_evidence_complete(intent: AgentIntent, packet: Mapping[str, Any]) -> bool:
    if not isinstance(intent.action, TaggedIntentAction):
        return "additional_evidence" not in packet
    action = packet.get("action")
    if not isinstance(action, Mapping) or not isinstance(action.get("review"), Mapping):
        return False
    evidence = packet.get("additional_evidence")
    targets = action["review"].get("targets")
    if (not isinstance(evidence, list) or not isinstance(targets, list)
            or len(evidence) != len(targets) - 1):
        return False
    expected = [target for target in targets if target != intent.target]
    names = {need.name for need in intent.needs}
    for entry, target in zip(evidence, expected, strict=True):
        if not isinstance(entry, Mapping):
            return False
        selected, digests = entry.get("selected"), entry.get("object_digests")
        if (not isinstance(entry.get("target"), Mapping) or entry["target"].get("handle") != target
                or any(not isinstance(entry.get(key), str) or not entry[key]
                       for key in ("view_basis", "object_root"))
                or not isinstance(selected, Mapping) or set(selected) != names
                or not isinstance(digests, Mapping) or set(digests) != names
                or any(merkle_object_digest(selected[name]) != digests[name] for name in names)):
            return False
    review = action["review"]
    writable = review.get("writable")
    claims, checks = review.get("claims"), review.get("checks")
    proof, validation = review.get("proof"), review.get("proof_validation")
    expectation = intent.action.proof_expectation
    operation = intent.action.operation.to_data()
    expected_writable = (operation["kind"] in ("task-change", "author-batch", "recipe", "framework-migration", "proof-submission", "surface-edit")
                         or (operation["kind"] == "formal-plan" and operation.get("package") is not None)
                         or (operation["kind"] == "capability" and operation["capability"] in _CAPABILITY_WRITES))
    if (writable is not expected_writable or not isinstance(claims, Mapping)
            or not isinstance(claims.get("model_theorem_checked"), bool)
            or review.get("proof_expectation") != expectation):
        return False
    proof_checked = (isinstance(proof, Mapping) and isinstance(proof.get("receipt"), str)
                     and bool(proof["receipt"]) and isinstance(validation, Mapping)
                     and validation.get("lake_build") is True
                     and validation.get("strict_correspondence") is True
                     and validation.get("implementation_correspondence") is False
                     and claims.get("model_theorem_checked") is True)
    expected_checks = operation.get("checks")
    expected_delivery = operation.get("delivery")
    if operation["kind"] == "task-change":
        expected_checks = operation["task_change"]["checks"]
        expected_delivery = operation["task_change"]["delivery"]
    checked = (isinstance(checks, Mapping) and isinstance(checks.get("names"), list)
               and all(isinstance(name, str) for name in checks["names"])
               and isinstance(expected_checks, list) and bool(expected_checks)
               and set(checks["names"]) == set(expected_checks)
               and len(checks["names"]) == len(expected_checks)
               and isinstance(checks.get("configuration_basis"), str)
               and review.get("delivery") == expected_delivery)
    if not _review_complete(len(targets), len(evidence), checked, writable,
                            operation["kind"] == "proof-submission" or expectation == "model",
                            proof_checked, expectation == "implementation",
                            claims.get("implementation_correspondence") is True,
                            isinstance(review.get("diff"), str)):
        return False
    normalized = dict(packet)
    normalized["serialized_bytes"] = 0
    normalized["action"] = {key: value for key, value in action.items() if key != "basis"}
    basis = "fraa2:" + hashlib.sha256(_wire(["fr-agent-action-review-2", normalized])).hexdigest()
    return action.get("basis") == basis


def compile_intent(client: FrClient, intent: AgentIntent,
                   *, store: ObjectStore | None = None) -> CompiledIntent:
    """Compile an intent natively in one ``fr`` process and verify its packet."""
    if not isinstance(intent, AgentIntent):
        raise FrRuntimeError("compile_intent requires an AgentIntent")
    manifest = json.dumps(intent.to_data(), ensure_ascii=False, sort_keys=True,
                          separators=(",", ":"), allow_nan=False).encode("utf-8")
    report = client.call("intent", "--from", "-", input_bytes=manifest)
    value = report.to_data()
    intent_data = value.get("intent")
    target = value.get("target")
    selected = value.get("selected")
    digests = value.get("object_digests")
    execution = value.get("execution")
    limits = value.get("limits")
    action = value.get("action")
    expected_names = {need.name for need in intent.needs}
    manifest_sha256 = hashlib.sha256(manifest).hexdigest()
    identity_complete = all(isinstance(value.get(name), str) and value.get(name)
                            for name in ("revision", "context_basis", "view_basis", "object_root"))
    complete = (
        report.schema == "fr-agent-context-1"
        and isinstance(intent_data, Mapping)
        and intent_data.get("schema") == "fr-agent-intent-1"
        and intent_data.get("purpose") == intent.purpose
        and intent_data.get("manifest_sha256") == manifest_sha256
        and intent_data.get("basis") == f"frai1:{manifest_sha256}"
        and isinstance(target, Mapping) and target.get("handle") == intent.target
        and value.get("view") == "evidence" and value.get("profile") == "compact"
        and value.get("calls") == 0
        and identity_complete
        and value.get("cached_objects") == []
        and execution == {"engine": "native", "project_snapshots": 1,
                          "progressive_disclosure_calls": 0}
        and limits == {"packet_bytes": intent.packet_limit,
                       "progressive_disclosure_calls": intent.call_limit}
        and _action_complete(intent, action)
        and _tagged_evidence_complete(intent, value)
        and isinstance(selected, Mapping) and set(selected) == expected_names
        and isinstance(digests, Mapping) and set(digests) == expected_names
        and all(isinstance(digests[name], str)
                and merkle_object_digest(selected[name]) == digests[name]
                for name in expected_names)
    )
    serialized = json.dumps(value, ensure_ascii=False, sort_keys=True,
                            separators=(",", ":"), allow_nan=False).encode("utf-8")
    packet_bytes = value.get("serialized_bytes")
    sections = len({need.section for need in intent.needs})
    if (isinstance(packet_bytes, bool) or not isinstance(packet_bytes, int)
            or packet_bytes != len(serialized)
            or not _intent_admitted(len(intent.needs), sections, 0, intent.call_limit,
                                    packet_bytes, intent.packet_limit,
                                    isinstance(target, Mapping)
                                    and target.get("handle") == intent.target,
                                    identity_complete,
                                    complete)):
        raise FrRuntimeError("native agent intent failed its final admission policy")
    if not isinstance(selected, Mapping) or not isinstance(digests, Mapping):
        raise FrRuntimeError("native agent intent has no complete selected object map")
    stored: list[str] = []
    if store is not None:
        for name in sorted(expected_names):
            stored.append(store_merkle_value(
                store, selected[name], expected_digest=digests[name],
            ).digest)
    action_basis = action.get("basis") if isinstance(action, Mapping) else None
    preview_sha256 = hashlib.sha256(serialized).hexdigest()
    if store is not None and isinstance(intent.action, TaggedIntentAction):
        for evidence in value["additional_evidence"]:
            for name in sorted(expected_names):
                stored.append(store_merkle_value(store, evidence["selected"][name],
                    expected_digest=evidence["object_digests"][name]).digest)
    return CompiledIntent(intent, report, tuple(stored), manifest,
                          preview_sha256, action_basis)


def execute_intent(client: FrClient, compiled: CompiledIntent) -> IntentResult:
    """Execute one unchanged native intent action and its reviewed lifecycle."""
    if not isinstance(compiled, CompiledIntent) or compiled.action_basis is None:
        raise FrRuntimeError("execute_intent requires a compiled intent action")
    current = json.dumps(compiled.packet.to_data(), ensure_ascii=False, sort_keys=True,
                         separators=(",", ":"), allow_nan=False).encode("utf-8")
    if (hashlib.sha256(compiled.manifest).hexdigest()
            != hashlib.sha256(json.dumps(
                compiled.intent.to_data(), ensure_ascii=False, sort_keys=True,
                separators=(",", ":"), allow_nan=False,
            ).encode("utf-8")).hexdigest()
            or hashlib.sha256(current).hexdigest() != compiled.preview_sha256):
        raise FrRuntimeError("compiled intent action changed after review")
    report = client.call(
        "intent", "--from", "-", "--write", "--basis", compiled.action_basis,
        input_bytes=compiled.manifest,
    )
    return IntentResult(compiled, report)


def prepare_intent(client: FrClient, intent: AgentIntent,
                   *, store: ObjectStore | None = None) -> PreparedIntent:
    """Compile an intent into bounded reads and one verified context packet."""
    if not isinstance(intent, AgentIntent):
        raise FrRuntimeError("prepare_intent requires an AgentIntent")
    session = client.context(intent.target, view="evidence", token_limit=intent.token_limit,
                             store=store)
    start = session.calls
    sections = tuple(dict.fromkeys(need.section for need in intent.needs))
    for section in sections:
        remaining = intent.call_limit - (session.calls - start)
        if remaining < 1:
            raise FrRuntimeError("agent intent exhausted its shared call bound")
        session.materialize_section(section, max_calls=min(remaining, 64))
    selected = {need.name: need.absolute_pointer for need in intent.needs}
    packet = session.packet(selected, include_actions=False, max_bytes=intent.packet_limit)
    calls = session.calls - start
    if not _intent_admitted(len(intent.needs), len(sections), calls, intent.call_limit,
                            packet.at("/serialized_bytes"), intent.packet_limit,
                            session.session_identity()[3] == intent.target,
                            session.latest.session_identity() == session.reports[0].session_identity(),
                            set(packet.at("/selected")) == set(selected)):
        raise FrRuntimeError("prepared agent intent failed its final admission policy")
    return PreparedIntent(intent, session, packet)
