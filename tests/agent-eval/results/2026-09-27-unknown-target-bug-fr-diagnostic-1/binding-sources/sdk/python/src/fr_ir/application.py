from __future__ import annotations

from dataclasses import dataclass, field
import json
from pathlib import Path as FilePath
import re
from typing import Any, Mapping, Sequence

from .ir import IrError, merkle_object_digest

SCHEMA = "fr-http-application-1"
ADAPTERS = ("nextjs", "fastapi", "express", "go-net-http", "react")


def _encoded(value: Any, limit: int) -> str:
    chunks = []
    size = 0
    for chunk in json.JSONEncoder(ensure_ascii=False, separators=(",", ":"), allow_nan=False).iterencode(value):
        size += len(chunk.encode("utf-8"))
        if size > limit:
            raise IrError("serialized IR exceeds its byte bound")
        chunks.append(chunk)
    return "".join(chunks)


def adapter_reads(adapter: int, feature: int) -> bool:
    return (type(adapter) is int and type(feature) is int and adapter >= 0 and feature >= 0
            and ((feature <= 2 and adapter <= 3)
                 or (feature == 3 and adapter in (0, 4))))


def adapter_writes(adapter: int, feature: int) -> bool:
    return (type(adapter) is int and type(feature) is int and adapter >= 0 and feature >= 0
            and ((feature <= 2 and adapter <= 3) or (feature == 3 and adapter in (0, 4))))


def adapter_supports(adapter: int, feature: int) -> bool:
    return adapter_reads(adapter, feature) and adapter_writes(adapter, feature)


def adapters_compatible(source: int, target: int, feature: int) -> bool:
    return source != target and adapter_reads(source, feature) and adapter_writes(target, feature)


def json_status_admitted(status: int) -> bool:
    return type(status) is int and 200 <= status <= 599 and status not in (204, 205, 304)


def request_input_admitted(method: int, source: int, scalar: int) -> bool:
    return (all(type(value) is int and value >= 0 for value in (method, source, scalar))
            and method <= 5 and scalar <= 2
            and (source == 0 or source == 1 and 1 <= method <= 3))


def fastapi_input_admitted(source: int, scalar: int, alias_safe: bool,
                           embedded: bool, extra_metadata: bool) -> bool:
    return (type(source) is int and type(scalar) is int and source >= 0 and scalar >= 0
            and type(alias_safe) is bool and type(embedded) is bool
            and type(extra_metadata) is bool and source <= 1 and scalar <= 2
            and alias_safe and not extra_metadata
            and ((source == 0 and not embedded) or (source == 1 and embedded)))


def middleware_chain_admitted(total: int, resolved: int, configured: int) -> bool:
    return (all(type(value) is int and value >= 0 for value in (total, resolved, configured))
            and 1 <= total <= 64 and resolved == total and configured == 0)


def dependency_admitted(provider_safe: bool, configured: bool) -> bool:
    return (type(provider_safe) is bool and type(configured) is bool
            and provider_safe and not configured)


def _dotted_identifier(name: Any) -> bool:
    return (isinstance(name, str) and 0 < len(name.encode("utf-8")) <= 160
            and all(re.fullmatch(r"[A-Za-z_][A-Za-z_0-9]{0,127}", part)
                    for part in name.split(".")))


def _middleware_data(chain: Any) -> list[dict[str, Any]]:
    if not isinstance(chain, list) or len(chain) > 64:
        raise IrError("middleware chain exceeds its entry bound")
    result = []
    orders: set[int] = set()
    for entry in chain:
        row = _fields(entry, {"name", "request_order"})
        order = row["request_order"]
        if (not _dotted_identifier(row["name"]) or type(order) is not int
                or isinstance(order, bool) or not 1 <= order <= len(chain)
                or order in orders):
            raise IrError("middleware entries need dotted names and a unique 1-based request order")
        orders.add(order)
        result.append({"name": row["name"], "request_order": order})
    return result


@dataclass(frozen=True)
class HttpDependency:
    binding: str
    provider: str
    security: bool

    @classmethod
    def from_data(cls, value: Any) -> HttpDependency:
        row = _fields(value, {"binding", "provider", "security"})
        result = cls(row["binding"], row["provider"], row["security"])
        result.to_data()
        return result

    def to_data(self) -> dict[str, Any]:
        if (not isinstance(self.binding, str)
                or not re.fullmatch(r"[A-Za-z_][A-Za-z_0-9]{0,127}", self.binding)
                or not _dotted_identifier(self.provider)
                or type(self.security) is not bool):
            raise IrError("dependency needs a simple binding, a dotted provider and a marker")
        return {"binding": self.binding, "provider": self.provider, "security": self.security}


