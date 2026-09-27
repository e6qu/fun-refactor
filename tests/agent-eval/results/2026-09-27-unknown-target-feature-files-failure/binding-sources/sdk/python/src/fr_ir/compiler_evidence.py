"""Bound compiler observations without conflating compilation and syntax acceptance."""
from __future__ import annotations

from dataclasses import dataclass, replace
import hashlib
import json
from pathlib import Path
import re
import tempfile
from typing import Any, Mapping

from .context import ObjectStore, restore_stored_value, store_merkle_value
from .investigation import Evidence, EvidenceKind, TaskPlan, ResumedPlan
from .investigation_checks import attach_checks
from .runtime import FrClient, FrReport, FrRuntimeError, Occurrence


def _digest(value: Any, *, canonical: bool = True) -> str:
    return hashlib.sha256(json.dumps(value, sort_keys=canonical, ensure_ascii=False, separators=(",", ":")).encode()).hexdigest()


def _hash(value: Any, prefix: str = "") -> bool:
    return isinstance(value, str) and re.fullmatch(re.escape(prefix) + r"[0-9a-f]{64}", value) is not None


@dataclass(frozen=True)
class CompilerSpan:
    status: str
    primary: bool | None
    syntax: str
    occurrence: Occurrence | None
    source: Mapping[str, Any] | None

    def reveal(self, client: FrClient) -> FrReport:
        if self.occurrence is None or self.source is None:
            raise FrRuntimeError("compiler span has no source action")
        arguments = self.source.get("arguments")
        if not isinstance(arguments, list) or arguments[:2] != ["project", "show"] or any(not isinstance(x, str) for x in arguments):
            raise FrRuntimeError("expected a read-only source action")
        result = client.call(*arguments)
        if result.at("/revision") != self.occurrence.revision:
            raise FrRuntimeError("compiler source action crossed a revision")
        return result


@dataclass(frozen=True)
class CompilerDiagnostic:
    id: str
    index: int
    parent: int | None
    level: str
    code: str | None
    message: str
    spans: tuple[CompilerSpan, ...]


@dataclass(frozen=True)
class CompilerCollection:
    items: tuple[CompilerDiagnostic, ...]
    complete: bool
    disclosure_complete: bool
    pages: int
    continuation: Mapping[str, Any] | None


