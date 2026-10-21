import json
import sqlite3
from contextlib import closing
from pathlib import Path

import pytest

from fr_ir.context import DirectoryObjectStore, MemoryObjectStore, restore_stored_value, store_merkle_value
from fr_ir.flow import FlowCache
from fr_ir.flow_storage import restore_flow_report, store_flow_report
from fr_ir.ir import IrError
from fr_ir.runtime import FrClient, FrRuntimeError

FR = Path(__file__).resolve().parents[3] / "target/debug/fr"


def workspace(root):
    (root / "app.py").write_text("def entry():\n    return sink(source())\n")
    rules = root / "rules.json"
    rules.write_text('{"version":"one","sources":["source"],"sinks":["sink"]}')
    client = FrClient(root, executable=str(FR))
    return client, rules


def analyze(cache, client, rules):
    target = client.project("find", "entry").definition_target().handle
    return cache.analyze(client, target, standard_library=(), rules=rules, max_bytes=1_048_576)


@pytest.mark.parametrize("disk", [False, True])
def test_current_cache_reopens_and_rebinds_without_partial_reuse(tmp_path, disk):
    client, rules = workspace(tmp_path)
    store = DirectoryObjectStore(tmp_path / "objects") if disk else MemoryObjectStore()
    cache = FlowCache(store)
    original = analyze(cache, client, rules).report.to_data()
    cache = FlowCache.restore(store, cache.persist())
    (tmp_path / "extra.py").write_text("def independent():\n    return 7\n")
    result = analyze(cache, client, rules)
    assert result.reused and result.report.at("/revision") != original["revision"]
    manifest = restore_stored_value(store, cache.persist())
    assert manifest["schema"] == "fr-flow-cache"
    retained = restore_stored_value(store, manifest["entries"][original["input_digest"]])
    assert retained["schema"] == "fr-flow-record"
    cache = FlowCache.restore(store, cache.persist())
    assert analyze(cache, client, rules).reused
    (tmp_path / "app.py").write_text("def entry():\n    return sink(0)\n")
    fresh = analyze(cache, client, rules)
    assert not fresh.reused and not fresh.witnesses


def test_large_unicode_record_roundtrips_through_bounded_directory_records(tmp_path):
    store = DirectoryObjectStore(tmp_path / "objects")
    report = {"schema": "fr-dataflow-1", "complete": True, "text": '\"λ🙂\\\n' * 65_000}
    encoded = json.dumps(report, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
    assert 700_000 < len(encoded.encode()) < 1_048_576
    digest = store_flow_report(store, report)
    assert restore_flow_report(store, digest) == report
    with closing(sqlite3.connect(store.database)) as connection:
        count, maximum = connection.execute("SELECT count(*), max(length(record)) FROM objects").fetchone()
    assert count < 40 and maximum < 1_048_576


def test_changed_chunk_is_rejected_by_content_verification(tmp_path):
    store = DirectoryObjectStore(tmp_path / "objects")
    digest = store_flow_report(store, {"schema": "fr-dataflow-1", "complete": True})
    with closing(sqlite3.connect(store.database)) as connection, connection:
        for key, encoded in connection.execute("SELECT digest, record FROM objects").fetchall():
            record = json.loads(encoded)
            if record.get("kind") == "scalar" and 'fr-dataflow-1' in str(record.get("value")):
                record["value"] = record["value"].replace("true", "false")
                connection.execute("UPDATE objects SET record = ? WHERE digest = ?",
                                   (json.dumps(record).encode(), key))
                break
        else:
            pytest.fail("no report chunk found")
    with pytest.raises(IrError, match="verification"):
        restore_flow_report(store, digest)


@pytest.mark.parametrize("chunks", [[], ["x"] * 33, ["x" * 32769], ["{}", "{}"],
                                   [None], ['{"schema":"fr-dataflow-1","complete":true}'],
                                   ['{"complete":true,"schema":"fr-dataflow-1","schema":"fr-dataflow-1"}'],
                                   ['{"complete":true,"schema":"fr-dataflow-1","value":NaN}']])
def test_rehashed_malformed_envelopes_refuse(chunks):
    store = MemoryObjectStore()
    root = store_merkle_value(store, {"schema": "fr-flow-record", "chunks": chunks}).digest
    with pytest.raises(FrRuntimeError):
        restore_flow_report(store, root)


@pytest.mark.parametrize("value", [{"schema": "fr-dataflow-1", "complete": False},
                                  {"schema": "other", "complete": True},
                                  {"schema": "fr-dataflow-1", "complete": True, "large": "🙂" * 262144}])
def test_incomplete_or_oversized_reports_never_enter_store(value):
    store = MemoryObjectStore()
    with pytest.raises(FrRuntimeError):
        store_flow_report(store, value)
    assert len(store) == 0


@pytest.mark.parametrize("schema", ["fr-flow-cache-1", "fr-flow-cache-2", "unknown", [], None, 2])
def test_noncurrent_manifest_refuses(schema):
    store = MemoryObjectStore()
    root = store_merkle_value(store, {"schema": schema, "entries": {}}).digest
    with pytest.raises(FrRuntimeError, match="manifest"):
        FlowCache.restore(store, root)


@pytest.mark.parametrize("schema", ["fr-dataflow-1", "fr-flow-record-1"])
def test_unchunked_or_noncurrent_record_envelopes_refuse(schema):
    store = MemoryObjectStore()
    root = store_merkle_value(store, {"schema": schema, "complete": True}).digest
    with pytest.raises(FrRuntimeError, match="envelope"):
        restore_flow_report(store, root)
