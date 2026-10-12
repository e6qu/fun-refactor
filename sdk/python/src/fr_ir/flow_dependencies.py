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


def _special_attribute(value: str) -> bool:
    return value.startswith("__") and value.endswith("__")


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
class ImportResolution:
    module: str
    target: str | None
    packages: tuple[str, ...]
    candidates: tuple[ImportCandidate, ...]
    admitted: bool
    namespaces: tuple[str, ...]


def _resolution(item: dict[str, Any], files: dict[str, str], complete: bool,
                namespace_paths: frozenset[str]) -> ImportResolution:
    name = item["module"]
    if (not isinstance(name, str) or not 1 <= len(name.split(".")) <= 16
            or any(re.fullmatch(r"[A-Za-z_][A-Za-z_0-9]*", part) is None for part in name.split("."))
            or type(item["admitted"]) is not bool):
        raise FrRuntimeError("malformed module resolution")
    candidates = []
    for path, candidate in item["candidates"].items():
        status = candidate["status"]
        if status not in {"source", "missing", "outside-snapshot", "symlink", "directory"}:
            raise FrRuntimeError("unknown import candidate status")
        digest = _digest(candidate["digest"]) if status == "source" else None
        if status != "source" and "digest" in candidate:
            raise FrRuntimeError("non-source import candidate carries a digest")
        candidates.append(ImportCandidate(_path(path), status, digest))
        if path in files and digest != files[path]:
            raise FrRuntimeError("import candidate differs from its source input")
    statuses = {candidate.path: candidate.status for candidate in candidates}
    parts = name.split(".")
    expected: set[str] = set()
    packages = []
    namespaces = []
    admitted = True
    target = None
    for count in range(1, len(parts) + 1):
        stem = "/".join(parts[:count])
        module, package, stub, package_stub = f"{stem}.py", f"{stem}/__init__.py", f"{stem}.pyi", f"{stem}/__init__.pyi"
        expected.update((module, package, stub))
        expected.update((package_stub, stem))
        stubs_missing = statuses.get(stub) == "missing" and statuses.get(package_stub) == "missing"
        is_package = statuses.get(package) == "source" and statuses.get(module) == "missing" and stubs_missing
        is_module = count == len(parts) and statuses.get(module) == "source" and statuses.get(package) == "missing" and stubs_missing
        is_namespace = (statuses.get(stem) == "directory"
                        and statuses.get(module) == "missing" and statuses.get(package) == "missing" and stubs_missing)
        admitted = admitted and (is_package or is_module or is_namespace)
        if is_namespace:
            namespaces.append(stem)
        if count < len(parts):
            if is_package:
                packages.append(package)
        else:
            target = package if is_package else module if is_module else stem if is_namespace else None
    if not admitted:
        target = None
    if set(statuses) != expected:
        raise FrRuntimeError("import lookup must retain all candidate paths")
    if item["admitted"] != admitted or item["target"] != target or item["packages"] != packages:
        raise FrRuntimeError("import admission disagrees with its candidates")
    if item["namespaces"] != namespaces:
        raise FrRuntimeError("namespace paths disagree with their directory candidates")
    known = files.keys() | namespace_paths
    if complete and (not admitted or target not in known or not set(packages) <= files.keys()
                     or not set(namespaces) <= namespace_paths):
        raise FrRuntimeError("complete dependency closure omits an import")
    return ImportResolution(name, target, tuple(packages), tuple(candidates), admitted, tuple(namespaces))


@dataclass(frozen=True)
class ImportBinding:
    path: str
    name: str


@dataclass(frozen=True)
class ImportLookup:
    importer: str
    alias: str
    module: str
    member: str | None
    candidates: tuple[ImportCandidate, ...]
    admitted: bool
    resolution: str
    target: str | None
    packages: tuple[str, ...]
    prefix: str
    binding_chain: tuple[ImportBinding, ...]
    submodule: ImportResolution | None
    terminal_module: str | None
    namespaces: tuple[str, ...]
    star: int | None
    exports: tuple[str, ...] | None


