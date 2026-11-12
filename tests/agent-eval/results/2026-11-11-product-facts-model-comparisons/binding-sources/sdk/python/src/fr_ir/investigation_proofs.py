"""Retain local Lean evidence and attach named model theorems to investigations."""
from __future__ import annotations

from dataclasses import dataclass
import hashlib
import json
from pathlib import Path
import tempfile
from typing import Any, Mapping

from .context import ObjectStore, restore_stored_value, store_merkle_value
from .investigation import ResumedPlan, TaskPlan
from .runtime import FrClient, FrReport, FrRuntimeError


@dataclass(frozen=True)
class ProofReport:
    report: FrReport

    def __post_init__(self) -> None:
        data = self.report.to_data()
        fields = {"schema", "root", "package", "input_digest", "inputs", "executed", "stable", "passed",
                  "evidence", "results", "modules", "mutation_authority", "source_implementation_proved", "claim"}
        if set(data) != fields or data["schema"] != "fr-proof-evidence-1":
            raise FrRuntimeError("unsupported retained proof report")
        if any(type(data[key]) is not bool for key in ("executed", "stable", "passed", "mutation_authority", "source_implementation_proved")):
            raise FrRuntimeError("proof outcome flags must be boolean")
        if data["mutation_authority"] or data["source_implementation_proved"]:
            raise FrRuntimeError("retained model evidence cannot claim implementation proof or mutation authority")
        if (not isinstance(data["root"], str) or not Path(data["root"]).is_absolute()
                or not isinstance(data["package"], str) or not data["package"]
                or data["package"].startswith("/") or any(p in {"", ".", ".."} for p in data["package"].split("/"))):
            raise FrRuntimeError("proof report needs a workspace and normalized package")
        if not isinstance(data["inputs"], Mapping) or data["input_digest"] != _digest(data["inputs"]):
            raise FrRuntimeError("proof input digest mismatch")
        if (not isinstance(data["modules"], list) or not 1 <= len(data["modules"]) <= 128
                or not all(isinstance(m, str) and m.endswith(".lean") for m in data["modules"])
                or len(set(data["modules"])) != len(data["modules"]) or not isinstance(data["results"], list)):
            raise FrRuntimeError("proof modules must be distinct and bounded")
        if not data["executed"]:
            if data["passed"] or data["evidence"] is not None or data["results"]:
                raise FrRuntimeError("unexecuted proof review carries results")
        elif not isinstance(data["evidence"], Mapping):
            raise FrRuntimeError("executed proof report has no model evidence")
        if data["passed"]:
            results = data["results"]
            if (not data["stable"] or len(results) != len(data["modules"]) + 1
                    or any(not isinstance(row, Mapping) for row in results)
                    or [r.get("module") for r in results] != [None, *data["modules"]]):
                raise FrRuntimeError("passing proof report has incomplete module coverage")
            for row in results:
                outcome = row.get("result")
                if (not isinstance(outcome, Mapping) or outcome.get("passed") is not True
                        or outcome.get("exit_code") != 0 or outcome.get("timed_out") is not False
                        or outcome.get("output_limit_exceeded") is not False or outcome.get("error") is not None
                        or outcome.get("termination_error") is not None):
                    raise FrRuntimeError("passing proof report contains failure diagnostics")
            evidence = data["evidence"]
            if (evidence.get("schema") != 1 or evidence.get("correspondence", {}).get("proved_implementation_model") is not False
                    or evidence.get("verification", {}).get("report", {}).get("debts") != []
                    or evidence.get("verification", {}).get("report", {}).get("obligations") != 0):
                raise FrRuntimeError("passing proof report carries debt or unsupported correspondence")

    @property
    def passed(self) -> bool:
        return self.report.at("/passed") is True

    @property
    def basis(self) -> str:
        return self.report.at("/input_digest")

    @classmethod
    def review(cls, client: FrClient, package: str = "specs") -> ProofReport:
        report = cls(client.call("spec", "retain", package))
        if report.report.at("/root") != str(client.root) or report.report.at("/package") != package or report.report.at("/executed") is not False:
            raise FrRuntimeError("proof review returned another workspace, package or execution")
        return report

    def execute(self, client: FrClient) -> ProofReport:
        if self.report.at("/executed") or self.report.at("/root") != str(client.root):
            raise FrRuntimeError("execution needs a proof review for this workspace")
        arguments = ("spec", "retain", self.report.at("/package"), "--run", "--basis", self.basis)
        try:
            report = client.call(*arguments)
        except FrRuntimeError as error:
            if error.report is None or error.report.get("schema") != "fr-proof-evidence-1":
                raise
            report = FrReport(error.report, arguments)
        result = ProofReport(report)
        if (result.basis != self.basis or result.report.at("/root") != str(client.root)
                or result.report.at("/package") != self.report.at("/package") or result.report.at("/executed") is not True):
            raise FrRuntimeError("proof execution returned another input selection")
        return result

    def persist(self, store: ObjectStore) -> str:
        return store_merkle_value(store, self.report.to_data()).digest

    @classmethod
    def restore(cls, store: ObjectStore, digest: str) -> ProofReport:
        value = restore_stored_value(store, digest)
        if not isinstance(value, dict):
            raise FrRuntimeError("stored proof report must be an object")
        return cls(FrReport(value, ()))


