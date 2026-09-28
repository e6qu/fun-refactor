import json
from pathlib import Path
import subprocess
import sys

import pytest

from fr_ir.context import DirectoryObjectStore
from fr_ir.investigation import TaskPlan, TaskStep
from fr_ir.investigation_proofs import ProofReport, attach_proofs, run_proofs
from fr_ir.refinement import ModelComparison, ModelSnapshot, _digest
from fr_ir.runtime import FrClient, FrReport, FrRuntimeError

FR = Path(__file__).resolve().parents[3]/"target/debug/fr"
OLD = "pub fn allowed(a: bool, b: bool, unused: bool) -> bool { a && !b }\n"


def fixture(tmp_path, old=OLD):
    root = tmp_path/"project"
    root.mkdir()
    (root/"subject.rs").write_text(old)
    client = FrClient(root, executable=FR)
    client.call("spec", "init", "--write")
    return root, client, ModelSnapshot.capture(client, "subject.rs::allowed")


def prove(root, client, name, tactics="decide\n"):
    proof = root/"tactics.txt"
    proof.write_text(tactics)
    target = f"specs/FrSpecs/{name}.lean::preserves"
    client.call("spec", "proof-check", target, "--from", str(proof))
    client.call("spec", "prove", target, "--from", str(proof), "--write")


@pytest.mark.parametrize("language", ["rust", "python"])
def test_equivalence_reorders_removes_and_retains_old_source(tmp_path, language):
    root, client, before = fixture(tmp_path)
    store = DirectoryObjectStore(tmp_path/"objects")
    before = ModelSnapshot.restore(store, before.persist(store))
    (root/"subject.rs").unlink()
    path = "new.rs" if language == "rust" else "new.py"
    source = ("pub fn permits(b: bool, a: bool) -> bool { !b && a }\n" if language == "rust"
              else "def permits(b: bool, a: bool) -> bool:\n    return not b and a\n")
    (root/path).write_text(source)
    comparison = ModelComparison("Preserved", before, f"{path}::permits", (1, 0))
    review = comparison.review(client)
    assert len(review.at("/changes")) == 2 and review.at("/source_implementation_proved") is False
    assert comparison.execute(client, review).at("/applied") is True
    prove(root, client, comparison.name)
    plan = TaskPlan("Preserve the old Boolean model", ("same model outputs",), (
        TaskStep.proved("proof", "Are the models equal under the selected map?",
            proofs=(comparison.requirement(),), satisfies=("same model outputs",)),))
    result = run_proofs(plan, client, "proof", ProofReport.review(client), store)
    assert result.passed and result.resumed.complete
    restored = TaskPlan.restore(store, result.resumed.plan.store(store))
    assert restored.resume(client).complete
    # Fresh processes restore both the historical source and the checked plan.
    script = """import sys
from fr_ir.context import DirectoryObjectStore
from fr_ir.investigation import TaskPlan
from fr_ir.runtime import FrClient
store=DirectoryObjectStore(sys.argv[1])
assert TaskPlan.restore(store,sys.argv[2]).resume(FrClient(sys.argv[3],executable=sys.argv[4])).complete
"""
    subprocess.run([sys.executable, "-c", script, str(tmp_path/"objects"),
        result.resumed.plan.store(store), str(root), str(FR)], check=True)
    (root/path).write_text(source.replace("!b && a", "b && a").replace("not b and a", "b and a"))
    assert not restored.resume(client).complete
    with pytest.raises(FrRuntimeError, match="changed"):
        attach_proofs(restored, client, "proof", result.proofs)
    with pytest.raises(FrRuntimeError, match="comparison source changed"):
        client.call("spec", "check", "specs", "--strict")


@pytest.mark.parametrize("new,relation,passed", [
    ("a && b", "refines", True), ("a || b", "refines", False),
    ("a && b", "equivalent", False), ("a", "equivalent", True),
])
def test_refinement_direction_and_false_claims(tmp_path, new, relation, passed):
    root, client, before = fixture(tmp_path, "pub fn allowed(a: bool, b: bool) -> bool { a }\n")
    (root/"subject.rs").write_text(f"pub fn allowed(a: bool, b: bool) -> bool {{ {new} }}\n")
    comparison = ModelComparison("Rule", before, "subject.rs::allowed", (0, 1), relation)
    comparison.execute(client, comparison.review(client))
    if passed:
        prove(root, client, "Rule")
        assert ProofReport.review(client).execute(client).passed
    else:
        with pytest.raises(FrRuntimeError):
            prove(root, client, "Rule")
        assert not ProofReport.review(client).execute(client).passed


