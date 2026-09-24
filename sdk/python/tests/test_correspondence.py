from dataclasses import replace
import json
from pathlib import Path

import pytest

from fr_ir.context import DirectoryObjectStore, MemoryObjectStore
from fr_ir.correspondence import DeclarationIdentity, DeclarationSnapshot, IdentityPage
from fr_ir.investigation import Dependency, DependencyKind, Evidence, EvidenceKind, StepState, TaskPlan, TaskStep
from fr_ir.investigation_session import InvestigationSession, target_inputs
from fr_ir.runtime import FrClient, FrReport, FrRuntimeError

FR = Path(__file__).resolve().parents[3] / "target/debug/fr"
SOURCE = "def café(value):\n    return value + 1\n"


@pytest.fixture
def project(tmp_path):
    (tmp_path / "subject.py").write_text(SOURCE)
    return FrClient(tmp_path, executable=str(FR)), tmp_path


def test_move_and_unicode_rename_keep_distinct_identity_layers(project, tmp_path):
    client, root = project
    captured = DeclarationSnapshot.capture(client)
    old = captured.items[0]
    assert old.occurrence.text(SOURCE, revision=captured.revision) == SOURCE.rstrip()
    store = MemoryObjectStore()
    digest = captured.persist(store)
    assert digest != old.object_digest != captured.revision
    restored = DeclarationSnapshot.restore(store, digest)
    assert restored == captured
    (root / "subject.py").rename(root / "moved.py")
    moved = restored.compare(client)
    assert moved.complete and moved.matches[0].status == "matched"
    new = moved.matches[0].candidates[0]
    assert new.object_digest == old.object_digest and new.handle != old.handle
    selected = moved.select(client, {old.handle: new.handle})
    assert selected.items == (new,)
    (root / "moved.py").write_text(SOURCE.replace("café", "renamed"))
    renamed = restored.compare(client)
    assert renamed.matches[0].status == "matched"
    new = renamed.matches[0].candidates[0]
    assert new.object_digest != old.object_digest and new.name_erased_digest == old.name_erased_digest
    assert renamed.matches[0].reasons[new.handle] == ("identical-except-declaration-name",)
    with pytest.raises(FrRuntimeError):
        client.project("show", old.handle)
    with pytest.raises(FrRuntimeError, match="stale"):
        moved.select(client, {old.handle: selected.items[0].handle})


def test_copy_does_not_hide_changed_original(project):
    client, root = project
    snapshot = DeclarationSnapshot.capture(client)
    (root / "copy.py").write_text(SOURCE)
    (root / "subject.py").write_text(SOURCE.replace("+ 1", "+ 2"))
    report = snapshot.compare(client)
    match = report.matches[0]
    assert report.complete and match.status == "ambiguous"
    assert {c.path for c in match.candidates} == {"subject.py", "copy.py"}
    changed = next(c for c in match.candidates if c.path == "subject.py")
    assert match.reasons[changed.handle] == ("same-path-scope-name-kind",)
    assert report.select(client, {match.previous.handle: changed.handle}).items == (changed,)


def test_shared_candidate_is_ambiguous_and_cannot_be_selected_twice(project):
    client, root = project
    (root / "second.py").write_text(SOURCE)
    snapshot = DeclarationSnapshot.capture(client)
    (root / "second.py").unlink()
    report = snapshot.compare(client)
    assert all(m.status == "ambiguous" for m in report.matches)
    assert all(m.candidate_count == 1 and len(m.conflicts) == 1 for m in report.matches)
    choices = {m.previous.handle: m.candidates[0].handle for m in report.matches}
    with pytest.raises(FrRuntimeError, match="same declaration"):
        report.select(client, choices)


def test_missing_and_rename_with_body_change_remain_missing(project):
    client, root = project
    snapshot = DeclarationSnapshot.capture(client)
    (root / "subject.py").write_text(SOURCE.replace("café", "renamed").replace("+ 1", "+ 2"))
    report = snapshot.compare(client)
    assert report.matches[0].status == "missing"
    with pytest.raises(FrRuntimeError, match="not a disclosed candidate"):
        report.select(client, {snapshot.items[0].handle: "invented"})
    (root / "subject.py").unlink()
    assert snapshot.compare(client).matches[0].status == "missing"


