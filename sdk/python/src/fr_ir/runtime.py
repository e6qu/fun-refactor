"""Bounded subprocess runtime for agent-facing ``fr`` workflows.

The runtime keeps command responses as Python data.  It never invokes a shell and
only replays progressive-disclosure actions that were returned by ``fr`` itself.
"""

from __future__ import annotations

from dataclasses import dataclass, field
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
from typing import Any, Mapping, Sequence, TYPE_CHECKING, cast

from ._version import VERSION

if TYPE_CHECKING:
    from .ir import TaskChange
    from .context import ContextSession, ObjectStore
    from .intent import AgentIntent, CompiledIntent, IntentResult, PreparedIntent
    from .guide import AgentGoal, AgentGuide, GuideAction, GuideReview, GuideRun
    from .intent_actions import TaggedIntentAction
    from .formal_kernel import KernelRequest, KernelResult


_BASIS = re.compile(r"^frtc1:[0-9a-f]{64}$")
_REVISION = re.compile(r"^[0-9a-f]{64}$")
_MAX_ARGUMENTS = 128
_MAX_ARGUMENT_BYTES = 16_384
_PROTOCOL_REVISION = 1
_REQUEST_SCHEMAS = (
    "fr-agent-action-2",
    "fr-agent-goal-1",
    "fr-agent-intent-1",
    "fr-task-change-1",
)
_RESPONSE_SCHEMAS = (
    "fr-agent-action-result-2",
    "fr-agent-context-1",
    "fr-agent-guide-1",
    "fr-intent-operation-review-2",
    "fr-progressive-disclosure-1",
)


class FrRuntimeError(RuntimeError):
    """A bounded ``fr`` command failed or returned an invalid protocol value."""

    def __init__(
        self,
        message: str,
        *,
        arguments: Sequence[str] = (),
        exit_code: int | None = None,
        report: Mapping[str, Any] | None = None,
    ) -> None:
        super().__init__(message)
        self.arguments = tuple(arguments)
        self.exit_code = exit_code
        self.report = dict(report) if report is not None else None


def _integer(value: Any, label: str, *, positive: bool = False) -> int:
    if (isinstance(value, bool) or not isinstance(value, int)
            or value < (1 if positive else 0)):
        qualifier = "positive" if positive else "nonnegative"
        raise FrRuntimeError(f"{label} must be a {qualifier} integer")
    return value


@dataclass(frozen=True, order=True)
class ByteSpan:
    """A half-open UTF-8 byte range in source text."""

    start: int
    end: int

    def __post_init__(self) -> None:
        _integer(self.start, "byte span start")
        _integer(self.end, "byte span end")
        if self.end < self.start:
            raise FrRuntimeError("byte span end must not precede its start")

    @classmethod
    def from_data(cls, value: Any) -> ByteSpan:
        if not isinstance(value, Mapping) or set(value) != {"start", "end"}:
            raise FrRuntimeError("byte span must contain exactly start and end")
        return cls(value["start"], value["end"])

    def to_data(self) -> dict[str, int]:
        """Return the public JSON shape for this byte span."""
        return {"start": self.start, "end": self.end}

    def contains(self, other: ByteSpan) -> bool:
        """Return whether this span completely contains another span."""
        if not isinstance(other, ByteSpan):
            raise FrRuntimeError("byte span containment requires another ByteSpan")
        return self.start <= other.start <= other.end <= self.end


@dataclass(frozen=True, order=True)
class TextPosition:
    """A 1-based line and Unicode-column position in source text."""

    line: int
    col: int

    def __post_init__(self) -> None:
        _integer(self.line, "text position line", positive=True)
        _integer(self.col, "text position column", positive=True)

    @classmethod
    def from_data(cls, value: Any) -> TextPosition:
        if not isinstance(value, Mapping) or set(value) != {"line", "col"}:
            raise FrRuntimeError("text position must contain exactly line and col")
        return cls(value["line"], value["col"])


def _advance_text_position(start: TextPosition, text: str) -> TextPosition:
    """Advance a source position over Unicode text whose first byte is at ``start``."""
    lines = text.split("\n")
    if len(lines) == 1:
        return TextPosition(start.line, start.col + len(text))
    return TextPosition(start.line + len(lines) - 1, len(lines[-1]) + 1)


def _text_position_at(source: str, offset: int) -> TextPosition:
    """Map one UTF-8 byte offset using the native runtime's trailing-newline rule."""
    encoded = source.encode("utf-8")
    if offset > len(encoded):
        raise FrRuntimeError("text location extends beyond its source")
    try:
        prefix = encoded[:offset].decode("utf-8")
    except UnicodeDecodeError as error:
        raise FrRuntimeError("text location does not fall on UTF-8 boundaries") from error
    if offset == len(encoded) and source.endswith("\n"):
        prefix = prefix[:-1]
    tail = prefix.rsplit("\n", 1)[-1]
    return TextPosition(prefix.count("\n") + 1, len(tail) + 1)


