"""Paged, revision-bound semantic provenance with explicit missing origins."""
from __future__ import annotations

from dataclasses import dataclass
import hashlib
import json
import re
from typing import Any

from .runtime import FrClient, FrReport, FrRuntimeError, Occurrence


def _digest(value: Any) -> str:
    return hashlib.sha256(json.dumps(value, sort_keys=True, ensure_ascii=False,
                                     separators=(",", ":")).encode()).hexdigest()


@dataclass(frozen=True)
class SourceOrigins:
    status: str
    occurrences: tuple[Occurrence, ...]
    reason: str | None = None

    @classmethod
    def from_data(cls, value: Any, *, revision: str) -> SourceOrigins:
        try:
            status = value["status"]
            if status == "exact" and set(value) == {"status", "occurrence"}:
                occurrences = (Occurrence.from_data(value["occurrence"]),)
                reason = None
            elif status == "multiple" and set(value) == {"status", "occurrences"}:
                occurrences = tuple(Occurrence.from_data(o) for o in value["occurrences"])
                reason = None
                if len(occurrences) < 2 or len({o.id for o in occurrences}) != len(occurrences):
                    raise FrRuntimeError("multiple origins require distinct occurrences")
            elif status == "absent" and set(value) == {"status", "reason"}:
                occurrences = ()
                reason = value["reason"]
                if not isinstance(reason, str) or not reason:
                    raise FrRuntimeError("absent origin needs a reason")
            else:
                raise FrRuntimeError("unsupported source-origin shape")
            if any(o.revision != revision for o in occurrences):
                raise FrRuntimeError("source origin belongs to another revision")
            return cls(status, occurrences, reason)
        except (KeyError, TypeError, ValueError) as error:
            raise FrRuntimeError("malformed source origins") from error


@dataclass(frozen=True)
class SemanticOrigin:
    id: str
    pointer: str
    body_pointer: str | None
    kind: str
    origins: SourceOrigins
    rule: str


@dataclass(frozen=True)
class SemanticOrigins:
    report: FrReport
    items: tuple[SemanticOrigin, ...]
    revision: str
    basis: str
    complete: bool
    next_cursor: str | None

    @classmethod
    def from_report(cls, report: FrReport) -> SemanticOrigins:
        try:
            data = report.to_data()
            value = data["origins"]
            basis = data["semantic_basis"]
            revision = value["revision"]
            if (value["schema"] != "fr-semantic-origins-1" or value["semantic_basis"] != basis
                    or data.get("revision", revision) != revision or type(value["complete"]) is not bool
                    or value["mutation_authority"] is not False
                    or not isinstance(revision, str) or re.fullmatch(r"[0-9a-f]{64}", revision) is None
                    or not isinstance(basis, str) or re.fullmatch(r"frsm1:[0-9a-f]{64}", basis) is None):
                raise FrRuntimeError("semantic origins have inconsistent identity")
            items = []
            seen = set()
            for row in value["items"]:
                pointer = row["pointer"]
                if (not isinstance(pointer, str) or not pointer.startswith("/model/") or pointer in seen
                        or row["id"] != "frso1:" + _digest([basis, pointer])
                        or report.at(pointer + "/kind") != row["kind"]):
                    raise FrRuntimeError("semantic origin does not identify a returned node")
                seen.add(pointer)
                prefix = "/model/items/0/value/body"
                body_available = data.get("body_identity", {}).get("status") == "available"
                body_pointer = "/body" + pointer[len(prefix):] if body_available and pointer.startswith(prefix + "/") else None
                if row["body_pointer"] != body_pointer or not isinstance(row["rule"], str):
                    raise FrRuntimeError("semantic origin has an invalid body pointer or rule")
                items.append(SemanticOrigin(row["id"], pointer, body_pointer, row["kind"],
                                            SourceOrigins.from_data(row["origins"], revision=revision), row["rule"]))
            page = value["page"]
            if (any(type(page[key]) is not int or page[key] < 0 for key in ("before", "returned", "remaining", "total"))
                    or page["returned"] != len(items)
                    or page["total"] != page["before"] + len(items) + page["remaining"]
                    or (page["next"] is None) != (page["remaining"] == 0)
                    or value["complete"] != (data["status"] == "returned" and page["before"] == 0 and page["remaining"] == 0)):
                raise FrRuntimeError("semantic origin page has inconsistent coverage")
            return cls(report, tuple(items), revision, basis, value["complete"], page["next"])
        except (KeyError, TypeError, ValueError) as error:
            raise FrRuntimeError("malformed semantic origins") from error

    @classmethod
    def inspect(cls, client: FrClient, target: str, *, nodes: int = 256, limit: int = 40,
                pointer: str | None = None, cursor: str | None = None) -> SemanticOrigins:
        arguments = ["semantic", target, "--body", "--origins", "--nodes", str(nodes),
                     "--origin-limit", str(limit)]
        if pointer is not None:
            arguments.extend(["--origin-pointer", pointer])
        if cursor is not None:
            arguments.extend(["--origin-cursor", cursor])
        return cls.from_report(client.project(*arguments))

    def for_body_pointer(self, pointer: str) -> SemanticOrigin:
        matches = [item for item in self.items if item.body_pointer == pointer]
        if len(matches) != 1:
            raise FrRuntimeError("body pointer is absent from this origin page")
        return matches[0]

    def for_occurrence(self, occurrence: Occurrence) -> tuple[SemanticOrigin, ...]:
        if occurrence.revision != self.revision:
            raise FrRuntimeError("cannot link origins across revisions")
        return tuple(item for item in self.items if any(
            o.path == occurrence.path and o.location.span == occurrence.location.span
            for o in item.origins.occurrences
        ))
