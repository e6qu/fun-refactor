"""Source-free constructors for ``fr-semantic-body-1``.

The public namespaces mirror the Rust IR: ``Type``, ``Stmt``, ``Expr`` and
``TemplatePart``. Constructors reject nodes from another category immediately.
"""

from __future__ import annotations

from dataclasses import dataclass
from enum import Enum
import hashlib
import json
from pathlib import Path
import re
from typing import Any, Callable, Iterable, Mapping, Sequence

SCHEMA = "fr-semantic-body-1"
CHANGE_SCHEMA = "fr-semantic-change-1"
INTENT_SCHEMA = "fr-semantic-intent-1"
DISCLOSED_EDIT_SCHEMA = "fr-disclosed-edit-1"
DISCLOSED_IR_EDIT_SCHEMA = "fr-disclosed-ir-edit-1"
MERKLE_PROOF_SCHEMA = "fr-merkle-inclusion-1"
MERKLE_OBJECT_SCHEMA = "fr-merkle-object-1"
FORMAL_PLAN_SCHEMA = "fr-formal-plan-1"
PROOF_TASK_SCHEMA = "fr-proof-task-1"
PROOF_ATTEMPT_SCHEMA = "fr-proof-attempt-1"
PROPERTY_TASK_SCHEMA = "fr-property-task-1"
AGENT_PROPERTY_SCHEMA = "fr-formal-property-1"
TASK_CHANGE_SCHEMA = "fr-task-change-1"
_ABSENT = object()

TYPE_KINDS = ("unit", "bool", "int", "float", "string", "list", "set", "map", "optional", "tuple", "named", "fn")
STATEMENT_KINDS = ("return", "let", "assign", "tuple-assign", "if", "if-present", "while", "counted-for", "for-each-indexed", "defer", "err-defer", "switch", "match-variants", "while-present", "for-each", "expr", "assert", "comment", "local-function", "block", "throw", "try", "break", "break-with", "continue")
EXPRESSION_KINDS = ("int", "float", "str", "bool", "null", "name", "field", "index", "call", "binary", "unary", "await", "propagate", "keyword", "cast", "instance-of", "new", "record-lit", "coalesce", "ternary", "variant", "tuple", "list-lit", "map-lit", "template", "lambda", "set-lit", "comprehension")
TEMPLATE_KINDS = ("text", "expr")


class IrError(ValueError):
    """The requested value does not belong to the semantic IR contract."""


def _tagged_hash(*parts: Any) -> str:
    encoded = json.dumps(parts, ensure_ascii=False, separators=(",", ":"),
                         allow_nan=False).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


def _is_digest(value: Any) -> bool:
    return (isinstance(value, str) and len(value) == 64
            and all(character in "0123456789abcdef" for character in value))


def _object_binary_root(level: Sequence[str]) -> str:
    current = list(level)
    if not current:
        return _tagged_hash(MERKLE_OBJECT_SCHEMA, "empty")
    while len(current) > 1:
        following = []
        for position in range(0, len(current), 2):
            if position + 1 == len(current):
                following.append(current[position])
            else:
                following.append(_tagged_hash(
                    MERKLE_OBJECT_SCHEMA, "pair", current[position], current[position + 1]
                ))
        current = following
    return current[0]


def merkle_object_digest(value: Any) -> str:
    """Return the content address for one JSON value."""
    if value is None:
        return _tagged_hash(MERKLE_OBJECT_SCHEMA, "null")
    if isinstance(value, bool):
        return _tagged_hash(MERKLE_OBJECT_SCHEMA, "bool", value)
    if isinstance(value, (int, float)):
        if isinstance(value, float) and (value != value or value in (float("inf"), float("-inf"))):
            raise IrError("Merkle numbers must be finite JSON numbers")
        written = json.dumps(value, ensure_ascii=False, separators=(",", ":"))
        return _tagged_hash(MERKLE_OBJECT_SCHEMA, "number", written)
    if isinstance(value, str):
        return _tagged_hash(MERKLE_OBJECT_SCHEMA, "string", value)
    if isinstance(value, list):
        entries = [
            _tagged_hash(MERKLE_OBJECT_SCHEMA, "array-entry", index,
                         merkle_object_digest(child))
            for index, child in enumerate(value)
        ]
        return _tagged_hash(MERKLE_OBJECT_SCHEMA, "array", len(value),
                            _object_binary_root(entries))
    if isinstance(value, dict):
        if not all(isinstance(key, str) for key in value):
            raise IrError("Merkle object keys must be strings")
        entries = [
            _tagged_hash(MERKLE_OBJECT_SCHEMA, "object-entry", index, key,
                         merkle_object_digest(value[key]))
            for index, key in enumerate(sorted(value))
        ]
        return _tagged_hash(MERKLE_OBJECT_SCHEMA, "object", len(value),
                            _object_binary_root(entries))
    raise IrError(f"{type(value).__name__} is not a JSON value")


def merkle_object_pack(value: Any) -> dict[str, Any]:
    """Split a JSON tree into deduplicated objects suitable for object storage."""
    objects: dict[str, Any] = {}

    def visit(node: Any) -> str:
        digest = merkle_object_digest(node)
        if digest in objects:
            return digest
        if isinstance(node, list):
            record = {
                "schema": MERKLE_OBJECT_SCHEMA,
                "kind": "array",
                "children": [visit(child) for child in node],
            }
        elif isinstance(node, dict):
            record = {
                "schema": MERKLE_OBJECT_SCHEMA,
                "kind": "object",
                "entries": [[key, visit(node[key])] for key in sorted(node)],
            }
        else:
            record = {"schema": MERKLE_OBJECT_SCHEMA, "kind": "scalar", "value": node}
        objects[digest] = record
        return digest

    root = visit(value)
    return {
        "schema": "fr-merkle-object-pack-1",
        "algorithm": "sha256-tagged-binary-json-tree",
        "root": root,
        "objects": objects,
    }


def restore_merkle_object(
    root: str,
    objects: Mapping[str, Any] | Callable[[str], Any],
) -> Any:
    """Fetch, restore and verify a JSON tree rooted at one content digest."""
    active: set[str] = set()
    cache: dict[str, Any] = {}

    def fetch(digest: str) -> Any:
        if digest in cache:
            return cache[digest]
        record = objects(digest) if callable(objects) else objects.get(digest)
        cache[digest] = record
        return record

    def restore(digest: str) -> Any:
        if not _is_digest(digest):
            raise IrError("Merkle object references must be lowercase SHA-256 digests")
        if digest in active:
            raise IrError("Merkle object graph contains a cycle")
        record = fetch(digest)
        if not isinstance(record, Mapping) or record.get("schema") != MERKLE_OBJECT_SCHEMA:
            raise IrError(f"Merkle object {digest} is absent or malformed")
        active.add(digest)
        kind = record.get("kind")
        if kind == "scalar" and set(record) == {"schema", "kind", "value"}:
            value = record["value"]
        elif kind == "array" and isinstance(record.get("children"), list):
            value = [restore(child) for child in record["children"]]
        elif kind == "object" and isinstance(record.get("entries"), list):
            entries = record["entries"]
            if not all(isinstance(entry, list) and len(entry) == 2
                       and isinstance(entry[0], str) and isinstance(entry[1], str)
                       for entry in entries):
                raise IrError(f"Merkle object {digest} has malformed entries")
            keys = [entry[0] for entry in entries]
            if keys != sorted(set(keys)):
                raise IrError(f"Merkle object {digest} keys are not canonical")
            value = {key: restore(child) for key, child in entries}
        else:
            raise IrError(f"Merkle object {digest} has an unsupported shape")
        active.remove(digest)
        if merkle_object_digest(value) != digest:
            raise IrError(f"Merkle object {digest} fails content verification")
        return value

    return restore(root)


