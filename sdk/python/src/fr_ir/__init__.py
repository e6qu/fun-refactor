"""Source-free constructors for ``fr-semantic-body-1``.

The public namespaces mirror the Rust IR: ``Type``, ``Stmt``, ``Expr`` and
``TemplatePart``. Constructors reject nodes from another category immediately.
"""

from __future__ import annotations

from dataclasses import dataclass
from enum import Enum
import json
from pathlib import Path
from typing import Any, Iterable, Mapping, Sequence

SCHEMA = "fr-semantic-body-1"
_ABSENT = object()

TYPE_KINDS = ("unit", "bool", "int", "float", "string", "list", "set", "map", "optional", "tuple", "named", "fn")
STATEMENT_KINDS = ("return", "let", "assign", "tuple-assign", "if", "if-present", "while", "counted-for", "for-each-indexed", "defer", "err-defer", "switch", "match-variants", "while-present", "for-each", "expr", "assert", "comment", "local-function", "block", "throw", "try", "break", "break-with", "continue")
EXPRESSION_KINDS = ("int", "float", "str", "bool", "null", "name", "field", "index", "call", "binary", "unary", "await", "propagate", "keyword", "cast", "instance-of", "new", "record-lit", "coalesce", "ternary", "variant", "tuple", "list-lit", "map-lit", "template", "lambda", "set-lit", "comprehension")
TEMPLATE_KINDS = ("text", "expr")


class IrError(ValueError):
    """The requested value does not belong to the semantic IR contract."""


class BinaryOp(str, Enum):
    ADD = "add"
    SUB = "sub"
    MUL = "mul"
    DIV = "div"
    FLOOR_DIV = "floor-div"
    TRUE_DIV = "true-div"
    REM = "rem"
    FLOOR_REM = "floor-rem"
    EQ = "eq"
    NE = "ne"
    LT = "lt"
    LE = "le"
    GT = "gt"
    GE = "ge"
    AND = "and"
    OR = "or"
    XOR = "xor"


class UnaryOp(str, Enum):
    NOT = "not"
    NEG = "neg"
    UNWRAP = "unwrap"


class ParamKind(str, Enum):
    NORMAL = "normal"
    VAR_ARGS = "var-args"
    KEYWORD_ARGS = "keyword-args"
    MARKER = "marker"


@dataclass(frozen=True)
class _Node:
    category: str
    kind: str
    value: Any = _ABSENT

    def to_data(self, _seen: set[int] | None = None) -> dict[str, Any]:
        seen = set() if _seen is None else _seen
        identity = id(self)
        if identity in seen:
            raise IrError("semantic IR must not contain cycles")
        seen.add(identity)
        try:
            data: dict[str, Any] = {"kind": self.kind}
            if self.value is not _ABSENT:
                data["value"] = _data(self.value, seen)
            return data
        finally:
            seen.remove(identity)


class TypeNode(_Node):
    """A value accepted only at an IR type boundary."""


class StatementNode(_Node):
    """A value accepted only at an IR statement boundary."""


class ExpressionNode(_Node):
    """A value accepted only at an IR expression boundary."""


class TemplateNode(_Node):
    """A value accepted only at an interpolated-string part boundary."""


def _node(category: str, kind: str, value: Any = _ABSENT) -> _Node:
    classes = {
        "type": TypeNode,
        "statement": StatementNode,
        "expr": ExpressionNode,
        "template": TemplateNode,
    }
    return classes[category](category, kind, value)


def _expect(value: Any, category: str, field: str) -> _Node:
    if not isinstance(value, _Node) or value.category != category:
        raise IrError(f"{field} must be a {category} node")
    return value


def _optional(value: Any, category: str, field: str) -> _Node | None:
    return None if value is None else _expect(value, category, field)


def _nodes(values: Iterable[Any], category: str, field: str) -> list[_Node]:
    return [_expect(value, category, field) for value in values]


def _pairs(values: Iterable[Sequence[Any]], left: str, right: str, field: str) -> list[list[Any]]:
    pairs: list[list[Any]] = []
    for value in values:
        if len(value) != 2:
            raise IrError(f"{field} entries must contain two values")
        first, second = value
        if left in {"type", "statement", "expr", "template"}:
            first = _expect(first, left, field)
        elif left == "string" and not isinstance(first, str):
            raise IrError(f"{field} first values must be strings")
        if right in {"type", "statement", "expr", "template"}:
            second = _expect(second, right, field)
        elif right == "string" and not isinstance(second, str):
            raise IrError(f"{field} second values must be strings")
        pairs.append([first, second])
    return pairs


