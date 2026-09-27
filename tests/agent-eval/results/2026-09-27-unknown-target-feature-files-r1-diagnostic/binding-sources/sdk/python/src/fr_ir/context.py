"""Content-addressed progressive context for local ``fr`` agent sessions."""

from __future__ import annotations

from dataclasses import dataclass
import json
import os
from pathlib import Path
import re
import secrets
import stat
from typing import Any, Mapping, Protocol

from .ir import merkle_object_digest, merkle_object_pack, restore_merkle_object
from .runtime import (
    Disclosure, DisclosureAction, FrClient, FrReport, FrRuntimeError, SourceFragment,
    TextLocation, TextPosition, _advance_text_position, _copy_json, _pointer,
)


_DIGEST = re.compile(r"^[0-9a-f]{64}$")
_PACK_MAX_OBJECTS = 65_536
_PACK_MAX_BYTES = 67_108_864
_RECORD_MAX_BYTES = 1_048_576


def _context_materialization_admitted(
    calls: int,
    call_limit: int,
    session_matches: bool,
    complete: bool,
    digest_matches: bool,
) -> bool:
    return (1 <= call_limit <= 64 and calls <= call_limit and session_matches
            and complete and digest_matches)


def _object_store_admitted(
    objects: int,
    encoded_bytes: int,
    digest_matches: bool,
    records_canonical: bool,
    root_present: bool,
) -> bool:
    return (1 <= objects <= _PACK_MAX_OBJECTS and encoded_bytes <= _PACK_MAX_BYTES
            and digest_matches and records_canonical and root_present)


class ObjectStore(Protocol):
    """The two operations required from a Merkle object backend."""

    def get(self, digest: str) -> Mapping[str, Any] | None:
        """Return one record by content digest, or ``None`` when absent."""

    def put(self, digest: str, record: Mapping[str, Any]) -> None:
        """Store one immutable record under its content digest."""


class MemoryObjectStore:
    """An in-memory object store for one agent process."""

    def __init__(self) -> None:
        self._objects: dict[str, dict[str, Any]] = {}

    def get(self, digest: str) -> Mapping[str, Any] | None:
        value = self._objects.get(_checked_digest(digest))
        return _copy_json(value) if value is not None else None

    def put(self, digest: str, record: Mapping[str, Any]) -> None:
        digest = _checked_digest(digest)
        value = _copy_json(record)
        previous = self._objects.get(digest)
        if previous is not None and previous != value:
            raise FrRuntimeError("Merkle object storage detected conflicting immutable bytes")
        self._objects[digest] = value

    def __len__(self) -> int:
        return len(self._objects)


