"""Unchecked runtime effects are distinct from a finished scalar value model."""
from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from .runtime import FrReport, FrRuntimeError, Occurrence

KINDS = {"implicit-exception", "shared-read", "shared-write", "mutable-value",
         "dynamic-call", "exception-handler", "resource-lifetime"}


@dataclass(frozen=True)
class EffectLimit:
    kind: str
    reason: str
    occurrence: Occurrence


@dataclass(frozen=True)
class RuntimeEffects:
    exception_safety_established: bool
    shared_mutable_values_supported: bool
    complete_means: str
    sites: tuple[EffectLimit, ...]

    @classmethod
    def from_report(cls, report: FrReport) -> RuntimeEffects:
        data: Any = report.to_data()
        try:
            scope = data["runtime_effects"]
            if (report.schema != "fr-dataflow-1" or not isinstance(scope, dict)
                    or set(scope) != {"exception_safety_established", "shared_mutable_values_supported", "complete_means"}
                    or scope["exception_safety_established"] is not False
                    or scope["shared_mutable_values_supported"] is not False
                    or not isinstance(scope["complete_means"], str) or not scope["complete_means"]
                    or not isinstance(data["effect_limits"], list)):
                raise FrRuntimeError("unsupported runtime effect disclosure")
            if "omitted" in data and "effect_limits" in data["omitted"]:
                raise FrRuntimeError("effect locations were omitted by the response budget")
            files = data["inputs"]["modules"]["files"] if "modules" in data["inputs"] else {
                data["inputs"]["source"]["path"]: data["inputs"]["source"]["digest"]}
            if not isinstance(files, dict):
                raise FrRuntimeError("effect locations need source file identities")
            sites = []
            seen = set()
            for value in data["effect_limits"]:
                if (not isinstance(value, dict) or set(value) != {"kind", "reason", "occurrence"}
                        or value["kind"] not in KINDS or not isinstance(value["reason"], str) or not value["reason"]):
                    raise FrRuntimeError("malformed runtime effect location")
                occurrence = Occurrence.from_data(value["occurrence"])
                key = (occurrence.id, value["kind"])
                if (occurrence.revision != data["revision"] or occurrence.path not in files
                        or occurrence.role != "unchecked-effect" or key in seen):
                    raise FrRuntimeError("effect location escapes the current source or repeats a site")
                seen.add(key)
                sites.append(EffectLimit(value["kind"], value["reason"], occurrence))
            return cls(False, False, scope["complete_means"], tuple(sites))
        except (KeyError, TypeError, ValueError) as error:
            raise FrRuntimeError("malformed runtime effect disclosure") from error