def test_wrong_map_does_not_prove_noncommutative_boolean_function(tmp_path):
    root, client, before = fixture(tmp_path)
    (root/"subject.rs").write_text("pub fn allowed(b: bool, a: bool) -> bool { a && !b }\n")
    comparison = ModelComparison("Wrong", before, "subject.rs::allowed", (0, 1))
    comparison.execute(client, comparison.review(client))
    with pytest.raises(FrRuntimeError):
        prove(root, client, "Wrong")


@pytest.mark.parametrize("source", [
    "pub fn allowed(a: i64) -> bool { a > 0 }\n",
    "pub fn allowed(a: bool) -> bool { helper(a) }\n",
    "pub fn allowed(a: bool) -> bool { println!(\"effect\"); a }\n",
    "pub unsafe fn allowed(a: bool) -> bool { a }\n",
    "pub fn allowed("+", ".join(f"a{i}: bool" for i in range(9))+") -> bool { a0 }\n",
])
def test_snapshot_refuses_outside_boolean_domain(tmp_path, source):
    root = tmp_path/"project"
    root.mkdir()
    (root/"subject.rs").write_text(source)
    with pytest.raises(FrRuntimeError):
        ModelSnapshot.capture(FrClient(root, executable=FR), "subject.rs::allowed")


@pytest.mark.parametrize("field", ["source", "model", "mapping", "relation", "missing-manifest", "marker", "proof"])
def test_comparison_context_tampering_never_retains_a_pass(tmp_path, field):
    root, client, before = fixture(tmp_path)
    comparison = ModelComparison("Bound", before, "subject.rs::allowed", (0, 1, 2))
    comparison.execute(client, comparison.review(client))
    prove(root, client, "Bound")
    manifest = root/"specs/FrSpecs/Bound.refinement.json"
    module = root/"specs/FrSpecs/Bound.lean"
    data = json.loads(manifest.read_text())
    if field == "source":
        data["request"]["before"]["source"] = OLD.replace("a && !b", "a || b")
    elif field == "mapping":
        data["request"]["arguments"] = [1, 0, 2]
    elif field == "relation":
        data["request"]["relation"] = "refines"
    elif field == "model":
        module.write_text(module.read_text().replace("namespace Before", "namespace Modified"))
    elif field == "marker":
        module.write_text("\n".join(module.read_text().splitlines()[1:])+"\n")
    elif field == "proof":
        module.write_text(module.read_text().replace("  decide", "  sorry"))
    if field == "missing-manifest":
        manifest.unlink()
    else:
        manifest.write_text(json.dumps(data))
    if field == "proof":
        assert not ProofReport.review(client).execute(client).passed
    else:
        with pytest.raises(FrRuntimeError):
            ProofReport.review(client).execute(client)


def test_snapshot_rederivation_rejects_forged_model_even_with_new_digest(tmp_path):
    root, client, before = fixture(tmp_path)
    data = before.report.to_data()
    data["model"]["kernel"]["lean_definition"] = "def forged : Bool := true"
    data["digest"] = _digest([data[k] for k in ("schema", "target", "source", "model")])
    forged = ModelSnapshot(FrReport(data, ()))
    with pytest.raises(FrRuntimeError, match="differs from retained source"):
        ModelComparison("Forged", forged, "subject.rs::allowed", (0, 1, 2)).review(client)


def test_stale_review_and_existing_claim_refuse_before_writes(tmp_path):
    root, client, before = fixture(tmp_path)
    comparison = ModelComparison("Fresh", before, "subject.rs::allowed", (0, 1, 2))
    review = comparison.review(client)
    (root/"subject.rs").write_text(OLD.replace("a && !b", "a || b"))
    with pytest.raises(FrRuntimeError, match="review changed"):
        comparison.execute(client, review)
    assert not (root/"specs/FrSpecs/Fresh.lean").exists()
    comparison.execute(client, comparison.review(client))
    with pytest.raises(FrRuntimeError, match="already exist"):
        comparison.review(client)