def validated_endpoint_agreement(method: bool, path: bool, inputs: bool,
                                 status: bool, response: bool) -> bool:
    return all(value is True for value in (method, path, inputs, status, response))


def dispositions_complete(input: int, assigned: int, unique: bool, exact_ids: bool) -> bool:
    return (type(input) is int and type(assigned) is int and input >= 0 and assigned >= 0
            and input == assigned and unique is True and exact_ids is True)


def endpoint_agreement(method: bool, path: bool, status: bool, response: bool) -> bool:
    return all(value is True for value in (method, path, status, response))


def static_resources_admitted(nodes: int, depth: int, encoded_bytes: int) -> bool:
    return (type(nodes) is int and type(depth) is int and type(encoded_bytes) is int
            and 1 <= nodes <= 1024 and 0 <= depth <= 32
            and 0 <= encoded_bytes <= 1_048_576)


@dataclass(frozen=True)
class Literal:
    value: Any
    kind: str = field(default="literal", init=False)


@dataclass(frozen=True)
class Path:
    name: str
    kind: str = field(default="path", init=False)


@dataclass(frozen=True)
class Input:
    name: str
    kind: str = field(default="input", init=False)


@dataclass(frozen=True)
class Service:
    method: str
    path: str
    kind: str = field(default="service", init=False)


@dataclass(frozen=True)
class Object:
    fields: Mapping[str, HttpExpression]
    kind: str = field(default="object", init=False)


@dataclass(frozen=True)
class Array:
    items: Sequence[HttpExpression]
    kind: str = field(default="array", init=False)


HttpExpression = Literal | Path | Input | Service | Object | Array


def _service_path(path: Any) -> bool:
    if (not isinstance(path, str) or not path.startswith("/")
            or len(path.encode("utf-8")) > 2048 or not path.isascii()
            or "//" in path or len(path) > 1 and path.endswith("/")
            or "{" in path or "}" in path):
        return False
    return all(segment not in (".", "..")
               and re.fullmatch(r"[A-Za-z0-9_.~\-]*", segment) is not None
               for segment in path.split("/"))


def _service_calls(expr: Any, calls: list[tuple[str, str]]) -> None:
    if isinstance(expr, Service):
        calls.append((expr.method, expr.path))
    elif isinstance(expr, Object):
        for value in expr.fields.values():
            _service_calls(value, calls)
    elif isinstance(expr, Array):
        for value in expr.items:
            _service_calls(value, calls)


@dataclass(frozen=True)
class HttpInput:
    name: str
    source: str
    scalar: str

    @classmethod
    def from_data(cls, value: Any) -> HttpInput:
        row = _fields(value, {"name", "source", "scalar"})
        result = cls(row["name"], row["source"], row["scalar"])
        result.to_data()
        return result

    def to_data(self) -> dict[str, str]:
        if (not isinstance(self.name, str)
                or not re.fullmatch(r"[A-Za-z_][A-Za-z_0-9]{0,127}", self.name)
                or self.source not in ("query", "json-body")
                or self.scalar not in ("string", "integer", "boolean")):
            raise IrError("request input has an unsupported name, source or scalar type")
        return {"name": self.name, "source": self.source, "scalar": self.scalar}


def _fields(value: Any, required: set[str], optional: set[str] | None = None) -> Mapping[str, Any]:
    if (not isinstance(value, Mapping) or not required <= set(value)
            or not set(value) <= required | (optional or set())):
        raise IrError("application fields do not match their strict schema")
    return value


def expression_from_data(value: Any) -> HttpExpression:
    return _read_expression(value, 0, [0])


