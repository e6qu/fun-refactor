from pathlib import Path

import pytest

from fr_ir.context import MemoryObjectStore
from fr_ir.flow import ControlGraph, FlowCache
from fr_ir.runtime import FrClient, FrRuntimeError

FR = Path(__file__).resolve().parents[3] / "target/debug/fr"


def workspace(tmp_path):
    (tmp_path / "subject.py").write_text(
        "def run(count):\n    value = 0\n    while count:\n        sink(value)\n"
        "        value = source()\n        count = count - 1\n    return value\n"
    )
    rules = tmp_path / "rules.json"
    rules.write_text('{"version":"one","sources":["source"],"sinks":["sink"]}')
    client = FrClient(tmp_path, executable=str(FR), max_output_bytes=1_048_576)
    return client, rules


def target(client):
    return client.project("find", "run").definition_target().handle


def evidence(result):
    value = result.report.to_data()
    value.pop("execution")
    value.pop("context_basis", None)
    return value


def test_cold_warm_and_unrelated_edit_reuse_equal_clean_analysis(tmp_path):
    client, rules = workspace(tmp_path)
    store = MemoryObjectStore()
    cache = FlowCache(store)
    cold = cache.analyze(client, target(client), rules=rules, steps=1024, max_bytes=1_048_576)
    assert not cold.reused
    assert cold.graphs and cold.witnesses
    restored = FlowCache.restore(store, cache.persist())
    warm = restored.analyze(client, target(client), rules=rules, steps=1024, max_bytes=1_048_576)
    assert warm.reused
    assert evidence(cold) == evidence(warm)
    (tmp_path / "aaa.py").write_text("def earlier():\n    return 123\n")
    reused = restored.analyze(client, target(client), rules=rules, steps=1024, max_bytes=1_048_576)
    clean = FlowCache(store).analyze(client, target(client), rules=rules, steps=1024, max_bytes=1_048_576)
    assert reused.reused
    assert evidence(reused) == evidence(clean)
    assert reused.report.at("/revision") != cold.report.at("/revision")
    assert reused.report.at("/execution/analysis_steps") == 0
    assert reused.witnesses[0].occurrences[0].revision == reused.report.at("/revision")


@pytest.mark.parametrize("change", ["source", "rules", "configuration", "budget", "context", "negative-lookup"])
def test_relevant_changes_force_clean_analysis(tmp_path, change):
    client, rules = workspace(tmp_path)
    cache = FlowCache(MemoryObjectStore())
    cache.analyze(client, target(client), rules=rules, steps=1024)
    steps, context = 1024, "generic"
    if change == "source":
        file = tmp_path / "subject.py"
        file.write_text(file.read_text().replace("value = source()", "value = 1"))
    elif change == "rules":
        rules.write_text(rules.read_text().replace('"one"', '"two"'))
    elif change == "configuration":
        (tmp_path / "pyproject.toml").write_text('[project]\nname = "example"\nversion = "1.0"\n')
    elif change == "budget":
        steps = 512
    elif change == "context":
        context = "html"
    else:
        with (tmp_path / "subject.py").open("a") as stream:
            stream.write("\ndef new_helper():\n    return 1\n")
    fresh = cache.analyze(client, target(client), rules=rules, steps=steps, context=context)
    assert not fresh.reused
    clean = FlowCache(MemoryObjectStore()).analyze(client, target(client), rules=rules, steps=steps, context=context)
    assert evidence(fresh) == evidence(clean)


def test_tampered_cache_manifest_refuses_and_graphs_check_revisions_and_edges(tmp_path):
    client, rules = workspace(tmp_path)
    store = MemoryObjectStore()
    cache = FlowCache(store)
    result = cache.analyze(client, target(client), rules=rules, steps=1024)
    graph = result.report.at("/control_flow/run")
    with pytest.raises(FrRuntimeError, match="revision"):
        ControlGraph.from_data(graph, revision="0" * 64)
    graph["edges"][0]["to"] = 99999
    with pytest.raises(FrRuntimeError, match="edge"):
        ControlGraph.from_data(graph, revision=result.report.at("/revision"))
    root = cache.persist()

    class Corrupt:
        def get(self, key):
            record = store.get(key)
            if key == root:
                return {"kind": "scalar", "value": "forged"}
            return record

        def put(self, key, value):
            store.put(key, value)

    with pytest.raises((FrRuntimeError, ValueError)):
        FlowCache.restore(Corrupt(), root)


def test_rebinding_covers_multiple_origins_and_unused_helper_arguments(tmp_path):
    client, rules = workspace(tmp_path)
    (tmp_path / "subject.py").write_text(
        "def ignore(value):\n    return 0\ndef run(flag):\n    a = source()\n    b = source()\n"
        "    sink(a + b)\n    sink(b)\n    ignore(source())\n    return a + b\n"
    )
    store = MemoryObjectStore()
    cache = FlowCache(store)
    cache.analyze(client, target(client), rules=rules, steps=1024, max_bytes=1_048_576)
    for index in range(4):
        (tmp_path / "aaa.py").write_text(f"def unrelated():\n    return {index}\n")
        reused = cache.analyze(client, target(client), rules=rules, steps=1024, max_bytes=1_048_576)
        clean = FlowCache(store).analyze(client, target(client), rules=rules, steps=1024, max_bytes=1_048_576)
        assert reused.reused
        assert evidence(reused) == evidence(clean)


def test_incomplete_analyses_are_recomputed(tmp_path):
    client, rules = workspace(tmp_path)
    cache = FlowCache(MemoryObjectStore())
    for _ in range(2):
        result = cache.analyze(client, target(client), rules=rules, steps=1)
        assert not result.reused
        assert result.report.at("/complete") is False


def test_moves_deletions_and_stale_handles_do_not_rebind_actions(tmp_path):
    client, rules = workspace(tmp_path)
    cache = FlowCache(MemoryObjectStore())
    old = target(client)
    cache.analyze(client, old, rules=rules, steps=1024)
    (tmp_path / "subject.py").rename(tmp_path / "moved.py")
    with pytest.raises(FrRuntimeError):
        cache.analyze(client, old, rules=rules, steps=1024)
    moved = cache.analyze(client, target(client), rules=rules, steps=1024)
    assert not moved.reused
    old = target(client)
    (tmp_path / "moved.py").unlink()
    with pytest.raises(FrRuntimeError):
        cache.analyze(client, old, rules=rules, steps=1024)


def test_unreadable_configuration_forces_incomplete_rebuild(tmp_path):
    client, rules = workspace(tmp_path)
    (tmp_path / "pyproject.toml").write_bytes(b"\xff")
    cache = FlowCache(MemoryObjectStore())
    for _ in range(2):
        result = cache.analyze(client, target(client), rules=rules, steps=1024)
        assert not result.reused
        assert "configuration-snapshot-incomplete" in result.report.at("/cutoffs")
