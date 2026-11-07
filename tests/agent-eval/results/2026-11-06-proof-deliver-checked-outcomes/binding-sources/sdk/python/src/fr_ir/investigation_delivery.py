"""Retain reviewed delivery outcomes before attaching post-change check evidence."""
from __future__ import annotations

from dataclasses import dataclass
import hashlib
import json
from typing import Any, Mapping

from .context import ObjectStore, restore_stored_value, store_merkle_value
from .investigation import DependencyKind, ResumedPlan, StepState, TaskPlan
from .investigation_checks import attach_checks
from .runtime import FrClient, FrReport, FrRuntimeError, TaskResult, TaskReview


@dataclass(frozen=True)
class DeliveryReceipt:
    """Verified local records bind reviewed requirements and all delivery stages."""

    manifest_json: str
    result: TaskResult

    def __post_init__(self) -> None:
        if (not isinstance(self.manifest_json, str) or len(self.manifest_json.encode()) > 65536
                or self.result.at("/manifest_sha256") != hashlib.sha256(self.manifest_json.encode()).hexdigest()):
            raise FrRuntimeError("delivery receipt manifest identity differs")
        names = self.manifest.get("acceptance_checks")
        if (self.manifest.get("schema") != "fr-task-change-1" or not isinstance(names, list)
                or not 1 <= len(names) <= 32 or any(not isinstance(n, str) or not n for n in names)
                or len(set(names)) != len(names)):
            raise FrRuntimeError("delivery receipt requires named post-change acceptance checks")
        workflow = self.result.at("/workflow")
        if (not isinstance(workflow, Mapping) or workflow.get("schema") != "fr-workflow-1"
                or workflow.get("executed") is not True or type(workflow.get("passed")) is not bool
                or workflow["passed"] != self.result.passed):
            raise FrRuntimeError("delivery receipt has inconsistent execution outcomes")
        delivery = self.manifest["delivery"]
        expected = (["check-original"] if delivery.get("check-original", False) else [])
        expected += ["apply", "check-applied", "check-acceptance"]
        if delivery.get("exercise-reversal", False):
            expected += ["undo", "check-restored", "redo", "check-applied", "check-acceptance"]
        if delivery.get("patch") is not None:
            expected += ["deliver-patch"]
        stages = workflow.get("stages")
        if (not isinstance(stages, list) or len(stages) != len(expected)
                or any(not isinstance(s, Mapping) for s in stages)
                or [s.get("stage") for s in stages] != expected):
            raise FrRuntimeError("delivery receipt has incomplete stage coverage")
        statuses = [s.get("status") for s in stages]
        if self.result.passed:
            if statuses != ["passed"] * len(expected):
                raise FrRuntimeError("passing delivery contains an incomplete stage")
        else:
            if statuses.count("failed") != 1:
                raise FrRuntimeError("failed delivery needs one failed stage")
            failed = statuses.index("failed")
            if statuses != ["passed"] * failed + ["failed"] + ["pending"] * (len(expected)-failed-1):
                raise FrRuntimeError("failed delivery continued after failure")
        for stage in stages:
            if stage["stage"] != "check-acceptance" or stage["status"] != "passed":
                continue
            report = stage.get("result", {})
            rows = report.get("results", [])
            if (report.get("schema") != "fr-checks-1" or report.get("passed") is not True
                    or any(report.get(k) is not True for k in (
                        "executed", "configuration_stable", "source_snapshot_stable", "toolchain_stable"))
                    or len(rows) != len(names) or {r.get("name") for r in rows} != set(names)
                    or any(r.get("passed") is not True for r in rows)):
                raise FrRuntimeError("acceptance stage lacks complete stable passing checks")

    @property
    def manifest(self) -> Mapping[str, Any]:
        value = json.loads(self.manifest_json)
        if not isinstance(value, Mapping):
            raise FrRuntimeError("delivery manifest must be an object")
        return value

    @property
    def passed(self) -> bool:
        return self.result.passed

    @property
    def checks(self) -> FrReport | None:
        reports = [s["result"] for s in self.result.at("/workflow/stages")
                   if s["stage"] == "check-acceptance" and "result" in s]
        return FrReport(reports[-1], ()) if reports else None

    @classmethod
    def capture(cls, review: TaskReview, result: TaskResult) -> DeliveryReceipt:
        if (result.task_change_basis != review.task_change_basis
                or result.at("/manifest_sha256") != hashlib.sha256(review.manifest).hexdigest()):
            raise FrRuntimeError("delivery result differs from its reviewed manifest")
        return cls(review.manifest.decode(), result)

    def persist(self, store: ObjectStore) -> str:
        self.__post_init__()
        return store_merkle_value(store, {"schema":"fr-delivery-receipt-1", "manifest":self.manifest_json,
            "result":self.result.to_data(), "basis":self.result.task_change_basis}).digest

    @classmethod
    def restore(cls, store: ObjectStore, digest: str) -> DeliveryReceipt:
        value = restore_stored_value(store, digest)
        if not isinstance(value, Mapping) or set(value) != {"schema", "manifest", "result", "basis"} or value["schema"] != "fr-delivery-receipt-1":
            raise FrRuntimeError("stored object is not a delivery receipt")
        return cls(value["manifest"], TaskResult(value["result"], (), value["basis"]))