def _read_expression(value: Any, depth: int, nodes: list[int]) -> HttpExpression:
    nodes[0] += 1
    if depth > 32 or nodes[0] > 1024:
        raise IrError("HTTP expression exceeds its node or depth bound")
    row = _fields(value, {"kind"}, {"value", "name", "method", "path", "fields", "items"})
    kind = row["kind"]
    if kind == "literal":
        return Literal(_fields(row, {"kind", "value"})["value"])
    if kind == "path":
        return Path(_fields(row, {"kind", "name"})["name"])
    if kind == "input":
        return Input(_fields(row, {"kind", "name"})["name"])
    if kind == "service":
        row = _fields(row, {"kind", "method", "path"})
        return Service(row["method"], row["path"])
    if kind == "object":
        fields = _fields(row, {"kind", "fields"})["fields"]
        if not isinstance(fields, Mapping) or len(fields) > 1024:
            raise IrError("HTTP object requires bounded fields")
        return Object({name: _read_expression(child, depth + 1, nodes) for name, child in fields.items()})
    if kind == "array":
        items = _fields(row, {"kind", "items"})["items"]
        if not isinstance(items, list) or len(items) > 1024:
            raise IrError("HTTP array requires bounded items")
        return Array([_read_expression(child, depth + 1, nodes) for child in items])
    raise IrError("unsupported HTTP expression kind")


def _expression(expr: HttpExpression, parameters: set[str], inputs: set[str], depth: int,
                nodes: list[int]) -> dict[str, Any]:
    nodes[0] += 1
    if depth > 32 or nodes[0] > 1024:
        raise IrError("HTTP expression exceeds its node or depth bound")
    if isinstance(expr, Literal):
        value = expr.value
        if not (value is None or type(value) is bool
                or type(value) is int and -(2**53 - 1) <= value <= 2**53 - 1
                or isinstance(value, str) and len(value.encode("utf-8")) <= 65536):
            raise IrError("HTTP literals require primitives and exactly representable integers")
        return {"kind": "literal", "value": value}
    if isinstance(expr, Path):
        if not isinstance(expr.name, str) or expr.name not in parameters:
            raise IrError("HTTP expression refers to an undeclared path parameter")
        return {"kind": "path", "name": expr.name}
    if isinstance(expr, Input):
        if not isinstance(expr.name, str) or expr.name not in inputs:
            raise IrError("HTTP expression refers to an undeclared request input")
        return {"kind": "input", "name": expr.name}
    if isinstance(expr, Service):
        if (expr.method not in ("GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS")
                or not _service_path(expr.path) or len(expr.path.encode("utf-8")) > 2048
                or "//" in expr.path or len(expr.path) > 1 and expr.path.endswith("/")
                or not expr.path.isascii()):
            raise IrError("service calls need a portable method and a bounded literal root-relative path")
        return {"kind": "service", "method": expr.method, "path": expr.path}
    if isinstance(expr, Object):
        if not isinstance(expr.fields, Mapping) or any(
                not isinstance(name, str) or len(name.encode("utf-8")) > 256
                for name in expr.fields):
            raise IrError("HTTP object fields require bounded string names")
        return {"kind": "object", "fields": {
            name: _expression(value, parameters, inputs, depth + 1, nodes)
            for name, value in sorted(expr.fields.items())}}
    if isinstance(expr, Array) and isinstance(expr.items, (list, tuple)):
        return {"kind": "array", "items": [
            _expression(value, parameters, inputs, depth + 1, nodes) for value in expr.items]}
    raise IrError("unsupported HTTP expression")


