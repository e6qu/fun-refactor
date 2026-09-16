from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Mapping, Sequence

from .ir import IrError, merkle_object_digest

SCHEMA = "fr-pure-kernel-1"
SEMANTICS = {
    "arithmetic": "checked-signed-64", "division": "truncate-toward-zero",
    "remainder": "dividend-sign", "partiality": "explicit-failure",
    "bindings": "de-bruijn-nearest-first", "source_correspondence": False,
}
_OPERATORS = {"add", "sub", "mul", "div", "rem", "eq", "ne", "lt", "le", "gt", "ge", "and", "or", "xor"}
_FAILURES = {"fuel", "unbound", "type", "overflow", "division-by-zero", "missing-field", "index", "limit"}


def kernel_limits_admitted(fuel: int, environment: int, nodes: int, depth: int) -> bool:
    return (all(type(value) is int and value >= 0 for value in (fuel, environment, nodes, depth))
            and 1 <= fuel <= 256 and environment <= 64 and nodes <= 4096 and depth <= 64)


def _mapping(value: Any, keys: set[str]) -> Mapping[str, Any]:
    if not isinstance(value, Mapping) or set(value) != keys:
        raise IrError(f"kernel fields must be exactly {sorted(keys)}")
    return value


def _index(value: Any) -> int:
    if type(value) is not int or not 0 <= value < 2**64:
        raise IrError("kernel index must be an unsigned 64-bit integer")
    return value


def _count(depth: int, nodes: list[int]) -> None:
    nodes[0] += 1
    if nodes[0] > 4096 or depth > 64:
        raise IrError("kernel exceeds its node or depth ceiling")


def _value(value: Any, depth: int, nodes: list[int]) -> dict[str, Any]:
    _count(depth, nodes)
    if not isinstance(value, Mapping) or not isinstance(value.get("kind"), str):
        raise IrError("kernel value requires its tagged kind")
    kind = value["kind"]
    _mapping(value, {"kind"} if kind == "unit" else {"kind", "value"})
    if kind == "unit":
        return {"kind": kind}
    payload = value["value"]
    if kind == "bool":
        if type(payload) is not bool:
            raise IrError("kernel bool must be boolean")
    elif kind == "int":
        if type(payload) is not int or not -(2**63) <= payload < 2**63:
            raise IrError("kernel int must be signed 64-bit")
    elif kind == "string":
        if not isinstance(payload, str) or len(payload.encode("utf-8")) > 65_536:
            raise IrError("kernel string exceeds its byte ceiling")
    elif kind in {"tuple", "list"}:
        if not isinstance(payload, list):
            raise IrError("kernel sequence must be a list")
        payload = [_value(item, depth + 1, nodes) for item in payload]
    elif kind == "record":
        if not isinstance(payload, Mapping) or any(not isinstance(name, str) or len(name.encode("utf-8")) > 256 for name in payload):
            raise IrError("kernel record requires bounded string field names")
        payload = {name: _value(item, depth + 1, nodes) for name, item in payload.items()}
    elif kind == "option":
        payload = None if payload is None else _value(payload, depth + 1, nodes)
    elif kind == "result":
        payload = _mapping(payload, {"ok", "value"})
        if type(payload["ok"]) is not bool:
            raise IrError("kernel result branch must be boolean")
        payload = {"ok": payload["ok"], "value": _value(payload["value"], depth + 1, nodes)}
    else:
        raise IrError(f"unknown kernel value kind {kind}")
    return {"kind": kind, "value": payload}


