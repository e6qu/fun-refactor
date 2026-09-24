"""Durable syntax identities and explicit selection after repository changes."""
from __future__ import annotations

from dataclasses import asdict, dataclass
import hashlib
import json
from pathlib import Path
import re
import tempfile
from typing import Any, Mapping

from .context import ObjectStore, restore_stored_value, store_merkle_value
from .runtime import FrClient, FrReport, FrRuntimeError, Occurrence


def _encoded(value: Any) -> str:
    return json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(",", ":"))


def _digest(value: Any) -> str:
    return hashlib.sha256(_encoded(value).encode()).hexdigest()


def _hex(value: Any) -> str:
    if not isinstance(value, str) or re.fullmatch(r"[0-9a-f]{64}", value) is None:
        raise FrRuntimeError("invalid declaration digest")
    return value


def _text(value: Any) -> str:
    if not isinstance(value, str) or not value:
        raise FrRuntimeError("declaration text must be nonempty")
    return value


@dataclass(frozen=True)
class DeclarationIdentity:
    handle: str
    revision: str
    path: str
    name: str
    kind: str
    language: str
    scope: tuple[str, ...]
    object_digest: str
    name_erased_digest: str
    occurrence: Occurrence

    def to_data(self) -> dict[str, Any]:
        return json.loads(json.dumps(asdict(self)))

    @classmethod
    def from_data(cls, value: Any) -> DeclarationIdentity:
        try:
            if set(value) != set(cls.__dataclass_fields__):
                raise FrRuntimeError("unknown declaration identity fields")
            revision = _hex(value["revision"])
            handle = _text(value["handle"])
            origin = Occurrence.from_data(value["occurrence"])
            span = origin.location.span
            # Native Span serialization retains its declaration order inside this digest.
            material = [revision, value["path"], {"start": span.start, "end": span.end}, "declaration"]
            origin_id = hashlib.sha256(json.dumps(material, ensure_ascii=False, separators=(",", ":")).encode()).hexdigest()
            if (re.fullmatch(rf"frp1:{revision[:32]}:[0-9a-f]+", handle) is None
                    or origin.revision != revision or origin.enclosing != handle
                    or origin.path != value["path"] or origin.role != "declaration"
                    or origin.id != "fro1:" + origin_id or not isinstance(value["scope"], list)):
                raise FrRuntimeError("inconsistent declaration occurrence or handle")
            return cls(handle, revision, origin.path, _text(value["name"]), _text(value["kind"]),
                       _text(value["language"]), tuple(_text(s) for s in value["scope"]),
                       _hex(value["object_digest"]), _hex(value["name_erased_digest"]), origin)
        except (KeyError, TypeError, ValueError) as error:
            raise FrRuntimeError("malformed declaration identity") from error


def _rules(old: DeclarationIdentity, new: DeclarationIdentity) -> tuple[str, ...]:
    rules = []
    if old.object_digest == new.object_digest:
        rules.append("identical-declaration-content")
    if old.name_erased_digest == new.name_erased_digest:
        rules.append("identical-except-declaration-name")
    if (old.path, old.scope, old.name, old.kind, old.language) == (new.path, new.scope, new.name, new.kind, new.language):
        rules.append("same-path-scope-name-kind")
    return tuple(rules)


@dataclass(frozen=True)
class Correspondence:
    previous: DeclarationIdentity
    status: str
    candidates: tuple[DeclarationIdentity, ...]
    reasons: Mapping[str, tuple[str, ...]]
    candidate_count: int
    candidates_complete: bool
    conflicts: tuple[str, ...]

    @classmethod
    def from_data(cls, value: Any, revision: str) -> Correspondence:
        try:
            if set(value) != {"previous", "status", "candidates", "reasons", "candidate_count",
                              "candidates_complete", "conflicts", "action_rebound"}:
                raise FrRuntimeError("unknown correspondence fields")
            previous = DeclarationIdentity.from_data(value["previous"])
            candidates = tuple(DeclarationIdentity.from_data(v) for v in value["candidates"])
            handles = {c.handle for c in candidates}
            count = value["candidate_count"]
            conflicts = tuple(value["conflicts"])
            if (type(count) is not int or count < len(candidates) or len(handles) != len(candidates)
                    or any(c.revision != revision for c in candidates)
                    or type(value["candidates_complete"]) is not bool
                    or value["candidates_complete"] != (count == len(candidates))
                    or len(set(conflicts)) != len(conflicts) or previous.handle in conflicts
                    or any(not isinstance(c, str) or re.fullmatch(rf"frp1:{previous.revision[:32]}:[0-9a-f]+", c) is None for c in conflicts)
                    or value["action_rebound"] is not False):
                raise FrRuntimeError("inconsistent correspondence candidates")
            expected = "missing" if count == 0 else "matched" if count == 1 and not conflicts else "ambiguous"
            if value["status"] != expected or len(value["reasons"]) != len(candidates):
                raise FrRuntimeError("inconsistent correspondence status")
            reasons = {}
            for candidate, reason in zip(candidates, value["reasons"]):
                rules = _rules(previous, candidate)
                if (not rules or set(reason) != {"handle", "rules", "content_equal"}
                        or reason["handle"] != candidate.handle or tuple(reason["rules"]) != rules
                        or type(reason["content_equal"]) is not bool
                        or reason["content_equal"] != (previous.object_digest == candidate.object_digest)):
                    raise FrRuntimeError("inconsistent correspondence rule")
                reasons[candidate.handle] = rules
            return cls(previous, expected, candidates, reasons, count, value["candidates_complete"], conflicts)
        except (KeyError, TypeError, ValueError) as error:
            raise FrRuntimeError("malformed correspondence") from error