class DirectoryObjectStore:
    """A bounded local directory backend for immutable Merkle records."""

    def __init__(self, root: str | os.PathLike[str]) -> None:
        requested = Path(root)
        requested.mkdir(parents=True, exist_ok=True)
        if requested.is_symlink() or not requested.is_dir():
            raise FrRuntimeError("Merkle object store root must be a real directory")
        self.root = requested.resolve()

    def _path(self, digest: str, *, create: bool = False) -> Path:
        digest = _checked_digest(digest)
        parent = self.root / digest[:2]
        if create:
            parent.mkdir(exist_ok=True)
        if parent.exists() and (parent.is_symlink() or parent.resolve().parent != self.root):
            raise FrRuntimeError("Merkle object store shard escapes its root")
        return parent / f"{digest[2:]}.json"

    @staticmethod
    def _encoded(record: Mapping[str, Any]) -> bytes:
        try:
            encoded = json.dumps(record, ensure_ascii=False, sort_keys=True,
                                 separators=(",", ":"), allow_nan=False).encode("utf-8")
        except (TypeError, ValueError) as error:
            raise FrRuntimeError("Merkle object record is not canonical JSON") from error
        if len(encoded) > _RECORD_MAX_BYTES:
            raise FrRuntimeError("Merkle object record exceeds 1 MiB")
        return encoded

    @staticmethod
    def _read(path: Path) -> bytes:
        flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0)
        try:
            descriptor = os.open(path, flags)
            with os.fdopen(descriptor, "rb") as source:
                status = os.fstat(source.fileno())
                if not stat.S_ISREG(status.st_mode) or status.st_size > _RECORD_MAX_BYTES:
                    raise FrRuntimeError("Merkle object record is unsafe or oversized")
                encoded = source.read(_RECORD_MAX_BYTES + 1)
        except FrRuntimeError:
            raise
        except OSError as error:
            raise FrRuntimeError("Merkle object record is unreadable") from error
        if len(encoded) > _RECORD_MAX_BYTES:
            raise FrRuntimeError("Merkle object record is unsafe or oversized")
        return encoded

    def get(self, digest: str) -> Mapping[str, Any] | None:
        path = self._path(digest)
        if not path.exists():
            return None
        try:
            value = json.loads(self._read(path))
        except (UnicodeDecodeError, json.JSONDecodeError) as error:
            raise FrRuntimeError("Merkle object record is unreadable") from error
        if not isinstance(value, dict):
            raise FrRuntimeError("Merkle object record is malformed")
        return value

    def put(self, digest: str, record: Mapping[str, Any]) -> None:
        digest = _checked_digest(digest)
        encoded = self._encoded(record)
        path = self._path(digest, create=True)
        if path.exists():
            if self._read(path) != encoded:
                raise FrRuntimeError("Merkle object storage detected conflicting immutable bytes")
            return
        temporary = path.with_name(
            f".{path.name}.{os.getpid()}.{secrets.token_hex(8)}.tmp"
        )
        try:
            descriptor = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
            with os.fdopen(descriptor, "wb") as output:
                output.write(encoded)
                output.flush()
                os.fsync(output.fileno())
            try:
                os.link(temporary, path)
            except FileExistsError:
                if self._read(path) != encoded:
                    raise FrRuntimeError(
                        "Merkle object storage detected conflicting immutable bytes"
                    )
        except OSError as error:
            raise FrRuntimeError("Merkle object record could not be stored") from error
        finally:
            try:
                temporary.unlink(missing_ok=True)
            except OSError:
                pass


@dataclass(frozen=True)
class StoredMerkleValue:
    """The verified identity and storage cost of one cached JSON value."""

    digest: str
    objects: int
    encoded_bytes: int


def _checked_digest(digest: str) -> str:
    if not isinstance(digest, str) or not _DIGEST.fullmatch(digest):
        raise FrRuntimeError("Merkle object key must be a lowercase SHA-256 digest")
    return digest


def store_merkle_value(
    store: ObjectStore,
    value: Any,
    *,
    expected_digest: str | None = None,
) -> StoredMerkleValue:
    """Verify, split and store one JSON value as immutable Merkle records."""
    pack = merkle_object_pack(value)
    digest = pack["root"]
    if expected_digest is not None and _checked_digest(expected_digest) != digest:
        raise FrRuntimeError("revealed value does not match its advertised object digest")
    objects = pack["objects"]
    encoded = {
        key: json.dumps(record, ensure_ascii=False, sort_keys=True,
                        separators=(",", ":"), allow_nan=False).encode("utf-8")
        for key, record in objects.items()
    }
    total = sum(len(item) for item in encoded.values())
    if not (1 <= len(objects) <= _PACK_MAX_OBJECTS and total <= _PACK_MAX_BYTES
            and digest in objects):
        raise FrRuntimeError("Merkle object pack exceeds the local storage admission bounds")
    records_canonical = True
    for key in sorted(objects, key=lambda item: (item == digest, item)):
        store.put(key, objects[key])
        observed = store.get(key)
        if not isinstance(observed, Mapping):
            records_canonical = False
            break
        try:
            observed_bytes = json.dumps(
                observed, ensure_ascii=False, sort_keys=True,
                separators=(",", ":"), allow_nan=False,
            ).encode("utf-8")
        except (TypeError, ValueError):
            records_canonical = False
            break
        if observed_bytes != encoded[key]:
            records_canonical = False
            break
    root_present = store.get(digest) is not None
    if not _object_store_admitted(
        len(objects), total, expected_digest is None or expected_digest == digest,
        records_canonical, root_present,
    ):
        raise FrRuntimeError("Merkle object backend failed immutable write verification")
    return StoredMerkleValue(digest, len(objects), total)