@dataclass(frozen=True)
class TextRange:
    """A half-open range between two 1-based source positions."""

    start: TextPosition
    end: TextPosition

    def __post_init__(self) -> None:
        if not isinstance(self.start, TextPosition) or not isinstance(self.end, TextPosition):
            raise FrRuntimeError("text range endpoints must be TextPosition values")
        if self.end < self.start:
            raise FrRuntimeError("text range end must not precede its start")

    @classmethod
    def from_data(cls, value: Any) -> TextRange:
        if not isinstance(value, Mapping) or set(value) != {"start", "end"}:
            raise FrRuntimeError("text range must contain exactly start and end")
        return cls(TextPosition.from_data(value["start"]), TextPosition.from_data(value["end"]))


@dataclass(frozen=True)
class TextLocation:
    """Matching half-open UTF-8 byte and 1-based line/column ranges."""

    span: ByteSpan
    range: TextRange

    def __post_init__(self) -> None:
        if not isinstance(self.span, ByteSpan) or not isinstance(self.range, TextRange):
            raise FrRuntimeError("text location fields have invalid types")

    @classmethod
    def from_data(cls, value: Any) -> TextLocation:
        if not isinstance(value, Mapping) or set(value) != {"span", "range"}:
            raise FrRuntimeError("text location must contain exactly span and range")
        return cls(ByteSpan.from_data(value["span"]), TextRange.from_data(value["range"]))

    def text(self, source: str) -> str:
        """Verify and slice this location from the exact UTF-8 source revision it describes."""
        if not isinstance(source, str):
            raise FrRuntimeError("text location source must be text")
        encoded = source.encode("utf-8")
        if self.span.end > len(encoded):
            raise FrRuntimeError("text location extends beyond its source")
        try:
            text = encoded[self.span.start:self.span.end].decode("utf-8")
        except UnicodeDecodeError as error:
            raise FrRuntimeError("text location does not fall on UTF-8 boundaries") from error
        expected = TextRange(
            _text_position_at(source, self.span.start),
            _text_position_at(source, self.span.end),
        )
        if self.range != expected:
            raise FrRuntimeError("text location line range does not match its source")
        return text


@dataclass(frozen=True)
class SourceFragment:
    """One exact, file-located source fragment returned by progressive disclosure."""

    id: str
    digest: str
    offset: int
    text: str
    total_bytes: int
    location: TextLocation

    def __post_init__(self) -> None:
        if not isinstance(self.id, str) or not self.id:
            raise FrRuntimeError("source fragment id must be nonempty text")
        if not isinstance(self.digest, str) or re.fullmatch(r"[0-9a-f]{64}", self.digest) is None:
            raise FrRuntimeError("source fragment digest must be a lowercase SHA-256 digest")
        _integer(self.offset, "source fragment offset")
        _integer(self.total_bytes, "source fragment total bytes")
        if not isinstance(self.text, str):
            raise FrRuntimeError("source fragment text must be text")
        if not isinstance(self.location, TextLocation):
            raise FrRuntimeError("source fragment location must be a TextLocation")
        if self.offset + len(self.text.encode("utf-8")) > self.total_bytes:
            raise FrRuntimeError("source fragment extends beyond its committed source")
        if self.location.span.end - self.location.span.start != len(self.text.encode("utf-8")):
            raise FrRuntimeError("source fragment location does not match its text")
        if self.location.span.start < self.offset:
            raise FrRuntimeError("source fragment location precedes its relative offset")
        expected_end = _advance_text_position(self.location.range.start, self.text)
        allowed_ends = {expected_end}
        if self.offset + len(self.text.encode("utf-8")) == self.total_bytes and self.text.endswith("\n"):
            allowed_ends.add(_advance_text_position(self.location.range.start, self.text[:-1]))
        if self.location.range.end not in allowed_ends:
            raise FrRuntimeError("source fragment line range does not match its text")

    @property
    def span(self) -> ByteSpan:
        """Return this fragment's file-relative half-open byte span."""
        return self.location.span

    @property
    def origin(self) -> int:
        """Return the file offset of the committed declaration source."""
        return self.location.span.start - self.offset

    @property
    def source_span(self) -> ByteSpan:
        """Return the complete committed declaration's file-relative byte span."""
        return ByteSpan(self.origin, self.origin + self.total_bytes)

    @classmethod
    def from_data(cls, value: Any) -> SourceFragment:
        fields = {"id", "domain", "digest", "offset", "returned_bytes", "total_bytes", "text",
                  "location"}
        if not isinstance(value, Mapping) or set(value) != fields or value.get("domain") != "exact-source":
            raise FrRuntimeError("exact source fragment has an unsupported shape")
        fragment = cls(value["id"], value["digest"], value["offset"], value["text"],
                       value["total_bytes"], TextLocation.from_data(value["location"]))
        if (isinstance(value["returned_bytes"], bool)
                or not isinstance(value["returned_bytes"], int)
                or value["returned_bytes"] != len(fragment.text.encode("utf-8"))):
            raise FrRuntimeError("source fragment returned byte count does not match its text")
        return fragment

    def text_at(self, location: TextLocation) -> str:
        """Extract a covered text location without treating byte offsets as character indexes."""
        if not isinstance(location, TextLocation):
            raise FrRuntimeError("source fragment extraction requires a TextLocation")
        if not self.span.contains(location.span):
            raise FrRuntimeError("text location is outside this source fragment")
        if (not self.location.range.start <= location.range.start
                or not location.range.end <= self.location.range.end):
            raise FrRuntimeError("text line range is outside this source fragment")
        encoded = self.text.encode("utf-8")
        start = location.span.start - self.span.start
        end = location.span.end - self.span.start
        try:
            prefix = encoded[:start].decode("utf-8")
            text = encoded[start:end].decode("utf-8")
        except UnicodeDecodeError as error:
            raise FrRuntimeError("text location does not fall on UTF-8 boundaries") from error
        expected_start = (self.location.range.end if start == len(encoded)
                          else _advance_text_position(self.location.range.start, prefix))
        expected_end = (self.location.range.end if end == len(encoded)
                        else _advance_text_position(expected_start, text))
        if location.range != TextRange(expected_start, expected_end):
            raise FrRuntimeError("text location line range does not match its source fragment")
        return text


