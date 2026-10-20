"""Explain recorded import gaps without changing the analysis or its claims."""
from __future__ import annotations

import argparse
from dataclasses import dataclass
import itertools
import json
from pathlib import Path
from typing import Any

from .flow_dependencies import FlowDependencies, ImportLookup
from .flow_initialization import FlowInitialization
from .runtime import FrReport, FrRuntimeError


_LIMITS = {
    "import-module-budget": "The search reached the existing limit of 16 source files and package directories.",
    "import-source-byte-budget": "The search reached the existing limit of 256 KiB (262,144 bytes) of source text.",
    "import-lookup-budget": "The search reached the existing limit of 128 module searches.",
    "import-binding-budget": "An import exceeded the existing limit of 128 names.",
    "import-component-budget": "An import exceeded the existing limit of 16 dotted name parts. For example, 'a.b' has two parts.",
}
_MESSAGES = {
    "unread-source": "Selected source exists but was not analyzed. This does not establish a missing name.",
    "local-module": "The analyzer could not select one supported local source file or package. An installed or standard-library module may still exist.",
    "missing-binding": "No supported definition for this name was found in the loaded source. Unchecked import-time code can still affect its runtime meaning.",
    "ambiguous": "This name has more than one possible definition in the recorded source.",
    "cyclic": "Following imported names returns to a name already visited; its meaning remains unchecked.",
    "budget": "Following the imported names reached a checking limit.",
    "unavailable": "The imported name's meaning remains unchecked.",
    "wildcard": "The names supplied by this wildcard import remain unchecked.",
}
_CONDITIONS = {
    "standard-library-shadowing-unchecked": "A local file or directory competes with a selected standard-library module or one of its dependencies.",
    "standard-library-import-collision": "A local import name also names a standard-library module; its runtime selection remains unchecked.",
    "intrinsic-standard-import-unchecked": "This import names a built-in module whose behavior needs an explicit selected model.",
    "metadata-import-unchecked": "The analyzer could not establish which branch of a metadata import runs.",
    "import-attribute-hook-unchecked": "A package's attribute hook can supply this imported name; its behavior remains unchecked.",
    "import-syntax-errors": "Some imported source contains syntax the parser could not accept.",
    "unsupported-import-form": "Some import statements are outside the supported syntax.",
    "ambiguous-module-binding": "Repeated or conflicting definitions prevent choosing what a name refers to.",
    "special-module-attributes": "Special module names or settings require behavior the analyzer does not check.",
    "star-exports-unchecked": "A wildcard import has no checked list of exported names.",
    "cyclic-module-initialization": "An import returns to a module before a required name is available in the model.",
    "import-initialization-order-unchecked": "The order of import-time operations has not been fully checked.",
    "package-child-binding-conflict": "Importing a child file can replace an existing name on its package; that interaction remains unchecked.",
    "unresolved-local-import": "Some imports have no unambiguous supported module under the checked project root.",
    "missing-or-ambiguous-import-member": "Some requested names remain unresolved; the import groups separate unread source from unchecked definitions.",
}
_PRIORITY = {"unread-source": 0, "budget": 1, "local-module": 2, "ambiguous": 3,
             "wildcard": 4, "cyclic": 5, "missing-binding": 6, "unavailable": 7}
_NAMED_CONDITIONS = {"unresolved-local-import", "missing-or-ambiguous-import-member"}


def _count(number: int, singular: str, plural: str | None = None) -> str:
    return f"{number} {singular if number == 1 else plural if plural is not None else singular + 's'}"


@dataclass(frozen=True)
class ImportGap:
    kind: str
    importer: str
    module: str
    bindings: tuple[tuple[str, str | None], ...]
    unread_paths: tuple[str, ...]

    @property
    def names(self) -> tuple[str, ...]:
        return tuple(alias for alias, _ in self.bindings)

    @property
    def explanation(self) -> str:
        return _MESSAGES[self.kind]


def _gap(lookup: ImportLookup, files: set[str], namespaces: set[str],
         source_candidates: set[str]) -> tuple[str, tuple[str, ...]] | None:
    if lookup.alias == "*" and lookup.exports is None:
        return "wildcard", ()
    if lookup.resolution in {"function", "value", "module", "future"}:
        return None
    if lookup.resolution in {"missing", "ambiguous", "cyclic", "budget"}:
        return ("missing-binding" if lookup.resolution == "missing" else lookup.resolution), ()
    if lookup.resolution != "unavailable":
        raise FrRuntimeError("unsupported import resolution in explanation")
    selected = {hop.path for hop in lookup.binding_chain}
    if lookup.admitted:
        selected.update(lookup.packages)
        if lookup.target is not None:
            selected.add(lookup.target)
    if lookup.submodule is not None and lookup.submodule.admitted:
        selected.update(lookup.submodule.packages)
        if lookup.submodule.target is not None:
            selected.add(lookup.submodule.target)
    unread = tuple(sorted((selected & source_candidates) - files - namespaces))
    if unread:
        return "unread-source", unread
    return ("unavailable" if lookup.admitted else "local-module"), ()