def verify_disclosure_commitment(digest: str, proof: Mapping[str, Any]) -> bool:
    """Verify a revealed subtree commitment against its disclosure root."""
    if (
        not isinstance(proof, Mapping)
        or proof.get("schema") != MERKLE_PROOF_SCHEMA
        or proof.get("algorithm") != "sha256-tagged-binary-json-tree"
        or not isinstance(proof.get("path"), list)
    ):
        return False
    current = digest
    if not _is_digest(current) or not _is_digest(proof.get("root")):
        return False
    if proof.get("leaf") != current:
        return False
    for step in proof["path"]:
        if not isinstance(step, Mapping) or step.get("container") not in ("array", "object"):
            return False
        index, length, branch = step.get("index"), step.get("length"), step.get("branch")
        if (
            isinstance(index, bool) or not isinstance(index, int)
            or isinstance(length, bool) or not isinstance(length, int)
            or not 0 <= index < length
            or not isinstance(branch, list)
        ):
            return False
        if step["container"] == "array":
            entry = _tagged_hash(MERKLE_OBJECT_SCHEMA, "array-entry", index, current)
        else:
            key = step.get("key")
            if not isinstance(key, str):
                return False
            entry = _tagged_hash(MERKLE_OBJECT_SCHEMA, "object-entry", index, key, current)
        position, width = index, length
        for sibling in branch:
            if not isinstance(sibling, Mapping) or width <= 1:
                return False
            expected = "left" if position % 2 else ("right" if position + 1 < width else "promote")
            if sibling.get("side") != expected:
                return False
            if expected == "promote":
                if set(sibling) != {"side"}:
                    return False
            else:
                digest = sibling.get("digest")
                if not _is_digest(digest):
                    return False
                entry = (_tagged_hash(MERKLE_OBJECT_SCHEMA, "pair", digest, entry)
                         if expected == "left" else
                         _tagged_hash(MERKLE_OBJECT_SCHEMA, "pair", entry, digest))
            position //= 2
            width = (width + 1) // 2
        if width != 1:
            return False
        current = _tagged_hash(MERKLE_OBJECT_SCHEMA, step["container"], length, entry)
    return proof["root"] == current


def verify_disclosure_proof(value: Any, proof: Mapping[str, Any]) -> bool:
    """Hash one complete revealed JSON value and verify its disclosure proof."""
    return verify_disclosure_commitment(merkle_object_digest(value), proof)


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


class NodeCategory(str, Enum):
    TYPE = "type"
    STATEMENT = "statement"
    EXPRESSION = "expression"
    TEMPLATE = "template"


class Role(str, Enum):
    STATEMENT = "statement"
    RESULT = "result"
    ANNOTATION = "annotation"
    INITIALIZER = "initializer"
    ASSIGNMENT_TARGET = "assignment-target"
    ASSIGNMENT_VALUE = "assignment-value"
    CONDITION = "condition"
    THEN_STATEMENT = "then-statement"
    ELSE_STATEMENT = "else-statement"
    BODY_STATEMENT = "body-statement"
    FINALLY_STATEMENT = "finally-statement"
    ITERABLE = "iterable"
    SUBJECT = "subject"
    EXPRESSION = "expression"
    MESSAGE = "message"
    CALLEE = "callee"
    ARGUMENT = "argument"
    RECEIVER = "receiver"
    INDEX = "index"
    LEFT = "left"
    RIGHT = "right"
    OPERAND = "operand"
    VALUE = "value"
    FALLBACK = "fallback"
    THEN_EXPRESSION = "then-expression"
    ELSE_EXPRESSION = "else-expression"
    ELEMENT = "element"
    TEMPLATE_PART = "template-part"
    TEMPLATE_EXPRESSION = "template-expression"
    LAMBDA_BODY = "lambda-body"
    COMPREHENSION_ELEMENT = "comprehension-element"
    COMPREHENSION_CONDITION = "comprehension-condition"
    TYPE_EXPRESSION = "type-expression"
    INNER_TYPE = "inner-type"
    MAP_KEY_TYPE = "map-key-type"
    MAP_VALUE_TYPE = "map-value-type"
    TUPLE_TYPE = "tuple-type"
    TYPE_ARGUMENT = "type-argument"
    PARAMETER_TYPE = "parameter-type"
    RETURN_TYPE = "return-type"


ROLE_NAMES = tuple(role.value for role in Role)
INTENT_OPERATIONS = (
    "set-int", "set-float", "set-string", "set-bool", "set-name", "set-field-name",
    "set-keyword-name", "set-binary-operator", "set-unary-operator", "set-template-text",
    "set-comment",
)


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

    def basis(self) -> str:
        canonical = self.to_json().encode("utf-8")
        return "frsb1:" + hashlib.sha256(canonical).hexdigest()

    def write(self, path: str | Path, *, indent: int | None = 2) -> None:
        Path(path).write_text(self.to_json(indent=indent) + "\n", encoding="utf-8")


def _pointer(path: str, *, allow_body: bool) -> str:
    if not isinstance(path, str) or len(path.encode("utf-8")) > 1024:
        raise IrError("semantic path must be a string of at most 1024 bytes")
    if not path.startswith("/") or (not allow_body and not path.startswith("/body/")):
        raise IrError("semantic path must be below /body")
    if allow_body and path != "/body" and not path.startswith("/body/"):
        raise IrError("semantic path must be at or below /body")
    parts = path[1:].split("/")
    if len(parts) > 64 or any(not part for part in parts):
        raise IrError("semantic path must contain 1 through 64 nonempty segments")
    for part in parts:
        index = 0
        while index < len(part):
            if part[index] == "~":
                if index + 1 == len(part) or part[index + 1] not in "01":
                    raise IrError("semantic path must use canonical RFC 6901 escapes")
                index += 1
            index += 1
    return path


@dataclass(frozen=True)
class _ChangeOperation:
    op: str
    fields: Mapping[str, Any]

    def to_data(self, _seen: set[int] | None = None) -> dict[str, Any]:
        seen = set() if _seen is None else _seen
        return {"op": self.op, **{key: _data(value, seen) for key, value in self.fields.items()}}


class Change:
    @staticmethod
    def Replace(path: str, value: _Node) -> _ChangeOperation:
        if not isinstance(value, _Node):
            raise IrError("semantic replacement value must be a typed IR node")
        category = "expression" if value.category == "expr" else value.category
        return _ChangeOperation("replace", {
            "path": _pointer(path, allow_body=False), "category": category, "value": value
        })

    @staticmethod
    def InsertStatement(path: str, index: int, value: _Node) -> _ChangeOperation:
        if isinstance(index, bool) or not isinstance(index, int) or index < 0:
            raise IrError("semantic insertion index must be a nonnegative integer")
        return _ChangeOperation("insert-statement", {
            "path": _pointer(path, allow_body=True), "index": index,
            "value": _expect(value, "statement", "value")
        })

    @staticmethod
    def DeleteStatement(path: str) -> _ChangeOperation:
        return _ChangeOperation("delete-statement", {"path": _pointer(path, allow_body=False)})


@dataclass(frozen=True)
class SemanticChange:
    base: str | SemanticBody
    operations: Sequence[_ChangeOperation]

    def __post_init__(self) -> None:
        base = self.base.basis() if isinstance(self.base, SemanticBody) else self.base
        valid_base = isinstance(base, str) and base.startswith("frsb1:") and len(base) == 70
        if not valid_base or any(character not in "0123456789abcdefABCDEF" for character in base[6:]):
            raise IrError("semantic change base must be an frsb1 SHA-256 identity")
        if not 1 <= len(self.operations) <= 64:
            raise IrError("semantic change needs 1 through 64 operations")
        if not all(isinstance(operation, _ChangeOperation) for operation in self.operations):
            raise IrError("operations must contain Change values")

    def to_data(self) -> dict[str, Any]:
        base = self.base.basis() if isinstance(self.base, SemanticBody) else self.base
        return {"schema": CHANGE_SCHEMA, "base": base,
                "operations": [operation.to_data(set()) for operation in self.operations]}

    def to_json(self, *, indent: int | None = None) -> str:
        return json.dumps(self.to_data(), ensure_ascii=False, indent=indent,
                          separators=None if indent else (",", ":"))

    def write(self, path: str | Path, *, indent: int | None = 2) -> None:
        Path(path).write_text(self.to_json(indent=indent) + "\n", encoding="utf-8")


@dataclass(frozen=True)
class LocatorStep:
    role: Role | str
    index: int | None = None
    category: NodeCategory | str | None = None
    kind: str | None = None
    label: str | None = None

    def __post_init__(self) -> None:
        _enum(self.role, Role, "semantic role")
        if self.index is not None and (
            isinstance(self.index, bool) or not isinstance(self.index, int) or self.index < 0
        ):
            raise IrError("semantic locator index must be a nonnegative integer")
        if self.category is not None:
            _enum(self.category, NodeCategory, "node category")
        for value, field in ((self.kind, "kind"), (self.label, "label")):
            if value is not None and not isinstance(value, str):
                raise IrError(f"semantic locator {field} must be a string")

    def to_data(self, _seen: set[int] | None = None) -> dict[str, Any]:
        data: dict[str, Any] = {"role": _enum(self.role, Role, "semantic role")}
        if self.index is not None:
            data["index"] = self.index
        if self.category is not None:
            data["category"] = _enum(self.category, NodeCategory, "node category")
        if self.kind is not None:
            data["kind"] = self.kind
        if self.label is not None:
            data["label"] = self.label
        return data