def test_scopes_disambiguate_changed_nested_declarations(project):
    client, root = project
    (root / "subject.py").write_text("class A:\n    def value(self):\n        return 1\n\nclass B:\n    def value(self):\n        return 2\n")
    all_items = DeclarationSnapshot.capture(client)
    methods = tuple(i.handle for i in all_items.items if i.name == "value")
    snapshot = all_items.subset(methods)
    (root / "subject.py").write_text("class A:\n    def value(self):\n        return 3\n\nclass B:\n    def value(self):\n        return 4\n")
    report = snapshot.compare(client)
    assert [m.status for m in report.matches] == ["matched", "matched"]
    assert all(m.previous.scope == m.candidates[0].scope for m in report.matches)
    assert len({m.previous.scope for m in report.matches}) == 2


def test_paging_and_candidate_budgets_preserve_incompleteness(project):
    client, root = project
    (root / "second.py").write_text("def different():\n    return 17\n")
    first = DeclarationSnapshot.capture(client, limit=1, max_pages=1)
    assert not first.complete and len(first.items) == 1
    assert not first.compare(client).complete
    full = DeclarationSnapshot.capture(client, limit=1)
    assert full.complete and len(full.items) == 3
    limited = full.compare(client, limit=1, max_pages=1)
    assert not limited.complete
    with pytest.raises(FrRuntimeError, match="complete"):
        limited.select(client, {limited.matches[0].previous.handle: limited.matches[0].candidates[0].handle})
    (root / "copy.py").write_text(SOURCE)
    clipped = full.compare(client, candidates=1)
    assert not clipped.complete
    ambiguous = next(m for m in clipped.matches if m.previous.name == "café")
    assert ambiguous.candidate_count == 2 and not ambiguous.candidates_complete
    assert ambiguous.status == "ambiguous"


def test_byte_budget_and_stale_cursor(project):
    client, root = project
    (root / "subject.py").write_text("\n".join(f"def name_{n}():\n    return {n}\n" for n in range(20)))
    page = IdentityPage.inspect(client, limit=20, max_bytes=4096)
    assert len(json.dumps(page.report.to_data(), separators=(",", ":"), ensure_ascii=False).encode()) <= 4096
    assert page.next_cursor and len(page.items) < 20
    assert DeclarationSnapshot.capture(client, limit=20, max_bytes=4096).complete
    (root / "new.py").write_text(SOURCE)
    with pytest.raises(FrRuntimeError, match="stale"):
        IdentityPage.inspect(client, limit=20, max_bytes=4096, cursor=page.next_cursor)


@pytest.mark.parametrize("field,value", [("complete", False), ("basis", "bad"), ("revision", "a" * 64)])
def test_page_tampering_refuses(project, field, value):
    client, _ = project
    data = IdentityPage.inspect(client).report.to_data()
    data[field] = value
    with pytest.raises(FrRuntimeError):
        IdentityPage.from_report(FrReport(data, ()))


@pytest.mark.parametrize("field,value", [("handle", "frp1:bad:1"), ("object_digest", "bad"),
                                        ("name_erased_digest", "bad"), ("scope", "wrong"), ("extra", True)])
def test_identity_tampering_refuses(project, field, value):
    client, _ = project
    data = DeclarationSnapshot.capture(client).items[0].to_data()
    data[field] = value
    with pytest.raises(FrRuntimeError):
        DeclarationIdentity.from_data(data)


def test_verified_merkle_objects_refuse_corruption(project, tmp_path):
    client, _ = project
    snapshot = DeclarationSnapshot.capture(client)
    store = MemoryObjectStore()
    digest = snapshot.persist(store)
    class Corrupt:
        def get(self, key):
            value = store.get(key)
            if key == digest:
                value["kind"] = "invented"
            return value
    with pytest.raises((FrRuntimeError, ValueError)):
        DeclarationSnapshot.restore(Corrupt(), digest)


