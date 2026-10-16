"""Source locations for module initialization the scalar model cannot check."""
from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from .runtime import FrReport, FrRuntimeError, Occurrence


_EXPLANATIONS = {
    "binding-collision": "This statement repeats a module binding; its effects remain unchecked.",
    "definition-time-code": "This function declaration exceeds the supported rules; defaults must be literal and annotations need a supported future directive.",
    "non-normalized-identifier": "Python normalizes this identifier; the model cannot resolve it reliably.",
    "module-statement": "This module-level statement is outside the supported initialization rules.",
    "class-definition": "Creating this class exceeds the supported rules for bases, methods or class-body statements.",
    "decorated-definition": "This declaration applies decorators during import; their effects remain unchecked.",
    "startup-import": "This import needs supported local import analysis; it has not been checked here.",
    "startup-control-flow": "This module-level branch, loop or handler can run code during import and remains unchecked.",
    "startup-call": "This statement calls a function during import; that call remains unchecked.",
    "startup-write": "This statement writes or deletes an object member during import; the write and its evaluated values remain unchecked.",
    "startup-assignment": "This assignment evaluates an unsupported value during import; its effects remain unchecked.",
    "initializer-node-budget": "Checking this initializer exceeds the existing 128-node syntax limit; its other operations may also be unsupported.",
}


@dataclass(frozen=True)
class InitializationSite:
    occurrence: Occurrence
    reasons: tuple[str, ...]

    @property
    def explanations(self) -> tuple[str, ...]:
        return tuple(_EXPLANATIONS[reason] for reason in self.reasons)


@dataclass(frozen=True)
class InitializationFile:
    path: str
    statements: int
    reasons: tuple[tuple[str, int], ...]

    @property
    def explanations(self) -> tuple[str, ...]:
        return tuple(_EXPLANATIONS[reason] for reason, _ in self.reasons)


@dataclass(frozen=True)
class FlowInitialization:
    sites: tuple[InitializationSite, ...]
    omitted: int
    by_file: tuple[InitializationFile, ...]

    @classmethod
    def from_report(cls, report: FrReport) -> FlowInitialization:
        data = report.to_data()
        try:
            value = data["initialization"]
            if (report.schema != "fr-dataflow-1" or not isinstance(value, dict)
                    or set(value) != {"sites", "omitted", "by_file"}
                    or not isinstance(value["sites"], list) or len(value["sites"]) > 16
                    or type(value["omitted"]) is not int or value["omitted"] < 0
                    or type(data["complete"]) is not bool or not isinstance(data["cutoffs"], list)):
                raise FrRuntimeError("unsupported initialization disclosure")
            files = data["inputs"]["modules"]["files"] if "modules" in data["inputs"] else {
                data["inputs"]["source"]["path"]: data["inputs"]["source"]["digest"],
            }
            if not isinstance(files, dict):
                raise FrRuntimeError("initialization needs source file identities")
            counts = value["by_file"]
            if not isinstance(counts, dict) or len(counts) > 16:
                raise FrRuntimeError("initialization needs bounded per-file counts")
            by_file = []
            for path, item in sorted(counts.items()):
                if (path not in files or not isinstance(item, dict)
                        or set(item) != {"statements", "reasons"}
                        or type(item["statements"]) is not int or item["statements"] <= 0
                        or not isinstance(item["reasons"], dict) or not item["reasons"]
                        or any(reason not in _EXPLANATIONS or type(count) is not int
                               or not 0 < count <= item["statements"]
                               for reason, count in item["reasons"].items())
                        or sum(item["reasons"].values()) < item["statements"]):
                    raise FrRuntimeError("malformed initialization file counts")
                by_file.append(InitializationFile(path, item["statements"], tuple(sorted(item["reasons"].items()))))
            sites = []
            shown: dict[str, int] = {}
            shown_reasons: dict[str, dict[str, int]] = {}
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
                path = occurrence.path
                shown[path] = shown.get(path, 0) + 1
                sample = shown_reasons.setdefault(path, {})
                for reason in reasons:
                    sample[reason] = sample.get(reason, 0) + 1
            if (sum(item.statements for item in by_file) != len(sites) + value["omitted"]
                    or any(path not in counts for path in shown)):
                raise FrRuntimeError("initialization totals disagree with locations")
            for item in by_file:
                missing = item.statements - shown.get(item.path, 0)
                sample = shown_reasons.get(item.path, {})
                reasons = dict(item.reasons)
                if (missing < 0 or any(reason not in reasons for reason in sample)
                        or any(not 0 <= count - sample.get(reason, 0) <= missing
                               for reason, count in item.reasons)
                        or sum(count - sample.get(reason, 0) for reason, count in item.reasons) < missing):
                    raise FrRuntimeError("initialization reason counts disagree with locations")
            unchecked = bool(sites or value["omitted"])
            if (unchecked != ("module-effects-unchecked" in data["cutoffs"])
                    or (unchecked and data["complete"] is not False)):
                raise FrRuntimeError("initialization disclosure disagrees with completeness")
            return cls(tuple(sites), value["omitted"], tuple(by_file))
        except (KeyError, TypeError, ValueError) as error:
            raise FrRuntimeError("missing or malformed initialization disclosure") from error