@dataclass(frozen=True)
class HttpRoute:
    method: str
    path: str
    status: int
    response: HttpExpression
    inputs: Sequence[HttpInput] = ()
    dependencies: Sequence[HttpDependency] = ()

    @classmethod
    def from_data(cls, value: Any) -> HttpRoute:
        row = _fields(value, {"method", "path", "status", "response"}, {"inputs", "dependencies"})
        raw_inputs = row.get("inputs", [])
        raw_dependencies = row.get("dependencies", [])
        if not isinstance(raw_inputs, list) or not isinstance(raw_dependencies, list):
            raise IrError("request inputs and dependencies must be lists")
        route = cls(row["method"], row["path"], row["status"],
                    expression_from_data(row["response"]),
                    tuple(HttpInput.from_data(item) for item in raw_inputs),
                    tuple(HttpDependency.from_data(item) for item in raw_dependencies))
        route.to_data()
        return route

    def parameters(self) -> set[str]:
        path = self.path
        if (not isinstance(path, str) or not path.startswith("/") or len(path.encode("utf-8")) > 2048
                or not path.isascii() or "//" in path or len(path) > 1 and path.endswith("/")):
            raise IrError("route needs a bounded absolute literal path")
        parameters: set[str] = set()
        for segment in path.split("/"):
            if segment.startswith("{") and segment.endswith("}"):
                name = segment[1:-1]
                if (len(name) > 128 or not re.fullmatch(r"[A-Za-z_][A-Za-z_0-9]*", name)
                        or name in parameters):
                    raise IrError("path parameters require unique simple names")
                parameters.add(name)
            elif segment in (".", "..") or not re.fullmatch(r"[A-Za-z0-9_.~\-]*", segment):
                raise IrError("route pattern requires a separately modeled matcher")
        if len(parameters) > 32:
            raise IrError("route exceeds its path parameter bound")
        return parameters

    def to_data(self) -> dict[str, Any]:
        if self.method not in ("GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"):
            raise IrError("unsupported portable JSON HTTP method")
        if not json_status_admitted(self.status):
            raise IrError("status does not admit a portable JSON response body")
        parameters = self.parameters()
        if (not isinstance(self.inputs, (list, tuple)) or len(self.inputs) > 32
                or any(not isinstance(item, HttpInput) for item in self.inputs)):
            raise IrError("route exceeds its request input bound")
        serialized_inputs = [item.to_data() for item in self.inputs]
        names = [item["name"] for item in serialized_inputs]
        if len(set(names)) != len(names) or parameters.intersection(names):
            raise IrError("request inputs need unique names distinct from path parameters")
        if (not isinstance(self.dependencies, (list, tuple)) or len(self.dependencies) > 16
                or any(not isinstance(item, HttpDependency) for item in self.dependencies)):
            raise IrError("route exceeds its dependency bound")
        serialized_dependencies = [item.to_data() for item in self.dependencies]
        bindings = [item["binding"] for item in serialized_dependencies]
        if (len(set(bindings)) != len(bindings) or parameters.intersection(bindings)
                or set(names).intersection(bindings)):
            raise IrError("dependency bindings need unique names distinct from inputs and path parameters")
        method_code = ("GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS").index(self.method)
        for item in serialized_inputs:
            source_code = ("query", "json-body").index(item["source"])
            scalar_code = ("string", "integer", "boolean").index(item["scalar"])
            if not request_input_admitted(method_code, source_code, scalar_code):
                raise IrError("request input source is not portable for this method")
        response = _expression(self.response, parameters, set(names), 0, [0])
        _encoded(response, 1_048_576)
        value: dict[str, Any] = {"method": self.method, "path": self.path,
                                 "status": self.status, "response": response}
        if serialized_inputs:
            value["inputs"] = serialized_inputs
        if serialized_dependencies:
            value["dependencies"] = serialized_dependencies
        return value


@dataclass(frozen=True)
class RouteBundle:
    routes: Sequence[HttpRoute]
    middleware: Sequence[Mapping[str, Any]] = ()
    schema: str = field(default=SCHEMA, init=False)

    @classmethod
    def from_data(cls, value: Any) -> RouteBundle:
        _encoded(value, 1_048_576)
        row = _fields(value, {"schema", "routes"}, {"middleware"})
        if row["schema"] != SCHEMA or not isinstance(row["routes"], list) or not 1 <= len(row["routes"]) <= 256:
            raise IrError("unsupported or unbounded HTTP application")
        bundle = cls([HttpRoute.from_data(route) for route in row["routes"]],
                     _middleware_data(row.get("middleware", [])))
        bundle.to_data()
        return bundle

    def to_data(self) -> dict[str, Any]:
        if (not isinstance(self.routes, (list, tuple)) or not 1 <= len(self.routes) <= 256
                or any(not isinstance(route, HttpRoute) for route in self.routes)):
            raise IrError("route bundle requires 1..256 endpoints")
        routes = [route.to_data() for route in self.routes]
        middleware = _middleware_data(list(self.middleware))
        for index, route in enumerate(self.routes):
            calls: list[tuple[str, str]] = []
            _service_calls(route.response, calls)
            for method, path in calls:
                candidates = sum(
                    1 for target, candidate in enumerate(self.routes)
                    if target != index and candidate.path == path and candidate.method == method)
                if candidates != 1:
                    raise IrError(
                        "service calls require exactly one same-bundle route target; "
                        "ambiguous, unresolved and nonlocal targets remain manual")
        for index, route in enumerate(routes):
            for previous in routes[:index]:
                left, right = previous["path"].split("/"), route["path"].split("/")
                if (len(left) == len(right) and all(
                        a == b or a.startswith("{") or b.startswith("{") for a, b in zip(left, right))
                        and (previous["path"] != route["path"] or previous["method"] == route["method"])):
                    raise IrError("route matchers overlap or duplicate an endpoint")
        value: dict[str, Any] = {"schema": self.schema, "routes": routes}
        if middleware:
            value["middleware"] = middleware
        _encoded(value, 1_048_576)
        return value

    def to_json(self) -> str:
        return _encoded(self.to_data(), 1_048_576)

    def object_digest(self) -> str:
        return merkle_object_digest(json.loads(self.to_json()))

    def write(self, path: FilePath) -> None:
        """Write only the authored IR; conversion remains a reviewed fr operation."""
        encoded = self.to_json()
        with path.open("x", encoding="utf-8", newline="\n") as output:
            output.write(encoded)