def _portable_integer(value: str) -> bool:
    return value == "0" or bool(value) and value[0] in "123456789" and value.isascii() and value.isdigit()


def _portable_float(value: str) -> bool:
    parts = value.split(".")
    return len(parts) == 2 and _portable_integer(parts[0]) and bool(parts[1]) and parts[1].isascii() and parts[1].isdigit()


def _portable_name(value: str) -> bool:
    return (
        bool(value)
        and len(value.encode("utf-8")) <= 128
        and value.isascii()
        and (value[0].isalpha() or value[0] == "_")
        and all(character.isalnum() or character == "_" for character in value)
    )


def _intent_steps(target: Iterable[LocatorStep]) -> list[LocatorStep]:
    result = list(target)
    if not 1 <= len(result) <= 64:
        raise IrError("semantic intent locator needs 1 through 64 role steps")
    if not all(isinstance(step, LocatorStep) for step in result):
        raise IrError("semantic intent target must contain LocatorStep values")
    return result


@dataclass(frozen=True)
class _IntentOperation:
    op: str
    target: Sequence[LocatorStep]
    before: Any
    after: Any

    def to_data(self, _seen: set[int] | None = None) -> dict[str, Any]:
        return {
            "op": self.op,
            "target": [step.to_data() for step in self.target],
            "from": _data(self.before, set()),
            "to": _data(self.after, set()),
        }


def _intent_scalar_values(op: str, before: Any, after: Any) -> tuple[Any, Any]:
    if before == after and type(before) is type(after):
        raise IrError("semantic intent operation must change its scalar")
    string_operations = {"set-string", "set-template-text", "set-comment"}
    name_operations = {"set-name", "set-field-name", "set-keyword-name"}
    if op == "set-int":
        if not all(isinstance(value, str) and _portable_integer(value) for value in (before, after)):
            raise IrError("set-int needs portable decimal integer strings")
    elif op == "set-float":
        if not all(isinstance(value, str) and _portable_float(value) for value in (before, after)):
            raise IrError("set-float needs portable decimal strings with a fractional part")
    elif op == "set-bool":
        if not all(type(value) is bool for value in (before, after)):
            raise IrError("set-bool needs Boolean values")
    elif op in string_operations:
        if not all(isinstance(value, str) for value in (before, after)):
            raise IrError(f"{op} needs string values")
    elif op in name_operations:
        if not all(isinstance(value, str) and _portable_name(value) for value in (before, after)):
            raise IrError(f"{op} needs portable identifiers")
    elif op == "set-binary-operator":
        before = _enum(before, BinaryOp, "binary operator")
        after = _enum(after, BinaryOp, "binary operator")
    elif op == "set-unary-operator":
        before = _enum(before, UnaryOp, "unary operator")
        after = _enum(after, UnaryOp, "unary operator")
    else:
        raise IrError(f"unknown semantic intent operation: {op}")
    return before, after


def _intent_scalar(
    op: str,
    target: Iterable[LocatorStep],
    before: Any,
    after: Any,
) -> _IntentOperation:
    before, after = _intent_scalar_values(op, before, after)
    return _IntentOperation(op, _intent_steps(target), before, after)


class Intent:
    @staticmethod
    def SetInt(target: Iterable[LocatorStep], before: str, after: str) -> _IntentOperation:
        return _intent_scalar("set-int", target, before, after)

    @staticmethod
    def SetFloat(target: Iterable[LocatorStep], before: str, after: str) -> _IntentOperation:
        return _intent_scalar("set-float", target, before, after)

    @staticmethod
    def SetString(target: Iterable[LocatorStep], before: str, after: str) -> _IntentOperation:
        return _intent_scalar("set-string", target, before, after)

    @staticmethod
    def SetBool(target: Iterable[LocatorStep], before: bool, after: bool) -> _IntentOperation:
        return _intent_scalar("set-bool", target, before, after)

    @staticmethod
    def SetName(target: Iterable[LocatorStep], before: str, after: str) -> _IntentOperation:
        return _intent_scalar("set-name", target, before, after)

    @staticmethod
    def SetFieldName(target: Iterable[LocatorStep], before: str, after: str) -> _IntentOperation:
        return _intent_scalar("set-field-name", target, before, after)

    @staticmethod
    def SetKeywordName(target: Iterable[LocatorStep], before: str, after: str) -> _IntentOperation:
        return _intent_scalar("set-keyword-name", target, before, after)

    @staticmethod
    def SetBinaryOperator(target: Iterable[LocatorStep], before: BinaryOp | str, after: BinaryOp | str) -> _IntentOperation:
        return _intent_scalar("set-binary-operator", target, before, after)

    @staticmethod
    def SetUnaryOperator(target: Iterable[LocatorStep], before: UnaryOp | str, after: UnaryOp | str) -> _IntentOperation:
        return _intent_scalar("set-unary-operator", target, before, after)

    @staticmethod
    def SetTemplateText(target: Iterable[LocatorStep], before: str, after: str) -> _IntentOperation:
        return _intent_scalar("set-template-text", target, before, after)

    @staticmethod
    def SetComment(target: Iterable[LocatorStep], before: str, after: str) -> _IntentOperation:
        return _intent_scalar("set-comment", target, before, after)


@dataclass(frozen=True)
class ScalarRequest:
    operation: str
    before: Any
    after: Any

    def __post_init__(self) -> None:
        before, after = _intent_scalar_values(self.operation, self.before, self.after)
        object.__setattr__(self, "before", before)
        object.__setattr__(self, "after", after)

    def to_data(self) -> dict[str, Any]:
        return {
            "operation": self.operation,
            "from": _data(self.before, set()),
            "to": _data(self.after, set()),
        }

    def to_json(self, *, indent: int | None = None) -> str:
        return json.dumps(self.to_data(), ensure_ascii=False, indent=indent,
                          separators=None if indent else (",", ":"))

    def write(self, path: str | Path, *, indent: int | None = 2) -> None:
        Path(path).write_text(self.to_json(indent=indent) + "\n", encoding="utf-8")


@dataclass(frozen=True)
class DisclosedEditRequest:
    """Exact opaque scalar capability input for author, task and batch manifests."""

    edit: str
    to: str

    def __post_init__(self) -> None:
        digest = self.edit.removeprefix("frde1:") if isinstance(self.edit, str) else ""
        if (
            not isinstance(self.edit, str)
            or not self.edit.startswith("frde1:")
            or len(digest) != 64
            or any(character not in "0123456789abcdef" for character in digest)
        ):
            raise IrError("disclosed edit must be an exact frde1 SHA-256 identity")
        if not isinstance(self.to, str):
            raise IrError("disclosed edit replacement must be a string CLI scalar")

    def to_data(self) -> dict[str, Any]:
        return {"edit": self.edit, "to": self.to}

    def to_json(self, *, indent: int | None = None) -> str:
        return json.dumps(self.to_data(), ensure_ascii=False, indent=indent,
                          separators=None if indent else (",", ":"))


@dataclass(frozen=True)
class DisclosedIrEditRequest:
    """Opaque structural capability plus an optional typed IR replacement."""

    edit: str
    value: _Node | None = None

    def __post_init__(self) -> None:
        digest = self.edit.removeprefix("frdi1:") if isinstance(self.edit, str) else ""
        if (
            not isinstance(self.edit, str)
            or not self.edit.startswith("frdi1:")
            or len(digest) != 64
            or any(character not in "0123456789abcdef" for character in digest)
        ):
            raise IrError("disclosed IR edit must be an exact frdi1 SHA-256 identity")
        if self.value is not None and not isinstance(self.value, _Node):
            raise IrError("disclosed IR edit value must be a typed IR node or None")

    def to_data(self) -> dict[str, Any]:
        data: dict[str, Any] = {"edit": self.edit}
        if self.value is not None:
            data["value"] = self.value.to_data()
        return data

    def to_json(self, *, indent: int | None = None) -> str:
        return json.dumps(self.to_data(), ensure_ascii=False, indent=indent,
                          separators=None if indent else (",", ":"))


