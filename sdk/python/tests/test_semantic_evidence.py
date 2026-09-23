import json
from dataclasses import replace
from pathlib import Path
import subprocess
import sys

import pytest

from fr_ir.context import MemoryObjectStore
from fr_ir.guide import AgentGoal, GoalSelector
from fr_ir.investigation import TaskPlan, TaskStep, StepState
from fr_ir.investigation_checks import attach_checks, restore_checks, run_checks
from fr_ir.origins import SemanticOrigins, SourceOrigins
from fr_ir.runtime import FrClient, FrReport, FrRuntimeError, Occurrence

ROOT = Path(__file__).resolve().parents[3]
FR = ROOT / "target/debug/fr"
FIXTURE = ROOT / "tests/agent-eval/semantic-evidence"


def client_for(tmp_path, source=None):
    (tmp_path / "subject.py").write_text(source or (FIXTURE / "subject.py").read_text())
    return FrClient(tmp_path, executable=str(FR))


def handle(client, name="total"):
    return client.project("find", name).definition_target().handle


def test_origins_match_independent_ast_and_authoring_pointers(tmp_path):
    client = client_for(tmp_path)
    report = SemanticOrigins.inspect(client, handle(client), limit=64)
    assert report.complete
    expected = json.loads(subprocess.check_output([sys.executable, str(FIXTURE / "oracle.py")]))
    calls = [item for item in report.items if item.kind == "call"]
    assert len(calls) == 2
    spans = [[item.origins.occurrences[0].location.span.start,
              item.origins.occurrences[0].location.span.end] for item in calls]
    assert spans == expected["calls"]
    assert calls[0].id != calls[1].id
    source = (tmp_path / "subject.py").read_text()
    for item in calls:
        occurrence = item.origins.occurrences[0]
        assert occurrence.text(source, revision=report.revision) == "café(amount)"
        assert report.for_body_pointer(item.body_pointer) == item
        assert report.for_occurrence(occurrence) == (item,)
    combined = [item for item in report.items if item.origins.status == "multiple"]
    assert len(combined) == 1
    assert [o.text(source, revision=report.revision) for o in combined[0].origins.occurrences] == ["result", "1"]
    assert any(item.kind == "null" and item.origins.status == "absent" for item in report.items)
    assert report.report.at("/origins/mutation_authority") is False


def test_paging_filtering_budgets_and_stale_cursors(tmp_path):
    client = client_for(tmp_path)
    target = handle(client)
    first = SemanticOrigins.inspect(client, target, limit=1)
    assert not first.complete and first.next_cursor
    second = SemanticOrigins.inspect(client, target, limit=1, cursor=first.next_cursor)
    assert second.items[0].id != first.items[0].id
    exact = SemanticOrigins.inspect(client, target, pointer=second.items[0].pointer)
    assert exact.complete and exact.items == second.items
    omitted = SemanticOrigins.inspect(client, target, nodes=1)
    assert not omitted.complete and not omitted.items
    (tmp_path / "subject.py").write_text((tmp_path / "subject.py").read_text() + "\n# changed\n")
    with pytest.raises(FrRuntimeError):
        SemanticOrigins.inspect(client, handle(client), cursor=first.next_cursor)
    with pytest.raises(FrRuntimeError):
        SemanticOrigins.inspect(client, target)
    with pytest.raises(FrRuntimeError, match="revision"):
        SemanticOrigins.inspect(client, handle(client)).for_occurrence(first.items[0].origins.occurrences[0])


def test_unmapped_languages_and_transformed_nodes_are_explicit(tmp_path):
    client = client_for(tmp_path, "def total(xs):\n    return xs % 2\n")
    report = SemanticOrigins.inspect(client, handle(client))
    assert report.items and all(item.origins.status == "absent" for item in report.items)
    (tmp_path / "value.rs").write_text("fn other(x: i32) -> i32 { x + 1 }\n")
    report = SemanticOrigins.inspect(client, handle(client, "other"))
    assert report.items and all(item.origins.status == "absent" for item in report.items)


def test_origin_validation_rejects_tampered_pointers_and_foreign_revisions(tmp_path):
    client = client_for(tmp_path)
    report = SemanticOrigins.inspect(client, handle(client))
    data = report.report.to_data()
    data["origins"]["items"][0]["pointer"] = "/model/items/0/value/body/0"
    with pytest.raises(FrRuntimeError):
        SemanticOrigins.from_report(FrReport(data, ()))
    value = {"status": "multiple", "occurrences": []}
    with pytest.raises(FrRuntimeError):
        SourceOrigins.from_data(value, revision=report.revision)