@dataclass(frozen=True)
class DefinitionLocation:
    """The exact identifier and complete AST definition locations for one symbol."""

    name: TextLocation
    definition: TextLocation

    def __post_init__(self) -> None:
        if not isinstance(self.name, TextLocation) or not isinstance(self.definition, TextLocation):
            raise FrRuntimeError("definition location fields must be TextLocation values")
        if not (self.definition.span.start <= self.name.span.start <= self.name.span.end
                <= self.definition.span.end):
            raise FrRuntimeError("definition location must contain its name location")
        if not (self.definition.range.start <= self.name.range.start
                <= self.name.range.end <= self.definition.range.end):
            raise FrRuntimeError("definition line range must contain its name range")

    @classmethod
    def from_data(cls, value: Any) -> DefinitionLocation:
        if not isinstance(value, Mapping) or set(value) != {"name", "definition"}:
            raise FrRuntimeError("definition location must contain exactly name and definition")
        return cls(TextLocation.from_data(value["name"]),
                   TextLocation.from_data(value["definition"]))


@dataclass(frozen=True)
class AgentTarget:
    """A revision-bound agent target with an optional exact AST definition location."""

    handle: str
    name: str | None = None
    kind: str | None = None
    path: str | None = None
    language: str | None = None
    position: str | None = None
    location: DefinitionLocation | None = None

    def __post_init__(self) -> None:
        if not isinstance(self.handle, str) or not self.handle:
            raise FrRuntimeError("agent target handle must be nonempty text")
        for name in ("name", "kind", "path", "language", "position"):
            value = getattr(self, name)
            if value is not None and not isinstance(value, str):
                raise FrRuntimeError(f"agent target {name} must be text or null")
        if self.location is not None and not isinstance(self.location, DefinitionLocation):
            raise FrRuntimeError("agent target location must be a DefinitionLocation or null")

    @classmethod
    def from_data(cls, value: Any) -> AgentTarget:
        fields = {"handle", "name", "kind", "path", "language", "position", "location"}
        if (not isinstance(value, Mapping) or not set(value) <= fields
                or not isinstance(value.get("handle"), str) or not value["handle"]):
            raise FrRuntimeError("agent target has an unsupported shape")
        for name in fields - {"handle", "location"}:
            field_value = value.get(name)
            if field_value is not None and not isinstance(field_value, str):
                raise FrRuntimeError(f"agent target {name} must be text or null")
        location = value.get("location")
        return cls(
            value["handle"], value.get("name"), value.get("kind"), value.get("path"),
            value.get("language"), value.get("position"),
            DefinitionLocation.from_data(location) if location is not None else None,
        )


def _copy_json(value: Any) -> Any:
    return json.loads(json.dumps(value, ensure_ascii=False, allow_nan=False))


