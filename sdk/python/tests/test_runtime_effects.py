"""Effect views preserve unchecked behavior, source identity and saved-answer limits."""
import copy
import json
from pathlib import Path

import pytest

from fr_ir.context import MemoryObjectStore
from fr_ir.flow import FlowCache
from fr_ir.flow_effects import RuntimeEffects
from fr_ir.runtime import FrClient, FrReport, FrRuntimeError

ROOT = Path(__file__).resolve().parents[3]


def workspace(tmp_path):
    (tmp_path / "subject.py").write_text("def entry():\n    return send((read_user() // 2) % 7)\n")
    rules = tmp_path / "rules.json"
    rules.write_text(json.dumps({"version": "effects", "sources": ["read_user"], "sinks": ["send"],
                                 "propagators": [], "sanitizers": {}}))
    return FrClient(tmp_path, executable=str(ROOT / "target/debug/fr"), max_output_bytes=262144), rules


def analyze(client, rules, cache):
    handle = client.project("find", "entry").definition_target().handle
    return cache.analyze(client, handle, standard_library=(), rules=rules, context="generic", steps=4096,
                         depth=8, max_bytes=262144, summaries=True, imports=False)


def test_typed_effect_sites_rebind_to_the_current_revision_when_a_saved_answer_is_reused(tmp_path):
    client, rules = workspace(tmp_path)
    cache = FlowCache(MemoryObjectStore())
    first = analyze(client, rules, cache)
    effects = first.runtime_effects
    assert not effects.exception_safety_established
    assert not effects.shared_mutable_values_supported
    assert len(effects.sites) == 2 and all(site.kind == "implicit-exception" for site in effects.sites)
    assert effects.sites[0].occurrence.path == "subject.py"
    (tmp_path / "unrelated.py").write_text("def other():\n    return 0\n")
    reused = analyze(client, rules, cache)
    assert reused.reused
    current = reused.runtime_effects.sites[0].occurrence
    assert current.revision != effects.sites[0].occurrence.revision
    assert current.revision == reused.report.at("/revision")
    clean = analyze(client, rules, FlowCache(MemoryObjectStore()))
    assert reused.runtime_effects == clean.runtime_effects


@pytest.mark.parametrize("change", ["safety", "missing", "omitted", "outside", "duplicate"])
def test_typed_effect_views_refuse_altered_or_missing_disclosures(tmp_path, change):
    client, rules = workspace(tmp_path)
    result = analyze(client, rules, FlowCache(MemoryObjectStore()))
    data = copy.deepcopy(result.report.to_data())
    if change == "safety":
        data["runtime_effects"]["exception_safety_established"] = True
    elif change == "missing":
        del data["runtime_effects"]
    elif change == "omitted":
        data["omitted"] = {"effect_limits": 1}
        data["effect_limits"] = []
        data["complete"] = False
    elif change == "outside":
        data["effect_limits"][0]["occurrence"]["path"] = "outside.py"
    else:
        data["effect_limits"] *= 2
    with pytest.raises(FrRuntimeError):
        RuntimeEffects.from_report(FrReport(data, ()))