def checks_workspace(tmp_path, code="print('compiler identity: test-runtime'); assert 2 + 2 == 4"):
    client = client_for(tmp_path)
    (tmp_path / ".fr").mkdir()
    config = {"schema": 1, "checks": [{"name": "oracle", "argv": [sys.executable, "-c", code],
              "cwd": ".", "timeout_seconds": 10, "covers": ["finite runtime oracle"]}]}
    (tmp_path / ".fr/checks.json").write_text(json.dumps(config))
    plan = TaskPlan("validate total", ("oracle passes",), (
        TaskStep.checked("verify", "does the declared oracle pass?", checks=("oracle",),
                         satisfies=("oracle passes",)),))
    reviewed = client.call("checks", "--toolchain")
    return client, plan, reviewed


def test_executed_checks_complete_a_plan_and_retained_results_revalidate(tmp_path):
    client, plan, reviewed = checks_workspace(tmp_path)
    store = MemoryObjectStore()
    result = run_checks(plan, client, "verify", reviewed, store)
    assert result.passed and result.resumed.complete
    assert result.resumed.plan.steps[0].state == StepState.SATISFIED
    assert result.checks.at("/toolchain/programs/0/sha256")
    assert result.checks.at("/results/0/covers") == ["finite runtime oracle"]
    retained = restore_checks(store, result.report_root)
    assert retained.to_data() == result.checks.to_data()
    saved = result.resumed.plan.store(store)
    assert TaskPlan.restore(store, saved).resume(client).complete
    (tmp_path / "subject.py").write_text("def total(x):\n    return 0\n")
    stale = TaskPlan.restore(store, saved).resume(client)
    assert stale.invalidated == ("verify",) and not stale.complete
    with pytest.raises(FrRuntimeError, match="source snapshot"):
        attach_checks(plan.resume(client, transition="verify:start").plan, client, "verify", retained)


def test_failed_check_is_retained_without_satisfying_acceptance(tmp_path):
    client, plan, reviewed = checks_workspace(tmp_path, "raise RuntimeError('oracle disagreement')")
    store = MemoryObjectStore()
    result = run_checks(plan, client, "verify", reviewed, store)
    assert not result.passed and not result.resumed.complete
    assert result.attachment_error is None
    assert result.resumed.plan.steps[0].evidence[0].passed is False
    assert "oracle disagreement" in json.dumps(restore_checks(store, result.report_root).to_data())


def test_check_that_changes_source_cannot_attach_but_keeps_diagnostics(tmp_path):
    code = "from pathlib import Path; Path('subject.py').write_text('def changed(): pass\\n')"
    client, plan, reviewed = checks_workspace(tmp_path, code)
    result = run_checks(plan, client, "verify", reviewed, MemoryObjectStore())
    assert not result.passed and result.attachment_error
    assert result.report_root and result.checks.at("/source_snapshot_stable") is False
    assert result.resumed.invalidated == ("verify",)


@pytest.mark.parametrize("change", ["configuration", "toolchain", "unindexed_source"])
def test_bound_inputs_invalidate_checked_completion(tmp_path, change):
    client, plan, reviewed = checks_workspace(tmp_path)
    if change == "toolchain":
        wrapper = tmp_path / "runner"
        wrapper.write_text(f'#!/bin/sh\nexec "{sys.executable}" "$@"\n')
        wrapper.chmod(0o755)
        path = tmp_path / ".fr/checks.json"
        config = json.loads(path.read_text())
        config["checks"][0]["argv"][0] = str(wrapper)
        path.write_text(json.dumps(config))
        reviewed = client.call("checks", "--toolchain")
    result = run_checks(plan, client, "verify", reviewed, MemoryObjectStore())
    assert result.passed
    if change == "configuration":
        path = tmp_path / ".fr/checks.json"
        path.write_text(path.read_text() + "\n")
    elif change == "toolchain":
        wrapper.write_text(wrapper.read_text() + "# changed executable\n")
    else:
        (tmp_path / ".gitignore").write_text("ignored.py\n")
        (tmp_path / "ignored.py").write_text("x = 1\n")
    resumed = result.resumed.plan.resume(client)
    assert not resumed.complete and resumed.invalidated == ("verify",)