def _pointer(value: Any, pointer: str) -> Any:
    if pointer == "":
        return value
    if not isinstance(pointer, str) or not pointer.startswith("/"):
        raise FrRuntimeError("report pointer must be an RFC 6901 JSON Pointer")
    current = value
    for encoded in pointer[1:].split("/"):
        if re.search(r"~(?:[^01]|$)", encoded):
            raise FrRuntimeError("report pointer uses a non-canonical escape")
        part = encoded.replace("~1", "/").replace("~0", "~")
        if isinstance(current, list):
            if not part.isascii() or not part.isdigit() or (part.startswith("0") and part != "0"):
                raise FrRuntimeError(f"report pointer has an invalid array index at {encoded!r}")
            index = int(part)
            if index >= len(current):
                raise FrRuntimeError(f"report pointer is absent at {encoded!r}")
            current = current[index]
        elif isinstance(current, dict) and part in current:
            current = current[part]
        else:
            raise FrRuntimeError(f"report pointer is absent at {encoded!r}")
    return current


@dataclass(frozen=True)
class FrReport:
    """One parsed JSON object returned by ``fr``."""

    _value: Mapping[str, Any] = field(repr=False)
    arguments: tuple[str, ...]

    @property
    def schema(self) -> str | None:
        value = self._value.get("schema")
        return value if isinstance(value, str) else None

    def to_data(self) -> dict[str, Any]:
        """Return a detached JSON copy suitable for ordinary Python processing."""
        return _copy_json(self._value)

    def at(self, pointer: str = "") -> Any:
        """Return a detached value at one RFC 6901 pointer."""
        return _copy_json(_pointer(self._value, pointer))

    def definition_targets(self) -> tuple[AgentTarget, ...]:
        """Return revision-bound definitions from a project query report."""
        query = self._value.get("query")
        if self.schema != "fr-project-1" or query not in {
            "find", "select", "map", "explore", "show",
        }:
            raise FrRuntimeError("report is not a project definition query")
        revision = self._value.get("revision")
        if not isinstance(revision, str) or not _REVISION.fullmatch(revision):
            raise FrRuntimeError("project definition report has no valid revision")

        if query == "show":
            node = self._value.get("node")
            if not isinstance(node, Mapping):
                raise FrRuntimeError("project show report has no node object")
            rows: list[Any] = [node]
            columns_value = None
        else:
            rows_value = self._value.get("rows")
            if not isinstance(rows_value, list):
                raise FrRuntimeError("project definition report has no row array")
            rows = rows_value
            columns_value = self._value.get("columns")
        columns: tuple[str, ...] | None = None
        if columns_value is not None:
            if (not isinstance(columns_value, list)
                    or any(not isinstance(item, str) or not item for item in columns_value)
                    or len(set(columns_value)) != len(columns_value)):
                raise FrRuntimeError("project definition report columns are invalid")
            columns = tuple(columns_value)
            if "handle" not in columns or "location" not in columns:
                raise FrRuntimeError("project report does not expose definition locations")

        def optional_text(row: Mapping[str, Any], name: str) -> str | None:
            value = row.get(name)
            if value is None or isinstance(value, str):
                return value
            if (isinstance(value, Mapping) and set(value) == {"text", "omitted_bytes"}
                    and isinstance(value.get("text"), str)
                    and not isinstance(value.get("omitted_bytes"), bool)
                    and isinstance(value.get("omitted_bytes"), int)
                    and value["omitted_bytes"] > 0):
                return None
            raise FrRuntimeError(f"project definition {name} is malformed")

        targets: list[AgentTarget] = []
        handle_pattern = re.compile(rf"^frp1:{revision[:32]}:[0-9a-f]+$")
        for item in rows:
            if isinstance(item, Mapping):
                row = item
            elif isinstance(item, list) and columns is not None:
                if len(item) != len(columns):
                    raise FrRuntimeError("project definition row does not match its columns")
                row = dict(zip(columns, item, strict=True))
            else:
                raise FrRuntimeError("project definition row has an unsupported shape")
            location_value = row.get("location")
            if location_value is None:
                continue
            handle = row.get("handle")
            if not isinstance(handle, str) or not handle_pattern.fullmatch(handle):
                raise FrRuntimeError("project definition handle does not match its revision")
            location = DefinitionLocation.from_data(location_value)
            line = row.get("line")
            if line is not None and (_integer(line, "project definition line", positive=True)
                                     != location.name.range.start.line):
                raise FrRuntimeError("project definition line does not match its location")
            path = optional_text(row, "path")
            start = location.name.range.start
            reported_position = row.get("position")
            if reported_position is not None:
                if (not isinstance(reported_position, Mapping)
                        or set(reported_position) != {"line", "col"}
                        or _integer(reported_position["line"], "project definition position line",
                                    positive=True) != start.line
                        or _integer(reported_position["col"], "project definition position column",
                                    positive=True) != start.col):
                    raise FrRuntimeError(
                        "project definition position does not match its location"
                    )
            position = f"{path}:{start.line}:{start.col}" if path is not None else None
            targets.append(AgentTarget(
                handle,
                optional_text(row, "name"),
                optional_text(row, "kind"),
                path,
                optional_text(row, "language"),
                position,
                location,
            ))
        return tuple(targets)

    def definition_target(self) -> AgentTarget:
        """Return the sole definition, refusing absent or ambiguous selections."""
        targets = self.definition_targets()
        if len(targets) != 1:
            raise FrRuntimeError(
                f"project definition query returned {len(targets)} definitions; expected exactly one"
            )
        return targets[0]