def restore_stored_value(store: ObjectStore, digest: str) -> Any:
    """Restore a stored value while verifying every fetched object."""
    return restore_merkle_object(_checked_digest(digest), store.get)


class ContextSession:
    """One session-bound progressive traversal whose reports remain local."""

    def __init__(
        self,
        client: FrClient,
        initial: Disclosure,
        *,
        store: ObjectStore | None = None,
    ) -> None:
        if not isinstance(client, FrClient) or not isinstance(initial, Disclosure):
            raise FrRuntimeError("context session requires one client and disclosure")
        self.client = client
        self._identity = initial.session_identity()
        self._reports = [initial]
        self.store = store
        self._cached: dict[str, StoredMerkleValue] = {}
        self._materialized: dict[str, Any] = {}
        self._materialized_digests: dict[str, str] = {}
        self._cache(initial)

    @property
    def latest(self) -> Disclosure:
        return self._reports[-1]

    @property
    def calls(self) -> int:
        return len(self._reports)

    @property
    def reports(self) -> tuple[Disclosure, ...]:
        return tuple(self._reports)

    def session_identity(self) -> tuple[str, str, str, str, str, str]:
        """Return the immutable revision, view and handle identity for this traversal."""
        return self._identity

    @property
    def cached_digests(self) -> tuple[str, ...]:
        return tuple(sorted(self._cached))

    def _cache_value(self, node: Any) -> None:
        if (not isinstance(node, Mapping) or "value" not in node
                or not isinstance(node.get("object_digest"), str)):
            return
        digest = node["object_digest"]
        if merkle_object_digest(node["value"]) != digest:
            raise FrRuntimeError("revealed value does not match its advertised object digest")
        pointer = self._pointer_from_address(node.get("address"))
        if pointer is not None:
            self._materialized[pointer] = _copy_json(node["value"])
        if self.store is not None:
            stored = store_merkle_value(self.store, node["value"], expected_digest=digest)
            self._cached[stored.digest] = stored

    def _cache(self, report: Disclosure) -> None:
        revealed = report._value.get("revealed")
        self._cache_value(revealed)
        if isinstance(revealed, Mapping) and isinstance(revealed.get("children"), list):
            for child in revealed["children"]:
                self._cache_value(child)

    @staticmethod
    def _pointer_from_address(address: str | None) -> str | None:
        if not isinstance(address, str) or "#" not in address:
            return None
        pointer = address.split("#", 1)[1]
        if pointer != "" and not pointer.startswith("/"):
            return None
        return pointer

    def _append(self, action: DisclosureAction) -> Disclosure:
        report = self.client.follow(action)
        if report.session_identity() != self._identity:
            raise FrRuntimeError("disclosure continuation crossed its bound session")
        self._reports.append(report)
        self._cache(report)
        return report

    def reveal(self, pointer: str, *, max_calls: int = 16) -> Disclosure:
        """Follow exact ancestor and page actions until one JSON Pointer is revealed."""
        if (not isinstance(pointer, str)
                or pointer != "" and not pointer.startswith("/")
                or re.search(r"~(?:[^01]|$)", pointer)):
            raise FrRuntimeError("context reveal requires a canonical RFC 6901 pointer")
        if (isinstance(max_calls, bool) or not isinstance(max_calls, int)
                or not 1 <= max_calls <= 64):
            raise FrRuntimeError("context reveal call bound must be 1 through 64")
        visited: set[tuple[str, ...]] = set()
        for _ in range(max_calls):
            current = self._pointer_from_address(self.latest._value.get("revealed", {}).get(
                "address") if isinstance(self.latest._value.get("revealed"), Mapping) else None)
            if current == pointer:
                return self.latest
            exact: list[DisclosureAction] = []
            ancestors: list[DisclosureAction] = []
            pages: list[DisclosureAction] = []
            known_arguments: set[tuple[str, ...]] = set()
            for report in reversed(self._reports):
                for action in report.actions():
                    if action.arguments in visited or action.arguments in known_arguments:
                        continue
                    known_arguments.add(action.arguments)
                    action_pointer = self._pointer_from_address(action.address)
                    reaches_pointer = action_pointer is not None and (
                        action_pointer == "" or pointer == action_pointer
                        or pointer.startswith(f"{action_pointer}/")
                    )
                    if action.kind == "continuation" and reaches_pointer:
                        pages.append(action)
                    elif action_pointer == pointer:
                        exact.append(action)
                    elif reaches_pointer:
                        ancestors.append(action)
            candidates = exact or ancestors or pages
            if not candidates:
                raise FrRuntimeError(f"no exact disclosure action reaches {pointer!r}")
            action = candidates[0]
            visited.add(action.arguments)
            report = self._append(action)
            revealed = report._value.get("revealed")
            if (isinstance(revealed, Mapping)
                    and self._pointer_from_address(revealed.get("address")) == pointer):
                return report
        raise FrRuntimeError(f"disclosure traversal exceeded {max_calls} calls")

    def reveal_section(self, section: str, *, max_calls: int = 16) -> Disclosure:
        """Reveal one top-level semantic, evidence or project section by name."""
        if (not isinstance(section, str) or not section or "/" in section
                or "~" in section or len(section.encode("utf-8")) > 128):
            raise FrRuntimeError("context section name is invalid")
        return self.reveal(f"/model/{section}", max_calls=max_calls)

    def source_text(self, location: TextLocation, *, max_calls: int = 64) -> str:
        """Reveal and return one exact AST text location from this bound source revision."""
        if not isinstance(location, TextLocation):
            raise FrRuntimeError("source extraction requires a TextLocation")
        if (isinstance(max_calls, bool) or not isinstance(max_calls, int)
                or not 1 <= max_calls <= 64):
            raise FrRuntimeError("source extraction call bound must be 1 through 64")
        commitment = self._reports[0]._value.get("commitment")
        source_root = commitment.get("source_root") if isinstance(commitment, Mapping) else None
        if not isinstance(source_root, str) or _DIGEST.fullmatch(source_root) is None:
            raise FrRuntimeError("disclosure session has no exact source commitment")
        target_location = self._reports[0].target.location
        if target_location is None:
            raise FrRuntimeError("disclosure session has no exact definition location")
        definition = target_location.definition
        if (not definition.span.contains(location.span)
                or not definition.range.start <= location.range.start
                or not location.range.end <= definition.range.end):
            raise FrRuntimeError("text location lies outside the bound target definition")
        origin = definition.span.start
        relative_start = location.span.start - origin
        relative_end = location.span.end - origin
        definition_bytes = definition.span.end - origin

        start_calls = len(self._reports)
        followed = {report.arguments for report in self._reports}
        while True:
            fragments = [fragment for report in self._reports
                         if (fragment := report.source_fragment) is not None]
            if fragments:
                totals = {fragment.total_bytes for fragment in fragments}
                if (any(fragment.digest != source_root for fragment in fragments)
                        or len(totals) != 1):
                    raise FrRuntimeError("revealed source fragments changed their committed source")
                total = next(iter(totals))
                if total != definition_bytes:
                    raise FrRuntimeError("committed source does not match the bound target definition")
                if any(fragment.source_span != definition.span for fragment in fragments):
                    raise FrRuntimeError("source fragment locations do not match the bound target definition")
                output = bytearray()
                ordered = sorted(fragments, key=lambda item: item.offset)
                for fragment in ordered:
                    encoded = fragment.text.encode("utf-8")
                    if fragment.offset != len(output):
                        raise FrRuntimeError("revealed source fragments overlap or leave a gap")
                    output.extend(encoded)
                if len(output) >= relative_end:
                    try:
                        def position(offset: int) -> TextPosition:
                            if offset == definition_bytes:
                                return definition.range.end
                            prefix = bytes(output[:offset]).decode("utf-8")
                            return _advance_text_position(definition.range.start, prefix)

                        for fragment in ordered:
                            start = position(fragment.offset)
                            end = position(fragment.offset + len(fragment.text.encode("utf-8")))
                            if fragment.location.range.start != start or fragment.location.range.end != end:
                                raise FrRuntimeError(
                                    "source fragment line ranges do not match the bound target definition"
                                )
                        if (location.range.start != position(relative_start)
                                or location.range.end != position(relative_end)):
                            raise FrRuntimeError(
                                "text location line range does not match the bound target definition"
                            )
                        return bytes(output[relative_start:relative_end]).decode("utf-8")
                    except UnicodeDecodeError as error:
                        raise FrRuntimeError(
                            "text location does not fall on UTF-8 boundaries"
                        ) from error

            if len(self._reports) - start_calls >= max_calls:
                raise FrRuntimeError("source extraction exceeded its call bound")
            action = next((action for report in reversed(self._reports)
                           for action in report.actions(domain="exact-source")
                           if action.arguments not in followed), None)
            if action is None:
                raise FrRuntimeError("no exact source action reaches the text location")
            followed.add(action.arguments)
            self._append(action)

    def _remember(self, pointer: str, value: Any, digest: Any) -> Any:
        if not isinstance(digest, str) or merkle_object_digest(value) != digest:
            raise FrRuntimeError("materialized value does not match its advertised object digest")
        detached = _copy_json(value)
        self._materialized[pointer] = detached
        self._materialized_digests[pointer] = digest
        if self.store is not None:
            stored = store_merkle_value(self.store, detached, expected_digest=digest)
            self._cached[stored.digest] = stored
        return _copy_json(detached)

    def _bounded_follow(self, action: DisclosureAction, limit: int) -> Disclosure:
        if len(self._reports) >= limit:
            raise FrRuntimeError("disclosure materialization exceeded its call bound")
        return self._append(action)

    def _materialize_report(
        self,
        report: Disclosure,
        pointer: str,
        limit: int,
        active: set[str],
    ) -> Any:
        if pointer in active:
            raise FrRuntimeError("disclosure materialization contains a pointer cycle")
        revealed = report._value.get("revealed")
        if (not isinstance(revealed, Mapping)
                or self._pointer_from_address(revealed.get("address")) != pointer):
            raise FrRuntimeError("disclosure response does not reveal the requested pointer")
        digest = revealed.get("object_digest")
        if "value" in revealed:
            return self._remember(pointer, revealed["value"], digest)
        if "value_fragment" in revealed:
            active.add(pointer)
            chunks: list[tuple[int, bytes]] = []
            current = report
            total: int | None = None
            while True:
                node = current._value.get("revealed")
                if (not isinstance(node, Mapping)
                        or self._pointer_from_address(node.get("address")) != pointer
                        or node.get("object_digest") != digest
                        or not isinstance(node.get("value_fragment"), str)):
                    active.remove(pointer)
                    raise FrRuntimeError("disclosed string page is malformed")
                offset, returned, candidate_total = (
                    node.get("offset"), node.get("returned_bytes"), node.get("total_bytes"),
                )
                encoded = node["value_fragment"].encode("utf-8")
                if (isinstance(offset, bool) or not isinstance(offset, int) or offset < 0
                        or returned != len(encoded) or isinstance(candidate_total, bool)
                        or not isinstance(candidate_total, int) or candidate_total < 0
                        or total is not None and total != candidate_total):
                    active.remove(pointer)
                    raise FrRuntimeError("disclosed string page has inconsistent byte bounds")
                total = candidate_total
                chunks.append((offset, encoded))
                continuation = next(
                    (item for item in current.actions() if item.kind == "continuation"), None,
                )
                if continuation is None:
                    break
                current = self._bounded_follow(continuation, limit)
            active.remove(pointer)
            chunks.sort()
            position = 0
            output = bytearray()
            for offset, encoded in chunks:
                if offset != position:
                    raise FrRuntimeError("disclosed string pages overlap or leave a gap")
                output.extend(encoded)
                position += len(encoded)
            if total is None or position != total:
                raise FrRuntimeError("disclosed string pages do not cover the complete value")
            try:
                value = output.decode("utf-8")
            except UnicodeDecodeError as error:
                raise FrRuntimeError("disclosed string pages are not UTF-8") from error
            return self._remember(pointer, value, digest)
        if not isinstance(revealed.get("children"), list):
            raise FrRuntimeError("disclosure response has no materializable value")

        active.add(pointer)
        rows: list[tuple[str, Any]] = []
        current = report
        kind = revealed.get("value_kind")
        while True:
            node = current._value.get("revealed")
            if (not isinstance(node, Mapping) or node.get("object_digest") != digest
                    or self._pointer_from_address(node.get("address")) != pointer
                    or not isinstance(node.get("children"), list)):
                active.remove(pointer)
                raise FrRuntimeError("disclosed child page changed its node identity")
            if kind is None and isinstance(node.get("value_kind"), str):
                kind = node["value_kind"]
            for row in node["children"]:
                if not isinstance(row, Mapping) or not isinstance(row.get("key"), str):
                    active.remove(pointer)
                    raise FrRuntimeError("disclosed child row is malformed")
                key = row["key"]
                if "value" in row and isinstance(row.get("object_digest"), str):
                    if merkle_object_digest(row["value"]) != row["object_digest"]:
                        active.remove(pointer)
                        raise FrRuntimeError("disclosed child value fails its object digest")
                    child = _copy_json(row["value"])
                else:
                    hole = row.get("hole")
                    if not isinstance(hole, Mapping) or not isinstance(hole.get("reveal"), Mapping):
                        active.remove(pointer)
                        raise FrRuntimeError("disclosed child has no exact reveal action")
                    child_pointer = self._pointer_from_address(hole.get("address"))
                    if (child_pointer is None
                            or not child_pointer.startswith(f"{pointer}/")):
                        active.remove(pointer)
                        raise FrRuntimeError("disclosed child escapes its parent pointer")
                    action = DisclosureAction.from_data(
                        hole["reveal"], hole, session=self._identity,
                    )
                    child_report = self._bounded_follow(action, limit)
                    child = self._materialize_report(
                        child_report, child_pointer, limit, active,
                    )
                rows.append((key, child))
            continuation = next(
                (item for item in current.actions() if item.kind == "continuation"), None,
            )
            if continuation is None:
                break
            current = self._bounded_follow(continuation, limit)
        active.remove(pointer)

        keys = [key for key, _ in rows]
        if kind == "array":
            if keys != [str(index) for index in range(len(rows))]:
                raise FrRuntimeError("disclosed array children are not contiguous")
            value = [child for _, child in rows]
        elif kind == "object":
            if len(keys) != len(set(keys)):
                raise FrRuntimeError("disclosed object repeats a child key")
            value = {key: child for key, child in rows}
        elif not rows:
            candidates = [candidate for candidate in ({}, [])
                          if merkle_object_digest(candidate) == digest]
            if len(candidates) != 1:
                raise FrRuntimeError("empty disclosed container has no unique object kind")
            value = candidates[0]
        else:
            raise FrRuntimeError("disclosed container does not state its object kind")
        return self._remember(pointer, value, digest)

    def materialize(self, pointer: str, *, max_calls: int = 64) -> Any:
        """Reconstruct one selected subtree from exact bounded disclosure actions."""
        if pointer in self._materialized:
            return _copy_json(self._materialized[pointer])
        if (isinstance(max_calls, bool) or not isinstance(max_calls, int)
                or not 1 <= max_calls <= 64):
            raise FrRuntimeError("materialization call bound must be 1 through 64")
        start = len(self._reports)
        report = self.reveal(pointer, max_calls=max_calls)
        value = self._materialize_report(report, pointer, start + max_calls, set())
        digest = self._materialized_digests.get(pointer)
        admitted = _context_materialization_admitted(
            len(self._reports) - start,
            max_calls,
            all(item.session_identity() == self._identity for item in self._reports[start:]),
            pointer in self._materialized,
            isinstance(digest, str) and merkle_object_digest(value) == digest,
        )
        if not admitted:
            raise FrRuntimeError("materialized context failed its final admission policy")
        return value

    def materialize_section(self, section: str, *, max_calls: int = 64) -> Any:
        """Reconstruct one top-level section and cache its Merkle object records."""
        if (not isinstance(section, str) or not section or "/" in section
                or "~" in section or len(section.encode("utf-8")) > 128):
            raise FrRuntimeError("context section name is invalid")
        return self.materialize(f"/model/{section}", max_calls=max_calls)

    def _select(self, pointer: str) -> Any:
        roots = sorted(
            (root for root in self._materialized
             if pointer == root or pointer.startswith(f"{root}/")),
            key=len,
            reverse=True,
        )
        if roots:
            root = roots[0]
            return _copy_json(_pointer(self._materialized[root], pointer[len(root):]))
        return self.latest.at(pointer)

    def packet(
        self,
        selected: Mapping[str, str],
        *,
        include_actions: bool = True,
        max_bytes: int = 4_096,
    ) -> FrReport:
        """Build one bounded, provenance-bound packet from selected local values."""
        if (not isinstance(selected, Mapping) or not 1 <= len(selected) <= 32
                or any(not isinstance(name, str) or not re.fullmatch(r"[A-Za-z][A-Za-z0-9_-]{0,63}", name)
                       or not isinstance(pointer, str) for name, pointer in selected.items())):
            raise FrRuntimeError("context packet needs 1 through 32 named JSON Pointer selections")
        if (isinstance(max_bytes, bool) or not isinstance(max_bytes, int)
                or not 1_024 <= max_bytes <= 65_536):
            raise FrRuntimeError("context packet bound must be 1024 through 65536 bytes")
        revision, view_basis, object_root, handle, view, profile = self._identity
        value: dict[str, Any] = {
            "schema": "fr-agent-context-1",
            "revision": revision,
            "context_basis": self.latest._value.get("context_basis"),
            "view_basis": view_basis,
            "object_root": object_root,
            "view": view,
            "profile": profile,
            "target": _copy_json(self.latest._value.get("target")),
            "calls": self.calls,
            "selected": {name: self._select(pointer) for name, pointer in selected.items()},
            "cached_objects": list(self.cached_digests),
        }
        if include_actions:
            value["actions"] = [{
                "kind": action.kind,
                "domain": action.domain,
                "address": action.address,
                "object_digest": action.object_digest,
                "reason": action.reason,
                "arguments": list(action.arguments),
            } for action in self.latest.actions()]
        value["serialized_bytes"] = 0
        for _ in range(3):
            encoded = json.dumps(value, ensure_ascii=False, sort_keys=True,
                                 separators=(",", ":"), allow_nan=False).encode("utf-8")
            if value["serialized_bytes"] == len(encoded):
                break
            value["serialized_bytes"] = len(encoded)
        encoded = json.dumps(value, ensure_ascii=False, sort_keys=True,
                             separators=(",", ":"), allow_nan=False).encode("utf-8")
        if len(encoded) > max_bytes:
            raise FrRuntimeError(
                f"selected context is {len(encoded)} bytes and exceeds the {max_bytes}-byte bound"
            )
        value["serialized_bytes"] = len(encoded)
        return FrReport(value, ("context", "packet"))
