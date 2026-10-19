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
from .flow_standard_library import MEMBERS, selected_contract

MODULE_POLICY = ("Single-root modules/packages; local from-import names share a provider search. "
                 "Child searches use the same budget. Explicit standard model bindings each use one lookup and require absent local candidates. "
                 "Cycles require available names in execution order. "
                 "Annotation directives need absent local __future__ candidates. "
                 "Dynamic imports, merged namespaces and search paths stay unchecked.")


def _expanded_lookups(modules: dict[str, Any]) -> tuple[list[dict[str, Any]], int]:
    limits = modules["limits"]
    if (modules["policy"] != MODULE_POLICY or not isinstance(limits, dict)
            or any(type(value) is not int for value in limits.values()) or limits != {
            "modules": 16, "source_bytes": 262144, "imports": 128, "names_per_import": 128,
            "module_components": 16, "binding_chain": 16}):
        raise FrRuntimeError("unsupported module lookup contract")
    records = modules["lookups"]
    if not isinstance(records, list) or len(records) > 128:
        raise FrRuntimeError("import lookup count exceeds its budget")
    fields = {"module", "candidates", "opaque_candidates", "packages", "namespaces", "target", "admitted", "importer", "alias",
              "member", "prefix", "exports", "future", "submodule", "resolution", "binding_chain", "terminal_module", "members", "names"}
    member_fields = {"name", "resolution", "binding_chain", "submodule", "terminal_module"}
    named_fields = {"alias", "member", "prefix", "resolution", "binding_chain", "submodule", "terminal_module"}
    expanded = []
    count = len(records)
    for record in records:
        if not isinstance(record, dict) or set(record) != fields:
            raise FrRuntimeError("malformed compact import lookup")
        members = record["members"]
        wildcard = record["alias"] == "*"
        named = not wildcard and record["member"] is not None
        names = record["names"]
        if not isinstance(members, list) or len(members) > 128 or not wildcard and record["exports"] is not None:
            raise FrRuntimeError("malformed import member inventory")
        if named:
            if (not isinstance(names, list) or not 1 <= len(names) <= 128 or len(members) + 1 != len(names)
                    or names[0] != [record["alias"], record["member"]] or record["prefix"] != record["alias"]
                    or any(not isinstance(pair, list) or len(pair) != 2 or any(not isinstance(name, str)
                        or re.fullmatch(r"[A-Za-z_][A-Za-z_0-9]*", name) is None for name in pair) for pair in names)):
                raise FrRuntimeError("named import inventory differs from its bindings")
        elif names is not None or not wildcard and members:
            raise FrRuntimeError("unexpected named import inventory")
        if wildcard and (record["exports"] is not None and not isinstance(record["exports"], list)
                or [member.get("name") if isinstance(member, dict) else None for member in members] != (record["exports"] or [])):
            raise FrRuntimeError("wildcard member inventory differs from its exports")
        anchor = {key: value for key, value in record.items() if key not in {"members", "names"}}
        anchor["star"] = None
        count += anchor["submodule"] is not None
        index = len(expanded)
        expanded.append(anchor)
        for offset, member in enumerate(members):
            if not isinstance(member, dict) or set(member) != (member_fields if wildcard else named_fields):
                raise FrRuntimeError("malformed shared import binding")
            count += member["submodule"] is not None
            binding = {**anchor, **{key: value for key, value in member.items() if key != "name"}}
            if wildcard:
                binding.update(alias=member["name"], member=member["name"], prefix=member["name"], star=index, exports=None)
            elif names[offset + 1] != [member["alias"], member["member"]] or member["prefix"] != member["alias"]:
                raise FrRuntimeError("named member differs from its requested binding")
            expanded.append(binding)
    if count > 128:
        raise FrRuntimeError("package-child searches exceed the import budget")
    return expanded, count


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
    opaque_candidates: tuple[str, ...]