def test_check_report_rejects_changed_outcomes_and_incomplete_dependencies(tmp_path):
    client, plan, reviewed = checks_workspace(tmp_path)
    store = MemoryObjectStore()
    result = run_checks(plan, client, "verify", reviewed, store)
    data = result.checks.to_data()
    data["results"][0]["exit_code"] = 7
    started = plan.resume(client, transition="verify:start").plan
    with pytest.raises(FrRuntimeError, match="failure diagnostics"):
        attach_checks(started, client, "verify", FrReport(data, ()))
    incomplete = replace(plan, steps=(replace(plan.steps[0], inputs=plan.steps[0].inputs[:1]),))
    with pytest.raises(FrRuntimeError, match="dependencies"):
        attach_checks(incomplete.resume(client, transition="verify:start").plan, client, "verify", result.checks)


def test_guide_action_keeps_revision_and_ready_arguments(tmp_path):
    client = client_for(tmp_path)
    guide = client.guide(AgentGoal("understand", selector=GoalSelector(name="total")))
    action = next(action for action in guide.actions() if not action.to_data().get("author_fields"))
    step = TaskStep.from_guide("inspect", "what computes the total?", action, satisfies=("located",))
    assert step.action == action.arguments
    assert step.action_input == action.to_data()["input"]
    plan = TaskPlan("understand", ("located",), (step,))
    assert plan.resume(client).plan.steps[0].state == StepState.READY
    (tmp_path / "subject.py").write_text("def total(x):\n    return x\n")
    assert plan.resume(client).invalidated == ("inspect",)


def test_flow_occurrences_link_to_semantic_body_nodes(tmp_path):
    client = client_for(tmp_path, "def café(x):\n    return x\ndef total(amount):\n    return café(amount) + café(amount)\n")
    target = handle(client)
    origins = SemanticOrigins.inspect(client, target, limit=64)
    flow = client.project("dataflow", target, "--steps", "512")
    events = [Occurrence.from_data(event["occurrence"]) for event in flow.at("/events")]
    for item in origins.items:
        if item.kind == "call":
            source = item.origins.occurrences[0]
            matching = [event for event in events if event.location.span == source.location.span]
            assert matching
            assert all(item in origins.for_occurrence(event) for event in matching)


def test_native_continuation_and_page_coverage_validation(tmp_path):
    client = client_for(tmp_path)
    first = SemanticOrigins.inspect(client, handle(client), limit=1)
    continuation = first.report.at("/origins/continuation/arguments")
    second = SemanticOrigins.from_report(client.call(*continuation))
    assert first.items[0].id != second.items[0].id
    data = first.report.to_data()
    data["origins"]["complete"] = True
    with pytest.raises(FrRuntimeError, match="coverage"):
        SemanticOrigins.from_report(FrReport(data, ()))


def test_retained_check_objects_reject_tampering(tmp_path):
    client, plan, reviewed = checks_workspace(tmp_path)
    store = MemoryObjectStore()
    result = run_checks(plan, client, "verify", reviewed, store)

    class CorruptStore:
        def get(self, key):
            if key == result.report_root:
                return {"schema": "invented", "kind": "scalar", "value": True}
            return store.get(key)

        def put(self, key, record):
            store.put(key, record)

    with pytest.raises((FrRuntimeError, ValueError)):
        restore_checks(CorruptStore(), result.report_root)


def test_changed_review_refuses_before_execution(tmp_path):
    client, plan, reviewed = checks_workspace(tmp_path)
    config_path = tmp_path / ".fr/checks.json"
    config = json.loads(config_path.read_text())
    config["checks"][0]["argv"] = [sys.executable, "-c", "open('should-not-run', 'w').write('bad')"]
    config_path.write_text(json.dumps(config))
    with pytest.raises(FrRuntimeError, match="changed"):
        run_checks(plan, client, "verify", reviewed, MemoryObjectStore())
    assert not (tmp_path / "should-not-run").exists()


def test_origin_continuation_preserves_explicit_source_policy(tmp_path):
    client = client_for(tmp_path, "def total(x):\n    del x\n    return 1\n")
    first = client.project("semantic", handle(client), "--body", "--origins", "--unsupported-source", "--origin-limit", "1")
    following = first.at("/origins/continuation/arguments")
    assert "--unsupported-source" in following
    second = SemanticOrigins.from_report(client.call(*following))
    assert second.basis == first.at("/semantic_basis")
    assert second.report.at("/source_policy") == "explicit-unsupported-source"


def test_origins_refuse_edit_plan_only_output(tmp_path):
    client = client_for(tmp_path)
    with pytest.raises(FrRuntimeError, match="cannot be used"):
        client.project("semantic", handle(client), "--body", "--origins", "--locator-op", "set-int",
                       "--locator-from", "1", "--intent-to", "2")