def _semantic_basis(value: str | SemanticBody, description: str) -> str:
    base = value.basis() if isinstance(value, SemanticBody) else value
    valid = isinstance(base, str) and base.startswith("frsb1:") and len(base) == 70
    if not valid or any(character not in "0123456789abcdefABCDEF" for character in base[6:]):
        raise IrError(f"{description} base must be an frsb1 SHA-256 identity")
    return base


@dataclass(frozen=True)
class SemanticIntent:
    base: str | SemanticBody
    operations: Sequence[_IntentOperation]

    def __post_init__(self) -> None:
        _semantic_basis(self.base, "semantic intent")
        if not 1 <= len(self.operations) <= 64:
            raise IrError("semantic intent needs 1 through 64 operations")
        if not all(isinstance(operation, _IntentOperation) for operation in self.operations):
            raise IrError("operations must contain Intent values")

    def to_data(self) -> dict[str, Any]:
        return {
            "schema": INTENT_SCHEMA,
            "base": _semantic_basis(self.base, "semantic intent"),
            "operations": [operation.to_data() for operation in self.operations],
        }

    def to_json(self, *, indent: int | None = None) -> str:
        return json.dumps(self.to_data(), ensure_ascii=False, indent=indent,
                          separators=None if indent else (",", ":"))

    def write(self, path: str | Path, *, indent: int | None = 2) -> None:
        Path(path).write_text(self.to_json(indent=indent) + "\n", encoding="utf-8")


def _exact_mapping(value: Any, keys: set[str], description: str) -> Mapping[str, Any]:
    if not isinstance(value, Mapping) or set(value) != keys:
        raise IrError(f"{description} must contain exactly {sorted(keys)}")
    return value


@dataclass(frozen=True)
class FormalBinding:
    """One Rust-to-Lean name and type mapping in a formal plan."""

    name: str
    rust_type: str
    lean_type: str

    @classmethod
    def from_data(cls, value: Any) -> FormalBinding:
        value = _exact_mapping(value, {"name", "rust_type", "lean_type"}, "formal binding")
        if not all(isinstance(value[key], str) and value[key] for key in value):
            raise IrError("formal binding fields must be non-empty strings")
        return cls(value["name"], value["rust_type"], value["lean_type"])

    def to_data(self) -> dict[str, str]:
        return {"name": self.name, "rust_type": self.rust_type, "lean_type": self.lean_type}


class PropertyTerm:
    """Source-free constructors for the agent property term IR."""

    @staticmethod
    def variable(name: str) -> dict[str, Any]:
        return {"kind": "variable", "name": name}

    @staticmethod
    def model(*arguments: Mapping[str, Any]) -> dict[str, Any]:
        return {"kind": "model", "arguments": [dict(item) for item in arguments]}

    @staticmethod
    def boolean(value: bool) -> dict[str, Any]:
        return {"kind": "boolean", "value": value}

    @staticmethod
    def integer(value: int, lean_type: str) -> dict[str, Any]:
        return {"kind": "integer", "value": value, "lean_type": lean_type}

    @staticmethod
    def unary(operator: str, operand: Mapping[str, Any]) -> dict[str, Any]:
        return {"kind": "unary", "operator": operator, "operand": dict(operand)}

    @staticmethod
    def binary(operator: str, left: Mapping[str, Any], right: Mapping[str, Any]) -> dict[str, Any]:
        return {"kind": "binary", "operator": operator, "left": dict(left), "right": dict(right)}

    @staticmethod
    def if_(condition: Mapping[str, Any], then: Mapping[str, Any], otherwise: Mapping[str, Any]) -> dict[str, Any]:
        return {"kind": "if", "condition": dict(condition), "then": dict(then), "otherwise": dict(otherwise)}


class PropertyProposition:
    """Source-free constructors for the agent proposition IR."""

    @staticmethod
    def relation(kind: str, left: Mapping[str, Any], right: Mapping[str, Any]) -> dict[str, Any]:
        return {"kind": kind, "left": dict(left), "right": dict(right)}

    @staticmethod
    def equals(left: Mapping[str, Any], right: Mapping[str, Any]) -> dict[str, Any]:
        return PropertyProposition.relation("equals", left, right)

    @staticmethod
    def not_equals(left: Mapping[str, Any], right: Mapping[str, Any]) -> dict[str, Any]:
        return PropertyProposition.relation("not-equals", left, right)

    @staticmethod
    def less_than(left: Mapping[str, Any], right: Mapping[str, Any]) -> dict[str, Any]:
        return PropertyProposition.relation("less-than", left, right)

    @staticmethod
    def less_or_equal(left: Mapping[str, Any], right: Mapping[str, Any]) -> dict[str, Any]:
        return PropertyProposition.relation("less-or-equal", left, right)

    @staticmethod
    def greater_than(left: Mapping[str, Any], right: Mapping[str, Any]) -> dict[str, Any]:
        return PropertyProposition.relation("greater-than", left, right)

    @staticmethod
    def greater_or_equal(left: Mapping[str, Any], right: Mapping[str, Any]) -> dict[str, Any]:
        return PropertyProposition.relation("greater-or-equal", left, right)

    @staticmethod
    def holds(term: Mapping[str, Any]) -> dict[str, Any]:
        return {"kind": "holds", "term": dict(term)}

    @staticmethod
    def not_(proposition: Mapping[str, Any]) -> dict[str, Any]:
        return {"kind": "not", "proposition": dict(proposition)}

    @staticmethod
    def all_of(*propositions: Mapping[str, Any]) -> dict[str, Any]:
        return {"kind": "and", "propositions": [dict(item) for item in propositions]}

    @staticmethod
    def any_of(*propositions: Mapping[str, Any]) -> dict[str, Any]:
        return {"kind": "or", "propositions": [dict(item) for item in propositions]}

    @staticmethod
    def implies(premise: Mapping[str, Any], conclusion: Mapping[str, Any]) -> dict[str, Any]:
        return {"kind": "implies", "premise": dict(premise), "conclusion": dict(conclusion)}


_LEAN_AGENT_NAME = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*$")
_LEAN_RESERVED = {
    "axiom", "by", "def", "else", "end", "false", "if", "in", "let", "match",
    "namespace", "then", "theorem", "true",
}


def _agent_name(value: Any, description: str) -> str:
    if not isinstance(value, str) or not _LEAN_AGENT_NAME.fullmatch(value) or value in _LEAN_RESERVED:
        raise IrError(f"{description} must be a safe Lean identifier")
    return value


def _property_node(counter: list[int], depth: int) -> None:
    counter[0] += 1
    if counter[0] > 64:
        raise IrError("agent property exceeds the 64-node ceiling")
    if depth > 16:
        raise IrError("agent property exceeds the 16-level depth ceiling")