@dataclass(frozen=True)
class CompatibilityReport(FrReport):
    """An exact native/SDK version and agent-protocol agreement."""

    def __post_init__(self) -> None:
        binary = self._value.get("binary")
        python = self._value.get("python")
        protocol = self._value.get("protocol")
        if (self.schema != "fr-sdk-compatibility-1"
                or not isinstance(binary, Mapping)
                or binary.get("distribution") != "fun-refactor"
                or binary.get("version") != VERSION
                or not isinstance(python, Mapping)
                or python.get("distribution") != "fun-refactor-ir"
                or python.get("version_requirement") != f"=={VERSION}"
                or not isinstance(protocol, Mapping)
                or protocol.get("revision") != _PROTOCOL_REVISION
                or protocol.get("request_schemas") != list(_REQUEST_SCHEMAS)
                or protocol.get("response_schemas") != list(_RESPONSE_SCHEMAS)):
            raise FrRuntimeError(
                f"fr binary is incompatible with fun-refactor-ir {VERSION}"
            )

    @property
    def version(self) -> str:
        """Return the exact shared native and SDK release version."""
        return VERSION


@dataclass(frozen=True)
class DisclosureAction:
    """An exact, server-issued progressive-disclosure continuation."""

    arguments: tuple[str, ...]
    domain: str | None = None
    address: str | None = None
    object_digest: str | None = None
    kind: str = "reveal"
    reason: str | None = None
    _session: tuple[str, str, str, str, str, str] | None = field(
        default=None, repr=False, compare=False,
    )

    def __post_init__(self) -> None:
        arguments = self.arguments
        if (self.kind not in ("reveal", "continuation")
                or not isinstance(arguments, tuple) or not 3 <= len(arguments) <= 32
                or any(not isinstance(item, str) or "\0" in item for item in arguments)
                or arguments[:2] != ("project", "disclose")
                or "--reveal" not in arguments):
            raise FrRuntimeError("disclosure action is not an exact project disclose continuation")

    @classmethod
    def from_data(
        cls,
        value: Any,
        metadata: Mapping[str, Any] | None = None,
        *,
        session: tuple[str, str, str, str, str, str] | None = None,
    ) -> DisclosureAction:
        if not isinstance(value, Mapping) or set(value) != {"arguments"}:
            raise FrRuntimeError("disclosure action must contain only an arguments array")
        arguments = value["arguments"]
        if not isinstance(arguments, list):
            raise FrRuntimeError("disclosure action is not an exact project disclose continuation")
        meta = metadata or {}
        return cls(
            tuple(arguments),
            meta.get("domain") if isinstance(meta.get("domain"), str) else None,
            meta.get("address") if isinstance(meta.get("address"), str) else None,
            meta.get("object_digest") if isinstance(meta.get("object_digest"), str) else None,
            _session=session,
        )

    @classmethod
    def from_continuation(
        cls,
        value: Any,
        metadata: Mapping[str, Any] | None = None,
        *,
        session: tuple[str, str, str, str, str, str] | None = None,
    ) -> DisclosureAction:
        """Read one exact page continuation returned by ``fr``."""
        if (not isinstance(value, Mapping) or not set(value) <= {"arguments", "reason"}
                or "arguments" not in value or not isinstance(value["arguments"], list)
                or ("reason" in value and not isinstance(value["reason"], str))):
            raise FrRuntimeError("disclosure continuation has an unsupported shape")
        meta = metadata or {}
        return cls(
            tuple(value["arguments"]),
            meta.get("domain") if isinstance(meta.get("domain"), str) else None,
            meta.get("address") if isinstance(meta.get("address"), str) else None,
            meta.get("object_digest") if isinstance(meta.get("object_digest"), str) else None,
            "continuation",
            value.get("reason"),
            session,
        )


