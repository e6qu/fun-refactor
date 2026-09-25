import json
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
    return cache.analyze(client, target, rules=rules, max_bytes=1_048_576)


@pytest.mark.parametrize("disk", [False, True])
def test_legacy_cache_reopens_rebinds_and_migrates_without_partial_reuse(tmp_path, disk):
    client, rules = workspace(tmp_path)
    store = DirectoryObjectStore(tmp_path / "objects") if disk else MemoryObjectStore()
    original = analyze(FlowCache(store), client, rules).report.to_data()
    legacy = store_merkle_value(store, original).digest
    root = store_merkle_value(store, {"schema": "fr-flow-cache-1", "entries": {
        original["input_digest"]: legacy}}).digest
    cache = FlowCache.restore(store, root)
    (tmp_path / "extra.py").write_text("def independent():\n    return 7\n")
    result = analyze(cache, client, rules)
    assert result.reused and result.report.at("/revision") != original["revision"]
    manifest = restore_stored_value(store, cache.persist())
    assert manifest["schema"] == "fr-flow-cache-2"
    retained = restore_stored_value(store, manifest["entries"][original["input_digest"]])
    assert retained["schema"] == "fr-flow-record-1"
    cache = FlowCache.restore(store, cache.persist())
    assert analyze(cache, client, rules).reused
    (tmp_path / "app.py").write_text("def entry():\n    return sink(0)\n")
    fresh = analyze(cache, client, rules)
    assert not fresh.reused and not fresh.witnesses


def test_large_unicode_record_roundtrips_through_bounded_directory_records(tmp_path):
    store = DirectoryObjectStore(tmp_path)
    report = {"schema": "fr-dataflow-1", "complete": True, "text": '\"λ🙂\\\n' * 65_000}
    encoded = json.dumps(report, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
    assert 700_000 < len(encoded.encode()) < 1_048_576
    digest = store_flow_report(store, report)
    assert restore_flow_report(store, digest) == report
    assert len(list(tmp_path.rglob("*.json"))) < 40
    assert all(path.stat().st_size < 1_048_576 for path in tmp_path.rglob("*.json"))


def test_changed_chunk_is_rejected_by_content_verification(tmp_path):
    store = DirectoryObjectStore(tmp_path)
    digest = store_flow_report(store, {"schema": "fr-dataflow-1", "complete": True})
    for path in tmp_path.rglob("*.json"):
        record = json.loads(path.read_text())
        if record.get("kind") == "scalar" and 'fr-dataflow-1' in str(record.get("value")):
            record["value"] = record["value"].replace("true", "false")
            path.write_text(json.dumps(record))
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
    root = store_merkle_value(store, {"schema": "fr-flow-record-1", "chunks": chunks}).digest
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


@pytest.mark.parametrize("schema", ["fr-flow-cache-3", [], None, 2])
def test_unknown_manifest_version_refuses(schema):
    store = MemoryObjectStore()
    root = store_merkle_value(store, {"schema": schema, "entries": {}}).digest
    with pytest.raises(FrRuntimeError, match="manifest"):
        FlowCache.restore(store, root)