def _binding_chains(lookups: list[ImportLookup], files: dict[str, str], namespaces: frozenset[str]) -> None:
    known_modules = files.keys() | namespaces
    bindings: dict[tuple[str, str], list[ImportLookup]] = {}
    for lookup in lookups:
        bindings.setdefault((lookup.importer, lookup.alias), []).append(lookup)
        chain = lookup.binding_chain
        if len(chain) > 16 or len(set(chain)) != len(chain):
            raise FrRuntimeError("import binding chain exceeds its budget or repeats a binding")
        if chain and (lookup.member is None or (chain[0].path, chain[0].name) != (lookup.target, lookup.member)):
            raise FrRuntimeError("import binding chain starts outside its selected member")
        if lookup.resolution == "function" and (not chain or any(hop.path not in files for hop in chain)):
            raise FrRuntimeError("resolved function lacks a source-bound binding chain")
        child = lookup.submodule
        if child is not None and (lookup.member is None or lookup.target is None
                or lookup.target not in (lookup.module.replace('.', '/') + '/__init__.py',
                                         lookup.module.replace('.', '/') if lookup.target in namespaces else None)
                or child.module != f"{lookup.module}.{lookup.member}"
                or lookup.member.startswith('__') and lookup.member.endswith('__')
                or chain or lookup.resolution not in {"module", "unavailable"}):
            raise FrRuntimeError("child module lookup disagrees with its package member")
        if lookup.resolution == "module":
            if lookup.terminal_module not in known_modules or any(hop.path not in files for hop in chain):
                raise FrRuntimeError("module alias lacks a source-bound terminal")
            if chain:
                if child is not None:
                    raise FrRuntimeError("child module lookup carries an attribute chain")
            elif lookup.terminal_module != (child.target if child is not None else lookup.target):
                raise FrRuntimeError("module alias terminal differs from its selected module")
            elif lookup.member is not None and child is None:
                raise FrRuntimeError("module re-export omits its binding chain")
        elif lookup.terminal_module is not None:
            raise FrRuntimeError("non-module lookup carries a terminal module")
    for lookup in lookups:
        if lookup.resolution not in {"function", "module"}:
            continue
        for index, hop in enumerate(lookup.binding_chain):
            following = bindings.get((hop.path, hop.name), [])
            suffix = lookup.binding_chain[index + 1:]
            if not suffix and lookup.resolution == "function":
                if following:
                    raise FrRuntimeError("function binding chain ends at another import")
            elif (len(following) != 1 or following[0].resolution != lookup.resolution
                    or following[0].binding_chain != suffix
                    or following[0].terminal_module != lookup.terminal_module
                    or lookup.resolution == "module" and following[0].prefix != following[0].alias):
                raise FrRuntimeError("binding chain disagrees with its re-export lookup")


