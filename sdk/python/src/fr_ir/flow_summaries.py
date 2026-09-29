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
class SummaryParameter:
    name: str
    kind: str
    site: Occurrence
    default: Occurrence | None

    @property
    def required(self) -> bool:
        return self.default is None

    @classmethod
    def from_data(cls, value: Any) -> SummaryParameter:
        if (not isinstance(value, dict) or set(value) != {"name", "kind", "site", "default"}
                or not isinstance(value["name"], str) or not value["name"].isascii()
                or not value["name"].isidentifier()
                or not isinstance(value["kind"], str)
                or value["kind"] not in {"positional-only", "positional-or-keyword", "keyword-only"}):
            raise FrRuntimeError("malformed summary parameter")
        site = Occurrence.from_data(value["site"])
        default = None if value["default"] is None else Occurrence.from_data(value["default"])
        if site.role != "summary-parameter" or site.location.span.end - site.location.span.start != len(value["name"]):
            raise FrRuntimeError("malformed parameter location")
        if default is not None and (default.role != "parameter-default"
                or default.path != site.path or default.revision != site.revision
                or default.location.span.start < site.location.span.end):
            raise FrRuntimeError("malformed default location")
        return cls(value["name"], value["kind"], site, default)


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
    signature: tuple[SummaryParameter, ...] | None = None

    @property
    def return_parameters(self) -> tuple[int, ...]:
        origins = {trace.origin for trace in self.returns}
        return tuple(index for index, parameter in enumerate(self.parameters) if parameter in origins)


_EXPRESSION_CONTROL = {
    "schema": "fr-expression-control-1",
    "evaluation": "left-to-right; each operand once per expression transfer.",
    "selectors": "Boolean and None literals, not, parentheses and nested selected expressions.",
    "unknowns": "retain both alternatives; no variable or call-result truth specialization.",
    "comparisons": "ordered operands; each comparison result may be true or false.",
    "effects": "join normal, sink and explicit-raise alternatives.",
    "boundary": "scalar truth and comparisons; no overloaded protocols or implicit exceptions.",
    "path_feasibility": False,
}


@dataclass(frozen=True)
class ExpressionControl:
    evaluation: str
    selectors: str
    unknowns: str
    comparisons: str
    effects: str
    boundary: str
    path_feasibility: bool

    @classmethod
    def from_data(cls, value: Any) -> ExpressionControl:
        if (not isinstance(value, dict) or value != _EXPRESSION_CONTROL
                or type(value.get("path_feasibility")) is not bool):
            raise FrRuntimeError("unsupported expression control contract")
        return cls(**{key: item for key, item in value.items() if key != "schema"})


_CALL_BINDING = {
    "schema": "fr-call-binding-1",
    "parameters": "required ASCII-named positional-only, positional-or-keyword and keyword-only parameters.",
    "evaluation": "explicit argument values in source order before parameter binding; once per transfer.",
    "binding": "positional slots followed by exact keyword names; substitute in declaration order.",
    "invalid": "incomplete analysis for missing, excess, duplicate or unknown arguments; no TypeError model.",
    "boundary": "no defaults, annotations, variadics, unpacking, keyword external-rule contracts or non-normalized identifier spellings.",
    "implicit_exceptions": False,
}


_LITERAL_CALL_BINDING = {
    **_CALL_BINDING,
    "schema": "fr-call-binding-2",
    "parameters": "ASCII-named positional-only, positional-or-keyword and keyword-only parameters; required or immutable literal defaults.",
    "boundary": "no evaluated or mutable defaults, annotations, variadics, unpacking, keyword external-rule contracts or non-normalized identifier spellings.",
    "defaults": "omitted slots use source-free immutable scalar literals; explicit arguments override defaults; analysis refuses definition-time effects.",
}


@dataclass(frozen=True)
class CallBinding:
    parameters: str
    evaluation: str
    binding: str
    invalid: str
    boundary: str
    implicit_exceptions: bool
    defaults: str | None = None

    @classmethod
    def from_data(cls, value: Any) -> CallBinding:
        if (not isinstance(value, dict) or value not in (_CALL_BINDING, _LITERAL_CALL_BINDING)
                or type(value.get("implicit_exceptions")) is not bool):
            raise FrRuntimeError("unsupported call binding contract")
        return cls(**{key: item for key, item in value.items() if key != "schema"})