@dataclass(frozen=True)
class Disclosure(FrReport):
    """A budget-checked progressive-disclosure response."""

    def __post_init__(self) -> None:
        if self.schema != "fr-progressive-disclosure-1":
            raise FrRuntimeError("response is not a progressive disclosure report")
        budget = self._value.get("token_budget")
        if not isinstance(budget, Mapping):
            raise FrRuntimeError("disclosure response has no token budget")
        limit, used = budget.get("limit"), budget.get("used_upper_bound")
        if (isinstance(limit, bool) or not isinstance(limit, int)
                or isinstance(used, bool) or not isinstance(used, int)
                or limit < 0 or not 0 <= used <= limit):
            raise FrRuntimeError("disclosure response exceeds or misstates its token budget")

    def actions(self, *, domain: str | None = None) -> tuple[DisclosureAction, ...]:
        """Return deduplicated exact continuations, optionally filtered by domain."""
        found: list[DisclosureAction] = []
        seen: set[tuple[str, ...]] = set()
        session = self._session_identity(required=False)

        continuation = self._value.get("continuation")
        revealed = self._value.get("revealed")
        if isinstance(continuation, Mapping):
            metadata = revealed if isinstance(revealed, Mapping) else None
            action = DisclosureAction.from_continuation(
                continuation, metadata, session=session,
            )
            if domain is None or action.domain == domain:
                seen.add(action.arguments)
                found.append(action)

        def visit(node: Any) -> None:
            if isinstance(node, dict):
                reveal = node.get("reveal")
                if isinstance(reveal, Mapping):
                    action = DisclosureAction.from_data(reveal, node, session=session)
                    if action.arguments not in seen and (domain is None or action.domain == domain):
                        seen.add(action.arguments)
                        found.append(action)
                for child in node.values():
                    visit(child)
            elif isinstance(node, list):
                for child in node:
                    visit(child)

        visit(self._value)
        return tuple(found)

    @property
    def target(self) -> AgentTarget:
        """Return the typed target bound to this disclosure snapshot."""
        return AgentTarget.from_data(self._value.get("target"))

    @property
    def source_fragment(self) -> SourceFragment | None:
        """Return the exact source fragment revealed by this response, when present."""
        revealed = self._value.get("revealed")
        if not isinstance(revealed, Mapping) or revealed.get("domain") != "exact-source":
            return None
        return SourceFragment.from_data(revealed)

    def _session_identity(
        self, *, required: bool,
    ) -> tuple[str, str, str, str, str, str] | None:
        commitment = self._value.get("commitment")
        target = self._value.get("target")
        values = (
            self._value.get("revision"),
            self._value.get("view_basis"),
            commitment.get("object_root") if isinstance(commitment, Mapping) else None,
            target.get("handle") if isinstance(target, Mapping) else None,
            self._value.get("view"),
            self._value.get("profile"),
        )
        if all(isinstance(value, str) and value for value in values):
            return cast(tuple[str, str, str, str, str, str], values)
        if required:
            raise FrRuntimeError("disclosure report has no complete session identity")
        return None

    def session_identity(self) -> tuple[str, str, str, str, str, str]:
        """Return the immutable revision, view and object identity for this session."""
        identity = self._session_identity(required=True)
        assert identity is not None
        return identity


def _session_step(
    state: int,
    action: int,
    preview_valid: bool,
    manifest_matches: bool,
    basis_matches: bool,
) -> int:
    """Mirror the finite Rust/Lean task-session admission kernel."""
    if state == 0 and action == 0 and preview_valid:
        return 1
    if state == 1 and action == 1 and preview_valid and manifest_matches and basis_matches:
        return 2
    return 3


@dataclass(frozen=True)
class TaskReview(FrReport):
    """An immutable task-change preview bound to its exact canonical manifest."""

    manifest: bytes = field(repr=False)
    manifest_sha256: str
    task_change_basis: str
    preview_sha256: str = field(init=False)

    def __post_init__(self) -> None:
        actual = hashlib.sha256(self.manifest).hexdigest()
        preview_valid = (
            self.schema == "fr-task-change-1"
            and self._value.get("ready") is True
            and self._value.get("executed") is False
            and self._value.get("manifest_sha256") == actual == self.manifest_sha256
            and self._value.get("task_change_basis") == self.task_change_basis
            and bool(_BASIS.fullmatch(self.task_change_basis))
        )
        if _session_step(0, 0, preview_valid, True, True) != 1:
            raise FrRuntimeError("task-change preview is incomplete or does not bind its manifest")
        object.__setattr__(self, "preview_sha256", hashlib.sha256(
            json.dumps(self._value, ensure_ascii=False, sort_keys=True,
                       separators=(",", ":"), allow_nan=False).encode("utf-8")
        ).hexdigest())


@dataclass(frozen=True)
class TaskResult(FrReport):
    """The checked result of one exact reviewed task-change execution."""

    task_change_basis: str

    def __post_init__(self) -> None:
        if (self.schema != "fr-task-change-1" or self._value.get("executed") is not True
                or self._value.get("task_change_basis") != self.task_change_basis
                or not _BASIS.fullmatch(self.task_change_basis)):
            raise FrRuntimeError("task-change result does not match the reviewed execution")

    @property
    def passed(self) -> bool:
        return self._value.get("passed") is True


