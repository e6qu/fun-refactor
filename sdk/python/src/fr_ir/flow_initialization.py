"""Source locations for module initialization the scalar model cannot check."""
from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from .runtime import FrReport, FrRuntimeError, Occurrence


_EXPLANATIONS = {
    "binding-collision": "This statement repeats a module binding; its effects remain unchecked.",
    "definition-time-code": "This function declaration exceeds the supported rules, which exclude evaluated defaults and annotations.",
    "non-normalized-identifier": "Python normalizes this identifier; the model cannot resolve it reliably.",
    "module-statement": "This module-level statement is outside the supported initialization rules.",
}


@dataclass(frozen=True)
class InitializationSite:
    occurrence: Occurrence
    reasons: tuple[str, ...]

    @property
    def explanations(self) -> tuple[str, ...]:
        return tuple(_EXPLANATIONS[reason] for reason in self.reasons)


@dataclass(frozen=True)
class FlowInitialization:
    sites: tuple[InitializationSite, ...]
    omitted: int

    @classmethod
    def from_report(cls, report: FrReport) -> FlowInitialization:
        data = report.to_data()
        try:
            value = data["initialization"]
            if (report.schema != "fr-dataflow-1" or not isinstance(value, dict)
                    or set(value) != {"sites", "omitted"}
                    or not isinstance(value["sites"], list) or len(value["sites"]) > 16
                    or type(value["omitted"]) is not int or value["omitted"] < 0
                    or type(data["complete"]) is not bool or not isinstance(data["cutoffs"], list)):
                raise FrRuntimeError("unsupported initialization disclosure")
            files = data["inputs"]["modules"]["files"] if "modules" in data["inputs"] else {
                data["inputs"]["source"]["path"]: data["inputs"]["source"]["digest"],
            }
            if not isinstance(files, dict):
                raise FrRuntimeError("initialization needs source file identities")
            sites = []
            previous = None
            for item in value["sites"]:
                if not isinstance(item, dict) or set(item) != {"occurrence", "reasons"}:
                    raise FrRuntimeError("malformed initialization site")
                occurrence = Occurrence.from_data(item["occurrence"])
                reasons = item["reasons"]
                if (occurrence.revision != data["revision"] or occurrence.path not in files
                        or occurrence.role != "module-initialization"
                        or not isinstance(reasons, list) or not reasons
                        or any(not isinstance(reason, str) or reason not in _EXPLANATIONS for reason in reasons)
                        or len(reasons) != len(set(reasons))):
                    raise FrRuntimeError("initialization site escapes its contract")
                key = (occurrence.path, occurrence.location.span.start, occurrence.location.span.end)
                if previous is not None and key <= previous:
                    raise FrRuntimeError("initialization sites must be unique and source ordered")
                previous = key
                sites.append(InitializationSite(occurrence, tuple(reasons)))
            unchecked = bool(sites or value["omitted"])
            if (unchecked != ("module-effects-unchecked" in data["cutoffs"])
                    or (unchecked and data["complete"] is not False)):
                raise FrRuntimeError("initialization disclosure disagrees with completeness")
            return cls(tuple(sites), value["omitted"])
        except (KeyError, TypeError, ValueError) as error:
            raise FrRuntimeError("missing or malformed initialization disclosure") from error