def _property_term_type(
    value: Any,
    kernel: Mapping[str, Any],
    parameters: Mapping[str, str],
    counter: list[int],
    depth: int,
) -> str:
    _property_node(counter, depth)
    if not isinstance(value, Mapping) or not isinstance(value.get("kind"), str):
        raise IrError("agent property term must be a tagged object")
    kind = value["kind"]
    if kind == "variable":
        value = _exact_mapping(value, {"kind", "name"}, "property variable")
        name = value["name"]
        if name not in parameters:
            raise IrError(f"agent property variable {name!r} is not a parameter")
        return parameters[name]
    if kind == "model":
        value = _exact_mapping(value, {"kind", "arguments"}, "property model call")
        if not isinstance(value["arguments"], list) or len(value["arguments"]) != len(kernel["inputs"]):
            raise IrError("agent property model call has the wrong arity")
        for argument, expected in zip(value["arguments"], kernel["inputs"]):
            if _property_term_type(argument, kernel, parameters, counter, depth + 1) != expected["lean_type"]:
                raise IrError("agent property model argument type does not match its input")
        return kernel["output"]["lean_type"]
    if kind == "boolean":
        value = _exact_mapping(value, {"kind", "value"}, "property Boolean")
        if not isinstance(value["value"], bool):
            raise IrError("agent property Boolean must contain a bool")
        return "Bool"
    if kind == "integer":
        value = _exact_mapping(value, {"kind", "value", "lean_type"}, "property integer")
        number, lean_type = value["value"], value["lean_type"]
        if (not isinstance(number, int) or isinstance(number, bool) or lean_type not in ("Int", "Nat")
                or (lean_type == "Nat" and number < 0)):
            raise IrError("agent property integer literal is invalid")
        return lean_type
    if kind == "unary":
        value = _exact_mapping(value, {"kind", "operator", "operand"}, "property unary term")
        operand = _property_term_type(value["operand"], kernel, parameters, counter, depth + 1)
        if (value["operator"], operand) not in (("not", "Bool"), ("negate", "Int")):
            raise IrError("agent property unary operator does not accept its operand type")
        return operand
    if kind == "binary":
        value = _exact_mapping(value, {"kind", "operator", "left", "right"}, "property binary term")
        left = _property_term_type(value["left"], kernel, parameters, counter, depth + 1)
        right = _property_term_type(value["right"], kernel, parameters, counter, depth + 1)
        if left != right:
            raise IrError("agent property binary operands must have the same type")
        admitted = ((value["operator"] in ("add", "subtract", "multiply") and left in ("Int", "Nat"))
                    or (value["operator"] in ("and", "or") and left == "Bool"))
        if not admitted:
            raise IrError("agent property binary operator does not accept its operand type")
        return left
    if kind == "if":
        value = _exact_mapping(value, {"kind", "condition", "then", "otherwise"}, "property if term")
        if _property_term_type(value["condition"], kernel, parameters, counter, depth + 1) != "Bool":
            raise IrError("agent property if condition must be Bool")
        then_type = _property_term_type(value["then"], kernel, parameters, counter, depth + 1)
        other_type = _property_term_type(value["otherwise"], kernel, parameters, counter, depth + 1)
        if then_type != other_type:
            raise IrError("agent property if branches must have the same type")
        return then_type
    raise IrError(f"unsupported agent property term kind {kind!r}")


def _validate_property_proposition(
    value: Any,
    kernel: Mapping[str, Any],
    parameters: Mapping[str, str],
    counter: list[int],
    depth: int,
) -> None:
    _property_node(counter, depth)
    if not isinstance(value, Mapping) or not isinstance(value.get("kind"), str):
        raise IrError("agent proposition must be a tagged object")
    kind = value["kind"]
    relations = {
        "equals", "not-equals", "less-than", "less-or-equal", "greater-than", "greater-or-equal",
    }
    if kind in relations:
        value = _exact_mapping(value, {"kind", "left", "right"}, "property relation")
        left = _property_term_type(value["left"], kernel, parameters, counter, depth + 1)
        right = _property_term_type(value["right"], kernel, parameters, counter, depth + 1)
        if left != right or (kind not in ("equals", "not-equals") and left not in ("Int", "Nat")):
            raise IrError("agent property relation does not accept its operand types")
        return
    if kind == "holds":
        value = _exact_mapping(value, {"kind", "term"}, "property holds")
        if _property_term_type(value["term"], kernel, parameters, counter, depth + 1) != "Bool":
            raise IrError("agent property holds requires a Bool term")
        return
    if kind == "not":
        value = _exact_mapping(value, {"kind", "proposition"}, "property negation")
        _validate_property_proposition(value["proposition"], kernel, parameters, counter, depth + 1)
        return
    if kind in ("and", "or"):
        value = _exact_mapping(value, {"kind", "propositions"}, "property connective")
        parts = value["propositions"]
        if not isinstance(parts, list) or not 2 <= len(parts) <= 8:
            raise IrError("agent property conjunctions and disjunctions require 2 to 8 children")
        for part in parts:
            _validate_property_proposition(part, kernel, parameters, counter, depth + 1)
        return
    if kind == "implies":
        value = _exact_mapping(value, {"kind", "premise", "conclusion"}, "property implication")
        _validate_property_proposition(value["premise"], kernel, parameters, counter, depth + 1)
        _validate_property_proposition(value["conclusion"], kernel, parameters, counter, depth + 1)
        return
    raise IrError(f"unsupported agent proposition kind {kind!r}")


@dataclass(frozen=True)
class AgentProperty:
    """One agent-authored property bound to a disclosed property task."""

    data: Mapping[str, Any]

    @classmethod
    def from_data(cls, value: Any, task: PropertyTask | None = None) -> AgentProperty:
        value = _exact_mapping(
            value, {"schema", "task_digest", "name", "parameters", "proposition"}, "agent property"
        )
        if value["schema"] != AGENT_PROPERTY_SCHEMA or not _is_digest(value["task_digest"]):
            raise IrError("agent property schema or task digest is invalid")
        _agent_name(value["name"], "agent property name")
        if not isinstance(value["parameters"], list) or len(value["parameters"]) > 8:
            raise IrError("agent property accepts at most 8 parameters")
        parameters: dict[str, str] = {}
        for parameter in value["parameters"]:
            parameter = _exact_mapping(parameter, {"name", "lean_type"}, "agent property parameter")
            name = _agent_name(parameter["name"], "agent property parameter name")
            if not isinstance(parameter["lean_type"], str) or not parameter["lean_type"]:
                raise IrError("agent property parameter type must be a non-empty string")
            if name in parameters:
                raise IrError("agent property parameter names must be unique")
            parameters[name] = parameter["lean_type"]
        if task is not None:
            if value["task_digest"] != task.object_digest:
                raise IrError("agent property task identity is stale or belongs to another model")
            allowed = {item["lean_type"] for item in task.kernel["inputs"]}
            allowed.add(task.kernel["output"]["lean_type"])
            if not set(parameters.values()) <= allowed:
                raise IrError("agent property parameter type is outside the disclosed kernel signature")
            _validate_property_proposition(value["proposition"], task.kernel, parameters, [0], 1)
        return cls(json.loads(json.dumps(value)))

    @classmethod
    def from_json(cls, text: str, task: PropertyTask | None = None) -> AgentProperty:
        try:
            return cls.from_data(json.loads(text), task)
        except json.JSONDecodeError as error:
            raise IrError(f"invalid agent property JSON: {error.msg}") from error

    def to_data(self) -> dict[str, Any]:
        return json.loads(json.dumps(self.data))

    def to_json(self, *, indent: int | None = None) -> str:
        return json.dumps(self.to_data(), ensure_ascii=False, indent=indent,
                          separators=None if indent else (",", ":"))

    def write(self, path: str | Path, *, indent: int | None = 2) -> None:
        Path(path).write_text(self.to_json(indent=indent) + "\n", encoding="utf-8")