class FrClient:
    """A bounded, structured client for one local ``fr`` project root."""

    def __init__(
        self,
        root: str | os.PathLike[str],
        *,
        executable: str | os.PathLike[str] = "fr",
        timeout: float = 120.0,
        max_output_bytes: int = 1_048_576,
    ) -> None:
        root_path = Path(root)
        if not root_path.is_dir():
            raise FrRuntimeError("fr client root must be an existing directory")
        if (not isinstance(timeout, (int, float)) or isinstance(timeout, bool) or timeout <= 0):
            raise FrRuntimeError("fr client timeout must be positive")
        if (not isinstance(max_output_bytes, int) or isinstance(max_output_bytes, bool)
                or not 1_024 <= max_output_bytes <= 16_777_216):
            raise FrRuntimeError("fr client output budget must be 1024 through 16777216 bytes")
        self.root = root_path.resolve()
        self.executable = os.fspath(executable)
        self.timeout = float(timeout)
        self.max_output_bytes = max_output_bytes

    @staticmethod
    def _arguments(arguments: Sequence[str]) -> tuple[str, ...]:
        result = tuple(arguments)
        if (not 1 <= len(result) <= _MAX_ARGUMENTS
                or any(not isinstance(item, str) or "\0" in item for item in result)
                or sum(len(item.encode("utf-8")) for item in result) > _MAX_ARGUMENT_BYTES):
            raise FrRuntimeError("fr call exceeds the bounded string-argument contract")
        return result

    def call(self, *arguments: str, input_bytes: bytes | None = None) -> FrReport:
        """Run one JSON-mode command and return its parsed object."""
        args = self._arguments(arguments)
        if input_bytes is not None and len(input_bytes) > 65_536:
            raise FrRuntimeError("fr standard-input payload exceeds 64 KiB", arguments=args)
        command = [self.executable, "--json", "-C", os.fspath(self.root), *args]
        try:
            completed = subprocess.run(
                command,
                input=input_bytes,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                timeout=self.timeout,
                check=False,
            )
        except (OSError, subprocess.TimeoutExpired) as error:
            raise FrRuntimeError(f"fr command could not complete: {error}", arguments=args) from error
        if (len(completed.stdout) > self.max_output_bytes
                or len(completed.stderr) > self.max_output_bytes):
            raise FrRuntimeError("fr command exceeded the client output budget", arguments=args,
                                 exit_code=completed.returncode)
        report: Mapping[str, Any] | None = None
        try:
            decoded = json.loads(completed.stdout)
            if isinstance(decoded, dict):
                report = decoded
        except (UnicodeDecodeError, json.JSONDecodeError):
            pass
        if completed.returncode != 0:
            detail = completed.stderr.decode("utf-8", "replace").strip()
            if report is not None:
                candidate = report.get("error") or report.get("message")
                if isinstance(candidate, str):
                    detail = candidate
            raise FrRuntimeError(detail or "fr command failed", arguments=args,
                                 exit_code=completed.returncode, report=report)
        if report is None:
            raise FrRuntimeError("fr command did not return one JSON object", arguments=args,
                                 exit_code=completed.returncode)
        return FrReport(_copy_json(report), args)

    def project(self, *arguments: str) -> FrReport:
        """Run one structured project query."""
        return self.call("project", *arguments)

    def compatibility(self) -> CompatibilityReport:
        """Fail unless this SDK exactly matches the native agent protocol."""
        report = self.call("compatibility")
        return CompatibilityReport(report._value, report.arguments)

    def kernel(self, request: KernelRequest, *, report_bytes: int = 65_536) -> KernelResult:
        from .formal_kernel import KernelResult
        data = request.to_data()
        result = self.call("spec", "kernel", "--from", "-", "--report-bytes", str(report_bytes),
                           input_bytes=json.dumps(data, ensure_ascii=False, separators=(",", ":")).encode("utf-8"))
        return KernelResult.from_data(result.to_data(), request)

    def guide(self, goal: AgentGoal) -> AgentGuide:
        """Choose a deterministic workflow without exposing intermediate project reports."""
        from .guide import guide_goal
        return guide_goal(self, goal)

    def compile_guided_intent(self, guide: AgentGuide, action: TaggedIntentAction,
                              *, store: ObjectStore | None = None) -> CompiledIntent:
        """Compile an authored intent using its retained, freshness-checked guide."""
        from .guide import compile_guided_intent
        return compile_guided_intent(self, guide, action, store=store)

    def review_guide(self, guide: AgentGuide, action: TaggedIntentAction,
                     *, store: ObjectStore | None = None) -> GuideReview:
        """Create one immutable native review for an admitted writable guide route."""
        from .guide import review_guide
        return review_guide(self, guide, action, store=store)

    def follow_guide(self, action: GuideAction, inputs=None) -> FrReport:
        """Run one author-bound read/preview action retained by an authoritative guide."""
        from .guide import follow_guide
        return follow_guide(self, action, inputs)

    def complete_guide(self, goal: AgentGoal, inputs=None) -> GuideRun:
        """Select and follow every read/preview action without exposing intermediate reports."""
        from .guide import complete_guide
        return complete_guide(self, goal, inputs)

    def execute_guide(self, review: GuideReview) -> IntentResult:
        """Execute one unchanged native review whose guide remains current."""
        from .guide import execute_guide
        return execute_guide(self, review)

    def disclose(
        self,
        handle: str,
        *,
        view: str = "semantic",
        profile: str = "compact",
        depth: int = 3,
        token_limit: int = 4_096,
        proofs: bool = False,
    ) -> Disclosure:
        """Start a bounded progressive-disclosure session."""
        if view not in ("semantic", "evidence", "project", "application") or profile not in ("compact", "expanded"):
            raise FrRuntimeError("disclosure view or profile is unsupported")
        if (isinstance(depth, bool) or not isinstance(depth, int) or not 0 <= depth <= 8
                or isinstance(token_limit, bool) or not isinstance(token_limit, int)
                or not 1_024 <= token_limit <= (4_096 if profile == "compact" else 16_384)):
            raise FrRuntimeError("disclosure depth or token limit is outside its profile")
        args = ["project", "disclose", handle, "--view", view, "--profile", profile,
                "--depth", str(depth), "--token-limit", str(token_limit)]
        if proofs:
            args.append("--proofs")
        report = self.call(*args)
        return Disclosure(report._value, report.arguments)

    def follow(self, action: DisclosureAction) -> Disclosure:
        """Follow one unchanged action returned by a disclosure report."""
        if not isinstance(action, DisclosureAction):
            raise FrRuntimeError("follow requires a DisclosureAction returned by a report")
        report = self.call(*action.arguments)
        disclosure = Disclosure(report._value, report.arguments)
        if action._session is not None and disclosure.session_identity() != action._session:
            raise FrRuntimeError("disclosure continuation crossed its bound session")
        return disclosure

    def context(
        self,
        handle: str,
        *,
        view: str = "semantic",
        profile: str = "compact",
        depth: int = 3,
        token_limit: int = 4_096,
        proofs: bool = False,
        store: ObjectStore | None = None,
    ) -> ContextSession:
        """Start one session-bound progressive context workspace."""
        from .context import ContextSession

        initial = self.disclose(
            handle, view=view, profile=profile, depth=depth,
            token_limit=token_limit, proofs=proofs,
        )
        return ContextSession(self, initial, store=store)

    def review(self, change: TaskChange) -> TaskReview:
        """Preview one typed task change and retain its exact canonical bytes."""
        from .ir import TaskChange

        if not isinstance(change, TaskChange):
            raise FrRuntimeError("review requires a TaskChange")
        manifest = change.to_json(indent=None).encode("utf-8")
        report = self.call("task-change", "--from", "-", input_bytes=manifest)
        digest = hashlib.sha256(manifest).hexdigest()
        basis = report._value.get("task_change_basis")
        if not isinstance(basis, str):
            raise FrRuntimeError("task-change preview has no reviewed basis")
        return TaskReview(report._value, report.arguments, manifest, digest, basis)

    def prepare(
        self,
        intent: AgentIntent,
        *,
        store: ObjectStore | None = None,
    ) -> PreparedIntent:
        """Compile one declarative intent into a bounded context packet."""
        from .intent import prepare_intent
        return prepare_intent(self, intent, store=store)

    def compile(
        self,
        intent: AgentIntent,
        *,
        store: ObjectStore | None = None,
    ) -> CompiledIntent:
        """Compile one intent natively in a single project snapshot."""
        from .intent import compile_intent
        return compile_intent(self, intent, store=store)

    def execute_intent(self, compiled: CompiledIntent) -> IntentResult:
        """Execute only the unchanged action retained by a compiled intent."""
        from .intent import execute_intent
        return execute_intent(self, compiled)

    def execute(self, review: TaskReview) -> TaskResult:
        """Execute only the unchanged manifest and basis held by one review."""
        if not isinstance(review, TaskReview):
            raise FrRuntimeError("execute requires a TaskReview")
        digest_matches = hashlib.sha256(review.manifest).hexdigest() == review.manifest_sha256
        basis_matches = review._value.get("task_change_basis") == review.task_change_basis
        current_preview = hashlib.sha256(
            json.dumps(review._value, ensure_ascii=False, sort_keys=True,
                       separators=(",", ":"), allow_nan=False).encode("utf-8")
        ).hexdigest()
        preview_valid = (
            review.schema == "fr-task-change-1"
            and review._value.get("ready") is True
            and review._value.get("executed") is False
            and current_preview == review.preview_sha256
        )
        if _session_step(1, 1, preview_valid, digest_matches, basis_matches) != 2:
            raise FrRuntimeError("review changed before task-change execution")
        report = self.call(
            "task-change", "--from", "-", "--write", "--basis", review.task_change_basis,
            input_bytes=review.manifest,
        )
        return TaskResult(report._value, report.arguments, review.task_change_basis)