@dataclass(frozen=True)
class StaticText:
    value: str
    kind: str = field(default="text", init=False)


@dataclass(frozen=True)
class StaticState:
    name: str
    kind: str = field(default="state", init=False)


@dataclass(frozen=True)
class SetState:
    state: str
    value: Any
    kind: str = field(default="set-state", init=False)


@dataclass(frozen=True)
class ToggleState:
    state: str
    kind: str = field(default="toggle-state", init=False)


ComponentEvent = SetState | ToggleState


@dataclass(frozen=True)
class StaticElement:
    tag: str
    attributes: Mapping[str, str]
    children: Sequence[StaticNode]
    events: Mapping[str, ComponentEvent] = field(default_factory=dict)
    kind: str = field(default="element", init=False)


StaticNode = StaticText | StaticState | StaticElement


def _state_literal(value: Any) -> bool:
    return (value is None or type(value) is bool
            or type(value) is int and -(2**53 - 1) <= value <= 2**53 - 1
            or isinstance(value, str) and len(value.encode("utf-8")) <= 65536)


def _literal_kind(value: Any) -> int:
    if value is None:
        return 0
    if type(value) is bool:
        return 1
    if isinstance(value, str):
        return 2
    return 3


def _event_from_data(value: Any) -> ComponentEvent:
    row = _fields(value, {"kind"}, {"state", "value"})
    if row["kind"] == "set-state":
        return SetState(_fields(row, {"kind", "state", "value"})["state"], row["value"])
    if row["kind"] == "toggle-state":
        return ToggleState(_fields(row, {"kind", "state"})["state"])
    raise IrError("unsupported component event kind")


def _event_data(event: ComponentEvent, states: Mapping[str, Any]) -> dict[str, Any]:
    if isinstance(event, ToggleState):
        if type(states.get(event.state)) is not bool:
            raise IrError("component events need an on* name and a declared state with a matching literal")
        return {"kind": "toggle-state", "state": event.state}
    if isinstance(event, SetState):
        initial = states.get(event.state, None)
        if (event.state not in states or not _state_literal(event.value)
                or _literal_kind(event.value) != _literal_kind(initial)):
            raise IrError("component events need an on* name and a declared state with a matching literal")
        return {"kind": "set-state", "state": event.state, "value": event.value}
    raise IrError("unsupported component event")


def _static_node_from_data(value: Any, depth: int, count: list[int]) -> StaticNode:
    count[0] += 1
    if depth > 32 or count[0] > 1024:
        raise IrError("static component exceeds its node or depth bound")
    row = _fields(value, {"kind"}, {"value", "name", "tag", "attributes", "events", "children"})
    if row["kind"] == "text":
        return StaticText(_fields(row, {"kind", "value"})["value"])
    if row["kind"] == "state":
        return StaticState(_fields(row, {"kind", "name"})["name"])
    if row["kind"] == "element":
        row = _fields(row, {"kind", "tag", "attributes", "children"}, {"events"})
        if not isinstance(row["attributes"], Mapping) or not isinstance(row["children"], list):
            raise IrError("static element requires attributes and children")
        raw_events = row.get("events", {})
        if not isinstance(raw_events, Mapping):
            raise IrError("static element events require an object")
        return StaticElement(row["tag"], dict(row["attributes"]), [
            _static_node_from_data(child, depth + 1, count) for child in row["children"]],
            {name: _event_from_data(event) for name, event in raw_events.items()})
    raise IrError("unsupported static component node")