@dataclass(frozen=True)
class StandardImport:
    importer: str
    module: str
    alias: str
    member: str | None
    candidates: tuple[ImportCandidate, ...]


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
    raw_opaque = item['opaque_candidates']
    if not isinstance(raw_opaque, list) or len(raw_opaque) > 128:
        raise FrRuntimeError('opaque import candidate count exceeds its budget')
    opaque = tuple(_path(path) for path in raw_opaque)
    if tuple(sorted(set(opaque))) != opaque:
        raise FrRuntimeError('opaque import candidates repeat or reorder a path')
    stems = ['/'.join(parts[:count]) for count in range(1, len(parts) + 1)]
    stems += [stem + '/__init__' for stem in stems]
    if any(not any(path == stem + '.pyc' or path.startswith(stem + '.') and '/' not in path[len(stem) + 1:]
                   and path.endswith(('.so', '.pyd', '.dll')) for stem in stems) for path in opaque):
        raise FrRuntimeError('opaque candidate lies outside its imported name')
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
    admitted = admitted and not opaque
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
    return ImportResolution(name, target, tuple(packages), tuple(candidates), admitted, tuple(namespaces), opaque)


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
    future: bool
    opaque_candidates: tuple[str, ...]


@dataclass(frozen=True)
class FunctionAlias:
    path: str
    name: str
    target: str
    offset: int


@dataclass(frozen=True)
class ValueAlias:
    """A second name for an earlier checked value in one module."""
    path: str
    name: str
    target: str
    offset: int


@dataclass(frozen=True)
class ValueBinding:
    path: str
    name: str
    offset: int
    kind: str


def _binding_chains(lookups: list[ImportLookup], files: dict[str, str], namespaces: frozenset[str],
                    aliases: tuple[FunctionAlias, ...], value_aliases: tuple[ValueAlias, ...],
                    values: tuple[ValueBinding, ...]) -> None:
    known_modules = files.keys() | namespaces
    assignments = {(alias.path, alias.name): alias for alias in aliases}
    value_assignments = {(alias.path, alias.name): alias for alias in value_aliases}
    if set(assignments) & set(value_assignments):
        raise FrRuntimeError("function and value aliases overlap")
    all_assignments = {**assignments, **value_assignments}
    declarations = {(value.path, value.name) for value in values}
    if set(all_assignments) & declarations:
        raise FrRuntimeError("alias overlaps a value declaration")
    referenced = set()
    bindings: dict[tuple[str, str], list[ImportLookup]] = {}
    for lookup in lookups:
        bindings.setdefault((lookup.importer, lookup.alias), []).append(lookup)
        chain = lookup.binding_chain
        if len(chain) > 16 or len(set(chain)) != len(chain):
            raise FrRuntimeError("import binding chain exceeds its budget or repeats a binding")
        if chain and (lookup.member is None or (chain[0].path, chain[0].name) != (lookup.target, lookup.member)):
            raise FrRuntimeError("import binding chain starts outside its selected member")
        if lookup.resolution in {"function", "value"} and (not chain or any(hop.path not in files for hop in chain)):
            raise FrRuntimeError("resolved member lacks a source-bound binding chain")
        if lookup.resolution in {"function", "value"}:
            terminal = (chain[-1].path, chain[-1].name)
            if (terminal in declarations) != (lookup.resolution == "value"):
                raise FrRuntimeError("imported value differs from its declaration")
            if lookup.resolution == "value":
                referenced.add(terminal)
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
        if lookup.resolution not in {"function", "value", "module"}:
            continue
        for index, hop in enumerate(lookup.binding_chain):
            following = bindings.get((hop.path, hop.name), [])
            suffix = lookup.binding_chain[index + 1:]
            key = (hop.path, hop.name)
            assignment = all_assignments.get(key)
            if assignment is not None:
                role = "value" if key in value_assignments else "function"
                if (lookup.resolution != role or following or not suffix
                        or (suffix[0].path, suffix[0].name) != (assignment.path, assignment.target)):
                    raise FrRuntimeError("alias chain disagrees with its assignment")
                continue
            if not suffix and lookup.resolution in {"function", "value"}:
                if following:
                    raise FrRuntimeError("member binding chain ends at another import")
            elif (len(following) != 1 or following[0].resolution != lookup.resolution
                    or following[0].binding_chain != suffix
                    or following[0].terminal_module != lookup.terminal_module
                    or lookup.resolution == "module" and following[0].prefix != following[0].alias):
                raise FrRuntimeError("binding chain disagrees with its re-export lookup")
    if referenced != declarations:
        raise FrRuntimeError("value inventory differs from its referenced imports")
    for assignment in (*aliases, *value_aliases):
        role = "value" if isinstance(assignment, ValueAlias) else "function"
        if (assignment.path, assignment.name) in bindings:
            raise FrRuntimeError("alias overlaps an import binding")
        current = assignment
        seen: set[tuple[str, str]] = set()
        for _ in range(16):
            key = (current.path, current.name)
            if key in seen:
                raise FrRuntimeError(f"{role} alias repeats a binding")
            seen.add(key)
            target = (current.path, current.target)
            next_alias = all_assignments.get(target)
            if next_alias is not None:
                if (isinstance(next_alias, ValueAlias) != (role == "value")
                        or next_alias.offset >= current.offset):
                    raise FrRuntimeError("alias changes kind or refers to a later assignment")
                current = next_alias
                continue
            imported = bindings.get(target, [])
            if imported:
                if (len(imported) != 1 or imported[0].resolution != role
                        or len(seen) + 1 + len(imported[0].binding_chain) > 16):
                    raise FrRuntimeError("alias lacks a bounded target of its declared kind")
            elif len(seen) + 1 > 16:
                raise FrRuntimeError(f"{role} alias exceeds its binding budget")
            break
        else:
            raise FrRuntimeError(f"{role} alias exceeds its binding budget")