@dataclass(frozen=True)
class PropertyTask:
    """A revision-bound signature and grammar for an agent-authored property."""

    target: Mapping[str, Any]
    kernel: Mapping[str, Any]
    contract: Mapping[str, Any]
    templates: Sequence[Mapping[str, Any]]
    object_digest: str
    actions: Mapping[str, Sequence[str]]
    token_budget: Mapping[str, Any]

    @classmethod
    def from_data(cls, value: Any) -> PropertyTask:
        value = _exact_mapping(value, {
            "schema", "target", "kernel", "contract", "templates", "object_digest", "actions",
            "token_budget",
        }, "property task")
        if value["schema"] != PROPERTY_TASK_SCHEMA:
            raise IrError(f"property task schema must be {PROPERTY_TASK_SCHEMA}")
        target = _exact_mapping(value["target"], {"source", "symbol", "source_hash"}, "property target")
        kernel = _exact_mapping(value["kernel"], {"model", "inputs", "output"}, "property kernel")
        contract = _exact_mapping(value["contract"], {
            "author", "format", "proof_author", "allowed_terms", "allowed_propositions",
            "allowed_unary_operators", "allowed_binary_operators", "max_parameters", "max_nodes",
            "max_depth",
        }, "property contract")
        actions = _exact_mapping(value["actions"], {"plan", "scaffold"}, "property actions")
        if (not all(isinstance(target[key], str) and target[key] for key in target)
                or not _is_digest(target["source_hash"])):
            raise IrError("property task target is malformed")
        if (not isinstance(kernel["model"], str) or not kernel["model"]
                or not isinstance(kernel["inputs"], list)):
            raise IrError("property task kernel is malformed")
        inputs = [FormalBinding.from_data(item).to_data() for item in kernel["inputs"]]
        output = FormalBinding.from_data(kernel["output"]).to_data()
        checked_kernel = {"model": kernel["model"], "inputs": inputs, "output": output}
        if contract["author"] != "agent" or contract["proof_author"] != "agent":
            raise IrError("property and proof authors must be agent")
        if contract["format"] != AGENT_PROPERTY_SCHEMA:
            raise IrError("property task format is invalid")
        for field in (
            "allowed_terms", "allowed_propositions", "allowed_unary_operators", "allowed_binary_operators",
        ):
            _string_vector(contract[field], f"property contract {field}")
        if (contract["max_parameters"], contract["max_nodes"], contract["max_depth"]) != (8, 64, 16):
            raise IrError("property task ceilings are unsupported")
        if not isinstance(value["templates"], list):
            raise IrError("property task templates must be a list")
        templates = []
        for template in value["templates"]:
            template = _exact_mapping(template, {"kind", "value"}, "property template")
            if not isinstance(template["kind"], str) or not isinstance(template["value"], Mapping):
                raise IrError("property template is malformed")
            templates.append(json.loads(json.dumps(template)))
        checked_actions = {key: _string_vector(actions[key], f"property task {key}") for key in actions}
        budget = _byte_budget(value["token_budget"], "property task budget")
        core = {
            "schema": value["schema"], "target": value["target"], "kernel": value["kernel"],
            "contract": value["contract"], "templates": value["templates"],
        }
        if not _is_digest(value["object_digest"]) or merkle_object_digest(core) != value["object_digest"]:
            raise IrError("property task fails its Merkle content address")
        return cls(dict(target), checked_kernel, dict(contract), tuple(templates), value["object_digest"],
                   checked_actions, dict(budget))

    def property(
        self,
        name: str,
        parameters: Sequence[Mapping[str, str]],
        proposition: Mapping[str, Any],
    ) -> AgentProperty:
        return AgentProperty.from_data({
            "schema": AGENT_PROPERTY_SCHEMA,
            "task_digest": self.object_digest,
            "name": name,
            "parameters": [dict(item) for item in parameters],
            "proposition": dict(proposition),
        }, self)

    def to_data(self) -> dict[str, Any]:
        return {
            "schema": PROPERTY_TASK_SCHEMA, "target": dict(self.target), "kernel": dict(self.kernel),
            "contract": dict(self.contract), "templates": [dict(item) for item in self.templates],
            "object_digest": self.object_digest,
            "actions": {key: list(value) for key, value in self.actions.items()},
            "token_budget": dict(self.token_budget),
        }


@dataclass(frozen=True)
class FormalProperty:
    kind: str
    name: str
    proposition: str
    proof_status: str = "unproved"
    agent_spec: AgentProperty | None = None

    @classmethod
    def from_data(cls, value: Any) -> FormalProperty:
        keys = set(value) if isinstance(value, Mapping) else set()
        expected = {"kind", "name", "proposition", "proof_status"}
        if keys not in (expected, expected | {"agent_spec"}):
            raise IrError("formal property has missing or unknown fields")
        if not all(isinstance(value[key], str) and value[key] for key in expected):
            raise IrError("formal property fields must be non-empty strings")
        agent_spec = AgentProperty.from_data(value["agent_spec"]) if "agent_spec" in value else None
        if (value["kind"] == "agent") != (agent_spec is not None):
            raise IrError("formal agent property must retain its authored specification")
        return cls(
            value["kind"], value["name"], value["proposition"], value["proof_status"], agent_spec
        )

    def to_data(self) -> dict[str, Any]:
        value = {
            "kind": self.kind,
            "name": self.name,
            "proposition": self.proposition,
            "proof_status": self.proof_status,
        }
        if self.agent_spec is not None:
            value["agent_spec"] = self.agent_spec.to_data()
        return value


@dataclass(frozen=True)
class FormalKernel:
    module: str
    model: str
    inputs: Sequence[FormalBinding]
    output: FormalBinding
    semantic_ir: Any
    lean_definition: str

    @classmethod
    def from_data(cls, value: Any) -> FormalKernel:
        value = _exact_mapping(
            value,
            {"module", "model", "inputs", "output", "semantic_ir", "lean_definition"},
            "formal kernel",
        )
        if not isinstance(value["inputs"], list):
            raise IrError("formal kernel inputs must be a list")
        if not all(isinstance(value[key], str) and value[key]
                   for key in ("module", "model", "lean_definition")):
            raise IrError("formal kernel names and definition must be non-empty strings")
        return cls(
            value["module"], value["model"],
            tuple(FormalBinding.from_data(item) for item in value["inputs"]),
            FormalBinding.from_data(value["output"]), value["semantic_ir"],
            value["lean_definition"],
        )

    def to_data(self) -> dict[str, Any]:
        return {
            "module": self.module,
            "model": self.model,
            "inputs": [item.to_data() for item in self.inputs],
            "output": self.output.to_data(),
            "semantic_ir": self.semantic_ir,
            "lean_definition": self.lean_definition,
        }


@dataclass(frozen=True)
class FormalPlan:
    """Validated Python mirror of ``fr-formal-plan-1``."""

    target: Mapping[str, str]
    kernel: FormalKernel
    properties: Sequence[FormalProperty]
    correspondence: Mapping[str, str]
    assumptions: Sequence[str]
    obligations: Sequence[str]
    actions: Mapping[str, Sequence[str]]
    object_digest: str

    @classmethod
    def from_data(cls, value: Any) -> FormalPlan:
        keys = {
            "schema", "target", "kernel", "properties", "correspondence",
            "assumptions", "obligations", "object_digest", "actions",
        }
        value = _exact_mapping(value, keys, "formal plan")
        if value["schema"] != FORMAL_PLAN_SCHEMA:
            raise IrError(f"formal plan schema must be {FORMAL_PLAN_SCHEMA}")
        target = _exact_mapping(value["target"], {"source", "symbol", "source_hash"}, "formal target")
        correspondence = _exact_mapping(
            value["correspondence"],
            {"source_identity", "signature_surface", "model_generation", "implementation_model"},
            "formal correspondence",
        )
        actions = _exact_mapping(value["actions"], {"scaffold", "goals", "verify"}, "formal actions")
        if not all(isinstance(target[key], str) and target[key] for key in target):
            raise IrError("formal target fields must be non-empty strings")
        if not all(isinstance(correspondence[key], str) and correspondence[key]
                   for key in correspondence):
            raise IrError("formal correspondence fields must be non-empty strings")
        for field in ("properties", "assumptions", "obligations"):
            if not isinstance(value[field], list):
                raise IrError(f"formal plan {field} must be a list")
        if not all(isinstance(item, str) for field in ("assumptions", "obligations")
                   for item in value[field]):
            raise IrError("formal assumptions and obligations must contain strings")
        if not all(isinstance(actions[key], list)
                   and all(isinstance(part, str) for part in actions[key]) for key in actions):
            raise IrError("formal actions must contain argument-vector string lists")
        core = {key: value[key] for key in (
            "schema", "target", "kernel", "properties", "correspondence",
            "assumptions", "obligations",
        )}
        if not _is_digest(value["object_digest"]) or merkle_object_digest(core) != value["object_digest"]:
            raise IrError("formal plan fails its Merkle content address")
        kernel = FormalKernel.from_data(value["kernel"])
        properties = tuple(FormalProperty.from_data(item) for item in value["properties"])
        for property_ in properties:
            if property_.agent_spec is None:
                continue
            spec = property_.agent_spec.to_data()
            parameters = {item["name"]: item["lean_type"] for item in spec["parameters"]}
            _validate_property_proposition(
                spec["proposition"], kernel.to_data(), parameters, [0], 1
            )
        return cls(
            dict(target), kernel, properties,
            dict(correspondence), tuple(value["assumptions"]), tuple(value["obligations"]),
            {key: tuple(actions[key]) for key in actions}, value["object_digest"],
        )

    @classmethod
    def from_json(cls, text: str) -> FormalPlan:
        return cls.from_data(json.loads(text))

    def to_data(self) -> dict[str, Any]:
        return {
            "schema": FORMAL_PLAN_SCHEMA,
            "target": dict(self.target),
            "kernel": self.kernel.to_data(),
            "properties": [item.to_data() for item in self.properties],
            "correspondence": dict(self.correspondence),
            "assumptions": list(self.assumptions),
            "obligations": list(self.obligations),
            "object_digest": self.object_digest,
            "actions": {key: list(value) for key, value in self.actions.items()},
        }

    def to_json(self, *, indent: int | None = None) -> str:
        return json.dumps(self.to_data(), ensure_ascii=False, indent=indent,
                          separators=None if indent else (",", ":"))

    def write(self, path: str | Path, *, indent: int | None = 2) -> None:
        Path(path).write_text(self.to_json(indent=indent) + "\n", encoding="utf-8")