def _term(value: Any, depth: int, nodes: list[int]) -> dict[str, Any]:
    _count(depth, nodes)
    if not isinstance(value, Mapping) or not isinstance(value.get("kind"), str):
        raise IrError("kernel term requires its tagged kind")
    kind = value["kind"]
    fields = {
        "value": {"value"}, "bound": {"index"}, "let": {"value", "body"},
        "if": {"condition", "then", "otherwise"}, "binary": {"operator", "left", "right"},
        "not": {"operand"}, "neg": {"operand"}, "tuple": {"items"}, "list": {"items"},
        "record": {"fields"}, "option": {"value"}, "result": {"ok", "value"},
        "field": {"value", "name"}, "index": {"value", "index"},
    }
    if kind not in fields:
        raise IrError(f"unknown kernel term kind {kind}")
    data = dict(_mapping(value, fields[kind] | {"kind"}))
    if kind == "value":
        data["value"] = _value(data["value"], depth + 1, nodes)
    elif kind in {"tuple", "list"}:
        if not isinstance(data["items"], list):
            raise IrError("kernel term items must be a list")
        data["items"] = [_term(item, depth + 1, nodes) for item in data["items"]]
    elif kind == "record":
        if not isinstance(data["fields"], Mapping) or any(not isinstance(name, str) or len(name.encode("utf-8")) > 256 for name in data["fields"]):
            raise IrError("kernel term fields require bounded string names")
        data["fields"] = {name: _term(item, depth + 1, nodes) for name, item in data["fields"].items()}
    else:
        if kind in {"bound", "index"}:
            _index(data["index"])
        if kind == "binary" and (not isinstance(data["operator"], str) or data["operator"] not in _OPERATORS):
            raise IrError("unknown kernel binary operator")
        if kind == "result" and type(data["ok"]) is not bool:
            raise IrError("kernel result branch must be boolean")
        if kind == "field" and (not isinstance(data["name"], str) or len(data["name"].encode("utf-8")) > 256):
            raise IrError("kernel field name exceeds its byte ceiling")
        for key in fields[kind] - {"index", "operator", "ok", "name"}:
            if kind == "option" and data[key] is None:
                continue
            data[key] = _term(data[key], depth + 1, nodes)
    return data


@dataclass(frozen=True)
class KernelValue:
    kind: str
    value: Any = None

    def to_data(self) -> dict[str, Any]:
        return _value({"kind": self.kind} if self.kind == "unit" else {"kind": self.kind, "value": self.value}, 0, [0])

    @classmethod
    def from_data(cls, value: Any) -> KernelValue:
        data = _value(value, 0, [0])
        return cls(data["kind"], data.get("value"))


@dataclass(frozen=True)
class KernelTerm:
    kind: str
    fields: Mapping[str, Any] = field(default_factory=dict)

    def to_data(self) -> dict[str, Any]:
        if "kind" in self.fields:
            raise IrError("kernel term fields cannot replace its kind")
        return _term({"kind": self.kind, **self.fields}, 0, [0])

    @classmethod
    def from_data(cls, value: Any) -> KernelTerm:
        data = _term(value, 0, [0])
        return cls(data["kind"], {key: item for key, item in data.items() if key != "kind"})

    @staticmethod
    def literal(value: KernelValue) -> KernelTerm:
        return KernelTerm("value", {"value": value.to_data()})

    @staticmethod
    def bound(index: int) -> KernelTerm:
        return KernelTerm("bound", {"index": _index(index)})

    @staticmethod
    def let(value: KernelTerm, body: KernelTerm) -> KernelTerm:
        return KernelTerm("let", {"value": value.to_data(), "body": body.to_data()})

    @staticmethod
    def binary(operator: str, left: KernelTerm, right: KernelTerm) -> KernelTerm:
        return KernelTerm.from_data({"kind": "binary", "operator": operator, "left": left.to_data(), "right": right.to_data()})


@dataclass(frozen=True)
class KernelRequest:
    term: KernelTerm
    environment: Sequence[KernelValue] = ()
    fuel: int = 64

    def to_data(self) -> dict[str, Any]:
        if not kernel_limits_admitted(self.fuel, len(self.environment), 0, 0):
            raise IrError("kernel request exceeds its fuel or environment ceiling")
        nodes = [0]
        term = _term(self.term.to_data(), 0, nodes)
        environment = [_value(value.to_data(), 0, nodes) for value in self.environment]
        return {"schema": SCHEMA, "term": term, "environment": environment, "fuel": self.fuel}