def satisfied(plan, client, id):
    started = plan.resume(client, transition=f"{id}:start")
    evidence = Evidence("observation", EvidenceKind.OBSERVATION, started.input_digests[id], True, "retained-test-observation")
    updated = replace(started.plan, steps=tuple(replace(s, evidence=(evidence,)) if s.id == id else s for s in started.plan.steps))
    return updated.resume(client, transition=f"{id}:satisfy").plan


def session_fixture(client, root):
    (root / "independent.py").write_text("def independent():\n    return 99\n")
    snapshot = DeclarationSnapshot.capture(client, "subject.py")
    snapshot = snapshot.subset(tuple(i.handle for i in snapshot.items if i.name == "café"))
    old = snapshot.items[0]
    plan = TaskPlan("continue diagnosis", ("diagnosed",), (
        TaskStep("target", "where is the defect?", target_inputs(),
                 action=("project", "show", old.handle), satisfies=("diagnosed",)),
        TaskStep("dependent", "can we repair it?", (Dependency(DependencyKind.SOURCE, "subject.py"),), depends_on=("target",)),
        TaskStep("independent", "is the independent fact valid?", (Dependency(DependencyKind.SOURCE, "independent.py"),)),
    ))
    plan = satisfied(plan, client, "independent")
    plan = satisfied(plan, client, "target")
    plan = satisfied(plan, client, "dependent")
    return InvestigationSession.bind(plan, {"target": snapshot})


def test_session_resumption_refresh_and_independent_evidence(project, tmp_path_factory):
    client, root = project
    session = session_fixture(client, root)
    store = DirectoryObjectStore(tmp_path_factory.mktemp("objects"))
    digest = session.persist(store)
    restored = InvestigationSession.restore(store, digest)
    assert restored.to_data() == session.to_data()
    (root / "subject.py").rename(root / "moved.py")
    resumed = restored.resume(client)
    assert resumed.resumed.invalidated == ("target", "dependent")
    assert resumed.resumed.plan.steps[2].state == StepState.SATISFIED
    match = resumed.correspondence["target"].matches[0]
    refreshed = resumed.refresh(client, "target", {match.previous.handle: match.candidates[0].handle},
                                inputs=target_inputs("moved.py"))
    step = refreshed.plan.steps[0]
    assert step.state == StepState.READY and not step.action and not step.evidence and step.action_input is None
    assert step.satisfies == ("diagnosed",) and refreshed.plan.steps[1].state == StepState.STALE
    assert refreshed.plan.steps[2] == resumed.resumed.plan.steps[2]
    assert not refreshed.resume(client).resumed.complete
    assert refreshed.targets["target"].items[0].path == "moved.py"
    assert InvestigationSession.restore(store, digest).targets["target"].items[0].path == "subject.py"


def test_session_does_not_refresh_missing_partial_or_captured_inputs(project):
    client, root = project
    session = session_fixture(client, root)
    resumed = session.resume(client)
    match = resumed.correspondence["target"].matches[0]
    with pytest.raises(FrRuntimeError, match="every target"):
        resumed.refresh(client, "target", {}, inputs=target_inputs())
    with pytest.raises(FrRuntimeError, match="uncaptured"):
        resumed.refresh(client, "target", {match.previous.handle: match.candidates[0].handle}, inputs=session.plan.steps[0].inputs)
    (root / "subject.py").unlink()
    missing = session.resume(client)
    with pytest.raises(FrRuntimeError, match="not a disclosed candidate"):
        missing.refresh(client, "target", {match.previous.handle: "invented"},
                        inputs=target_inputs())


def test_interrupted_step_reopens_ready_without_evidence(project):
    client, root = project
    session = session_fixture(client, root)
    plan = session.plan.resume(client, transition="target:reset").plan
    plan = plan.resume(client, transition="target:start").plan
    session = InvestigationSession.bind(plan, session.targets)
    resumed = session.resume(client)
    assert resumed.resumed.plan.steps[0].state == StepState.READY
    assert not resumed.resumed.plan.steps[0].evidence
    assert not resumed.resumed.complete