def _static_node(node: StaticNode, depth: int, count: list[int],
                 states: Mapping[str, Any]) -> dict[str, Any]:
    count[0] += 1
    count[1] = max(count[1], depth)
    if depth > 32 or count[0] > 1024:
        raise IrError("static component exceeds its node or depth bound")
    if isinstance(node, StaticText):
        if (not isinstance(node.value, str) or not node.value
                or len(node.value.encode("utf-8")) > 65536 or node.value.strip() != node.value):
            raise IrError("static text must be bounded and have explicit whitespace")
        return {"kind": "text", "value": node.value}
    if isinstance(node, StaticState):
        if not isinstance(node.name, str) or node.name not in states:
            raise IrError("state references require a declared component state")
        return {"kind": "state", "name": node.name}
    if isinstance(node, StaticElement):
        if (not isinstance(node.tag, str) or not re.fullmatch(r"[a-z][a-z0-9-]{0,127}", node.tag)
                or not isinstance(node.attributes, Mapping)
                or not isinstance(node.children, (list, tuple))
                or not isinstance(node.events, Mapping)):
            raise IrError("static element fields are invalid")
        attributes: dict[str, str] = {}
        for name, value in sorted(node.attributes.items()):
            if (not isinstance(name, str) or not re.fullmatch(r"[A-Za-z][A-Za-z0-9_-]{0,127}", name)
                    or name.startswith("on") or name in ("style", "dangerouslySetInnerHTML")
                    or not isinstance(value, str) or len(value.encode("utf-8")) > 65536):
                raise IrError("static component attribute exceeds the literal safe subset")
            attributes[name] = value
        events: dict[str, Any] = {}
        for name, event in sorted(node.events.items()):
            if (not isinstance(name, str) or len(name) > 64
                    or not re.fullmatch(r"on[A-Z][A-Za-z0-9]*", name)):
                raise IrError("component events need an on* name and a declared state with a matching literal")
            events[name] = _event_data(event, states)
        value: dict[str, Any] = {"kind": "element", "tag": node.tag, "attributes": attributes,
                "children": [_static_node(child, depth + 1, count, states) for child in node.children]}
        if events:
            value["events"] = events
        return value
    raise IrError("unsupported static component node")


@dataclass(frozen=True)
class ComponentState:
    name: str
    setter: str
    initial: Any

    @classmethod
    def from_data(cls, value: Any) -> ComponentState:
        row = _fields(value, {"name", "setter", "initial"})
        result = cls(row["name"], row["setter"], row["initial"])
        result.to_data()
        return result

    def to_data(self) -> dict[str, Any]:
        if (not isinstance(self.name, str) or not re.fullmatch(r"[a-z_][A-Za-z_0-9]{0,127}", self.name)
                or not isinstance(self.setter, str)
                or not re.fullmatch(r"[A-Za-z_][A-Za-z_0-9]{0,127}", self.setter)
                or not _state_literal(self.initial)):
            raise IrError("component state needs unique simple names and a bounded literal initial value")
        return {"name": self.name, "setter": self.setter, "initial": self.initial}


@dataclass(frozen=True)
class StaticComponent:
    name: str
    root: StaticNode
    client: bool = False
    state: Sequence[ComponentState] = ()

    @classmethod
    def from_data(cls, value: Any) -> StaticComponent:
        row = _fields(value, {"name", "root"}, {"client", "state"})
        raw_state = row.get("state", [])
        if not isinstance(raw_state, list):
            raise IrError("component state requires a list")
        component = cls(row["name"], _static_node_from_data(row["root"], 0, [0]),
                        row.get("client", False),
                        [ComponentState.from_data(item) for item in raw_state])
        component.to_data()
        return component

    def to_data(self) -> dict[str, Any]:
        if (not isinstance(self.name, str) or not re.fullmatch(r"[A-Z][A-Za-z0-9_]{0,127}", self.name)):
            raise IrError("static component needs a bounded upper-case identifier")
        if type(self.client) is not bool:
            raise IrError("component client boundary requires a boolean")
        if not isinstance(self.state, (list, tuple)) or len(self.state) > 16:
            raise IrError("component exceeds its state declaration bound")
        states: dict[str, Any] = {}
        setters: set[str] = set()
        serialized_state = []
        for entry in self.state:
            row = entry.to_data() if isinstance(entry, ComponentState) else ComponentState.from_data(entry).to_data()
            if row["name"] in states or row["setter"] in setters:
                raise IrError("component state names must be unique")
            states[row["name"]] = row["initial"]
            setters.add(row["setter"])
            serialized_state.append(row)
        if states and not self.client:
            raise IrError("component state requires an explicit client boundary")
        count = [0, 0]
        value: dict[str, Any] = {"name": self.name,
                                 "root": _static_node(self.root, 0, count, states)}
        if self.client:
            value["client"] = True
        if serialized_state:
            value["state"] = serialized_state
        encoded = _encoded(value, 1_048_576)
        if not static_resources_admitted(count[0], count[1], len(encoded.encode("utf-8"))):
            raise IrError("static component exceeds its resource policy")
        return value


