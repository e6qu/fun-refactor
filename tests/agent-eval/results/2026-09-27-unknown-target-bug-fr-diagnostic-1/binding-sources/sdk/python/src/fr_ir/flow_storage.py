"""Whole-report flow storage in bounded, content-verified JSON chunks."""
from __future__ import annotations

import json
from typing import Any

from .context import ObjectStore, restore_stored_value, store_merkle_value
from .runtime import FrRuntimeError

_CHUNK_CHARACTERS = 32_768
_REPORT_BYTES = 1_048_576


def _encode(value: Any) -> str:
    try:
        return json.dumps(value, sort_keys=True, ensure_ascii=False,
                          separators=(",", ":"), allow_nan=False)
    except (TypeError, ValueError) as error:
        raise FrRuntimeError("flow record must contain finite JSON") from error


def store_flow_report(store: ObjectStore, report: dict[str, Any]) -> str:
    """Store the complete report; chunks do not permit partial analysis reuse."""
    if report.get("schema") != "fr-dataflow-1" or report.get("complete") is not True:
        raise FrRuntimeError("only complete dataflow reports enter the cache")
    encoded = _encode(report)
    if len(encoded.encode("utf-8")) > _REPORT_BYTES:
        raise FrRuntimeError("flow record exceeds 1 MiB")
    chunks = [encoded[index:index + _CHUNK_CHARACTERS]
              for index in range(0, len(encoded), _CHUNK_CHARACTERS)]
    return store_merkle_value(store, {"schema": "fr-flow-record-1", "chunks": chunks}).digest


def restore_flow_report(store: ObjectStore, digest: str) -> dict[str, Any]:
    """Verify chunks or a legacy report before native dependency validation."""
    value = restore_stored_value(store, digest)
    if isinstance(value, dict) and value.get("schema") == "fr-flow-record-1":
        chunks = value.get("chunks")
        if (set(value) != {"schema", "chunks"} or not isinstance(chunks, list)
                or not 1 <= len(chunks) <= _REPORT_BYTES // _CHUNK_CHARACTERS
                or any(not isinstance(chunk, str) or not 1 <= len(chunk) <= _CHUNK_CHARACTERS
                       or index < len(chunks) - 1 and len(chunk) != _CHUNK_CHARACTERS
                       for index, chunk in enumerate(chunks))):
            raise FrRuntimeError("flow record has invalid chunks")
        encoded = "".join(chunks)
        if len(encoded.encode("utf-8")) > _REPORT_BYTES:
            raise FrRuntimeError("flow record exceeds 1 MiB")
        try:
            value = json.loads(encoded)
        except (ValueError, RecursionError) as error:
            raise FrRuntimeError("flow record contains invalid JSON") from error
        if _encode(value) != encoded:
            raise FrRuntimeError("flow record JSON is not canonical")
    if (not isinstance(value, dict) or value.get("schema") != "fr-dataflow-1"
            or value.get("complete") is not True):
        raise FrRuntimeError("cached flow record is not a complete dataflow report")
    return value