def _string_vector(value: Any, description: str) -> tuple[str, ...]:
    if not isinstance(value, list) or not all(isinstance(item, str) for item in value):
        raise IrError(f"{description} must be an argument-vector string list")
    return tuple(value)


def _byte_budget(value: Any, description: str) -> dict[str, Any]:
    value = _exact_mapping(value, {"limit", "used_upper_bound", "measurement"}, description)
    if (not isinstance(value["limit"], int) or isinstance(value["limit"], bool)
            or not isinstance(value["used_upper_bound"], int)
            or isinstance(value["used_upper_bound"], bool)
            or not 0 <= value["used_upper_bound"] <= value["limit"]
            or value["measurement"] != "serialized_utf8_bytes"):
        raise IrError(f"{description} is not a valid serialized byte budget")
    return dict(value)


@dataclass(frozen=True)
class ProofTask:
    """A proof context and empty templates whose tactics an agent must author."""

    goal: Mapping[str, Any]
    contract: Mapping[str, Any]
    templates: Sequence[Mapping[str, Any]]
    object_digest: str
    actions: Mapping[str, Sequence[str]]
    token_budget: Mapping[str, Any]

    @classmethod
    def from_data(cls, value: Any) -> ProofTask:
        value = _exact_mapping(value, {
            "schema", "goal", "contract", "templates", "object_digest", "actions", "token_budget",
        }, "proof task")
        if value["schema"] != PROOF_TASK_SCHEMA:
            raise IrError(f"proof task schema must be {PROOF_TASK_SCHEMA}")
        goal = _exact_mapping(value["goal"], {
            "id", "name", "spec", "line", "theorem", "source_anchor", "signature_map",
            "proof_region", "object_digest", "prove_template", "verify",
        }, "proof task goal")
        for key in ("id", "name", "spec", "theorem", "proof_region", "object_digest"):
            if not isinstance(goal[key], str) or not goal[key]:
                raise IrError("proof task goal fields must be non-empty strings")
        if not _is_digest(goal["id"]) or not _is_digest(goal["object_digest"]):
            raise IrError("proof task goal identities must be lowercase SHA-256 digests")
        if not isinstance(goal["line"], int) or isinstance(goal["line"], bool) or goal["line"] < 1:
            raise IrError("proof task goal line must be a positive integer")
        if not all(goal[key] is None or isinstance(goal[key], str)
                   for key in ("source_anchor", "signature_map")):
            raise IrError("proof task goal anchors must be strings or null")
        _string_vector(goal["prove_template"], "proof task goal prove template")
        _string_vector(goal["verify"], "proof task goal verify action")
        contract = _exact_mapping(value["contract"], {
            "author", "format", "insertion_point", "normalization", "forbidden", "checker",
        }, "proof input contract")
        actions = _exact_mapping(value["actions"], {"check", "apply", "verify"}, "proof task actions")
        budget = _byte_budget(value["token_budget"], "proof task budget")
        if contract["author"] != "agent":
            raise IrError("proof task author must be agent")
        if not all(isinstance(contract[key], str) and contract[key]
                   for key in ("format", "insertion_point", "normalization", "checker")):
            raise IrError("proof input contract fields must be non-empty strings")
        _string_vector(contract["forbidden"], "proof input forbidden list")
        if not isinstance(value["templates"], list):
            raise IrError("proof templates must be a list")
        templates = []
        for template in value["templates"]:
            template = _exact_mapping(template, {"kind", "lines"}, "proof template")
            if not isinstance(template["kind"], str) or not template["kind"]:
                raise IrError("proof template kind must be a non-empty string")
            lines = _string_vector(template["lines"], "proof template lines")
            if not any("agent-written" in line for line in lines):
                raise IrError("proof template must reserve agent-written tactics")
            templates.append({"kind": template["kind"], "lines": list(lines)})
        checked_actions = {key: _string_vector(actions[key], f"proof task {key}") for key in actions}
        core = {
            "schema": value["schema"], "goal": value["goal"], "contract": value["contract"],
            "templates": value["templates"],
        }
        if not _is_digest(value["object_digest"]) or merkle_object_digest(core) != value["object_digest"]:
            raise IrError("proof task fails its Merkle content address")
        return cls(dict(goal), dict(contract), tuple(templates), value["object_digest"],
                   checked_actions, dict(budget))

    @classmethod
    def from_json(cls, text: str) -> ProofTask:
        return cls.from_data(json.loads(text))

    def to_data(self) -> dict[str, Any]:
        return {
            "schema": PROOF_TASK_SCHEMA, "goal": dict(self.goal), "contract": dict(self.contract),
            "templates": [dict(item) for item in self.templates], "object_digest": self.object_digest,
            "actions": {key: list(value) for key, value in self.actions.items()},
            "token_budget": dict(self.token_budget),
        }


@dataclass(frozen=True)
class ProofAttempt:
    """Lean's bounded result for exact agent-written proof bytes."""

    data: Mapping[str, Any]

    @classmethod
    def from_data(cls, value: Any) -> ProofAttempt:
        value = _exact_mapping(value, {
            "schema", "goal_id", "proof_digest", "checker", "passed", "diagnostics",
            "diagnostics_omitted", "receipt", "actions", "token_budget",
        }, "proof attempt")
        if value["schema"] != PROOF_ATTEMPT_SCHEMA:
            raise IrError(f"proof attempt schema must be {PROOF_ATTEMPT_SCHEMA}")
        if not _is_digest(value["goal_id"]) or not _is_digest(value["proof_digest"]):
            raise IrError("proof attempt identities must be lowercase SHA-256 digests")
        if not isinstance(value["checker"], str) or not isinstance(value["passed"], bool):
            raise IrError("proof attempt checker and result are malformed")
        if not isinstance(value["diagnostics"], list):
            raise IrError("proof attempt diagnostics must be a list")
        for diagnostic in value["diagnostics"]:
            diagnostic = _exact_mapping(
                diagnostic, {"severity", "line", "column", "message", "context"}, "proof diagnostic"
            )
            if diagnostic["severity"] not in ("error", "warning"):
                raise IrError("proof diagnostic severity is invalid")
            if not isinstance(diagnostic["message"], str):
                raise IrError("proof diagnostic message must be a string")
            _string_vector(diagnostic["context"], "proof diagnostic context")
            for location in ("line", "column"):
                item = diagnostic[location]
                if item is not None and (not isinstance(item, int) or isinstance(item, bool) or item < 0):
                    raise IrError("proof diagnostic locations must be non-negative integers or null")
        omitted = value["diagnostics_omitted"]
        if not isinstance(omitted, int) or isinstance(omitted, bool) or omitted < 0:
            raise IrError("proof diagnostics omitted count must be a non-negative integer")
        _byte_budget(value["token_budget"], "proof attempt budget")
        actions = _exact_mapping(value["actions"], {"revise", "apply", "verify"}, "proof actions")
        _string_vector(actions["revise"], "proof revise action")
        _string_vector(actions["verify"], "proof verify action")
        if actions["apply"] is not None:
            _string_vector(actions["apply"], "proof apply action")
        expected_receipt = merkle_object_digest({
            "schema": "fr-proof-receipt-1", "goal_id": value["goal_id"],
            "proof_digest": value["proof_digest"], "checker": value["checker"],
        })
        if value["passed"]:
            if value["receipt"] != expected_receipt or actions["apply"] is None:
                raise IrError("accepted proof attempt has an invalid receipt or no apply action")
        elif value["receipt"] is not None or actions["apply"] is not None:
            raise IrError("rejected proof attempt cannot have a receipt or apply action")
        return cls(json.loads(json.dumps(value)))

    @classmethod
    def from_json(cls, text: str) -> ProofAttempt:
        return cls.from_data(json.loads(text))

    def to_data(self) -> dict[str, Any]:
        return json.loads(json.dumps(self.data))


@dataclass(frozen=True)
class ProjectReference:
    """A JSON Pointer to one earlier project request."""

    request: str
    pointer: str

    def __post_init__(self) -> None:
        if (not self.request or len(self.request) > 80
                or not re.fullmatch(r"[A-Za-z0-9._-]+", self.request)):
            raise IrError("project reference request has an invalid ID")
        if not self.pointer.startswith("/") or len(self.pointer.encode()) > 512:
            raise IrError("project reference pointer must be a bounded JSON Pointer")

    def to_data(self) -> dict[str, str]:
        return {"request": self.request, "pointer": self.pointer}


