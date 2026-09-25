from dataclasses import replace
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

import pytest

from fr_ir.context import MemoryObjectStore
from fr_ir.flow import FlowCache
from fr_ir.flow_dependencies import FlowDependencies
from fr_ir.investigation import Dependency, DependencyKind, Evidence, EvidenceKind, TaskPlan, TaskStep, StepState
from fr_ir.runtime import FrClient, FrReport, FrRuntimeError

ROOT = Path(__file__).resolve().parents[3]
FIXTURE = ROOT / "tests/agent-eval/imported-flow"


def workspace(tmp_path):
    for path in FIXTURE.glob("*.py"):
        if path.name != "oracle.py":
            shutil.copyfile(path, tmp_path / path.name)
    rules = tmp_path / "rules.json"
    rules.write_text(json.dumps({"version": "imported-1", "sources": ["source"], "sinks": ["sink"],
                                 "sanitizers": {"clean": "html"}}))
    return FrClient(tmp_path, executable=str(ROOT / "target/debug/fr"), max_output_bytes=1_048_576), rules


def analyze(client, rules, name="positive", cache=None, **options):
    handle = client.project("find", name).definition_target().handle
    return (cache or FlowCache(MemoryObjectStore())).analyze(
        client, handle, rules=rules, steps=options.pop("steps", 4096),
        max_bytes=options.pop("max_bytes", 1_048_576), summaries=True, imports=True,
        context=options.pop("context", "html"), **options)


def evidence(result):
    value = result.report.to_data()
    value.pop("execution")
    value.pop("context_basis", None)
    return value


def test_imports_match_runtime_and_independent_call_coordinates(tmp_path):
    client, rules = workspace(tmp_path)
    oracle = json.loads(subprocess.check_output(["python3", str(FIXTURE / "oracle.py"), str(tmp_path)]))
    for context, expected in oracle.items():
        for name, reaches in expected["outcomes"].items():
            result = analyze(client, rules, name, context=context)
            assert result.report.at("/complete"), result.report.at("/cutoffs")
            assert bool(result.witnesses) == reaches, (context, name)
            assert result.dependencies.complete
            assert len(result.dependencies.files) == 5
            for witness in result.witnesses:
                for point in witness.occurrences:
                    if point.role in {"source", "sink", "call-result", "summary-call"}:
                        span = point.location.span
                        assert [span.start, span.end] in expected["call_spans"][point.path]
    result = analyze(client, rules)
    assert result.summaries.for_function("relay.py::forward").return_parameters == (0,)
    assert result.summaries.for_function("leaf.py::identity").return_parameters == (0,)
    assert {point.path for point in result.witnesses[0].occurrences} == {"app.py", "relay.py", "leaf.py"}
    assert analyze(client, rules, "negative").summaries.for_function("erase.py::forward").return_parameters == ()


def test_imported_cache_restores_and_rebinds_all_files(tmp_path):
    client, rules = workspace(tmp_path)
    store = MemoryObjectStore()
    cache = FlowCache(store)
    first = analyze(client, rules, cache=cache)
    cache = FlowCache.restore(store, cache.persist())
    assert evidence(first) == evidence(analyze(client, rules, cache=cache))
    (tmp_path / "unrelated.py").write_text("def unrelated():\n    return 42\n")
    reused = analyze(client, rules, cache=cache)
    assert reused.reused and evidence(reused) == evidence(analyze(client, rules))
    reused.summaries
    reused.graphs


@pytest.mark.parametrize("change", ["helper", "delete", "rename", "package", "stub", "config", "rules", "duplicate", "import-alias"])
def test_dependency_changes_match_clean_rebuild(tmp_path, change):
    client, rules = workspace(tmp_path)
    cache = FlowCache(MemoryObjectStore())
    first = analyze(client, rules, cache=cache)
    if change == "helper":
        (tmp_path / "leaf.py").write_text("def identity(value):\n    return 0\n")
    elif change == "delete":
        (tmp_path / "leaf.py").unlink()
    elif change == "rename":
        (tmp_path / "leaf.py").rename(tmp_path / "moved.py")
    elif change == "package":
        (tmp_path / "leaf").mkdir()
        (tmp_path / "leaf/__init__.py").write_text("def identity(value):\n    return 0\n")
    elif change == "stub":
        (tmp_path / "leaf.pyi").write_text("def identity(value): ...\n")
    elif change == "config":
        (tmp_path / "pyproject.toml").write_text('[project]\nname="changed"\n')
    elif change == "rules":
        rules.write_text(rules.read_text().replace("imported-1", "imported-2"))
    elif change == "duplicate":
        with (tmp_path / "leaf.py").open("a") as stream:
            stream.write("\ndef identity(value):\n    return 0\n")
    else:
        path = tmp_path / "relay.py"
        path.write_text(path.read_text().replace("from leaf import identity", "from erase import forward as identity"))
    fresh = analyze(client, rules, cache=cache)
    assert not fresh.reused
    assert fresh.report.at("/input_digest") != first.report.at("/input_digest")
    assert evidence(fresh) == evidence(analyze(client, rules))
    assert fresh.report.at("/complete") == (change in {"helper", "config", "rules", "import-alias"})
    if change in {"helper", "import-alias"}:
        assert not fresh.witnesses


