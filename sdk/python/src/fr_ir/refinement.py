"""Retain Boolean source/model snapshots and review old/new Lean obligations."""
from __future__ import annotations

from dataclasses import dataclass
import hashlib
import json
from pathlib import Path
import re
import tempfile
from typing import Any, Literal, Mapping

from .context import ObjectStore, restore_stored_value, store_merkle_value
from .investigation import ProofRequirement
from .runtime import FrClient, FrReport, FrRuntimeError


def _digest(value: Any) -> str:
    return hashlib.sha256(json.dumps(value, sort_keys=True, ensure_ascii=False,
        separators=(",", ":"), allow_nan=False).encode()).hexdigest()


@dataclass(frozen=True)
class ModelSnapshot:
    report: FrReport

    def __post_init__(self) -> None:
        data = self.report.to_data()
        if (set(data) != {"schema", "target", "source", "model", "digest"}
                or data["schema"] != "fr-model-snapshot-1"
                or not isinstance(data["source"], str) or len(data["source"].encode()) > 65536
                or not isinstance(data["target"], str) or not isinstance(data["model"], Mapping)
                or data["digest"] != _digest([data[k] for k in ("schema", "target", "source", "model")])):
            raise FrRuntimeError("invalid model snapshot or digest")

    @classmethod
    def capture(cls, client: FrClient, target: str) -> ModelSnapshot:
        result = cls(client.call("spec", "snapshot", target))
        if result.report.at("/target") != target:
            raise FrRuntimeError("snapshot returned another target")
        return result

    def persist(self, store: ObjectStore) -> str:
        return store_merkle_value(store, self.report.to_data()).digest

    @classmethod
    def restore(cls, store: ObjectStore, digest: str) -> ModelSnapshot:
        value = restore_stored_value(store, digest)
        if not isinstance(value, dict):
            raise FrRuntimeError("stored model snapshot must be an object")
        return cls(FrReport(value, ()))


@dataclass(frozen=True)
class ModelComparison:
    name: str
    before: ModelSnapshot
    after: str
    arguments: tuple[int, ...]
    relation: Literal["equivalent", "refines"] = "equivalent"

    def __post_init__(self) -> None:
        if (not isinstance(self.name, str) or re.fullmatch(r"[A-Z][A-Za-z0-9]{0,63}", self.name) is None
                or self.relation not in ("equivalent", "refines")
                or not isinstance(self.after, str) or "::" not in self.after
                or not isinstance(self.before, ModelSnapshot)
                or not isinstance(self.arguments, (tuple, list))
                or len(self.arguments) > 8
                or any(type(i) is not int or not 0 <= i < 8 for i in self.arguments)):
            raise FrRuntimeError("invalid Boolean model comparison")
        object.__setattr__(self, "arguments", tuple(self.arguments))

    def to_data(self) -> dict[str, Any]:
        return {"schema": "fr-model-comparison-1", "name": self.name,
                "before": self.before.report.to_data(), "after": self.after,
                "relation": self.relation, "arguments": list(self.arguments)}

    def _call(self, client: FrClient, package: str, basis: str | None = None) -> FrReport:
        with tempfile.TemporaryDirectory(prefix="fr-comparison-") as temporary:
            path = Path(temporary)/"request.json"
            path.write_text(json.dumps(self.to_data(), ensure_ascii=False))
            args = ["spec", "compare", "--from", str(path), "--package", package]
            if basis is not None:
                args.extend(["--write", "--basis", basis])
            report = client.call(*args)
        if (report.schema != "fr-comparison-review-1" or report.at("/package") != package
                or report.at("/before_digest") != self.before.report.at("/digest")
                or report.at("/relation") != self.relation or report.at("/arguments") != list(self.arguments)
                or report.at("/source_implementation_proved") is not False):
            raise FrRuntimeError("comparison report differs from the requested claim")
        return report

    def review(self, client: FrClient, package: str = "specs") -> FrReport:
        return self._call(client, package)

    def execute(self, client: FrClient, review: FrReport) -> FrReport:
        if review.schema != "fr-comparison-review-1" or review.at("/applied") is not False:
            raise FrRuntimeError("comparison execution needs an unexecuted review")
        current = self.review(client, review.at("/package"))
        if current.to_data() != review.to_data():
            raise FrRuntimeError("comparison review changed")
        return self._call(client, review.at("/package"), review.at("/basis"))

    def requirement(self, package: str = "specs") -> ProofRequirement:
        return ProofRequirement(package, f"FrSpecs/{self.name}.lean", "preserves")