@dataclass(frozen=True)
class ImportExplanation:
    """A view of one recorded answer, without a workspace freshness check."""
    revision: str
    input_digest: str
    analysis_complete: bool
    module_search_complete: bool
    files_read: int
    module_searches: int
    limit_explanations: tuple[str, ...]
    conditions: tuple[str, ...]
    gaps: tuple[ImportGap, ...]
    initialization: FlowInitialization
    analysis_cutoffs: tuple[str, ...]

    @property
    def unchecked_statements(self) -> int:
        return sum(item.statements for item in self.initialization.by_file)

    @property
    def unresolved_requests(self) -> int:
        return sum(len(gap.bindings) for gap in self.gaps)

    @property
    def import_check_complete(self) -> bool:
        return self.module_search_complete and not self.gaps and self.unchecked_statements == 0

    @classmethod
    def from_report(cls, report: FrReport) -> ImportExplanation:
        dependencies = FlowDependencies.from_report(report)
        initialization = FlowInitialization.from_report(report)
        data = report.to_data()
        cutoffs = data["cutoffs"]
        if (type(data["complete"]) is not bool or not isinstance(cutoffs, list)
                or any(not isinstance(item, str) for item in cutoffs)
                or cutoffs != sorted(set(cutoffs)) or data["complete"] != (not cutoffs)
                or not set(dependencies.cutoffs) <= set(cutoffs)):
            raise FrRuntimeError("analysis completeness disagrees with recorded stopping reasons")
        revision = data["revision"]
        if (not isinstance(revision, str) or len(revision) != 64
                or any(character not in "0123456789abcdef" for character in revision)):
            raise FrRuntimeError("import explanation needs a recorded source revision")
        files = set(dict(dependencies.files))
        namespaces = set(dependencies.namespaces)
        candidates = {candidate.path for lookup in dependencies.lookups for candidate in lookup.candidates
                      if candidate.status == "source"}
        candidates.update(candidate.path for lookup in dependencies.lookups if lookup.submodule is not None
                          for candidate in lookup.submodule.candidates if candidate.status == "source")
        grouped: dict[tuple[str, str, str, tuple[str, ...]], list[tuple[str, str | None]]] = {}
        for lookup in dependencies.lookups:
            gap = _gap(lookup, files, namespaces, candidates)
            if gap is not None:
                kind, paths = gap
                grouped.setdefault((kind, lookup.importer, lookup.module, paths), []).append((lookup.alias, lookup.member))
        gaps = tuple(ImportGap(kind, importer, module, tuple(sorted(bindings,
                         key=lambda binding: (binding[0], "" if binding[1] is None else binding[1]))), paths)
                     for (kind, importer, module, paths), bindings in sorted(
                         grouped.items(), key=lambda pair: (_PRIORITY[pair[0][0]], pair[0][1:])))
        families = set()
        for reason in dependencies.cutoffs:
            if reason in _LIMITS or reason in _CONDITIONS and reason not in _NAMED_CONDITIONS:
                families.add(reason)
                continue
            family, separator, name = reason.partition(":")
            parts = name.split(".") if family == "unresolved-local-import" else [name]
            if (not separator or family not in _NAMED_CONDITIONS
                    or not 1 <= len(parts) <= 16
                    or not all(part.isascii() and part.isidentifier()
                               or family == "missing-or-ambiguous-import-member" and part == "*" for part in parts)):
                raise FrRuntimeError("unsupported module stopping reason in explanation")
            families.add(family)
        return cls(revision, data["input_digest"], data["complete"], dependencies.complete,
                   len(files), dependencies.lookup_count,
                   tuple(message for reason, message in _LIMITS.items() if reason in dependencies.cutoffs),
                   tuple(message for reason, message in _CONDITIONS.items() if reason in families),
                   gaps, initialization, tuple(cutoffs))

    def render(self, *, max_bytes: int = 4096, max_items: int = 8) -> str:
        """Keep whole items and explicit omission counts within a UTF-8 budget."""
        if type(max_bytes) is not int or not 1024 <= max_bytes <= 65536:
            raise FrRuntimeError("explanation byte budget must be 1024 to 65536")
        if type(max_items) is not int or not 1 <= max_items <= 32:
            raise FrRuntimeError("explanation item limit must be 1 to 32")
        status = "finished within its stated rules" if self.import_check_complete else "unfinished"
        overall = "finished within its declared model" if self.analysis_complete else "unfinished"
        lines = [f"Recorded Python value trace: {overall}.", f"Import checking: {status}.",
                 f"{_count(self.files_read, 'source file')} read; "
                 f"{_count(self.module_searches, 'module search', 'module searches')}; "
                 f"{_count(self.unchecked_statements, 'import-time statement')} unchecked.",
                 f"{_count(self.unresolved_requests, 'unresolved import request')} in {_count(len(self.gaps), 'group')}.",
                 *self.limit_explanations]
        if not self.analysis_complete:
            lines.append("The overall value trace is unfinished. An empty trace does not establish that input cannot reach output.")
            if self.import_check_complete:
                lines.append("Other operations still prevent a complete trace. This view explains imports; inspect the original analysis for those operations.")
        else:
            lines.append("The value trace finished within the declared model; it is not a proof of runtime behavior.")
        lines.append("This describes the saved report; it does not check whether the workspace has changed.")
        shown_groups, shown_files, shown_conditions = 0, 0, 0

        def footer(groups: int, files: int, conditions: int) -> str:
            return (f"Shown {groups}/{len(self.gaps)} import groups and "
                    f"{files}/{len(self.initialization.by_file)} startup file summaries; "
                    f"{conditions}/{len(self.conditions)} other import conditions.\n"
                    f"Recorded startup sample: {_count(len(self.initialization.sites), 'location')}; "
                    f"{_count(self.initialization.omitted, 'additional location')} omitted by the analyzer.\n"
                    "Each shown file summary includes at most one recorded location.\n")

        if len(("\n".join(lines) + "\n" + footer(0, 0, 0)).encode()) > max_bytes:
            raise FrRuntimeError("explanation totals exceed the selected byte budget")
        group_items = []
        for gap in self.gaps:
            source = ", ".join(json.dumps(path, ensure_ascii=False) for path in gap.unread_paths)
            names = ", ".join(
                (json.dumps(member, ensure_ascii=False) + " as " if member is not None and member != alias else "")
                + json.dumps(alias, ensure_ascii=False) for alias, member in gap.bindings[:3])
            if len(gap.names) > 3:
                names += f" and {len(gap.names) - 3} more"
            line = (f"Import in {json.dumps(gap.importer, ensure_ascii=False)} from "
                    f"{json.dumps(gap.module, ensure_ascii=False)} ({names}): {gap.explanation}")
            if source:
                line += f" Unread source: {source}."
            group_items.append(("group", line))
        file_items = []
        for item in self.initialization.by_file:
            sites = [site for site in self.initialization.sites if site.occurrence.path == item.path]
            line = f"During import, {json.dumps(item.path, ensure_ascii=False)}: {_count(item.statements, 'statement')} unchecked."
            if sites:
                site = sites[0]
                line += f" First recorded site: line {site.occurrence.location.range.start.line}. "
                line += " ".join(site.explanations)
            else:
                line += " No source location for this file fits in the recorded sample."
            file_items.append(("file", line))
        condition_items = [("condition", message) for message in self.conditions]
        for pair in itertools.zip_longest(condition_items, file_items, group_items):
            for candidate in pair:
                if candidate is None or shown_groups + shown_files + shown_conditions >= max_items:
                    continue
                kind, line = candidate
                groups = shown_groups + int(kind == "group")
                files = shown_files + int(kind == "file")
                conditions = shown_conditions + int(kind == "condition")
                text = "\n".join([*lines, line]) + "\n" + footer(groups, files, conditions)
                if len(text.encode()) <= max_bytes:
                    lines.append(line)
                    shown_groups, shown_files, shown_conditions = groups, files, conditions
        return "\n".join(lines) + "\n" + footer(shown_groups, shown_files, shown_conditions)


def _unique_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise FrRuntimeError("saved report repeats a JSON field")
        result[key] = value
    return result


def read_report(path: Path) -> FrReport:
    with path.open("rb") as stream:
        data = stream.read(1048577)
    if len(data) > 1048576:
        raise FrRuntimeError("saved report exceeds 1 MiB")
    value = json.loads(data, object_pairs_hook=_unique_object)
    if not isinstance(value, dict):
        raise FrRuntimeError("saved report must be a JSON object")
    return FrReport(value, ())


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", type=Path)
    parser.add_argument("--max-bytes", type=int, default=4096)
    parser.add_argument("--max-items", type=int, default=8)
    args = parser.parse_args()
    try:
        print(ImportExplanation.from_report(read_report(args.report)).render(
            max_bytes=args.max_bytes, max_items=args.max_items), end="")
    except (FrRuntimeError, OSError, ValueError, KeyError, TypeError) as error:
        parser.error(str(error))


if __name__ == "__main__":
    main()