def _checked_step(plan: TaskPlan, step_id: str, names: list[str]) -> None:
    selected = [s for s in plan.steps if s.id == step_id]
    required = {DependencyKind.WORKSPACE, DependencyKind.CHECK_CONFIGURATION,
                DependencyKind.CHECK_SOURCES, DependencyKind.CHECK_TOOLCHAIN}
    if (len(selected) != 1 or set(selected[0].required_checks) != set(names)
            or selected[0].required_proofs or not required <= {d.kind for d in selected[0].inputs}):
        raise FrRuntimeError("delivery step needs exactly the acceptance checks and complete checked dependencies")


def attach_delivery(plan: TaskPlan, client: FrClient, step: str, receipt: DeliveryReceipt) -> ResumedPlan:
    """Attach to an explicitly refreshed running step; stale observations remain stale."""
    receipt.__post_init__()
    _checked_step(plan, step, list(receipt.manifest["acceptance_checks"]))
    if not receipt.passed or receipt.checks is None:
        raise FrRuntimeError("failed delivery cannot satisfy a task")
    return attach_checks(plan, client, step, receipt.checks, satisfy=True)


@dataclass(frozen=True)
class DeliveredPlanRun:
    resumed: ResumedPlan
    receipt: DeliveryReceipt
    receipt_root: str
    started_plan_root: str
    plan_root: str
    attachment_error: str | None

    @property
    def passed(self) -> bool:
        return self.receipt.passed and self.attachment_error is None


def run_delivery(plan: TaskPlan, client: FrClient, step: str, reviewed: TaskReview,
                 store: ObjectStore) -> DeliveredPlanRun:
    """Execute one review, retain success/failure, then capture explicit post-change inputs."""
    manifest = json.loads(reviewed.manifest)
    names = manifest.get("acceptance_checks", [])
    if not names:
        raise FrRuntimeError("checked delivery needs post-change acceptance checks")
    _checked_step(plan, step, names)
    started = plan.resume(client, transition=f"{step}:start")
    started_root = started.plan.store(store)
    try:
        result = client.execute(reviewed)
    except FrRuntimeError as error:
        if error.report is None or error.report.get("schema") != "fr-task-change-1" or error.report.get("executed") is not True:
            raise
        result = TaskResult(error.report, (), reviewed.task_change_basis)
    receipt = DeliveryReceipt.capture(reviewed, result)
    receipt_root = receipt.persist(store)
    resumed = started.plan.resume(client)
    error_text = None
    if receipt.passed:
        try:
            refreshed = resumed.plan.resume(client, transition=f"{step}:reset")
            running = refreshed.plan.resume(client, transition=f"{step}:start")
            resumed = attach_delivery(running.plan, client, step, receipt)
            if next(s for s in resumed.plan.steps if s.id == step).state != StepState.SATISFIED:
                raise FrRuntimeError("delivery step did not become satisfied")
        except FrRuntimeError as error:
            error_text = str(error)
    else:
        error_text = "delivery failed; retained outcomes cannot satisfy acceptance"
    return DeliveredPlanRun(resumed, receipt, receipt_root, started_root,
                            resumed.plan.store(store), error_text)