def test_session_workspace_drift_during_comparison_refuses(project):
    client, root = project
    session = session_fixture(client, root)
    class Drift:
        def project(self, *args):
            result = client.project(*args)
            if args[0] == "investigate":
                (root / "new.py").write_text("def added():\n    return 0\n")
            return result
    with pytest.raises(FrRuntimeError, match="changed during"):
        session.resume(Drift())


def test_analyzer_change_invalidates_target_and_descendants(project):
    client, root = project
    session = session_fixture(client, root)
    steps = list(session.plan.steps)
    inputs = tuple(replace(d, digest="0" * 64) if d.kind == DependencyKind.DECLARATION_ANALYZER else d for d in steps[0].inputs)
    steps[0] = replace(steps[0], inputs=inputs)
    targets = {"target": replace(session.targets["target"], analyzer="0" * 64)}
    old_analyzer = InvestigationSession.bind(replace(session.plan, steps=tuple(steps)), targets)
    resumed = old_analyzer.resume(client)
    assert resumed.resumed.invalidated == ("target", "dependent")
    assert resumed.resumed.plan.steps[2].state == StepState.SATISFIED


def test_comparison_rejects_crossed_reasons_and_false_coverage(project):
    client, root = project
    snapshot = DeclarationSnapshot.capture(client)
    report = snapshot.compare(client)
    for field, value in [("status", "missing"), ("candidate_count", 99), ("action_rebound", True), ("reasons", [])]:
        data = report.pages[0].report.to_data()
        data["items"][0][field] = value
        with pytest.raises(FrRuntimeError):
            IdentityPage.from_report(FrReport(data, ()))


def test_source_changes_between_capture_pages_refuse(project):
    client, root = project
    class Drift:
        calls = 0
        def project(self, *args):
            result = client.project(*args)
            self.calls += 1
            if self.calls == 1:
                (root / "new.py").write_text("def added():\n    return 0\n")
            return result
    with pytest.raises(FrRuntimeError, match="stale"):
        DeclarationSnapshot.capture(Drift(), limit=1)


def test_added_same_name_declaration_preserves_ambiguity_and_stales_plan(project):
    client, root = project
    session = session_fixture(client, root)
    (root / "subject.py").write_text(SOURCE + "\ndef café(value):\n    return value + 7\n")
    resumed = session.resume(client)
    match = resumed.correspondence["target"].matches[0]
    assert match.status == "ambiguous" and match.candidate_count == 2
    assert resumed.resumed.plan.steps[0].state == StepState.STALE


def test_build_configuration_changes_stale_workspace_targets(project):
    client, root = project
    (root / "pyproject.toml").write_text("[project]\nname = 'fixture'\nversion = '1'\n")
    session = session_fixture(client, root)
    (root / "pyproject.toml").write_text("[project]\nname = 'fixture'\nversion = '2'\n")
    resumed = session.resume(client)
    assert resumed.resumed.invalidated == ("target", "dependent")
    assert resumed.resumed.plan.steps[2].state == StepState.SATISFIED


def test_capture_rejects_a_valid_correspondence_report(project):
    client, root = project
    snapshot = DeclarationSnapshot.capture(client)
    comparison = snapshot.compare(client)
    class CrossedQuery:
        def project(self, *args):
            return comparison.pages[0].report
    with pytest.raises(FrRuntimeError, match="another query or scope"):
        DeclarationSnapshot.capture(CrossedQuery())


@pytest.mark.parametrize("operation", ["capture", "compare"])
def test_requested_scope_cannot_change_in_transport(project, operation):
    client, root = project
    snapshot = DeclarationSnapshot.capture(client)
    class CrossedScope:
        def project(self, *args):
            data = client.project(*args).to_data()
            data["selection"] = "another-scope.py"
            return FrReport(data, args)
    with pytest.raises(FrRuntimeError, match="another query or scope"):
        if operation == "capture":
            DeclarationSnapshot.capture(CrossedScope())
        else:
            snapshot.compare(CrossedScope())