@dataclass(frozen=True)
class ApplicationNode:
    id: str
    kind: str
    source: Any
    data: Mapping[str, Any]
    children: Sequence[ApplicationNode]
    route: HttpRoute | None = None
    boundary: str | None = None
    component: StaticComponent | None = None
    middleware: Sequence[Mapping[str, Any]] = ()


def _node_from_data(value: Any, depth: int, count: list[int]) -> ApplicationNode:
    count[0] += 1
    if depth > 64 or count[0] > 4096:
        raise IrError("application hierarchy exceeds its bounds")
    row = _fields(value, {"id", "kind", "source", "data", "children"},
                  {"route", "boundary", "component", "middleware"})
    if not isinstance(row["children"], list):
        raise IrError("application children require an array")
    route = None if row.get("route") is None else HttpRoute.from_data(row["route"])
    component = None if row.get("component") is None else StaticComponent.from_data(row["component"])
    middleware = _middleware_data(row.get("middleware", []))
    return ApplicationNode(row["id"], row["kind"], row["source"], row["data"],
                           [_node_from_data(child, depth + 1, count) for child in row["children"]],
                           route, row.get("boundary"), component, middleware)


def _node_data(node: ApplicationNode, depth: int, ids: set[str]) -> dict[str, Any]:
    if (not isinstance(node, ApplicationNode) or depth > 64 or len(ids) >= 4096
            or not isinstance(node.id, str) or not 1 <= len(node.id.encode("utf-8")) <= 256
            or node.id in ids or not isinstance(node.kind, str)
            or not isinstance(node.data, Mapping) or not all(isinstance(key, str) for key in node.data)
            or not isinstance(node.children, (list, tuple))
            or node.boundary is not None and not isinstance(node.boundary, str)):
        raise IrError("application hierarchy has invalid fields, duplicate identities or excess size")
    ids.add(node.id)
    result = {"id": node.id, "kind": node.kind, "source": node.source, "data": dict(node.data),
              "children": [_node_data(child, depth + 1, ids) for child in node.children]}
    if node.route is not None:
        result["route"] = node.route.to_data()
    if node.component is not None:
        result["component"] = node.component.to_data()
    if node.boundary is not None:
        result["boundary"] = node.boundary
    middleware = _middleware_data(list(node.middleware))
    if middleware:
        result["middleware"] = middleware
    return result


@dataclass(frozen=True)
class ApplicationIr:
    revision: str
    applications: Sequence[ApplicationNode]
    omissions: Any
    schema: str = field(default="fr-application-ir-1", init=False)
    runtime_proved: bool = field(default=False, init=False)

    @classmethod
    def from_data(cls, value: Any) -> ApplicationIr:
        _encoded(value, 4_194_304)
        row = _fields(value, {"schema", "revision", "applications", "omissions", "runtime_proved"})
        if (row["schema"] != "fr-application-ir-1" or row["runtime_proved"] is not False
                or not isinstance(row["applications"], list)):
            raise IrError("application schema or runtime proof claim is invalid")
        count = [0]
        ir = cls(row["revision"], [_node_from_data(node, 0, count) for node in row["applications"]], row["omissions"])
        ir.to_data()
        return ir

    def to_data(self) -> dict[str, Any]:
        if (not isinstance(self.revision, str) or not re.fullmatch(r"[0-9a-f]{64}", self.revision)
                or not isinstance(self.applications, (list, tuple))):
            raise IrError("application revision or hierarchy is invalid")
        ids: set[str] = set()
        value = {"schema": self.schema, "revision": self.revision,
                 "applications": [_node_data(node, 0, ids) for node in self.applications],
                 "omissions": self.omissions, "runtime_proved": False}
        _encoded(value, 4_194_304)
        return value

    def object_digest(self) -> str:
        return merkle_object_digest(self.to_data())