def _strings(values: Iterable[str], field: str) -> list[str]:
    result = list(values)
    if not all(isinstance(value, str) for value in result):
        raise IrError(f"{field} must contain strings")
    return result


def _switch_arms(values: Iterable[Sequence[Any]]) -> list[list[Any]]:
    result: list[list[Any]] = []
    for pair in values:
        if len(pair) != 2:
            raise IrError("switch arms must contain literals and a body")
        result.append([_nodes(pair[0], "expr", "arm literals"), _nodes(pair[1], "statement", "arm body")])
    return result


def _enum(value: Any, ty: type[Enum], field: str) -> str:
    try:
        return ty(value).value
    except (TypeError, ValueError) as error:
        raise IrError(f"invalid {field}: {value!r}") from error


def _data(value: Any, seen: set[int]) -> Any:
    if isinstance(value, _Node):
        return value.to_data(seen)
    if hasattr(value, "to_data"):
        identity = id(value)
        if identity in seen:
            raise IrError("semantic IR must not contain cycles")
        seen.add(identity)
        try:
            return value.to_data(seen)
        finally:
            seen.remove(identity)
    if isinstance(value, list):
        return [_data(item, seen) for item in value]
    if isinstance(value, tuple):
        return [_data(item, seen) for item in value]
    if isinstance(value, dict):
        return {key: _data(item, seen) for key, item in value.items()}
    if isinstance(value, Enum):
        return value.value
    return value


class Type:
    @staticmethod
    def Unit() -> _Node: return _node("type", "unit")
    @staticmethod
    def Bool() -> _Node: return _node("type", "bool")
    @staticmethod
    def Int() -> _Node: return _node("type", "int")
    @staticmethod
    def Float() -> _Node: return _node("type", "float")
    @staticmethod
    def String() -> _Node: return _node("type", "string")
    @staticmethod
    def List(inner: _Node) -> _Node: return _node("type", "list", _expect(inner, "type", "inner"))
    @staticmethod
    def Set(inner: _Node) -> _Node: return _node("type", "set", _expect(inner, "type", "inner"))
    @staticmethod
    def Map(key: _Node, value: _Node) -> _Node:
        return _node("type", "map", [_expect(key, "type", "key"), _expect(value, "type", "value")])
    @staticmethod
    def Optional(inner: _Node) -> _Node: return _node("type", "optional", _expect(inner, "type", "inner"))
    @staticmethod
    def Tuple(parts: Iterable[_Node]) -> _Node: return _node("type", "tuple", _nodes(parts, "type", "parts"))
    @staticmethod
    def Named(name: str, args: Iterable[_Node] = ()) -> _Node:
        return _node("type", "named", {"name": name, "args": _nodes(args, "type", "args")})
    @staticmethod
    def Fn(params: Iterable[_Node], returns: _Node) -> _Node:
        return _node("type", "fn", {"params": _nodes(params, "type", "params"), "returns": _expect(returns, "type", "returns")})


@dataclass(frozen=True)
class Param:
    name: str
    ty: _Node | None = None
    default: _Node | None = None
    kind: ParamKind | str = ParamKind.NORMAL

    def __post_init__(self) -> None:
        _optional(self.ty, "type", "ty")
        _optional(self.default, "expr", "default")
        _enum(self.kind, ParamKind, "parameter kind")

    def to_data(self, _seen: set[int] | None = None) -> dict[str, Any]:
        data: dict[str, Any] = {"name": self.name}
        if self.ty is not None: data["ty"] = _data(self.ty, _seen or set())
        if self.default is not None: data["default"] = _data(self.default, _seen or set())
        kind = _enum(self.kind, ParamKind, "parameter kind")
        if kind != "normal": data["kind"] = kind
        return data


@dataclass(frozen=True)
class Function:
    name: str
    params: Sequence[Param] = ()
    returns: _Node | None = None
    body: Sequence[_Node] = ()
    doc: Sequence[str] = ()
    receiver: str | None = None
    receiver_binding: str | None = None
    exported: bool = False
    is_async: bool = False
    is_property: bool = False
    is_constructor: bool = False
    is_private: bool = False

    def __post_init__(self) -> None:
        if not all(isinstance(value, Param) for value in self.params): raise IrError("params must contain Param values")
        _optional(self.returns, "type", "returns")
        _nodes(self.body, "statement", "body")
        _strings(self.doc, "doc")

    def to_data(self, _seen: set[int] | None = None) -> dict[str, Any]:
        seen = _seen or set()
        data: dict[str, Any] = {"name": self.name}
        optional = (("doc", list(self.doc)), ("receiver", self.receiver), ("receiver_binding", self.receiver_binding),
                    ("params", list(self.params)), ("returns", self.returns), ("body", list(self.body)))
        for key, value in optional:
            if value not in (None, [], ()): data[key] = _data(value, seen)
        for key in ("exported", "is_async", "is_property", "is_constructor", "is_private"):
            value = getattr(self, key)
            if value: data[key] = True
        return data