@dataclass(frozen=True)
class CompilerEvidence:
    report: FrReport
    items: tuple[CompilerDiagnostic, ...]
    checks: FrReport | None = None

    @property
    def complete(self) -> bool:
        return self.report.at("/complete")

    @classmethod
    def inspect(cls, client: FrClient, checks: FrReport, *, check: str, format: str = "rustc-json",
                limit: int = 8, max_bytes: int = 32768, cursor: str | None = None) -> CompilerEvidence:
        if checks.schema != "fr-checks-1" or format not in {"rustc-json", "cargo-json"}:
            raise FrRuntimeError("compiler evidence needs retained checks and an admitted format")
        value = checks.to_data()
        with tempfile.TemporaryDirectory(prefix="fr-compiler-evidence-") as directory:
            source = Path(directory) / "checks.json"
            source.write_text(json.dumps(value, ensure_ascii=False), encoding="utf-8")
            args = ["compiler-evidence", "--from", str(source), "--digest", _digest(value), "--check", check,
                    "--format", format, "--limit", str(limit), "--bytes", str(max_bytes)]
            if cursor is not None:
                args.extend(["--cursor", cursor])
            report = client.project(*args)
        return cls.from_report(report, checks=checks)

    @classmethod
    def from_report(cls, report: FrReport, *, checks: FrReport | None = None) -> CompilerEvidence:
        try:
            data = report.to_data()
            inputs, basis, revision = data["inputs"], data["basis"], data["revision"]
            if (report.schema != "fr-compiler-evidence-1" or data["mutation_authority"] is not False
                    or not _hash(revision) or basis != "frcpe1:" + _digest(inputs)
                    or inputs["revision"] != revision or inputs["analyzer"] != data["analyzer"]
                    or inputs["toolchain"] != _digest(data["scope"]["toolchain"])
                    or inputs["check"] != data["scope"]["check"] or inputs["format"] != data["scope"]["format"]
                    or inputs["format"] not in {"rustc-json", "cargo-json"}
                    or any(not _hash(inputs[key]) for key in ["configuration", "source_revision", "report", "analyzer"])):
                raise FrRuntimeError("compiler evidence has inconsistent input identities")
            if checks is not None and (checks.schema != "fr-checks-1" or inputs["report"] != _digest(checks.to_data())):
                raise FrRuntimeError("compiler evidence belongs to another retained check report")
            if checks is not None:
                retained = checks.to_data()
                selected = [row for row in retained["results"] if row["name"] == inputs["check"]]
                if (len(selected) != 1 or inputs["configuration"] != retained["basis"]
                        or inputs["source_revision"] != retained["source_revision"]
                        or data["scope"]["toolchain"] != retained["toolchain"]
                        or any(data["scope"][key] != selected[0][key] for key in ["argv", "cwd", "covers"])
                        or any(data["outcome"][key] != selected[0][key] for key in ["passed", "exit_code"])):
                    raise FrRuntimeError("compiler scope or outcome differs from retained execution")
            items = []
            for row in data["items"]:
                core = row["core"]
                index, parent = core["index"], core["parent"]
                if (row["basis"] != basis or row["id"] != "frcd1:" + _digest([basis, core])
                        or core["rule"] != "rust-compiler-evidence-1:diagnostic"
                        or core["confidence"] != "reported-by-declared-check"
                        or type(index) is not int or index < 0
                        or parent is not None and (type(parent) is not int or not 0 <= parent < index)
                        or not isinstance(core["message"], str) or len(core["message"]) > 512
                        or core["code"] is not None and not isinstance(core["code"], str)
                        or not isinstance(core["level"], str) or not isinstance(core["spans"], list) or len(core["spans"]) > 16):
                    raise FrRuntimeError("compiler diagnostic has inconsistent identity or parent")
                spans = []
                for span in core["spans"]:
                    point = None if span["occurrence"] is None else Occurrence.from_data(span["occurrence"])
                    if (span["status"] not in {"exact", "invalid", "unavailable", "expanded"}
                            or (span["status"] == "exact") != (point is not None)
                            or span["syntax"] not in {"accepted", "rejected", "unknown"}
                            or point is None and span["syntax"] != "unknown"
                            or span["primary"] is not None and type(span["primary"]) is not bool):
                        raise FrRuntimeError("compiler span has inconsistent mapping coverage")
                    if point is not None and (point.revision != revision or point.role != (
                            "compiler-primary" if span["primary"] else "compiler-secondary") or point.id != "fro1:" + _digest([
                                revision, point.path, {"start":point.location.span.start,"end":point.location.span.end},point.role], canonical=False)):
                        raise FrRuntimeError("compiler occurrence has inconsistent identity")
                    spans.append(CompilerSpan(span["status"], span["primary"], span["syntax"], point, span["source"]))
                items.append(CompilerDiagnostic(row["id"], index, parent, core["level"], core["code"], core["message"], tuple(spans)))
            page, capture, outcome = data["page"], data["capture"], data["outcome"]
            if (any(type(page[key]) is not int or page[key] < 0 for key in ["before", "returned", "remaining", "total"])
                    or page["returned"] != len(items) or page["total"] != page["before"] + len(items) + page["remaining"]
                    or (page["next"] is None) != (page["remaining"] == 0)
                    or (data["continuation"] is None) != (page["next"] is None)
                    or capture["diagnostics"] != page["total"]
                    or any(item.index != page["before"] + i for i, item in enumerate(items))):
                raise FrRuntimeError("compiler diagnostic page has inconsistent coverage")
            disclosed = page["before"] == 0 and page["remaining"] == 0
            if (any(type(value) is not bool for value in [capture["complete"], outcome["execution_complete"], outcome["passed"], data["complete"],data["disclosure_complete"]])
                    or capture["complete"] != (capture["cutoffs"] == [])
                    or data["disclosure_complete"] != disclosed
                    or data["complete"] != (capture["complete"] and outcome["execution_complete"] and disclosed)):
                raise FrRuntimeError("compiler completeness differs from capture or disclosure")
            expected = [item.id for item in items if item.level in {"error", "ice"} and any(span.syntax == "accepted" for span in item.spans)]
            if ([row["diagnostic"] for row in data["disagreements"]] != expected
                    or any(row["kind"] != "syntax-accepted-compiler-error" for row in data["disagreements"])):
                raise FrRuntimeError("compiler disagreement differs from syntax observations")
            return cls(report, tuple(items), checks)
        except (KeyError, TypeError, ValueError, AttributeError) as error:
            raise FrRuntimeError("malformed compiler evidence report") from error

    def _read(self, client: FrClient, cursor: str | None) -> CompilerEvidence:
        if self.checks is None:
            raise FrRuntimeError("retained checks are required to refresh compiler evidence")
        value = self.inspect(client, self.checks, check=self.report.at("/scope/check"), format=self.report.at("/scope/format"),
                             limit=self.report.at("/budget/limit"), max_bytes=self.report.at("/budget/response_bytes"), cursor=cursor)
        if value.report.at("/basis") != self.report.at("/basis"):
            raise FrRuntimeError("compiler evidence crossed an input identity")
        return value

    def next(self, client: FrClient) -> CompilerEvidence:
        cursor = self.report.at("/page/next")
        if cursor is None:
            raise FrRuntimeError("compiler evidence has no continuation")
        result = self._read(client, cursor)
        if result.report.at("/page/before") != self.report.at("/page/before") + len(self.items):
            raise FrRuntimeError("compiler continuation skipped diagnostics")
        return result

    def collect(self, client: FrClient, *, max_pages: int = 64) -> CompilerCollection:
        if type(max_pages) is not int or not 1 <= max_pages <= 64:
            raise FrRuntimeError("compiler collection needs a page budget between 1 and 64")
        current, items, pages = self, list(self.items), 1
        while current.report.at("/continuation") is not None and pages < max_pages:
            current = current.next(client)
            items.extend(current.items)
            pages += 1
        disclosed = self.report.at("/page/before") == 0 and current.report.at("/continuation") is None
        return CompilerCollection(tuple(items), disclosed and self.report.at("/capture/complete") and self.report.at("/outcome/execution_complete"),
                                  disclosed, pages, current.report.at("/continuation"))

    def persist(self, store: ObjectStore) -> str:
        return store_merkle_value(store, {"report":self.report.to_data(), "checks":None if self.checks is None else self.checks.to_data()}).digest

    @classmethod
    def restore(cls, store: ObjectStore, digest: str) -> CompilerEvidence:
        value = restore_stored_value(store, digest)
        return cls.from_report(FrReport(value["report"], ()), checks=None if value["checks"] is None else FrReport(value["checks"], ()))

    def attach(self, plan: TaskPlan, client: FrClient, step: str) -> ResumedPlan:
        fresh = self._read(client, None)
        if self.checks is None:
            raise FrRuntimeError("compiler attachment requires retained checks")
        resumed = attach_checks(plan, client, step, self.checks)
        observation = Evidence("compiler:" + self.report.at("/scope/check"), EvidenceKind.OBSERVATION,
            resumed.input_digests[step], fresh.complete,
            "fr-compiler-evidence-1:" + _digest(self.report.to_data()))
        steps = tuple(replace(item, evidence=tuple(e for e in item.evidence if e.id != observation.id) + (observation,))
                      if item.id == step else item for item in resumed.plan.steps)
        return replace(resumed.plan, steps=steps).resume(client)
