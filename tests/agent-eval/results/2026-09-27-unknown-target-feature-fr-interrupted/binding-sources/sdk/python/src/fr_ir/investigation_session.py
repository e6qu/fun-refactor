"""Resume plans with durable targets and explicitly replace stale target inputs."""
from __future__ import annotations

from dataclasses import dataclass, replace
from typing import Any, Mapping

from .context import ObjectStore, restore_stored_value, store_merkle_value
from .correspondence import CorrespondenceReport, DeclarationSnapshot
from .investigation import Dependency, DependencyKind, ResumedPlan, StepState, TaskPlan
from .runtime import FrClient, FrRuntimeError


def target_inputs(*source_paths: str) -> tuple[Dependency, ...]:
    """Declare workspace and correspondence rules plus any explicit source dependencies."""
    return (Dependency(DependencyKind.WORKSPACE, "selected-project"),
            Dependency(DependencyKind.DECLARATION_ANALYZER, "declaration-correspondence"),
            *(Dependency(DependencyKind.SOURCE, path) for path in source_paths))


@dataclass(frozen=True)
class InvestigationSession:
    plan: TaskPlan
    targets: Mapping[str, DeclarationSnapshot]

    @classmethod
    def bind(cls, plan: TaskPlan, targets: Mapping[str, DeclarationSnapshot]) -> InvestigationSession:
        """Bind explicitly selected declarations to already captured workspace dependencies."""
        steps = {step.id: step for step in plan.steps}
        if not targets or not set(targets) <= set(steps) or len(targets) > 64:
            raise FrRuntimeError("session needs 1..64 known target steps")
        validated = {}
        seen = set()
        for id, snapshot in targets.items():
            snapshot = DeclarationSnapshot.from_data(snapshot.to_data())
            if not snapshot.complete or not snapshot.items:
                raise FrRuntimeError("target steps require complete explicit declaration selections")
            workspace = [d for d in steps[id].inputs if d.kind == DependencyKind.WORKSPACE]
            analyzer = [d for d in steps[id].inputs if d.kind == DependencyKind.DECLARATION_ANALYZER]
            if (len(workspace) != 1 or workspace[0].digest != snapshot.revision
                    or len(analyzer) != 1 or analyzer[0].digest != snapshot.analyzer):
                raise FrRuntimeError("target step must bind the snapshot workspace and analyzer")
            for item in snapshot.items:
                if item.handle in seen:
                    raise FrRuntimeError("session target steps must select distinct declarations")
                seen.add(item.handle)
            validated[id] = snapshot
        if len(seen) > 1000:
            raise FrRuntimeError("session exceeds 1000 target declarations")
        return cls(plan, validated)

    def to_data(self) -> dict[str, Any]:
        return {"schema": "fr-investigation-session-1", "plan": self.plan.to_data(),
                "targets": {id: snapshot.to_data() for id, snapshot in self.targets.items()}}

    def persist(self, store: ObjectStore) -> str:
        value = InvestigationSession.bind(self.plan, self.targets).to_data()
        return store_merkle_value(store, value).digest

    @classmethod
    def restore(cls, store: ObjectStore, digest: str) -> InvestigationSession:
        value = restore_stored_value(store, digest)
        if (not isinstance(value, dict) or set(value) != {"schema", "plan", "targets"}
                or value["schema"] != "fr-investigation-session-1" or not isinstance(value["targets"], dict)):
            raise FrRuntimeError("unsupported investigation session")
        return cls.bind(TaskPlan.from_data(value["plan"]),
                        {id: DeclarationSnapshot.from_data(v) for id, v in value["targets"].items()})

    def resume(self, client: FrClient, *, limit: int = 16, candidates: int = 16,
               max_bytes: int = 32768, max_pages: int = 64) -> ResumedInvestigation:
        session = InvestigationSession.bind(self.plan, self.targets)
        resumed = session.plan.resume(client)
        groups: dict[tuple[str, str], list[str]] = {}
        for id, snapshot in session.targets.items():
            groups.setdefault((snapshot.revision, snapshot.analyzer), []).append(id)
        matches = {}
        for (revision, analyzer), ids in groups.items():
            items = tuple(item for id in ids for item in session.targets[id].items)
            snapshot = DeclarationSnapshot(revision, "explicit-declarations", analyzer, items, True)
            report = snapshot.compare(client, limit=limit, candidates=candidates, max_bytes=max_bytes, max_pages=max_pages)
            if report.revision != resumed.report.at("/revision"):
                raise FrRuntimeError("workspace changed during investigation resumption")
            matches.update({id: report for id in ids})
        return ResumedInvestigation(InvestigationSession.bind(resumed.plan, session.targets), resumed, matches)


@dataclass(frozen=True)
class ResumedInvestigation:
    session: InvestigationSession
    resumed: ResumedPlan
    correspondence: Mapping[str, CorrespondenceReport]

    def refresh(self, client: FrClient, step_id: str, choices: Mapping[str, str], *,
                inputs: tuple[Dependency, ...]) -> InvestigationSession:
        """Replace one step's inputs; clear evidence and actions before native resumption."""
        if step_id not in self.session.targets:
            raise FrRuntimeError("unknown investigation target step")
        old = self.session.targets[step_id]
        if set(choices) != {item.handle for item in old.items}:
            raise FrRuntimeError("refresh needs a choice for every target of the step")
        workspace = [d for d in inputs if d.kind == DependencyKind.WORKSPACE]
        analyzer = [d for d in inputs if d.kind == DependencyKind.DECLARATION_ANALYZER]
        if len(workspace) != 1 or len(analyzer) != 1 or any(d.digest is not None for d in inputs):
            raise FrRuntimeError("refresh requires explicit uncaptured workspace and analyzer inputs")
        selected = self.correspondence[step_id].select(client, choices)
        steps = tuple(replace(step, inputs=inputs, state=StepState.PENDING, evidence=(), action=(), action_input=None)
                      if step.id == step_id else step for step in self.session.plan.steps)
        refreshed = replace(self.session.plan, steps=steps).resume(client)
        if refreshed.report.at("/revision") != selected.revision:
            raise FrRuntimeError("workspace changed during target refresh")
        targets = dict(self.session.targets)
        targets[step_id] = selected
        return InvestigationSession.bind(refreshed.plan, targets)