def test_negative_module_and_member_lookups_change_when_definitions_appear(tmp_path):
    client, rules = workspace(tmp_path)
    (tmp_path / "leaf.py").unlink()
    missing = analyze(client, rules)
    assert not missing.report.at("/complete")
    leaf = next(lookup for lookup in missing.dependencies.lookups if lookup.module == "leaf")
    assert not leaf.admitted and all(item.status == "missing" for item in leaf.candidates)
    (tmp_path / "leaf.py").write_text("def other(value):\n    return value\n")
    member = analyze(client, rules)
    assert not member.report.at("/complete")
    assert "missing-imported-member:identity" in member.report.at("/cutoffs")
    (tmp_path / "leaf.py").write_text("def identity(value):\n    return value\n")
    complete = analyze(client, rules)
    assert complete.report.at("/complete") and complete.witnesses
    assert len({item.report.at("/input_digest") for item in (missing, member, complete)}) == 3


@pytest.mark.parametrize("source,cutoff", [
    ("from leaf import *\ndef forward(value):\n    return identity(value)\n", "unsupported-import-form"),
    ("from .leaf import identity\ndef forward(value):\n    return identity(value)\n", "unsupported-import-form"),
    ("from package.leaf import identity\ndef forward(value):\n    return identity(value)\n", "unsupported-import-form"),
    ("import leaf\ndef forward(leaf):\n    return leaf.identity(0)\n", "ambiguous-call:leaf.identity"),
    ("import leaf\ndef forward(value):\n    result = leaf.identity(value)\n    leaf = 0\n    return result\n", "ambiguous-call:leaf.identity"),
    ("import leaf\nleaf = 0\ndef forward(value):\n    return leaf.identity(value)\n", "module-effects-unchecked"),
    ("import app\ndef forward(value):\n    return value\n", "cyclic-module-initialization"),
    ("import leaf as forward\ndef forward(value):\n    return value\n", "ambiguous-module-binding"),
])
def test_unsupported_import_execution_stays_incomplete(tmp_path, source, cutoff):
    client, rules = workspace(tmp_path)
    (tmp_path / "relay.py").write_text(source)
    result = analyze(client, rules)
    assert not result.report.at("/complete")
    assert cutoff in result.report.at("/cutoffs")


def test_imported_sink_sites_with_equal_byte_offsets_remain_distinct(tmp_path):
    client, rules = workspace(tmp_path)
    for name in ("left", "right"):
        (tmp_path / f"{name}.py").write_text("def emit(value):\n    sink(value)\n    return 0\n")
    (tmp_path / "app.py").write_text("import left\nimport right\ndef positive():\n    value = source()\n    left.emit(value)\n    return right.emit(value)\n")
    result = analyze(client, rules)
    assert result.report.at("/complete")
    assert {witness["site"]["path"] for witness in result.report.at("/witnesses")} == {"left.py", "right.py"}


def test_flow_dependency_resumes_independent_evidence_and_invalidates_dependents(tmp_path):
    client, rules = workspace(tmp_path)
    result = analyze(client, rules)
    plan = TaskPlan("inspect", ("flow",), (
        TaskStep("flow", "Does the source reach the sink?", (result.dependencies.dependency,), satisfies=("flow",)),
        TaskStep("independent", "Read unrelated source", (Dependency(DependencyKind.SOURCE, "note.py"),)),
        TaskStep("conclusion", "Explain", (Dependency(DependencyKind.SOURCE, "app.py"),), depends_on=("flow",)),
    ))
    resumed = plan.resume(client)
    assert resumed.plan.steps[0].state == StepState.READY
    for name in ("flow", "independent"):
        resumed = resumed.plan.resume(client, transition=f"{name}:start")
        receipt = Evidence(name, EvidenceKind.OBSERVATION, resumed.input_digests[name], True, "local-report")
        plan = replace(resumed.plan, steps=tuple(replace(step, evidence=(receipt,)) if step.id == name else step for step in resumed.plan.steps))
        resumed = plan.resume(client, transition=f"{name}:satisfy")
    store = MemoryObjectStore()
    plan = TaskPlan.restore(store, resumed.plan.store(store))
    (tmp_path / "unrelated.py").write_text("def other():\n    return 0\n")
    assert plan.resume(client).plan.steps[0].state == StepState.SATISFIED
    (tmp_path / "leaf.py").write_text("def identity(value):\n    return 0\n")
    stale = plan.resume(client)
    assert stale.plan.steps[0].state == StepState.STALE
    assert stale.plan.steps[2].state == StepState.STALE
    assert stale.plan.steps[1].state == StepState.SATISFIED