@dataclass(frozen=True)
class FunctionSummaries:
    functions: tuple[FunctionSummary, ...]
    converged: bool
    complete: bool
    rounds: int
    expression_control: ExpressionControl | None = None
    call_binding: CallBinding | None = None

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
            if (report.schema != "fr-dataflow-1" or data["semantics"] not in {"python-scalar-summaries-1", "python-scalar-summaries-2", "python-scalar-summaries-3", "python-scalar-summaries-4"}
                    or value["schema"] != "fr-function-summaries-1" or value["enabled"] is not True
                    or value["mutation_authority"] is not False):
                raise FrRuntimeError("report does not disclose function summaries")
            if (type(value["converged"]) is not bool or type(data["complete"]) is not bool
                    or type(value["rounds"]) is not int or value["rounds"] < 1
                    or not isinstance(value["functions"], dict) or len(value["functions"]) > 64
                    or (data["complete"] and not value["converged"])):
                raise FrRuntimeError("inconsistent summary convergence")
            control = None
            if data["semantics"] in {"python-scalar-summaries-2", "python-scalar-summaries-3", "python-scalar-summaries-4"}:
                control = ExpressionControl.from_data(data["expression_control"])
                ExpressionControl.from_data(data["inputs"]["expression_control"])
                if data["inputs"]["expression_control"] != data["expression_control"]:
                    raise FrRuntimeError("expression control disagrees with analysis inputs")
            elif (data.get("expression_control") is not None
                    or data["inputs"].get("expression_control") is not None):
                raise FrRuntimeError("legacy summaries cannot declare new expression control")
            binding = None
            if data["semantics"] in {"python-scalar-summaries-3", "python-scalar-summaries-4"}:
                expected_binding = _LITERAL_CALL_BINDING if data["semantics"] == "python-scalar-summaries-4" else _CALL_BINDING
                if data["call_binding"] != expected_binding:
                    raise FrRuntimeError("call binding disagrees with summary semantics")
                binding = CallBinding.from_data(data["call_binding"])
                CallBinding.from_data(data["inputs"]["call_binding"])
                if data["inputs"]["call_binding"] != data["call_binding"]:
                    raise FrRuntimeError("call binding disagrees with analysis inputs")
            elif (data.get("call_binding") is not None
                    or data["inputs"].get("call_binding") is not None):
                raise FrRuntimeError("legacy summaries cannot declare new call binding")
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
                signature = None
                if data["semantics"] == "python-scalar-summaries-4":
                    raw_signature = item["signature"]
                    if not isinstance(raw_signature, list) or len(raw_signature) != len(parameters):
                        raise FrRuntimeError("signature disagrees with summary parameters")
                    signature = tuple(SummaryParameter.from_data(raw) for raw in raw_signature)
                    seen = set()
                    rank, optional = 0, False
                    for parameter in signature:
                        current = {"positional-only": 0, "positional-or-keyword": 1, "keyword-only": 2}[parameter.kind]
                        if (parameter.name in seen or current < rank or parameter.site.revision != revision
                                or current < 2 and optional and parameter.required):
                            raise FrRuntimeError("inconsistent summary signature")
                        seen.add(parameter.name)
                        rank = current
                        optional |= not parameter.required
                elif "signature" in item:
                    raise FrRuntimeError("legacy summaries cannot declare signature metadata")
                functions.append(FunctionSummary(name, tuple(parameters), traces(item["returns"]),
                                                 traces(item["exceptional_returns"]), tuple(sinks),
                                                 item["normal_return"], item["may_raise"],
                                                 tuple(item["callees"]), item["evaluations"], signature))
            return cls(tuple(functions), value["converged"], data["complete"], value["rounds"], control, binding)
        except (KeyError, TypeError, ValueError, AttributeError) as error:
            raise FrRuntimeError("malformed function summaries") from error
