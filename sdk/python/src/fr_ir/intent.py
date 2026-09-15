"""Declarative bounded context requests for agent workflows."""

from __future__ import annotations
from dataclasses import dataclass
import re
from typing import Any, Mapping, TYPE_CHECKING

from .context import ContextSession, ObjectStore
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
        for value, low, high, label in ((self.token_limit, 1_024, 4_096, "token"),
                                        (self.call_limit, 1, 512, "call"),
                                        (self.packet_limit, 1_024, 65_536, "packet")):
            if isinstance(value, bool) or not isinstance(value, int) or not low <= value <= high:
                raise FrRuntimeError(f"agent intent {label} bound is invalid")
        object.__setattr__(self, "needs", needs)

    def to_data(self) -> dict[str, Any]:
        return {"schema": "fr-agent-intent-1", "target": self.target,
                "purpose": self.purpose,
                "needs": [{"name": n.name, "section": n.section, "pointer": n.pointer}
                          for n in self.needs],
                "token_limit": self.token_limit, "call_limit": self.call_limit,
                "packet_limit": self.packet_limit}


@dataclass(frozen=True)
class PreparedIntent:
    intent: AgentIntent
    session: ContextSession
    packet: FrReport

    def at(self, pointer: str = "") -> Any:
        return self.packet.at(pointer)

    def to_data(self) -> Mapping[str, Any]:
        return self.packet.to_data()


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