def test_mapping_bounds_and_transport_refusals(tmp_path):
    root, client, before = fixture(tmp_path)
    for arguments in [(0,), (0, 1, 3)]:
        with pytest.raises(FrRuntimeError, match="argument mapping"):
            ModelComparison("Bounds", before, "subject.rs::allowed", arguments).review(client)
    for arguments in [(True,), (-1,), (8,)]:
        with pytest.raises(FrRuntimeError):
            ModelComparison("Bounds", before, "subject.rs::allowed", arguments)
    data = ModelComparison("Bounds", before, "subject.rs::allowed", (0, 1, 2)).to_data()
    data["extra"] = True
    request = root/"request.json"
    request.write_text(json.dumps(data))
    with pytest.raises(FrRuntimeError, match="unknown field"):
        client.call("spec", "compare", "--from", str(request))
    del data["extra"]
    request.write_text(json.dumps(data))
    with pytest.raises(FrRuntimeError, match="review basis"):
        client.call("spec", "compare", "--from", str(request), "--write")
    (root/"link.rs").symlink_to(root/"subject.rs")
    with pytest.raises(FrRuntimeError, match="symlinks"):
        ModelSnapshot.capture(client, "link.rs::allowed")


@pytest.mark.parametrize("count", [0, 8])
def test_empty_and_full_boolean_domains_prove(tmp_path, count):
    parameters = ", ".join(f"a{i}: bool" for i in range(count))
    body = " && ".join(f"a{i}" for i in range(count)) or "true"
    root, client, before = fixture(tmp_path, f"pub fn allowed({parameters}) -> bool {{ {body} }}\n")
    comparison = ModelComparison("Domain", before, "subject.rs::allowed", tuple(range(count)))
    comparison.execute(client, comparison.review(client))
    prove(root, client, "Domain")
    report = ProofReport.review(client).execute(client)
    assert report.passed
    evidence = report.report.at("/evidence/model_comparisons")
    assert len(evidence) == 1 and evidence[0]["before_digest"] == before.report.at("/digest")
    assert evidence[0]["arguments"] == list(range(count))
    assert evidence[0]["relation"] == "equivalent" and evidence[0]["status"] == "checked_by_lean"
    assert evidence[0]["source_implementation_proved"] is False


def test_capture_and_request_byte_limits(tmp_path):
    root, client, before = fixture(tmp_path)
    (root/"large.rs").write_text(OLD+" "*65536)
    with pytest.raises(FrRuntimeError, match="65536 bytes"):
        ModelSnapshot.capture(client, "large.rs::allowed")
    request = root/"large.json"
    request.write_text(" "*262145)
    with pytest.raises(FrRuntimeError, match="262144 bytes"):
        client.call("spec", "compare", "--from", str(request))


def test_existing_formalization_retains_unused_boolean_parameters(tmp_path):
    root, client, before = fixture(tmp_path)
    plan = client.call("spec", "plan", "subject.rs::allowed", "--property", "ir-model").to_data()
    path = root/"formal-plan.json"
    path.write_text(json.dumps(plan))
    client.call("spec", "scaffold", "--from", str(path), "--write")
    target = f"specs/FrSpecs/{plan['kernel']['module']}.lean::{plan['properties'][0]['name']}"
    tactics = root/"tactics.txt"
    tactics.write_text("cases a <;> cases b <;> cases unused <;> rfl\n")
    checked = client.call("spec", "proof-check", target, "--from", str(tactics))
    assert checked.at("/passed") is True
    client.call("spec", "prove", target, "--from", str(tactics), "--write")
    report = ProofReport.review(client).execute(client)
    assert report.passed and report.report.at("/source_implementation_proved") is False
    assert len(report.report.at("/evidence/kernel_correspondence")) == 1


def test_comparison_saved_plan_reverses_its_generated_files(tmp_path):
    root, client, before = fixture(tmp_path)
    request = root/"request.json"
    comparison = ModelComparison("Saved", before, "subject.rs::allowed", (0, 1, 2))
    request.write_text(json.dumps(comparison.to_data()))
    review = comparison.review(client)
    saved = client.call("spec", "compare", "--from", str(request), "--save-plan", "--basis", review.at("/basis"))
    assert saved.at("/saved") is True and saved.at("/applied") is False
    module = root/"specs/FrSpecs/Saved.lean"
    manifest = root/"specs/FrSpecs/Saved.refinement.json"
    assert not module.exists() and not manifest.exists()
    transaction = str(saved.at("/transaction"))
    client.call("history", "apply", transaction, "--write")
    assert module.is_file() and manifest.is_file()
    client.call("history", "undo", transaction, "--write")
    assert not module.exists() and not manifest.exists()
    client.call("history", "redo", transaction, "--write")
    assert module.is_file() and manifest.is_file()
