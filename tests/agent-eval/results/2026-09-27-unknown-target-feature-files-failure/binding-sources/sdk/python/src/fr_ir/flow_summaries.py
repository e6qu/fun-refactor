"""Typed symbolic flow summaries; model convergence does not prove termination."""
from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from .runtime import FrReport, FrRuntimeError, Occurrence


@dataclass(frozen=True)
class SummaryTrace:
    origin: str
    occurrences: tuple[Occurrence, ...]


@dataclass(frozen=True)
class SinkEffect:
    sink: str
    site: Occurrence
    traces: tuple[SummaryTrace, ...]


@dataclass(frozen=True)
class FunctionSummary:
    function: str
    parameters: tuple[str, ...]
    returns: tuple[SummaryTrace, ...]
    exceptional_returns: tuple[SummaryTrace, ...]
    sinks: tuple[SinkEffect, ...]
    normal_return: bool
    may_raise: bool
    callees: tuple[str, ...]
    evaluations: int

    @property
    def return_parameters(self) -> tuple[int, ...]:
        origins = {trace.origin for trace in self.returns}
        return tuple(index for index, parameter in enumerate(self.parameters) if parameter in origins)


@dataclass(frozen=True)
class FunctionSummaries:
    functions: tuple[FunctionSummary, ...]
    converged: bool
    complete: bool
    rounds: int

    def for_function(self, name: str) -> FunctionSummary:
        for item in self.functions:
            if item.function == name:
                return item
        raise FrRuntimeError("function summary was not disclosed")

    @classmethod
    def from_report(cls, report: FrReport) -> FunctionSummaries:
        data = report.to_data()
        try:
            value = data["function_summaries"]
            if (report.schema != "fr-dataflow-1" or data["semantics"] != "python-scalar-summaries-1"
                    or value["schema"] != "fr-function-summaries-1" or value["enabled"] is not True
                    or value["mutation_authority"] is not False):
                raise FrRuntimeError("report does not disclose function summaries")
            if (type(value["converged"]) is not bool or type(data["complete"]) is not bool
                    or type(value["rounds"]) is not int or value["rounds"] < 1
                    or not isinstance(value["functions"], dict) or len(value["functions"]) > 64
                    or (data["complete"] and not value["converged"])):
                raise FrRuntimeError("inconsistent summary convergence")
            revision = data["revision"]

            def occurrence(raw: Any) -> Occurrence:
                result = Occurrence.from_data(raw)
                if result.revision != revision:
                    raise FrRuntimeError("summary occurrence belongs to another revision")
                return result

            functions = []
            for name, item in value["functions"].items():
                parameters = item["parameters"]
                if parameters != [f"argument:{name}:{index}" for index in range(len(parameters))]:
                    raise FrRuntimeError("malformed summary parameters")

                def traces(raw: Any) -> tuple[SummaryTrace, ...]:
                    result = []
                    for origin, trace in raw.items():
                        if (origin != trace["origin"] or
                                origin not in parameters and origin not in data["origins"]):
                            raise FrRuntimeError("summary trace has an undeclared origin")
                        points = tuple(occurrence(o) for o in trace["occurrences"])
                        if not points or len({point.id for point in points}) != len(points):
                            raise FrRuntimeError("summary trace needs distinct occurrences")
                        result.append(SummaryTrace(origin, points))
                    return tuple(result)

                sinks = []
                for key, effect in item["sinks"].items():
                    site = occurrence(effect["site"])
                    if key != site.id or not isinstance(effect["sink"], str) or not effect["sink"]:
                        raise FrRuntimeError("malformed summary sink")
                    sinks.append(SinkEffect(effect["sink"], site, traces(effect["flow"])))
                if (type(item["normal_return"]) is not bool or type(item["may_raise"]) is not bool
                        or type(item["evaluations"]) is not int or item["evaluations"] < 0
                        or not isinstance(item["callees"], list)
                        or any(not isinstance(callee, str) for callee in item["callees"])):
                    raise FrRuntimeError("malformed summary effects")
                if data["complete"] and any(callee not in value["functions"] for callee in item["callees"]):
                    raise FrRuntimeError("complete summary has an undisclosed callee")
                functions.append(FunctionSummary(name, tuple(parameters), traces(item["returns"]),
                                                 traces(item["exceptional_returns"]), tuple(sinks),
                                                 item["normal_return"], item["may_raise"],
                                                 tuple(item["callees"]), item["evaluations"]))
            return cls(tuple(functions), value["converged"], data["complete"], value["rounds"])
        except (KeyError, TypeError, ValueError, AttributeError) as error:
            raise FrRuntimeError("malformed function summaries") from error
