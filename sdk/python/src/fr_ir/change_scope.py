"""Retain indexed consumer discovery and bind its review to native task changes."""
from __future__ import annotations

from dataclasses import dataclass, replace
from typing import Any, Mapping, Sequence

from .context import ObjectStore, restore_stored_value, store_merkle_value
from .investigation import Dependency, DependencyKind, Evidence, EvidenceKind, ResumedPlan, StepState, TaskPlan
from .ir import TaskChange
from .runtime import FrClient, FrReport, FrRuntimeError, Occurrence


@dataclass(frozen=True)
class ChangeScope:
    report: FrReport

    def __post_init__(self) -> None:
        data = self.report.to_data()
        dependency = data.get("dependency", {})
        if (data.get("schema") != "fr-change-scope-1" or data.get("runtime_coverage") is not False
                or not isinstance(dependency, Mapping) or dependency.get("kind") != "change-scope"
                or dependency.get("digest") != data.get("input_digest")
                or not isinstance(dependency.get("key"), str)
                or not isinstance(data.get("revision"), str)
                or any(type(data.get(name)) is not bool for name in ("indexed_complete", "review_ready"))):
            raise FrRuntimeError("malformed change scope report")
        for name in ("targets", "consumers", "references", "unresolved", "test_candidates",
                     "check_candidates", "affected_paths", "unmapped_paths", "missing_mapped_paths", "cutoffs"):
            if not isinstance(data.get(name), list):
                raise FrRuntimeError("change scope needs bounded result arrays")
        for item in (*data["references"], *data["unresolved"]):
            occurrence = Occurrence.from_data(item["occurrence"])
            if occurrence.revision != data["revision"]:
                raise FrRuntimeError("change scope occurrence has another revision")
        if data["review_ready"] and (not data["indexed_complete"] or data["cutoffs"]
                or data["unresolved"] or data["unmapped_paths"] or data["missing_mapped_paths"]
                or data.get("omitted_records", 0) or not data["check_candidates"]):
            raise FrRuntimeError("incomplete change scope cannot be review ready")

    @classmethod
    def inspect(cls, client: FrClient, targets: Sequence[str], *, depth: int = 4,
                nodes: int = 128, references: int = 512, max_bytes: int = 65536) -> ChangeScope:
        if (not 1 <= len(targets) <= 16 or len(set(targets)) != len(targets)
                or any(not isinstance(t, str) or not t.startswith("frp1:") for t in targets)):
            raise FrRuntimeError("change scope needs 1..16 distinct declaration handles")
        return cls(client.project("change-scope", *targets, "--depth", str(depth),
                   "--nodes", str(nodes), "--references", str(references), "--bytes", str(max_bytes)))

    @property
    def ready(self) -> bool:
        return self.report.at("/review_ready") is True

    @property
    def dependency(self) -> Dependency:
        value = self.report.at("/dependency")
        return Dependency(DependencyKind.CHANGE_SCOPE, value["key"], value["digest"])

    @property
    def checks(self) -> tuple[str, ...]:
        return tuple(item["name"] for item in self.report.at("/check_candidates"))

    def bind(self, change: TaskChange) -> TaskChange:
        """Require native preflight to recheck this scope, target set and selected checks."""
        self.__post_init__()
        if not self.ready:
            raise FrRuntimeError("change scope has gaps; resolve them before scoped delivery")
        return replace(change, change_scope={"key": self.dependency.key, "digest": self.dependency.digest or ""})

    def observe(self, plan: TaskPlan, client: FrClient, step: str) -> ResumedPlan:
        """Attach discovery to an explicitly running step; never refresh stale prerequisites."""
        self.__post_init__()
        if not self.ready:
            raise FrRuntimeError("incomplete scope cannot satisfy a discovery step")
        original = [s for s in plan.steps if s.id == step]
        resumed = plan.resume(client)
        selected = [s for s in resumed.plan.steps if s.id == step]
        if (len(original) != 1 or original[0].state != StepState.RUNNING
                or len(selected) != 1 or selected[0].state != StepState.READY
                or self.dependency not in selected[0].inputs):
            raise FrRuntimeError("scope observation requires matching current inputs on a running step")
        evidence = Evidence("change-scope", EvidenceKind.OBSERVATION, resumed.input_digests[step],
                            True, "fr-change-scope:" + (self.dependency.digest or ""))
        steps = tuple(replace(s, state=StepState.RUNNING, evidence=tuple(e for e in s.evidence if e.id != evidence.id)+(evidence,))
                      if s.id == step else s for s in resumed.plan.steps)
        return replace(resumed.plan, steps=steps).resume(client, transition=f"{step}:satisfy")

    def persist(self, store: ObjectStore) -> str:
        self.__post_init__()
        return store_merkle_value(store, self.report.to_data()).digest

    @classmethod
    def restore(cls, store: ObjectStore, digest: str) -> ChangeScope:
        value: Any = restore_stored_value(store, digest)
        if not isinstance(value, Mapping):
            raise FrRuntimeError("stored change scope must be an object")
        return cls(FrReport(value, ()))
