"""Bounded control-flow evidence and verified, dependency-checked local reuse."""
from __future__ import annotations

from dataclasses import dataclass
import hashlib
import json
from pathlib import Path
import re
import tempfile
from typing import Any, Mapping

from .context import ObjectStore, restore_stored_value, store_merkle_value
from .investigation import FlowWitness, flow_witnesses
from .runtime import FrClient, FrReport, FrRuntimeError, Occurrence


@dataclass(frozen=True)
class ControlNode:
    id: int
    operation: str
    origins: tuple[Occurrence, ...]
    absent_reason: str | None


@dataclass(frozen=True)
class ControlEdge:
    source: int
    target: int
    rule: str


@dataclass(frozen=True)
class ControlGraph:
    function: str
    entry: int
    nodes: tuple[ControlNode, ...]
    edges: tuple[ControlEdge, ...]

    @classmethod
    def from_data(cls, value: Any, *, revision: str) -> ControlGraph:
        try:
            nodes = []
            for node in value["nodes"]:
                origin = node["origins"]
                status = origin["status"]
                if status == "exact":
                    occurrences = (Occurrence.from_data(origin["occurrence"]),)
                    reason = None
                elif status == "multiple":
                    occurrences = tuple(Occurrence.from_data(o) for o in origin["occurrences"])
                    reason = None
                    if len(occurrences) < 2:
                        raise FrRuntimeError("multiple origins need at least two occurrences")
                elif status == "absent":
                    occurrences = ()
                    reason = origin["reason"]
                    if not isinstance(reason, str) or not reason:
                        raise FrRuntimeError("absent origin needs a reason")
                else:
                    raise FrRuntimeError("unknown control-flow origin status")
                if any(o.revision != revision for o in occurrences):
                    raise FrRuntimeError("control-flow origin belongs to another revision")
                if type(node["id"]) is not int or node["id"] != len(nodes):
                    raise FrRuntimeError("control-flow node IDs must be contiguous")
                if node["operation"] not in {"entry", "exit", "exceptional-exit", "condition", "statement", "return", "raise", "jump"}:
                    raise FrRuntimeError("unknown control-flow operation")
                nodes.append(ControlNode(node["id"], node["operation"], occurrences, reason))
            ids = range(len(nodes))
            edges = []
            for edge in value["edges"]:
                if (type(edge["from"]) is not int or type(edge["to"]) is not int
                        or edge["from"] not in ids or edge["to"] not in ids
                        or edge["rule"] not in {"entry", "next", "true", "false", "return", "raise", "break", "continue"}):
                    raise FrRuntimeError("control-flow edge leaves its graph")
                edges.append(ControlEdge(edge["from"], edge["to"], edge["rule"]))
            if type(value["entry"]) is not int or value["entry"] not in ids:
                raise FrRuntimeError("control-flow entry leaves its graph")
            return cls(value["function"], value["entry"], tuple(nodes), tuple(edges))
        except (KeyError, TypeError, ValueError) as error:
            raise FrRuntimeError("malformed control-flow graph") from error


@dataclass(frozen=True)
class FlowAnalysis:
    report: FrReport

    @property
    def graphs(self) -> tuple[ControlGraph, ...]:
        data = self.report.to_data()
        return tuple(ControlGraph.from_data(g, revision=data["revision"])
                     for g in data["control_flow"].values())

    @property
    def witnesses(self) -> tuple[FlowWitness, ...]:
        return flow_witnesses(self.report)

    @property
    def reused(self) -> bool:
        return self.report.at("/execution/kind") == "retained"


class FlowCache:
    """Reuse trusted local records after native dependency and occurrence validation."""

    def __init__(self, store: ObjectStore, entries: Mapping[str, str] | None = None) -> None:
        self.store = store
        self._entries = dict(entries or {})
        if len(self._entries) > 256 or any(
            not isinstance(value, str) or re.fullmatch(r"[0-9a-f]{64}", value) is None
            for pair in self._entries.items() for value in pair
        ):
            raise FrRuntimeError("flow cache needs bounded digest mappings")

    def persist(self) -> str:
        return store_merkle_value(self.store, {
            "schema": "fr-flow-cache-1", "entries": self._entries,
        }).digest

    @classmethod
    def restore(cls, store: ObjectStore, digest: str) -> FlowCache:
        value = restore_stored_value(store, digest)
        if (not isinstance(value, dict) or set(value) != {"schema", "entries"}
                or value["schema"] != "fr-flow-cache-1" or not isinstance(value["entries"], dict)):
            raise FrRuntimeError("unsupported flow cache manifest")
        return cls(store, value["entries"])

    def analyze(self, client: FrClient, target: str, *, rules: Path | None = None,
                context: str = "generic", steps: int = 256, depth: int = 8,
                max_bytes: int = 65_536) -> FlowAnalysis:
        arguments = ["dataflow", target, "--steps", str(steps), "--depth", str(depth),
                     "--bytes", str(max_bytes), "--context", context]
        if rules is not None:
            arguments.extend(["--rules", str(rules)])
        basis = client.project(*arguments, "--inputs-only")
        if basis.schema != "fr-dataflow-inputs-1":
            raise FrRuntimeError("unsupported flow input contract")
        key = basis.at("/input_digest")
        retained = self._entries.get(key)
        if retained is None:
            report = client.project(*arguments)
        else:
            value = restore_stored_value(self.store, retained)
            encoded = json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(",", ":"))
            digest = hashlib.sha256(encoded.encode("utf-8")).hexdigest()
            with tempfile.TemporaryDirectory(prefix="fr-flow-") as directory:
                path = Path(directory) / "retained.json"
                path.write_text(encoded, encoding="utf-8")
                report = client.project(*arguments, "--reuse", str(path), "--reuse-digest", digest)
        if (report.schema != "fr-dataflow-1" or report.at("/input_digest") != key
                or report.at("/revision") != basis.at("/revision")):
            raise FrRuntimeError("flow inputs changed during analysis")
        analysis = FlowAnalysis(report)
        analysis.graphs
        analysis.witnesses
        if report.at("/complete") is True:
            self._entries[key] = store_merkle_value(self.store, report.to_data()).digest
        while len(self._entries) > 256:
            del self._entries[next(iter(self._entries))]
        return analysis
