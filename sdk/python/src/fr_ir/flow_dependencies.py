"""Declared module lookups and resumable flow input dependencies."""
from __future__ import annotations

from dataclasses import dataclass
import hashlib
import json
from pathlib import PurePosixPath
import re
from typing import Any

from .investigation import Dependency, DependencyKind
from .runtime import FrReport, FrRuntimeError


def _path(value: Any) -> str:
    if (not isinstance(value, str) or not value or "\\" in value
            or PurePosixPath(value).is_absolute()
            or any(part in {"", ".", ".."} for part in value.split("/"))):
        raise FrRuntimeError("flow dependency path must be normalized and relative")
    return value


def _digest(value: Any) -> str:
    if not isinstance(value, str) or re.fullmatch(r"[0-9a-f]{64}", value) is None:
        raise FrRuntimeError("flow dependency needs a digest")
    return value


@dataclass(frozen=True)
class ImportCandidate:
    path: str
    status: str
    digest: str | None


@dataclass(frozen=True)
class ImportLookup:
    importer: str
    alias: str
    module: str
    member: str | None
    candidates: tuple[ImportCandidate, ...]
    admitted: bool
    resolution: str


@dataclass(frozen=True)
class FlowDependencies:
    files: tuple[tuple[str, str], ...]
    lookups: tuple[ImportLookup, ...]
    complete: bool
    cutoffs: tuple[str, ...]
    _dependency: Dependency

    @property
    def dependency(self) -> Dependency:
        rules = json.loads(self._dependency.key)["rules"]
        if rules is not None:
            _path(rules)
        return self._dependency

    @classmethod
    def from_report(cls, report: FrReport) -> FlowDependencies:
        try:
            data = report.to_data()
            inputs = data["inputs"]
            modules = inputs["modules"]
            encoded = json.dumps(inputs, sort_keys=True, ensure_ascii=False, separators=(",", ":")).encode()
            if _digest(data["input_digest"]) != hashlib.sha256(encoded).hexdigest():
                raise FrRuntimeError("flow input digest differs from its dependencies")
            if (report.schema != "fr-dataflow-1" or inputs["summary_mode"] is not True
                    or modules["schema"] != "fr-flow-modules-1"
                    or type(modules["complete"]) is not bool
                    or not isinstance(modules["cutoffs"], list)
                    or any(not isinstance(cutoff, str) for cutoff in modules["cutoffs"])
                    or modules["complete"] != (not modules["cutoffs"])):
                raise FrRuntimeError("report lacks consistent module dependencies")
            files = tuple((_path(path), _digest(digest)) for path, digest in modules["files"].items())
            if not 1 <= len(files) <= 16:
                raise FrRuntimeError("module dependency count exceeds its budget")
            paths = dict(files)
            lookups = []
            for item in modules["lookups"]:
                if (item["importer"] not in paths or type(item["admitted"]) is not bool
                        or any(not isinstance(item[field], str) or re.fullmatch(r"[A-Za-z_][A-Za-z_0-9]*", item[field]) is None
                               for field in ("alias", "module"))
                        or (item["member"] is not None and (not isinstance(item["member"], str)
                            or re.fullmatch(r"[A-Za-z_][A-Za-z_0-9]*", item["member"]) is None))):
                    raise FrRuntimeError("malformed import lookup")
                candidates = []
                for path, candidate in item["candidates"].items():
                    status = candidate["status"]
                    if status not in {"source", "missing", "outside-snapshot", "symlink"}:
                        raise FrRuntimeError("unknown import candidate status")
                    digest = _digest(candidate["digest"]) if status == "source" else None
                    candidates.append(ImportCandidate(_path(path), status, digest))
                name = item["module"]
                expected = {f"{name}.py", f"{name}.pyi", f"{name}/__init__.py"}
                if {candidate.path for candidate in candidates} != expected:
                    raise FrRuntimeError("import lookup must retain all candidate paths")
                admitted = all(candidate.status == ("source" if candidate.path == f"{name}.py" else "missing")
                               for candidate in candidates)
                if item["admitted"] != admitted:
                    raise FrRuntimeError("import admission disagrees with its candidates")
                if modules["complete"] and (not admitted or f"{name}.py" not in paths):
                    raise FrRuntimeError("complete dependency closure omits an import")
                for candidate in candidates:
                    if candidate.path in paths and candidate.digest != paths[candidate.path]:
                        raise FrRuntimeError("import candidate differs from its source input")
                resolution = item["resolution"]
                if (resolution not in {"module", "function", "missing", "ambiguous", "unavailable"}
                        or modules["complete"] and resolution not in {"module", "function"}
                        or (resolution == "module" and item["member"] is not None)
                        or (resolution in {"function", "missing", "ambiguous"} and item["member"] is None)):
                    raise FrRuntimeError("import resolution disagrees with its declaration")
                lookups.append(ImportLookup(item["importer"], item["alias"], name, item["member"], tuple(candidates), admitted, resolution))
            if len(lookups) > 128:
                raise FrRuntimeError("import lookup count exceeds its budget")
            source = inputs["source"]
            if source["path"] not in paths or source["digest"] != paths[source["path"]]:
                raise FrRuntimeError("entry source differs from its dependency closure")
            rules = inputs["rule_file"]
            query = {"path": _path(source["path"]), "function": inputs["selection"]["name"],
                     "rules": rules, "context": inputs["context"], **inputs["budget"]}
            dependency = Dependency(DependencyKind.FLOW_INPUTS, json.dumps(query, sort_keys=True, separators=(",", ":")),
                                    _digest(data["input_digest"]))
            return cls(files, tuple(lookups), modules["complete"], tuple(modules["cutoffs"]), dependency)
        except (KeyError, TypeError, ValueError, AttributeError) as error:
            raise FrRuntimeError("malformed flow dependencies") from error