@dataclass(frozen=True)
class FlowDependencies:
    files: tuple[tuple[str, str], ...]
    lookups: tuple[ImportLookup, ...]
    complete: bool
    cutoffs: tuple[str, ...]
    _dependency: Dependency
    entry: ImportResolution
    namespaces: tuple[str, ...]

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
                    or modules["schema"] != "fr-flow-modules"
                    or type(modules["complete"]) is not bool
                    or not isinstance(modules["cutoffs"], list)
                    or any(not isinstance(cutoff, str) for cutoff in modules["cutoffs"])
                    or modules["complete"] != (not modules["cutoffs"])):
                raise FrRuntimeError("report lacks consistent module dependencies")
            files = tuple((_path(path), _digest(digest)) for path, digest in modules["files"].items())
            if not 1 <= len(files) <= 16:
                raise FrRuntimeError("module dependency count exceeds its budget")
            paths = dict(files)
            if not isinstance(modules["namespaces"], list):
                raise FrRuntimeError("namespace closure must be a list")
            namespaces = tuple(_path(path) for path in modules["namespaces"])
            if (tuple(sorted(set(namespaces))) != namespaces or set(namespaces) & paths.keys()
                    or len(files) + len(namespaces) > 16):
                raise FrRuntimeError("namespace closure exceeds its budget or repeats a module")
            namespace_paths = frozenset(namespaces)
            entry = _resolution(modules["entry"], paths, modules["complete"], namespace_paths=namespace_paths)
            if modules["complete"] and any(
                    _special_attribute(part) for part in entry.module.split(".")[1:]):
                raise FrRuntimeError("complete closure includes a special child module")
            lookups = []
            for item in modules["lookups"]:
                wildcard = item["alias"] == "*"
                exports, star = item["exports"], item["star"]
                if wildcard:
                    if (star is not None or item["member"] is not None or item["prefix"] != "*"
                            or item["binding_chain"] or item["submodule"] is not None
                            or exports is not None and (not isinstance(exports, list) or len(exports) > 128
                                or any(not isinstance(name, str) or re.fullmatch(r"[A-Za-z_][A-Za-z_0-9]*", name) is None for name in exports)
                                or exports != sorted(set(exports)))
                            or modules["complete"] and exports is None):
                        raise FrRuntimeError("malformed star import declaration")
                elif (exports is not None or star is not None and (type(star) is not int or not 0 <= star < len(lookups))):
                    raise FrRuntimeError("malformed star import expansion")
                if (item["importer"] not in paths or type(item["admitted"]) is not bool
                        or not wildcard and any(not isinstance(item[field], str) or re.fullmatch(r"[A-Za-z_][A-Za-z_0-9]*", item[field]) is None
                               for field in ("alias",))
                        or (item["member"] is not None and (not isinstance(item["member"], str)
                            or re.fullmatch(r"[A-Za-z_][A-Za-z_0-9]*", item["member"]) is None))):
                    raise FrRuntimeError("malformed import lookup")
                selected = _resolution(item, paths, modules["complete"], namespace_paths)
                if wildcard and exports is not None and (not selected.admitted or not any(
                        candidate.path == selected.target and candidate.status == "source" for candidate in selected.candidates)):
                    raise FrRuntimeError("star export list lacks a source module")
                if modules["complete"] and (
                        _special_attribute(item["alias"])
                        or item["member"] is not None and _special_attribute(item["member"])
                        or any(_special_attribute(part) for part in selected.module.split(".")[1:])):
                    raise FrRuntimeError("complete closure includes special module attributes")
                prefix = item["prefix"]
                if (prefix not in (item["alias"], selected.module) or prefix.split(".")[0] != item["alias"]
                        or item["member"] is not None and prefix != item["alias"]):
                    raise FrRuntimeError("import binding prefix disagrees with its alias")
                resolution = item["resolution"]
                if (resolution not in {"module", "function", "missing", "ambiguous", "unavailable", "cyclic", "budget"}
                        or wildcard and resolution not in {"module", "unavailable"}
                        or modules["complete"] and resolution not in {"module", "function"}
                        or (resolution in {"function", "missing", "ambiguous"} and item["member"] is None)):
                    raise FrRuntimeError("import resolution disagrees with its declaration")
                lookups.append(ImportLookup(item["importer"], item["alias"], selected.module, item["member"],
                                            selected.candidates, selected.admitted, resolution,
                                            selected.target, selected.packages, prefix,
                                            tuple(ImportBinding(_path(hop["path"]), hop["name"])
                                                  for hop in item["binding_chain"]),
                                            _resolution(item["submodule"], paths, modules["complete"], namespace_paths)
                                            if item["submodule"] is not None else None,
                                            _path(item["terminal_module"]) if item["terminal_module"] is not None else None,
                                            selected.namespaces, star, tuple(exports) if exports is not None else None))
            for index, anchor in enumerate(lookups):
                if anchor.star is not None:
                    declaration = lookups[anchor.star]
                    if (declaration.alias != "*" or declaration.exports is None
                            or anchor.member != anchor.alias or anchor.alias not in declaration.exports
                            or (anchor.importer, anchor.module, anchor.target, anchor.candidates, anchor.admitted, anchor.packages, anchor.namespaces)
                            != (declaration.importer, declaration.module, declaration.target, declaration.candidates, declaration.admitted, declaration.packages, declaration.namespaces)):
                        raise FrRuntimeError("star member differs from its declaration")
                if anchor.alias == "*":
                    expanded = [lookup.alias for lookup in lookups if lookup.star == index]
                    if sorted(expanded) != list(anchor.exports or ()):
                        raise FrRuntimeError("star import expansion omits or repeats an export")
            if any(not isinstance(hop.name, str) or re.fullmatch(r"[A-Za-z_][A-Za-z_0-9]*", hop.name) is None
                   for lookup in lookups for hop in lookup.binding_chain):
                raise FrRuntimeError("invalid re-export member name")
            _binding_chains(lookups, paths, frozenset(namespaces))
            if len(lookups) > 128:
                raise FrRuntimeError("import lookup count exceeds its budget")
            resolutions = [entry, *lookups, *(item.submodule for item in lookups if item.submodule is not None)]
            expected_namespaces = {path for resolution in resolutions if resolution is not None and resolution.admitted
                                   for path in resolution.namespaces}
            if (not set(namespaces) <= expected_namespaces
                    or modules["complete"] and set(namespaces) != expected_namespaces):
                raise FrRuntimeError("namespace closure disagrees with its admitted lookups")
            source = inputs["source"]
            if source["path"] not in paths or source["digest"] != paths[source["path"]]:
                raise FrRuntimeError("entry source differs from its dependency closure")
            if not entry.admitted or entry.target != source["path"]:
                raise FrRuntimeError("entry source differs from its module resolution")
            rules = inputs["rule_file"]
            query = {"path": _path(source["path"]), "function": inputs["selection"]["name"],
                     "rules": rules, "context": inputs["context"], **inputs["budget"]}
            dependency = Dependency(DependencyKind.FLOW_INPUTS, json.dumps(query, sort_keys=True, separators=(",", ":")),
                                    _digest(data["input_digest"]))
            return cls(files, tuple(lookups), modules["complete"], tuple(modules["cutoffs"]), dependency, entry, namespaces)
        except (KeyError, TypeError, ValueError, AttributeError) as error:
            raise FrRuntimeError("malformed flow dependencies") from error