@dataclass(frozen=True)
class IdentityPage:
    report: FrReport
    items: tuple[DeclarationIdentity, ...]
    matches: tuple[Correspondence, ...]
    revision: str
    analyzer: str
    basis: str
    before: int
    total: int
    next_cursor: str | None
    complete: bool

    @classmethod
    def from_report(cls, report: FrReport) -> IdentityPage:
        try:
            data = report.to_data()
            revision = _hex(data["revision"])
            if (report.schema != "fr-project-1" or data["identity_contract"] != "fr-declaration-identities-2"
                    or data["query"] not in {"identities", "correspondence"}
                    or type(data["input_complete"]) is not bool or type(data["complete"]) is not bool
                    or re.fullmatch(r"frpc1:[0-9a-f]{32}", data["basis"]) is None):
                raise FrRuntimeError("unsupported declaration page")
            matches = tuple(Correspondence.from_data(v, revision) for v in data["items"]) if data["query"] == "correspondence" else ()
            items = tuple(DeclarationIdentity.from_data(v) for v in data["items"]) if data["query"] == "identities" else ()
            identities = items or tuple(m.previous for m in matches)
            if len({i.handle for i in identities}) != len(identities) or any(i.revision != revision for i in items):
                raise FrRuntimeError("declaration page crosses identities")
            p = data["page"]
            if (any(type(p[k]) is not int or p[k] < 0 for k in ("total", "before", "returned", "remaining"))
                    or p["returned"] != len(identities) or p["total"] != p["before"] + p["returned"] + p["remaining"]
                    or p["next"] != (f'{data["basis"]}:{p["before"] + p["returned"]}' if p["remaining"] else None)
                    or (p["remaining"] > 0 and p["returned"] == 0)
                    or data["complete"] != (data["input_complete"] and p["before"] == 0 and p["remaining"] == 0
                                            and all(m.candidates_complete for m in matches))):
                raise FrRuntimeError("inconsistent declaration page coverage")
            return cls(report, items, matches, revision, _hex(data["analyzer"]), data["basis"], p["before"],
                       p["total"], p["next"], data["complete"])
        except (KeyError, TypeError, ValueError) as error:
            raise FrRuntimeError("malformed declaration page") from error

    @classmethod
    def inspect(cls, client: FrClient, target: str = ".", *, limit: int = 16,
                max_bytes: int = 32768, cursor: str | None = None) -> IdentityPage:
        args = ["identities", target, "--limit", str(limit), "--bytes", str(max_bytes)]
        if cursor is not None:
            args += ["--cursor", cursor]
        return cls.from_report(client.project(*args))


