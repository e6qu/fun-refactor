"""Paged model facts and exact syntax links with independent disclosure coverage."""
from __future__ import annotations

from dataclasses import dataclass
import hashlib
import json
from pathlib import Path
import re
from typing import Any, Mapping

from .context import ObjectStore, restore_stored_value, store_merkle_value
from .origins import SemanticOrigins
from .runtime import FrClient, FrReport, FrRuntimeError, Occurrence

KINDS = {"return", "raise", "witness", "completion", "summary-return", "summary-raise",
         "summary-sink", "summary-completion", "boundary"}


def _digest(value: Any, *, canonical: bool = True) -> str:
    return hashlib.sha256(json.dumps(value, sort_keys=canonical, ensure_ascii=False,
                                     separators=(",", ":")).encode()).hexdigest()


def _identity(value: Any, prefix: str = "") -> bool:
    return isinstance(value, str) and re.fullmatch(re.escape(prefix) + r"[0-9a-f]{64}", value) is not None


def _read(client: FrClient, action: Any, route: str) -> FrReport:
    if (not isinstance(action, Mapping) or not isinstance(action.get("arguments"), list)
            or action["arguments"][:2] != ["project", route]
            or any(not isinstance(arg, str) for arg in action["arguments"])):
        raise FrRuntimeError("expected a read-only flow fact action")
    return client.call(*action["arguments"])


@dataclass(frozen=True)
class FlowFact:
    id: str
    basis: str
    core: Mapping[str, Any]
    follow: Mapping[str, Any] | None

    @property
    def kind(self) -> str:
        return self.core["kind"]

    @classmethod
    def from_data(cls, row: Any, basis: str, inputs: str, analysis_complete: bool) -> FlowFact:
        core = row["core"]
        kind = core["kind"]
        if (row["basis"] != basis or row["id"] != "frff1:" + _digest([basis, core])
                or kind not in KINDS or core["input_digest"] != inputs
                or core["rule"] != f"python-scalar-facts-1:{kind}"
                or core["confidence"] != ("observed-cutoff" if kind == "boundary" else "model-derived")
                or core["scope"] != ("symbolic-function" if kind.startswith("summary-") else "selected-entry")
                or type(core["analysis_complete"]) is not bool or core["analysis_complete"] != analysis_complete
                or type(core["evidence_count"]) is not int or core["evidence_count"] < 0
                or not _identity(core["evidence_digest"]) or not isinstance(core["conclusion"], dict)
                or not isinstance(core["function"], str) or not core["function"]
                or core["assumptions_ref"] != "/analysis/assumptions" or core["omissions_ref"] != "/analysis/omitted"):
            raise FrRuntimeError("flow fact has inconsistent identity or scope")
        return cls(row["id"], basis, core, row.get("follow"))


@dataclass(frozen=True)
class FactPoint:
    index: int
    occurrence: Occurrence
    rule: Mapping[str, Any]
    mapping: Mapping[str, Any]
    raw_occurrence: Mapping[str, Any]

    def semantic(self, client: FrClient, index: int = 0) -> SemanticOrigins:
        try:
            link = self.mapping["items"][index]
        except (IndexError, KeyError) as error:
            raise FrRuntimeError("semantic mapping was not disclosed") from error
        report = SemanticOrigins.from_report(_read(client, link["follow"], "semantic"))
        if (report.revision != self.occurrence.revision or report.basis != link["semantic_basis"]
                or not any(item.id == link["id"] for item in report.for_occurrence(self.occurrence))):
            raise FrRuntimeError("semantic link no longer matches this occurrence")
        return report


@dataclass(frozen=True)
class FactCollection:
    facts: tuple[FlowFact, ...]
    evidence: tuple[FactPoint, ...]
    complete: bool
    disclosure_complete: bool
    pages: int
    continuation: Mapping[str, Any] | None