@pytest.mark.parametrize("change", ["admission", "digest", "missing-candidate", "path", "complete", "input-digest"])
def test_typed_dependency_tampering_refuses(tmp_path, change):
    client, rules = workspace(tmp_path)
    value = analyze(client, rules).report.to_data()
    modules = value["inputs"]["modules"]
    lookup = modules["lookups"][0]
    if change == "admission":
        lookup["admitted"] = False
    elif change == "digest":
        modules["files"]["relay.py"] = "0" * 64
    elif change == "missing-candidate":
        lookup["candidates"].pop(next(iter(lookup["candidates"])))
    elif change == "path":
        modules["files"]["../outside.py"] = "0" * 64
    elif change == "complete":
        modules["complete"] = False
    else:
        value["input_digest"] = "0" * 64
    if change != "input-digest":
        value["input_digest"] = hashlib.sha256(json.dumps(value["inputs"], sort_keys=True, ensure_ascii=False, separators=(",", ":")).encode()).hexdigest()
    with pytest.raises(FrRuntimeError):
        FlowDependencies.from_report(FrReport(value, ()))


def test_import_budgets_never_reuse_incomplete_results(tmp_path):
    client, rules = workspace(tmp_path)
    for number in range(18):
        text = f"import module{number + 1}\n" if number < 17 else ""
        (tmp_path / f"module{number}.py").write_text(text + "def forward(value):\n    return value\n")
    (tmp_path / "app.py").write_text("import module0\ndef positive():\n    return module0.forward(source())\n")
    cache = FlowCache(MemoryObjectStore())
    for _ in range(2):
        result = analyze(client, rules, cache=cache)
        assert not result.reused and not result.report.at("/complete")
        assert "import-module-budget" in result.report.at("/cutoffs")
    (tmp_path / "module0.py").write_text("#" + "x" * 262_144 + "\ndef forward(value):\n    return value\n")
    result = analyze(client, rules)
    assert "import-source-byte-budget" in result.report.at("/cutoffs")
    assert not result.report.at("/complete")


def test_import_lookup_and_analysis_budgets_are_explicit(tmp_path):
    client, rules = workspace(tmp_path)
    (tmp_path / "relay.py").write_text("\n".join(f"import leaf as alias{i}" for i in range(129)) + "\ndef forward(value):\n    return value\n")
    result = analyze(client, rules)
    assert "import-lookup-budget" in result.report.at("/cutoffs")
    assert len(result.dependencies.lookups) <= 128
    client, rules = workspace(tmp_path)
    result = analyze(client, rules, steps=1)
    assert "step-budget" in result.report.at("/cutoffs")
    assert not result.report.at("/complete")


def test_imported_symlink_and_ignored_source_do_not_establish_absence(tmp_path):
    client, rules = workspace(tmp_path)
    leaf = tmp_path / "leaf.py"
    leaf.unlink()
    (tmp_path / "alternate.py").write_text("def identity(value):\n    return value\n")
    leaf.symlink_to(tmp_path / "alternate.py")
    result = analyze(client, rules)
    assert not result.report.at("/complete")
    lookup = next(item for item in result.dependencies.lookups if item.module == "leaf")
    assert next(item for item in lookup.candidates if item.path == "leaf.py").status == "symlink"
    leaf.unlink()
    leaf.write_text("def identity(value):\n    return value\n")
    subprocess.run(["git", "init", "-q", str(tmp_path)], check=True)
    (tmp_path / ".gitignore").write_text("leaf.py\n")
    result = analyze(client, rules)
    assert not result.report.at("/complete")
    lookup = next(item for item in result.dependencies.lookups if item.module == "leaf")
    assert next(item for item in lookup.candidates if item.path == "leaf.py").status == "outside-snapshot"


def test_imported_origins_use_utf8_coordinates(tmp_path):
    client, rules = workspace(tmp_path)
    for name in ("app.py", "relay.py", "leaf.py"):
        path = tmp_path / name
        path.write_text('# café 🦀\n' + path.read_text())
    expected = json.loads(subprocess.check_output(["python3", str(FIXTURE / "oracle.py"), str(tmp_path)]))["html"]
    result = analyze(client, rules)
    for point in result.witnesses[0].occurrences:
        source = (tmp_path / point.path).read_text()
        point.text(source, revision=result.report.at("/revision"))
        if point.role in {"source", "sink", "call-result", "summary-call"}:
            assert [point.location.span.start, point.location.span.end] in expected["call_spans"][point.path]