@dataclass(frozen=True)
class DeclarationSnapshot:
    revision: str
    selection: str
    analyzer: str
    items: tuple[DeclarationIdentity, ...]
    complete: bool

    def to_data(self) -> dict[str, Any]:
        return {"schema": "fr-declaration-snapshot-1", "revision": self.revision, "selection": self.selection,
                "analyzer": self.analyzer, "items": [i.to_data() for i in self.items], "complete": self.complete}

    @classmethod
    def from_data(cls, value: Any) -> DeclarationSnapshot:
        try:
            if (set(value) != {"schema", "revision", "selection", "analyzer", "items", "complete"}
                    or value["schema"] != "fr-declaration-snapshot-1" or type(value["complete"]) is not bool):
                raise FrRuntimeError("unsupported declaration snapshot")
            items = tuple(DeclarationIdentity.from_data(i) for i in value["items"])
            revision = _hex(value["revision"])
            if (len(items) > 1000 or len({i.handle for i in items}) != len(items)
                    or any(i.revision != revision for i in items) or len(_encoded(value).encode()) > 1_048_576):
                raise FrRuntimeError("declaration snapshot exceeds scope or budget")
            return cls(revision, _text(value["selection"]), _hex(value["analyzer"]), items, value["complete"])
        except (KeyError, TypeError, ValueError) as error:
            raise FrRuntimeError("malformed declaration snapshot") from error

    @classmethod
    def capture(cls, client: FrClient, target: str = ".", *, limit: int = 16,
                max_bytes: int = 32768, max_pages: int = 64) -> DeclarationSnapshot:
        if type(max_pages) is not int or not 1 <= max_pages <= 64:
            raise FrRuntimeError("declaration capture needs 1..64 pages")
        items: list[DeclarationIdentity] = []
        cursor = None
        first = None
        for _ in range(max_pages):
            page = IdentityPage.inspect(client, target, limit=limit, max_bytes=max_bytes, cursor=cursor)
            first = first or page
            if (page.revision, page.analyzer, page.basis, page.total, page.before) != (
                    first.revision, first.analyzer, first.basis, first.total, len(items)):
                raise FrRuntimeError("declaration capture changed between pages")
            items.extend(page.items)
            cursor = page.next_cursor
            if cursor is None:
                break
        assert first is not None
        return cls.from_data(cls(first.revision, target, first.analyzer, tuple(items), cursor is None).to_data())

    def subset(self, handles: tuple[str, ...]) -> DeclarationSnapshot:
        if not handles or len(set(handles)) != len(handles):
            raise FrRuntimeError("select distinct declaration handles explicitly")
        items = {i.handle: i for i in self.items}
        if not set(handles) <= set(items):
            raise FrRuntimeError("declaration selection is outside captured scope")
        return DeclarationSnapshot.from_data(DeclarationSnapshot(self.revision, "explicit-declarations", self.analyzer,
                                              tuple(items[h] for h in handles), True).to_data())

    def persist(self, store: ObjectStore) -> str:
        return store_merkle_value(store, DeclarationSnapshot.from_data(self.to_data()).to_data()).digest

    @classmethod
    def restore(cls, store: ObjectStore, digest: str) -> DeclarationSnapshot:
        return cls.from_data(restore_stored_value(store, digest))

    def compare(self, client: FrClient, target: str = ".", *, limit: int = 16, candidates: int = 16,
                max_bytes: int = 32768, max_pages: int = 64) -> CorrespondenceReport:
        if type(max_pages) is not int or not 1 <= max_pages <= 64:
            raise FrRuntimeError("correspondence needs 1..64 pages")
        retained = DeclarationSnapshot.from_data(self.to_data()).to_data()
        digest = _digest(retained)
        pages = []
        matches: list[Correspondence] = []
        cursor = None
        with tempfile.TemporaryDirectory(prefix="fr-correspondence-") as directory:
            path = Path(directory) / "snapshot.json"
            path.write_text(_encoded(retained), encoding="utf-8")
            args = ["identities", target, "--from", str(path), "--digest", digest, "--limit", str(limit),
                    "--candidates", str(candidates), "--bytes", str(max_bytes)]
            for _ in range(max_pages):
                page = IdentityPage.from_report(client.project(*args, *(["--cursor", cursor] if cursor else [])))
                first = pages[0] if pages else page
                if (page.revision, page.basis, page.analyzer, page.before, page.total) != (
                        first.revision, first.basis, first.analyzer, len(matches), len(self.items)):
                    raise FrRuntimeError("correspondence changed between pages")
                if page.report.at("/input_digest") != digest or page.report.at("/input_complete") != self.complete:
                    raise FrRuntimeError("correspondence input changed")
                expected = self.items[len(matches):len(matches) + len(page.matches)]
                if tuple(m.previous for m in page.matches) != expected:
                    raise FrRuntimeError("correspondence changed retained declarations")
                pages.append(page)
                matches.extend(page.matches)
                cursor = page.next_cursor
                if cursor is None:
                    break
        complete = self.complete and cursor is None and all(m.candidates_complete for m in matches)
        return CorrespondenceReport(self, target, tuple(pages), tuple(matches), complete, limit, candidates, max_bytes, max_pages)


@dataclass(frozen=True)
class CorrespondenceReport:
    snapshot: DeclarationSnapshot
    target: str
    pages: tuple[IdentityPage, ...]
    matches: tuple[Correspondence, ...]
    complete: bool
    limit: int
    candidates: int
    max_bytes: int
    max_pages: int

    @property
    def revision(self) -> str:
        return self.pages[0].revision

    def select(self, client: FrClient, choices: Mapping[str, str]) -> DeclarationSnapshot:
        """Revalidate an explicit selection; the result supplies read targets, never a write review."""
        if not self.complete or not choices or not set(choices) <= {m.previous.handle for m in self.matches}:
            raise FrRuntimeError("selection requires complete correspondence and explicit old handles")
        if len(set(choices.values())) != len(choices):
            raise FrRuntimeError("several old targets cannot select the same declaration")
        fresh = self.snapshot.compare(client, self.target, limit=self.limit, candidates=self.candidates,
                                      max_bytes=self.max_bytes, max_pages=self.max_pages)
        if (not fresh.complete or fresh.revision != self.revision
                or tuple(p.report.to_data() for p in fresh.pages) != tuple(p.report.to_data() for p in self.pages)):
            raise FrRuntimeError("stale correspondence selection; compare again")
        selected = []
        for match in fresh.matches:
            if match.previous.handle in choices:
                candidate = next((c for c in match.candidates if c.handle == choices[match.previous.handle]), None)
                if candidate is None:
                    raise FrRuntimeError("selected declaration is not a disclosed candidate")
                selected.append(candidate)
        return DeclarationSnapshot.from_data(DeclarationSnapshot(fresh.revision, "explicit-declarations",
                                              fresh.pages[0].analyzer, tuple(selected), True).to_data())