def _digest(value: Any) -> str:
    return hashlib.sha256(json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def attach_proofs(plan: TaskPlan, client: FrClient, step: str, report: ProofReport, *, satisfy: bool = False) -> ResumedPlan:
    """Revalidate retained trusted output against current declarations and input identities."""
    if not report.passed:
        raise FrRuntimeError("proof attachment requires a passing execution")
    value = report.report.to_data()
    with tempfile.TemporaryDirectory(prefix="fr-proved-plan-") as temporary:
        directory = Path(temporary)
        source = directory / "plan.json"
        source.write_text(json.dumps(plan.to_data()), encoding="utf-8")
        retained = directory / "proof.json"
        retained.write_text(json.dumps(value, ensure_ascii=False), encoding="utf-8")
        arguments = ["investigate", "--from", str(source), "--proofs-from", str(retained),
                     "--proofs-digest", _digest(value), "--proof-step", step]
        if satisfy:
            arguments.extend(["--transition", f"{step}:satisfy"])
        resumed = client.project(*arguments)
    if resumed.schema != "fr-investigation-resume-1":
        raise FrRuntimeError("unsupported proof attachment report")
    data = resumed.to_data()
    if not isinstance(data.get("proof_attachment"), Mapping) or data["proof_attachment"].get("digest") != _digest(value):
        raise FrRuntimeError("proof attachment returned another evidence report")
    return ResumedPlan(TaskPlan.from_data(data["plan"]), data["input_digests"], tuple(data["invalidated"]), data["complete"], resumed)


@dataclass(frozen=True)
class ProvedPlanRun:
    resumed: ResumedPlan
    proofs: ProofReport
    report_root: str
    attachment_error: str | None

    @property
    def passed(self) -> bool:
        return self.attachment_error is None and self.proofs.passed


def run_proofs(plan: TaskPlan, client: FrClient, step: str, reviewed: ProofReport, store: ObjectStore) -> ProvedPlanRun:
    matches = [s for s in plan.steps if s.id == step]
    if len(matches) != 1 or not matches[0].required_proofs:
        raise FrRuntimeError("proof execution needs one step with required theorems")
    packages = {proof.package for proof in matches[0].required_proofs}
    if packages != {reviewed.report.at("/package")}:
        raise FrRuntimeError("run_proofs needs one reviewed package; attach multiple package reports explicitly")
    started = plan.resume(client, transition=f"{step}:start")
    proofs = reviewed.execute(client)
    root = proofs.persist(store)
    try:
        resumed = attach_proofs(started.plan, client, step, proofs, satisfy=True)
        error_text = None
    except FrRuntimeError as error:
        resumed = started.plan.resume(client)
        error_text = str(error)
    return ProvedPlanRun(resumed, proofs, root, error_text)