@dataclass(frozen=True)
class Catch:
    binding: str | None
    ty: _Node | None
    body: Sequence[_Node]

    def __post_init__(self) -> None:
        _optional(self.ty, "type", "ty")
        _nodes(self.body, "statement", "body")

    def to_data(self, _seen: set[int] | None = None) -> dict[str, Any]:
        seen = _seen or set()
        return {"binding": self.binding, "ty": _data(self.ty, seen), "body": _data(list(self.body), seen)}


@dataclass(frozen=True)
class VariantArm:
    variant: str
    bindings: Sequence[Sequence[str]] = ()
    body: Sequence[_Node] = ()

    def __post_init__(self) -> None:
        _pairs(self.bindings, "string", "string", "bindings")
        _nodes(self.body, "statement", "body")

    def to_data(self, _seen: set[int] | None = None) -> dict[str, Any]:
        data: dict[str, Any] = {"variant": self.variant}
        if self.bindings: data["bindings"] = _pairs(self.bindings, "string", "string", "bindings")
        if self.body: data["body"] = _data(list(self.body), _seen or set())
        return data


class Expr:
    @staticmethod
    def Int(value: str | int) -> _Node: return _node("expr", "int", str(value))
    @staticmethod
    def Float(value: str | float) -> _Node: return _node("expr", "float", str(value))
    @staticmethod
    def Str(value: str) -> _Node: return _node("expr", "str", value)
    @staticmethod
    def Bool(value: bool) -> _Node: return _node("expr", "bool", value)
    @staticmethod
    def Null() -> _Node: return _node("expr", "null")
    @staticmethod
    def Name(value: str) -> _Node: return _node("expr", "name", value)
    @staticmethod
    def Field(of: _Node, name: str) -> _Node: return _node("expr", "field", {"of": _expect(of, "expr", "of"), "name": name})
    @staticmethod
    def Index(of: _Node, index: _Node) -> _Node: return _node("expr", "index", {"of": _expect(of, "expr", "of"), "index": _expect(index, "expr", "index")})
    @staticmethod
    def Call(callee: _Node, args: Iterable[_Node] = ()) -> _Node: return _node("expr", "call", {"callee": _expect(callee, "expr", "callee"), "args": _nodes(args, "expr", "args")})
    @staticmethod
    def Binary(op: BinaryOp | str, left: _Node, right: _Node) -> _Node: return _node("expr", "binary", {"op": _enum(op, BinaryOp, "binary operator"), "left": _expect(left, "expr", "left"), "right": _expect(right, "expr", "right")})
    @staticmethod
    def Unary(op: UnaryOp | str, operand: _Node) -> _Node: return _node("expr", "unary", {"op": _enum(op, UnaryOp, "unary operator"), "operand": _expect(operand, "expr", "operand")})
    @staticmethod
    def Await(value: _Node) -> _Node: return _node("expr", "await", _expect(value, "expr", "value"))
    @staticmethod
    def Propagate(value: _Node) -> _Node: return _node("expr", "propagate", _expect(value, "expr", "value"))
    @staticmethod
    def Keyword(name: str, value: _Node) -> _Node: return _node("expr", "keyword", {"name": name, "value": _expect(value, "expr", "value")})
    @staticmethod
    def Cast(ty: _Node, value: _Node) -> _Node: return _node("expr", "cast", {"ty": _expect(ty, "expr", "ty"), "value": _expect(value, "expr", "value")})
    @staticmethod
    def InstanceOf(value: _Node, ty: _Node) -> _Node: return _node("expr", "instance-of", {"value": _expect(value, "expr", "value"), "ty": _expect(ty, "expr", "ty")})
    @staticmethod
    def New(callee: _Node, args: Iterable[_Node] = ()) -> _Node: return _node("expr", "new", {"callee": _expect(callee, "expr", "callee"), "args": _nodes(args, "expr", "args")})
    @staticmethod
    def RecordLit(ty: str, fields: Iterable[Sequence[Any]] = ()) -> _Node: return _node("expr", "record-lit", {"ty": ty, "fields": _pairs(fields, "string", "expr", "fields")})
    @staticmethod
    def Coalesce(value: _Node, fallback: _Node) -> _Node: return _node("expr", "coalesce", {"value": _expect(value, "expr", "value"), "fallback": _expect(fallback, "expr", "fallback")})
    @staticmethod
    def Ternary(condition: _Node, then: _Node, otherwise: _Node) -> _Node: return _node("expr", "ternary", {"condition": _expect(condition, "expr", "condition"), "then": _expect(then, "expr", "then"), "otherwise": _expect(otherwise, "expr", "otherwise")})
    @staticmethod
    def Variant(sum: str, name: str, fields: Iterable[Sequence[Any]] = ()) -> _Node: return _node("expr", "variant", {"sum": sum, "name": name, "fields": _pairs(fields, "string", "expr", "fields")})
    @staticmethod
    def Tuple(values: Iterable[_Node]) -> _Node: return _node("expr", "tuple", _nodes(values, "expr", "values"))
    @staticmethod
    def ListLit(values: Iterable[_Node]) -> _Node: return _node("expr", "list-lit", _nodes(values, "expr", "values"))
    @staticmethod
    def MapLit(values: Iterable[Sequence[Any]]) -> _Node: return _node("expr", "map-lit", _pairs(values, "expr", "expr", "values"))
    @staticmethod
    def Template(values: Iterable[_Node]) -> _Node: return _node("expr", "template", _nodes(values, "template", "values"))
    @staticmethod
    def Lambda(params: Sequence[Param], body: _Node, returns: _Node | None = None) -> _Node:
        if not all(isinstance(value, Param) for value in params): raise IrError("params must contain Param values")
        return _node("expr", "lambda", {"params": list(params), "returns": _optional(returns, "type", "returns"), "body": _expect(body, "expr", "body")})
    @staticmethod
    def SetLit(values: Iterable[_Node]) -> _Node: return _node("expr", "set-lit", _nodes(values, "expr", "values"))
    @staticmethod
    def Comprehension(element: _Node, binding: str, iterable: _Node, condition: _Node | None = None) -> _Node: return _node("expr", "comprehension", {"element": _expect(element, "expr", "element"), "binding": binding, "iterable": _expect(iterable, "expr", "iterable"), "condition": _optional(condition, "expr", "condition")})