@dataclass(frozen=True)
class SourceBinding:
    name: str
    source_type: str
    lean_type: str

    @classmethod
    def from_data(cls, value: Any) -> SourceBinding:
        data = _mapping(value, {"name", "source_type", "lean_type"})
        if not all(isinstance(item, str) and item for item in data.values()):
            raise IrError("source binding fields must be non-empty strings")
        return cls(data["name"], data["source_type"], data["lean_type"])

    def to_data(self) -> dict[str, str]:
        return {"name": self.name, "source_type": self.source_type, "lean_type": self.lean_type}


@dataclass(frozen=True)
class KernelEvidence:
    source_language: str
    term: KernelTerm
    bindings: Sequence[SourceBinding]
    ir_digest: str
    term_digest: str
    model_digest: str

    @classmethod
    def from_data(cls, value: Any, semantic_ir: Any, lean_definition: str) -> KernelEvidence:
        data = _mapping(value, {"schema", "source_language", "term", "bindings", "ir_digest", "term_digest", "model_digest", "evaluator_arithmetic", "lean_arithmetic", "source_correspondence"})
        if (data["schema"] != "fr-formal-evaluation-1"
                or not isinstance(data["source_language"], str) or not data["source_language"]
                or data["evaluator_arithmetic"] != "checked-signed-64"
                or data["lean_arithmetic"] != "mathematical-Int-and-Nat"
                or data["source_correspondence"] is not False or not isinstance(data["bindings"], list)):
            raise IrError("formal evaluation must state its exact arithmetic and correspondence boundaries")
        term = KernelTerm.from_data(data["term"])
        if (data["ir_digest"] != merkle_object_digest(semantic_ir)
                or data["term_digest"] != merkle_object_digest(term.to_data())
                or data["model_digest"] != merkle_object_digest(lean_definition)):
            raise IrError("formal evaluation identities do not match their own material")
        return cls(data["source_language"], term, tuple(SourceBinding.from_data(item) for item in data["bindings"]), data["ir_digest"], data["term_digest"], data["model_digest"])

    def to_data(self) -> dict[str, Any]:
        return {"schema": "fr-formal-evaluation-1", "source_language": self.source_language,
                "term": self.term.to_data(), "bindings": [binding.to_data() for binding in self.bindings],
                "ir_digest": self.ir_digest, "term_digest": self.term_digest, "model_digest": self.model_digest,
                "evaluator_arithmetic": "checked-signed-64", "lean_arithmetic": "mathematical-Int-and-Nat",
                "source_correspondence": False}


@dataclass(frozen=True)
class KernelResult:
    passed: bool
    value: KernelValue | None
    failure: str | None
    object_digest: str

    @classmethod
    def from_data(cls, value: Any, request: KernelRequest) -> KernelResult:
        value = _mapping(value, {"schema", "request_digest", "semantics", "passed", "value", "failure", "object_digest"})
        if (value["schema"] != "fr-pure-kernel-result-1" or value["semantics"] != SEMANTICS
                or not isinstance(value["semantics"], Mapping) or value["semantics"].get("source_correspondence") is not False
                or value["request_digest"] != merkle_object_digest(request.to_data())
                or type(value["passed"]) is not bool):
            raise IrError("kernel result differs from the reviewed request or semantics")
        core = {key: item for key, item in value.items() if key != "object_digest"}
        if merkle_object_digest(core) != value["object_digest"]:
            raise IrError("kernel result fails its Merkle content address")
        result = None if value["value"] is None else KernelValue.from_data(value["value"])
        if (value["passed"] and (result is None or value["failure"] is not None)
                or not value["passed"] and (result is not None or not isinstance(value["failure"], str) or value["failure"] not in _FAILURES)):
            raise IrError("kernel result has inconsistent success and failure evidence")
        return cls(value["passed"], result, value["failure"], value["object_digest"])
