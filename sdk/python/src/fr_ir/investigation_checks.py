"""Attach retained command evidence to plans without granting mutation authority."""
from __future__ import annotations

from dataclasses import dataclass
import hashlib
import json
from pathlib import Path
import tempfile

from .context import ObjectStore, restore_stored_value, store_merkle_value
from .investigation import DependencyKind, ResumedPlan, TaskPlan
from .runtime import FrClient, FrReport, FrRuntimeError


def attach_checks(plan: TaskPlan, client: FrClient, step: str, report: FrReport, *,
                  satisfy: bool = False) -> ResumedPlan:
    """Validate trusted retained check output against current native input identities."""
    encoded = json.dumps(report.to_data(), ensure_ascii=False, sort_keys=True, separators=(",", ":"))
    digest = hashlib.sha256(encoded.encode()).hexdigest()
    with tempfile.TemporaryDirectory(prefix="fr-checked-plan-") as temporary:
        directory = Path(temporary)
        source = directory / "plan.json"
        source.write_text(json.dumps(plan.to_data()), encoding="utf-8")
        result = directory / "checks.json"
        result.write_text(encoded, encoding="utf-8")
        arguments = ["investigate", "--from", str(source), "--checks-from", str(result),
                     "--checks-digest", digest, "--check-step", step]
        if satisfy:
            arguments.extend(["--transition", f"{step}:satisfy"])
        resumed = client.project(*arguments)
    data = resumed.to_data()
    if resumed.schema != "fr-investigation-resume-1":
        raise FrRuntimeError("unsupported check attachment report")
    return ResumedPlan(TaskPlan.from_data(data["plan"]), data["input_digests"],
                       tuple(data["invalidated"]), data["complete"], resumed)


def restore_checks(store: ObjectStore, digest: str) -> FrReport:
    value = restore_stored_value(store, digest)
    if not isinstance(value, dict) or value.get("schema") != "fr-checks-1":
        raise FrRuntimeError("stored object is not a check report")
    return FrReport(value, ())


@dataclass(frozen=True)
class CheckedPlanRun:
    resumed: ResumedPlan
    checks: FrReport
    report_root: str
    attachment_error: str | None

    @property
    def passed(self) -> bool:
        return self.attachment_error is None and self.checks.at("/passed") is True


def run_checks(plan: TaskPlan, client: FrClient, step_id: str, reviewed: FrReport,
               store: ObjectStore, *, output_bytes: int = 4096) -> CheckedPlanRun:
    """Run explicitly reviewed declarations, retain outcomes and validate plan attachment."""
    if (reviewed.schema != "fr-checks-1" or reviewed.at("/executed") is not False
            or reviewed.at("/root") != str(client.root)):
        raise FrRuntimeError("checked plan needs a reviewed check listing for this workspace")
    current = client.call("checks", "--toolchain")
    if (current.at("/basis") != reviewed.at("/basis")
            or current.at("/toolchain") != reviewed.at("/toolchain")):
        raise FrRuntimeError("reviewed check configuration or toolchain changed")
    matches = [step for step in plan.steps if step.id == step_id]
    if len(matches) != 1 or not matches[0].required_checks:
        raise FrRuntimeError("checked plan needs one step with required checks")
    names = matches[0].required_checks
    scope = {dependency.kind for dependency in matches[0].inputs}
    if not {DependencyKind.WORKSPACE, DependencyKind.CHECK_CONFIGURATION,
            DependencyKind.CHECK_SOURCES, DependencyKind.CHECK_TOOLCHAIN} <= scope:
        raise FrRuntimeError("checked step has incomplete input dependencies")
    if len(set(names)) != len(names) or not set(names) <= {check["name"] for check in current.at("/checks")}:
        raise FrRuntimeError("required checks differ from reviewed declarations")
    started = plan.resume(client, transition=f"{step_id}:start")
    arguments = ("checks", "--toolchain", "--run", ",".join(names), "--basis",
                 current.at("/basis"), "--output-bytes", str(output_bytes))
    try:
        report = client.call(*arguments)
    except FrRuntimeError as error:
        if error.report is None or error.report.get("schema") != "fr-checks-1":
            raise
        report = FrReport(error.report, arguments)
    root = store_merkle_value(store, report.to_data()).digest
    try:
        resumed = attach_checks(started.plan, client, step_id, report, satisfy=report.at("/passed") is True)
        error_text = None
    except FrRuntimeError as error:
        resumed = started.plan.resume(client)
        error_text = str(error)
    return CheckedPlanRun(resumed, report, root, error_text)