@dataclass(frozen=True)
class ProjectRequest:
    """One bounded source-free request inside a task-change manifest."""

    id: str
    arguments: Sequence[str | ProjectReference]

    def __post_init__(self) -> None:
        ProjectReference(self.id, "/validates-id")
        if not 1 <= len(self.arguments) <= 64:
            raise IrError("project request needs 1 through 64 arguments")
        if any(not isinstance(item, (str, ProjectReference)) for item in self.arguments):
            raise IrError("project request arguments must be strings or references")
        if sum(len(json.dumps(self._argument(item), ensure_ascii=False))
               for item in self.arguments) > 4096:
            raise IrError("project request exceeds the 4096-byte argument budget")

    @staticmethod
    def _argument(value: str | ProjectReference) -> Any:
        return value.to_data() if isinstance(value, ProjectReference) else value

    def to_data(self) -> dict[str, Any]:
        return {"id": self.id, "arguments": [self._argument(item) for item in self.arguments]}


_TASK_FRAGMENT_OPERATIONS = {
    "replace-body", "replace-body-semantic", "edit-body-semantic", "edit-body-intent",
    "replace-declaration", "insert-declaration",
}
_TASK_SCALAR_OPERATIONS = {"edit-body-scalar"}
_TASK_DISCLOSED_OPERATIONS = {"edit-body-disclosed"}
_TASK_DISCLOSED_IR_OPERATIONS = {"edit-body-disclosed-ir"}
_TASK_OPERATIONS = (_TASK_FRAGMENT_OPERATIONS | _TASK_SCALAR_OPERATIONS
                    | _TASK_DISCLOSED_OPERATIONS | _TASK_DISCLOSED_IR_OPERATIONS
                    | {"organize-imports"})


@dataclass(frozen=True)
class TaskTarget:
    """One exact authoring target, shaped like the Rust task-change IR."""

    id: str
    handle: str | ProjectReference
    op: str
    from_path: str | None = None
    fragment: str | None = None
    scalar: ScalarRequest | None = None
    disclosed: DisclosedEditRequest | None = None
    disclosed_ir: DisclosedIrEditRequest | None = None

    def __post_init__(self) -> None:
        ProjectReference(self.id, "/validates-id")
        if not isinstance(self.handle, (str, ProjectReference)) or not self.handle:
            raise IrError("task target handle must be a string or project reference")
        if self.op not in _TASK_OPERATIONS:
            raise IrError("task target operation is unsupported")
        fragments = int(self.from_path is not None) + int(self.fragment is not None)
        if fragments != int(self.op in _TASK_FRAGMENT_OPERATIONS):
            raise IrError("fragment operations need exactly one of from_path or fragment")
        if self.fragment is not None and (len(self.fragment.encode()) > 65536 or "\0" in self.fragment):
            raise IrError("inline fragment must be NUL-free UTF-8 of at most 64 KiB")
        expected = (
            int(self.scalar is not None), int(self.disclosed is not None),
            int(self.disclosed_ir is not None),
        )
        actual = (
            int(self.op in _TASK_SCALAR_OPERATIONS), int(self.op in _TASK_DISCLOSED_OPERATIONS),
            int(self.op in _TASK_DISCLOSED_IR_OPERATIONS),
        )
        if expected != actual:
            raise IrError("task target auxiliary input does not match its operation")

    def to_data(self) -> dict[str, Any]:
        value: dict[str, Any] = {
            "id": self.id,
            "handle": self.handle.to_data() if isinstance(self.handle, ProjectReference) else self.handle,
            "op": self.op,
        }
        if self.from_path is not None:
            value["from"] = self.from_path
        if self.fragment is not None:
            value["fragment"] = self.fragment
        if self.scalar is not None:
            value["scalar"] = self.scalar.to_data()
        if self.disclosed is not None:
            value["disclosed"] = self.disclosed.to_data()
        if self.disclosed_ir is not None:
            value["disclosed_ir"] = self.disclosed_ir.to_data()
        return value


@dataclass(frozen=True)
class TaskDelivery:
    """Checked lifecycle requested by one task change."""

    check_original: bool = True
    compact_success: bool = True
    exercise_reversal: bool = True
    patch: str | None = None
    check_output_bytes: int = 2048

    def __post_init__(self) -> None:
        if (not isinstance(self.check_original, bool) or not isinstance(self.compact_success, bool)
                or not isinstance(self.exercise_reversal, bool)):
            raise IrError("task delivery lifecycle flags must be booleans")
        if (not isinstance(self.check_output_bytes, int) or isinstance(self.check_output_bytes, bool)
                or not 0 <= self.check_output_bytes <= 65536):
            raise IrError("task delivery check output budget must be between 0 and 65536")
        if self.patch is not None:
            path = Path(self.patch)
            if (path.is_absolute() or not path.parts or any(part in (".", "..") for part in path.parts)
                    or any(part in (".git", ".fr-history") for part in path.parts)):
                raise IrError("task delivery patch must be a safe relative path")

    def to_data(self) -> dict[str, Any]:
        return {
            "check-original": self.check_original,
            "compact-success": self.compact_success,
            "exercise-reversal": self.exercise_reversal,
            "patch": self.patch,
            "check-output-bytes": self.check_output_bytes,
        }


@dataclass(frozen=True)
class TaskChange:
    """A complete reviewed task-change manifest with inline or file-backed fragments."""

    requests: Sequence[ProjectRequest]
    targets: Sequence[TaskTarget]
    postconditions: Mapping[str, Any]
    checks: Sequence[str]
    delivery: TaskDelivery = TaskDelivery()

    def __post_init__(self) -> None:
        if not 0 <= len(self.requests) <= 16:
            raise IrError("task change accepts 0 through 16 project requests")
        ids = [request.id for request in self.requests]
        if len(ids) != len(set(ids)):
            raise IrError("task change request IDs must be unique")
        seen: set[str] = set()
        for request in self.requests:
            for argument in request.arguments:
                if isinstance(argument, ProjectReference) and argument.request not in seen:
                    raise IrError("project request references must name an earlier request")
            seen.add(request.id)
        if not 1 <= len(self.targets) <= 32:
            raise IrError("task change needs 1 through 32 targets")
        target_ids = [target.id for target in self.targets]
        if len(target_ids) != len(set(target_ids)):
            raise IrError("task change target IDs must be unique")
        if any(isinstance(target.handle, ProjectReference)
               and target.handle.request not in seen for target in self.targets):
            raise IrError("task target references an unknown project request")
        allowed = {"files-changed", "edits", "changed-operations", "paths-changed"}
        if not self.postconditions or not set(self.postconditions) <= allowed:
            raise IrError("task change postconditions are empty or unknown")
        for name in ("files-changed", "edits", "changed-operations"):
            value = self.postconditions.get(name)
            if value is not None and (not isinstance(value, int) or isinstance(value, bool) or value < 0):
                raise IrError("numeric task postconditions must be non-negative integers")
        paths = self.postconditions.get("paths-changed")
        if paths is not None and (not isinstance(paths, list) or any(
                not isinstance(item, str) or Path(item).is_absolute()
                or not Path(item).parts or any(part in (".", "..") for part in Path(item).parts)
                for item in paths)):
            raise IrError("paths-changed must contain normalized relative paths")
        if (not self.checks or len(self.checks) != len(set(self.checks))
                or any(not isinstance(name, str) or not name for name in self.checks)):
            raise IrError("task change needs named checks")
        encoded = json.dumps(self.to_data(), ensure_ascii=False, separators=(",", ":")).encode()
        if len(encoded) > 65536:
            raise IrError("task change manifest exceeds 64 KiB")

    def to_data(self) -> dict[str, Any]:
        return {
            "schema": TASK_CHANGE_SCHEMA,
            "requests": [request.to_data() for request in self.requests],
            "targets": [target.to_data() for target in self.targets],
            "postconditions": dict(self.postconditions),
            "checks": list(self.checks),
            "delivery": self.delivery.to_data(),
        }

    def to_json(self, *, indent: int | None = None) -> str:
        return json.dumps(self.to_data(), ensure_ascii=False, indent=indent,
                          separators=None if indent else (",", ":"))

    def write(self, path: str | Path, *, indent: int | None = 2) -> None:
        Path(path).write_text(self.to_json(indent=indent) + "\n", encoding="utf-8")