def test_imported_raise_ends_caller_before_sink(tmp_path):
    client, rules = workspace(tmp_path)
    (tmp_path / "leaf.py").write_text("def identity(value):\n    raise value\n")
    result = analyze(client, rules)
    assert result.report.at("/complete") and not result.witnesses
    assert result.report.at("/completion/may_raise")
    assert not result.report.at("/completion/normal_return")
    assert result.summaries.for_function("leaf.py::identity").exceptional_returns


def test_imported_fact_pages_preserve_cross_file_semantic_links(tmp_path):
    from fr_ir.flow_facts import FlowFacts
    client, rules = workspace(tmp_path)
    target = client.project("find", "positive").definition_target().handle
    page = FlowFacts.inspect(client, target, rules=rules, imports=True, steps=4096, limit=64, max_bytes=1_048_576)
    witness = next(fact for fact in page.items if fact.kind == "witness")
    details = page.explain(client, witness)
    points = list(details.evidence)
    while details.report.at("/continuation") is not None:
        details = details.next(client)
        points.extend(details.evidence)
    assert {point.occurrence.path for point in points} == {"app.py", "relay.py", "leaf.py"}
    assert any(point.mapping["status"] == "mapped" and point.occurrence.path == "leaf.py" for point in points)
    (tmp_path / "leaf.py").write_text("def identity(value):\n    return 0\n")
    with pytest.raises(FrRuntimeError):
        page.explain(client, witness)


def test_external_rules_allow_analysis_but_refuse_workspace_plan_dependency(tmp_path):
    client, rules = workspace(tmp_path)
    outside = tmp_path.parent / "external-rules.json"
    shutil.move(rules, outside)
    result = analyze(client, outside)
    assert result.report.at("/complete")
    with pytest.raises(FrRuntimeError, match="relative"):
        result.dependencies.dependency


def test_deleted_entry_invalidates_an_existing_dependency(tmp_path):
    client, rules = workspace(tmp_path)
    result = analyze(client, rules)
    plan = TaskPlan("read", ("done",), (TaskStep("flow", "flow?", (result.dependencies.dependency,)),))
    current = plan.resume(client)
    (tmp_path / "app.py").unlink()
    assert current.plan.resume(client).plan.steps[0].state == StepState.STALE


def test_rule_contract_cannot_overlap_any_imported_binding(tmp_path):
    client, rules = workspace(tmp_path)
    (tmp_path / "leaf.py").write_text("def identity(value):\n    return value\ndef source():\n    return 0\n")
    with pytest.raises(FrRuntimeError, match="overlaps"):
        analyze(client, rules)


def test_cached_occurrence_cannot_escape_validated_module_closure(tmp_path):
    client, rules = workspace(tmp_path)
    result = analyze(client, rules)
    value = result.report.to_data()
    (tmp_path / "outside.py").write_text("def outside():\n    return 0\n")
    value["witnesses"][0]["trace"]["occurrences"][0]["path"] = "outside.py"
    encoded = json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(",", ":"))
    retained = tmp_path / "retained.json"
    retained.write_text(encoded)
    target = client.project("find", "positive").definition_target().handle
    with pytest.raises(FrRuntimeError, match="escapes"):
        client.project("dataflow", target, "--summaries", "--imports", "--rules", str(rules),
                       "--context", "html", "--steps", "4096", "--bytes", "1048576",
                       "--reuse", str(retained), "--reuse-digest", hashlib.sha256(encoded.encode()).hexdigest())


def test_analyzer_identity_changes_force_clean_analysis(tmp_path):
    client, rules = workspace(tmp_path)
    result = analyze(client, rules)
    value = result.report.to_data()
    value["inputs"]["analyzer"] = "0" * 64
    encoded = json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(",", ":"))
    retained = tmp_path / "retained.json"
    retained.write_text(encoded)
    target = client.project("find", "positive").definition_target().handle
    fresh = client.project("dataflow", target, "--summaries", "--imports", "--rules", str(rules),
                           "--context", "html", "--steps", "4096", "--bytes", "1048576",
                           "--reuse", str(retained), "--reuse-digest", hashlib.sha256(encoded.encode()).hexdigest())
    assert fresh.at("/execution/kind") == "analyzed"
    assert fresh.at("/complete") and fresh.at("/input_digest") == result.report.at("/input_digest")


def test_imported_response_cutoff_does_not_enter_cache(tmp_path):
    client, rules = workspace(tmp_path)
    cache = FlowCache(MemoryObjectStore())
    for _ in range(2):
        result = analyze(client, rules, cache=cache, max_bytes=16384)
        assert not result.reused and not result.report.at("/complete")
        assert "response-budget" in result.report.at("/cutoffs")