@dataclass(frozen=True)
class FlowDependencies:
    """Source dependencies; lookups expand per-name evidence, lookup_count counts module searches."""
    files: tuple[tuple[str, str], ...]
    lookups: tuple[ImportLookup, ...]
    function_aliases: tuple[FunctionAlias, ...]
    value_aliases: tuple[ValueAlias, ...]
    value_bindings: tuple[ValueBinding, ...]
    complete: bool
    cutoffs: tuple[str, ...]
    _dependency: Dependency
    entry: ImportResolution
    namespaces: tuple[str, ...]
    lookup_count: int
    standard_library: tuple[str, ...]
    standard_imports: tuple[StandardImport, ...]
    standard_shadowing: tuple[str, ...]

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
            raw_values = modules["value_bindings"]
            if not isinstance(raw_values, list) or len(raw_values) > 128:
                raise FrRuntimeError("value binding inventory exceeds its budget")
            values = []
            for value in raw_values:
                if (not isinstance(value, dict) or set(value) != {"path", "name", "offset", "kind"}
                        or value["path"] not in paths
                        or not isinstance(value["name"], str)
                        or re.fullmatch(r"[A-Za-z_][A-Za-z_0-9]*", value["name"]) is None
                        or _special_attribute(value["name"])
                        or value["kind"] not in {"literal", "class", "instance", "registry", "allocation"}
                        or type(value["offset"]) is not int or not 0 <= value["offset"] < 262144):
                    raise FrRuntimeError("malformed value binding")
                values.append(ValueBinding(value["path"], value["name"], value["offset"], value["kind"]))
            value_bindings = tuple(values)
            if (len({(value.path, value.name) for value in value_bindings}) != len(value_bindings)
                    or len({(value.path, value.offset) for value in value_bindings}) != len(value_bindings)
                    or [(value.path, value.offset) for value in value_bindings]
                       != sorted((value.path, value.offset) for value in value_bindings)):
                raise FrRuntimeError("value binding inventory repeats or reorders a declaration")
            raw_aliases = modules["function_aliases"]
            if not isinstance(raw_aliases, list) or len(raw_aliases) > 128:
                raise FrRuntimeError("function alias inventory exceeds its budget")
            aliases = []
            for alias in raw_aliases:
                if (not isinstance(alias, dict) or set(alias) != {"path", "name", "target", "offset"}
                        or alias["path"] not in paths
                        or any(not isinstance(alias[field], str)
                               or re.fullmatch(r"[A-Za-z_][A-Za-z_0-9]*", alias[field]) is None
                               for field in ("name", "target"))
                        or _special_attribute(alias["name"])
                        or type(alias["offset"]) is not int or not 0 <= alias["offset"] < 262144):
                    raise FrRuntimeError("malformed function alias")
                aliases.append(FunctionAlias(alias["path"], alias["name"], alias["target"], alias["offset"]))
            function_aliases = tuple(aliases)
            if (len({(alias.path, alias.name) for alias in function_aliases}) != len(function_aliases)
                    or len({(alias.path, alias.offset) for alias in function_aliases}) != len(function_aliases)
                    or [(alias.path, alias.offset) for alias in function_aliases]
                       != sorted((alias.path, alias.offset) for alias in function_aliases)):
                raise FrRuntimeError("function alias inventory repeats or reorders a declaration")
            raw_value_aliases = modules["value_aliases"]
            if not isinstance(raw_value_aliases, list) or len(raw_aliases) + len(raw_value_aliases) > 128:
                raise FrRuntimeError("combined alias inventory exceeds its budget")
            value_alias_list = []
            for alias in raw_value_aliases:
                if (not isinstance(alias, dict) or set(alias) != {"path", "name", "target", "offset"}
                        or alias["path"] not in paths
                        or any(not isinstance(alias[field], str)
                               or re.fullmatch(r"[A-Za-z_][A-Za-z_0-9]*", alias[field]) is None
                               for field in ("name", "target"))
                        or _special_attribute(alias["name"])
                        or type(alias["offset"]) is not int or not 0 <= alias["offset"] < 262144):
                    raise FrRuntimeError("malformed value alias")
                value_alias_list.append(ValueAlias(alias["path"], alias["name"], alias["target"], alias["offset"]))
            value_aliases = tuple(value_alias_list)
            all_aliases = (*function_aliases, *value_aliases)
            if (len({(alias.path, alias.name) for alias in all_aliases}) != len(all_aliases)
                    or len({(alias.path, alias.offset) for alias in all_aliases}) != len(all_aliases)
                    or [(alias.path, alias.offset) for alias in value_aliases]
                       != sorted((alias.path, alias.offset) for alias in value_aliases)):
                raise FrRuntimeError("value alias inventory repeats or reorders a declaration")
            if not isinstance(modules["namespaces"], list):
                raise FrRuntimeError("namespace closure must be a list")
            namespaces = tuple(_path(path) for path in modules["namespaces"])
            if (tuple(sorted(set(namespaces))) != namespaces or set(namespaces) & paths.keys()
                    or len(files) + len(namespaces) > 16):
                raise FrRuntimeError("namespace closure exceeds its budget or repeats a module")
            namespace_paths = frozenset(namespaces)
            standard_library = selected_contract(modules['standard_library'])
            raw_shadows = modules['standard_shadowing']
            if not isinstance(raw_shadows, list) or len(raw_shadows) > 4096:
                raise FrRuntimeError('malformed standard-library namespace guard')
            standard_shadowing = tuple(_path(path) for path in raw_shadows)
            if (tuple(sorted(set(standard_shadowing))) != standard_shadowing
                    or any(len(PurePosixPath(path).parts) != 1 for path in standard_shadowing)
                    or standard_shadowing and not any(name != 'builtins' for name in standard_library)
                    or modules['complete'] and standard_shadowing):
                raise FrRuntimeError('standard-library namespace guard hides a competing candidate')
            raw_standard = modules['standard_imports']
            if not isinstance(raw_standard, list) or len(raw_standard) > 128:
                raise FrRuntimeError('standard-library import count exceeds its budget')
            standard_imports = []
            standard_bindings = set()
            for item in raw_standard:
                if (not isinstance(item, dict) or set(item) != {'module', 'candidates', 'opaque_candidates', 'packages', 'namespaces', 'target',
                        'admitted', 'importer', 'alias', 'member', 'prefix'}
                        or item['module'] not in standard_library or item['importer'] not in paths
                        or not isinstance(item['alias'], str) or re.fullmatch(r'[A-Za-z_][A-Za-z_0-9]*', item['alias']) is None
                        or _special_attribute(item['alias']) or item['prefix'] != item['alias']
                        or item['member'] is not None and item['member'] not in MEMBERS[item['module']]):
                    raise FrRuntimeError('malformed standard-library import')
                selected = _resolution(item, paths, False, namespace_paths)
                if (selected.admitted or selected.target is not None or selected.packages or selected.namespaces or selected.opaque_candidates
                        or any(candidate.status != 'missing' for candidate in selected.candidates)):
                    raise FrRuntimeError('standard-library import hides a competing local candidate')
                binding = item['importer'], item['alias']
                if modules['complete'] and binding in standard_bindings:
                    raise FrRuntimeError('complete standard-library imports repeat a binding')
                standard_bindings.add(binding)
                standard_imports.append(StandardImport(item['importer'], selected.module, item['alias'], item['member'], selected.candidates))
            entry = _resolution(modules["entry"], paths, modules["complete"], namespace_paths=namespace_paths)
            if modules["complete"] and any(
                    _special_attribute(part) for part in entry.module.split(".")[1:]):
                raise FrRuntimeError("complete closure includes a special child module")
            records, lookup_count = _expanded_lookups(modules)
            lookup_count += len(standard_imports)
            if lookup_count > 128:
                raise FrRuntimeError('combined standard and local import count exceeds its budget')
            lookups = []
            for item in records:
                future = item["future"]
                if type(future) is not bool:
                    raise FrRuntimeError("future import marker must be a boolean")
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
                selected = _resolution(item, paths, modules["complete"] and not future, namespace_paths)
                if future and (selected.module != "__future__"
                        or item["alias"] != "annotations" or item["member"] != "annotations"
                        or item["prefix"] != "annotations" or item["resolution"] != "future"
                        or any(candidate.status != "missing" for candidate in selected.candidates) or selected.opaque_candidates
                        or item["binding_chain"] or item["submodule"] is not None
                        or item["terminal_module"] is not None or star is not None or exports is not None):
                    raise FrRuntimeError("postponed annotations lack an unshadowed future directive")
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
                if (resolution not in {"module", "function", "value", "future", "missing", "ambiguous", "unavailable", "cyclic", "budget"}
                        or (resolution == "future") != future
                        or wildcard and resolution not in {"module", "unavailable"}
                        or modules["complete"] and resolution not in {"module", "function", "value", "future"}
                        or (resolution in {"function", "value", "missing", "ambiguous"} and item["member"] is None)):
                    raise FrRuntimeError("import resolution disagrees with its declaration")
                lookups.append(ImportLookup(item["importer"], item["alias"], selected.module, item["member"],
                                            selected.candidates, selected.admitted, resolution,
                                            selected.target, selected.packages, prefix,
                                            tuple(ImportBinding(_path(hop["path"]), hop["name"])
                                                  for hop in item["binding_chain"]),
                                            _resolution(item["submodule"], paths, modules["complete"], namespace_paths)
                                            if item["submodule"] is not None else None,
                                            _path(item["terminal_module"]) if item["terminal_module"] is not None else None,
                                            selected.namespaces, star, tuple(exports) if exports is not None else None, future, selected.opaque_candidates))
            for index, anchor in enumerate(lookups):
                if modules['complete'] and (anchor.importer, anchor.alias) in standard_bindings:
                    raise FrRuntimeError('standard-library binding overlaps a local import')
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
            _binding_chains(lookups, paths, frozenset(namespaces), function_aliases, value_aliases, value_bindings)
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
                     "rules": rules, "context": inputs["context"], **inputs["budget"],
                     "standard_library": list(standard_library)}
            dependency = Dependency(DependencyKind.FLOW_INPUTS, json.dumps(query, sort_keys=True, separators=(",", ":")),
                                    _digest(data["input_digest"]))
            return cls(files, tuple(lookups), function_aliases, value_aliases, value_bindings, modules["complete"], tuple(modules["cutoffs"]), dependency, entry, namespaces, lookup_count, standard_library, tuple(standard_imports), standard_shadowing)
        except (KeyError, TypeError, ValueError, AttributeError) as error:
            raise FrRuntimeError("malformed flow dependencies") from error