class TemplatePart:
    @staticmethod
    def Text(value: str) -> _Node: return _node("template", "text", value)
    @staticmethod
    def Expr(value: _Node) -> _Node: return _node("template", "expr", _expect(value, "expr", "value"))


class Stmt:
    @staticmethod
    def Return(value: _Node | None = None) -> _Node: return _node("statement", "return", _optional(value, "expr", "value"))
    @staticmethod
    def Let(name: str, ty: _Node | None = None, value: _Node | None = None, mutable: bool = False) -> _Node: return _node("statement", "let", {"name": name, "ty": _optional(ty, "type", "ty"), "value": _optional(value, "expr", "value"), "mutable": mutable})
    @staticmethod
    def Assign(target: _Node, value: _Node) -> _Node: return _node("statement", "assign", {"target": _expect(target, "expr", "target"), "value": _expect(value, "expr", "value")})
    @staticmethod
    def TupleAssign(names: Iterable[str], value: _Node, declares: bool = False) -> _Node: return _node("statement", "tuple-assign", {"names": _strings(names, "names"), "value": _expect(value, "expr", "value"), "declares": declares})
    @staticmethod
    def If(condition: _Node, then: Iterable[_Node], otherwise: Iterable[_Node] = ()) -> _Node: return _node("statement", "if", {"condition": _expect(condition, "expr", "condition"), "then": _nodes(then, "statement", "then"), "otherwise": _nodes(otherwise, "statement", "otherwise")})
    @staticmethod
    def IfPresent(binding: str, value: _Node, then: Iterable[_Node], otherwise: Iterable[_Node] = ()) -> _Node: return _node("statement", "if-present", {"binding": binding, "value": _expect(value, "expr", "value"), "then": _nodes(then, "statement", "then"), "otherwise": _nodes(otherwise, "statement", "otherwise")})
    @staticmethod
    def While(condition: _Node, body: Iterable[_Node]) -> _Node: return _node("statement", "while", {"condition": _expect(condition, "expr", "condition"), "body": _nodes(body, "statement", "body")})
    @staticmethod
    def CountedFor(body: Iterable[_Node], init: _Node | None = None, condition: _Node | None = None, update: _Node | None = None) -> _Node: return _node("statement", "counted-for", {"init": _optional(init, "statement", "init"), "condition": _optional(condition, "expr", "condition"), "update": _optional(update, "statement", "update"), "body": _nodes(body, "statement", "body")})
    @staticmethod
    def ForEachIndexed(index: str, binding: str, iterable: _Node, body: Iterable[_Node]) -> _Node: return _node("statement", "for-each-indexed", {"index": index, "binding": binding, "iterable": _expect(iterable, "expr", "iterable"), "body": _nodes(body, "statement", "body")})
    @staticmethod
    def Defer(body: Iterable[_Node]) -> _Node: return _node("statement", "defer", _nodes(body, "statement", "body"))
    @staticmethod
    def ErrDefer(body: Iterable[_Node]) -> _Node: return _node("statement", "err-defer", _nodes(body, "statement", "body"))
    @staticmethod
    def Switch(subject: _Node, arms: Iterable[Sequence[Any]], default: Iterable[_Node] = ()) -> _Node:
        return _node("statement", "switch", {"subject": _expect(subject, "expr", "subject"), "arms": _switch_arms(arms), "default": _nodes(default, "statement", "default")})
    @staticmethod
    def MatchVariants(subject: _Node, sum: str, arms: Sequence[VariantArm], default: Iterable[_Node] = ()) -> _Node:
        if not all(isinstance(value, VariantArm) for value in arms): raise IrError("arms must contain VariantArm values")
        return _node("statement", "match-variants", {"subject": _expect(subject, "expr", "subject"), "sum": sum, "arms": list(arms), "default": _nodes(default, "statement", "default")})
    @staticmethod
    def WhilePresent(binding: str, value: _Node, body: Iterable[_Node]) -> _Node: return _node("statement", "while-present", {"binding": binding, "value": _expect(value, "expr", "value"), "body": _nodes(body, "statement", "body")})
    @staticmethod
    def ForEach(binding: str, iterable: _Node, body: Iterable[_Node]) -> _Node: return _node("statement", "for-each", {"binding": binding, "iterable": _expect(iterable, "expr", "iterable"), "body": _nodes(body, "statement", "body")})
    @staticmethod
    def Expr(value: _Node) -> _Node: return _node("statement", "expr", _expect(value, "expr", "value"))
    @staticmethod
    def Assert(condition: _Node, message: _Node | None = None) -> _Node: return _node("statement", "assert", {"condition": _expect(condition, "expr", "condition"), "message": _optional(message, "expr", "message")})
    @staticmethod
    def Comment(value: str) -> _Node: return _node("statement", "comment", value)
    @staticmethod
    def LocalFunction(value: Function) -> _Node:
        if not isinstance(value, Function): raise IrError("value must be a Function")
        return _node("statement", "local-function", value)
    @staticmethod
    def Block(body: Iterable[_Node]) -> _Node: return _node("statement", "block", _nodes(body, "statement", "body"))
    @staticmethod
    def Throw(value: _Node) -> _Node: return _node("statement", "throw", _expect(value, "expr", "value"))
    @staticmethod
    def Try(body: Iterable[_Node], catches: Sequence[Catch] = (), finally_: Iterable[_Node] = ()) -> _Node:
        if not all(isinstance(value, Catch) for value in catches): raise IrError("catches must contain Catch values")
        return _node("statement", "try", {"body": _nodes(body, "statement", "body"), "catches": list(catches), "finally": _nodes(finally_, "statement", "finally")})
    @staticmethod
    def Break() -> _Node: return _node("statement", "break")
    @staticmethod
    def BreakWith(label: str, value: _Node | None = None) -> _Node: return _node("statement", "break-with", {"label": label, "value": _optional(value, "expr", "value")})
    @staticmethod
    def Continue() -> _Node: return _node("statement", "continue")


@dataclass(frozen=True)
class SemanticBody:
    body: Sequence[_Node]

    def __post_init__(self) -> None:
        _nodes(self.body, "statement", "body")

    def to_data(self) -> dict[str, Any]:
        return {"schema": SCHEMA, "body": _data(list(self.body), set())}

    def to_json(self, *, indent: int | None = None) -> str:
        return json.dumps(self.to_data(), ensure_ascii=False, indent=indent, separators=None if indent else (",", ":"))

    def write(self, path: str | Path, *, indent: int | None = 2) -> None:
        Path(path).write_text(self.to_json(indent=indent) + "\n", encoding="utf-8")


__all__ = [
    "BinaryOp", "Catch", "EXPRESSION_KINDS", "Expr", "Function", "IrError", "Param",
    "ExpressionNode", "ParamKind", "SCHEMA", "STATEMENT_KINDS", "SemanticBody", "StatementNode",
    "Stmt", "TEMPLATE_KINDS", "TYPE_KINDS", "TemplateNode", "TemplatePart", "Type", "TypeNode",
    "UnaryOp", "VariantArm",
]