@dataclass(frozen=True)
class FlowFacts:
    report: FrReport
    items: tuple[FlowFact, ...]
    fact: FlowFact | None
    evidence: tuple[FactPoint, ...]

    @property
    def complete(self) -> bool:
        return self.report.at("/complete")

    @classmethod
    def inspect(cls, client: FrClient, target: str, *, rules: Path | None = None,
                context: str = "generic", steps: int = 1024, depth: int = 8, limit: int = 16,
                evidence_limit: int = 8, max_bytes: int = 65_536) -> FlowFacts:
        arguments = ["flow-facts", target, "--context", context, "--steps", str(steps),
                     "--depth", str(depth), "--limit", str(limit), "--evidence-limit", str(evidence_limit),
                     "--bytes", str(max_bytes)]
        if rules is not None:
            arguments.extend(["--rules", str(rules)])
        return cls.from_report(client.project(*arguments))

    @classmethod
    def from_report(cls, report: FrReport) -> FlowFacts:
        try:
            data = report.to_data()
            analysis, basis, revision = data["analysis"], data["basis"], data["revision"]
            if (report.schema != "fr-flow-facts-1" or data["mutation_authority"] is not False
                    or not _identity(basis, "frfb1:") or not _identity(revision)
                    or not _identity(analysis["input_digest"]) or type(analysis["complete"]) is not bool
                    or basis != "frfb1:" + _digest([revision, analysis["input_digest"], data["analyzer"]])
                    or analysis["input_digest"] != _digest(analysis["inputs"])
                    or not isinstance(analysis["cutoffs"], list) or not isinstance(analysis["omitted"], dict)
                    or analysis["complete"] and (analysis["cutoffs"] or analysis["omitted"])):
                raise FrRuntimeError("flow fact report has inconsistent analysis identity")
            parse = lambda row: FlowFact.from_data(row, basis, analysis["input_digest"], analysis["complete"])
            items = tuple(parse(row) for row in data["items"])
            fact = None if data["fact"] is None else parse(data["fact"])
            if len({item.id for item in items}) != len(items):
                raise FrRuntimeError("duplicate flow fact")
            evidence = []
            for row in data["evidence"]:
                raw = row["occurrence"]
                point = Occurrence.from_data(raw)
                if (point.revision != revision or point.id != "fro1:" + _digest([
                        revision, point.path, {"start":point.location.span.start,"end":point.location.span.end}, point.role], canonical=False)
                        or row["rule"]["id"] != f"python-scalar-facts-1:occurrence:{point.role}"):
                    raise FrRuntimeError("fact occurrence has inconsistent identity")
                mapping = row["mapping"]
                if (mapping["status"] not in {"mapped", "absent", "incomplete", "unavailable"}
                        or type(mapping["complete"]) is not bool or not isinstance(mapping["items"], list)
                        or len(mapping["items"]) > 4 or (mapping["status"] == "mapped") != bool(mapping["items"])
                        or mapping["status"] == "absent" and not mapping["complete"]
                        or mapping["status"] in {"incomplete", "unavailable"} and mapping["complete"]):
                    raise FrRuntimeError("semantic mapping has inconsistent coverage")
                for link in mapping["items"]:
                    pointer, semantic = link["pointer"], link["semantic_basis"]
                    if (not _identity(semantic, "frsm1:") or not isinstance(pointer, str)
                            or not pointer.startswith("/model/") or link["id"] != "frso1:" + _digest([semantic, pointer])
                            or link["relation"] not in {"exact", "member-of-multiple"}):
                        raise FrRuntimeError("semantic link has inconsistent identity")
                    prefix = "/model/items/0/value/body"
                    if link["body_pointer"] is not None and (
                            not _identity(link["body_basis"], "frsb1:") or not pointer.startswith(prefix + "/")
                            or link["body_pointer"] != "/body" + pointer[len(prefix):]):
                        raise FrRuntimeError("semantic link has an invalid authoring pointer")
                evidence.append(FactPoint(row["index"], point, row["rule"], mapping, raw))
            page = data["page"]
            if any(type(page[key]) is not int or page[key] < 0 for key in ["before", "returned", "remaining", "total"]):
                raise FrRuntimeError("invalid fact page counts")
            points = evidence if fact is not None else items
            if (page["returned"] != len(points) or page["total"] != page["before"] + len(points) + page["remaining"]
                    or (page["next"] is None) != (page["remaining"] == 0)
                    or (data["continuation"] is None) != (page["next"] is None)
                    or fact is not None and (items or page["total"] != fact.core["evidence_count"])
                    or fact is None and evidence):
                raise FrRuntimeError("inconsistent fact page coverage")
            if any(type(point.index) is not int or point.index != page["before"] + index for index, point in enumerate(evidence)):
                raise FrRuntimeError("fact evidence has a gap or reordered index")
            disclosed = page["before"] == 0 and page["remaining"] == 0
            if (type(data["complete"]) is not bool or type(data["disclosure"]["complete"]) is not bool
                    or data["disclosure"]["complete"] != disclosed or data["complete"] != (analysis["complete"] and disclosed)
                    or data["disclosure"]["kind"] != ("fact-evidence" if fact else "fact-catalogue")):
                raise FrRuntimeError("fact completeness differs from analysis or disclosure coverage")
            if fact is not None and disclosed and _digest([point.raw_occurrence for point in evidence]) != fact.core["evidence_digest"]:
                raise FrRuntimeError("fact evidence digest mismatch")
            return cls(report, items, fact, tuple(evidence))
        except (KeyError, TypeError, ValueError, AttributeError) as error:
            raise FrRuntimeError("malformed flow fact report") from error

    def _same(self, result: FlowFacts) -> None:
        for pointer in ["/basis", "/revision", "/target", "/analysis/input_digest"]:
            if result.report.at(pointer) != self.report.at(pointer):
                raise FrRuntimeError("fact follow crossed an analysis identity")

    def explain(self, client: FrClient, fact: FlowFact) -> FlowFacts:
        if fact.basis != self.report.at("/basis"):
            raise FrRuntimeError("fact belongs to another analysis")
        result = self.from_report(_read(client, fact.follow, "flow-facts"))
        self._same(result)
        if result.fact is None or result.fact.id != fact.id or result.report.at("/page/before") != 0:
            raise FrRuntimeError("fact follow returned a different explanation")
        return result

    def next(self, client: FrClient) -> FlowFacts:
        action = self.report.at("/continuation")
        if action is None:
            raise FrRuntimeError("fact page has no continuation")
        result = self.from_report(_read(client, action, "flow-facts"))
        self._same(result)
        if ((None if self.fact is None else self.fact.id) != (None if result.fact is None else result.fact.id)
                or result.report.at("/page/before") != self.report.at("/page/before") + self.report.at("/page/returned")):
            raise FrRuntimeError("fact continuation skipped or repeated evidence")
        return result

    def collect(self, client: FrClient, *, max_pages: int = 64) -> FactCollection:
        if type(max_pages) is not int or not 1 <= max_pages <= 64:
            raise FrRuntimeError("fact collection needs a page budget between 1 and 64")
        current, items, evidence, pages = self, list(self.items), list(self.evidence), 1
        while current.report.at("/continuation") is not None and pages < max_pages:
            current = current.next(client)
            items.extend(current.items)
            evidence.extend(current.evidence)
            pages += 1
        disclosed = self.report.at("/page/before") == 0 and current.report.at("/continuation") is None
        if len({item.id for item in items}) != len(items):
            raise FrRuntimeError("fact collection repeats identities")
        if self.fact is not None and disclosed and _digest([point.raw_occurrence for point in evidence]) != self.fact.core["evidence_digest"]:
            raise FrRuntimeError("collected fact evidence digest mismatch")
        return FactCollection(tuple(items), tuple(evidence), disclosed and self.report.at("/analysis/complete"),
                              disclosed, pages, current.report.at("/continuation"))

    def persist(self, store: ObjectStore) -> str:
        return store_merkle_value(store, self.report.to_data()).digest

    @classmethod
    def restore(cls, store: ObjectStore, digest: str) -> FlowFacts:
        return cls.from_report(FrReport(restore_stored_value(store, digest), ()))
