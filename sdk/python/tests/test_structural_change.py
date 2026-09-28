import json
from dataclasses import replace
from pathlib import Path
import subprocess
import sys

import pytest

from fr_ir.change_scope import ChangeScope
from fr_ir.context import DirectoryObjectStore
from fr_ir.ir import IrError, TaskChange, TaskDelivery, TaskTarget
from fr_ir.runtime import FrClient, FrRuntimeError
from fr_ir.structural_change import RefactorRequest

FR = Path(__file__).resolve().parents[3]/"target/debug/fr"


def fixture(tmp_path):
    root = tmp_path/"project"
    root.mkdir()
    (root/".fr").mkdir()
    (root/"artifacts").mkdir()
    sources = {
        "pricing.rs": "pub fn subtotal(a: i64, b: i64, _legacy: i64) -> i64 { a + b }\n",
        "api.rs": "pub fn quote() -> i64 { crate::pricing::subtotal(3, 2, 0) }\n",
        "main.rs": "mod pricing; mod api; fn main() { println!(\"{}\", api::quote()); }\n",
    }
    for name, source in sources.items():
        (root/name).write_text(source)
    checker = tmp_path/"check.py"
    checker.write_text("import subprocess,tempfile,pathlib\nwith tempfile.TemporaryDirectory() as d:\n"
        " p=pathlib.Path(d)/'probe'\n subprocess.run(['rustc','--edition=2021','main.rs','-o',str(p)],check=True)\n"
        " assert subprocess.check_output([str(p)],text=True).strip()=='5'\n")
    (root/".fr/checks.json").write_text(json.dumps({"schema": 1, "checks": [
        {"name": "behavior", "argv": [sys.executable, str(checker)], "cwd": ".",
         "timeout_seconds": 30, "covers": ["compiled public behavior"]}]}))
    (root/".fr/check-scopes.json").write_text(json.dumps({"schema": 1, "checks": [
        {"name": "behavior", "paths": list(sources)}]}))
    return root, FrClient(root, executable=FR)


def proposal(client, refactor, name="subtotal"):
    handle = client.project("find", name).definition_target().handle
    scope = ChangeScope.inspect(client, [handle])
    assert scope.ready, scope.report.to_data()
    change = TaskChange([], [refactor.target("migration", handle)],
        {"files-changed": 2, "paths-changed": ["pricing.rs", "api.rs"]}, ["behavior"],
        TaskDelivery(patch="artifacts/migration.patch"), acceptance_checks=["behavior"])
    return scope, scope.bind(change)


@pytest.mark.parametrize("refactor", [RefactorRequest.rename("sum_values"),
    RefactorRequest.remove_parameter(2), RefactorRequest.move_parameter(0, 1)])
def test_structural_delivery_compiles_reverses_and_replays(tmp_path, refactor):
    root, client = fixture(tmp_path)
    original = {p.name: p.read_bytes() for p in root.glob("*.rs")}
    scope, change = proposal(client, refactor)
    store = DirectoryObjectStore(tmp_path/"objects")
    restored = ChangeScope.restore(store, scope.persist(store))
    review = client.review(restored.bind(change))
    result = client.execute(review)
    assert result.at("/passed") is True
    patch = root/"artifacts/migration.patch"
    assert patch.is_file()
    subprocess.run(["git", "apply", "--reverse", str(patch)], cwd=root, check=True)
    assert {p.name: p.read_bytes() for p in root.glob("*.rs")} == original
    subprocess.run(["git", "apply", str(patch)], cwd=root, check=True)
    subprocess.run([sys.executable, str(tmp_path/"check.py")], cwd=root, check=True)
    if refactor.operation == "rename":
        assert "subtotal" not in (root/"api.rs").read_text()
    with pytest.raises(FrRuntimeError):
        client.review(change)


def test_structural_requires_binding_and_refuses_collision_without_writes(tmp_path):
    root, client = fixture(tmp_path)
    _, change = proposal(client, RefactorRequest.rename("sum_values"))
    with pytest.raises(FrRuntimeError, match="consumer scope"):
        client.review(replace(change, change_scope=None))
    path = root/"pricing.rs"
    path.write_text(path.read_text()+"pub fn sum_values() {}\n")
    original = path.read_bytes()
    _, change = proposal(client, RefactorRequest.rename("sum_values"))
    with pytest.raises(FrRuntimeError, match="already|collision"):
        client.review(change)
    assert path.read_bytes() == original


@pytest.mark.parametrize("arguments", ["3, 2, effect()", "3, 2, { 0 }", "3, 2, value", "3, 2, 1 + 2"])
def test_signature_refuses_nonliteral_arguments(tmp_path, arguments):
    root, client = fixture(tmp_path)
    path = root/"api.rs"
    path.write_text("fn effect() -> i64 { 0 }\npub fn quote() -> i64 { let value = 0; "
                    f"crate::pricing::subtotal({arguments}) }}\n")
    _, change = proposal(client, RefactorRequest.remove_parameter(2))
    before = path.read_bytes()
    with pytest.raises(FrRuntimeError, match="literal scalar"):
        client.review(change)
    assert path.read_bytes() == before


def test_signature_refuses_used_parameter_and_non_scalar_type(tmp_path):
    root, client = fixture(tmp_path)
    _, change = proposal(client, RefactorRequest.remove_parameter(0))
    with pytest.raises(FrRuntimeError, match="still reads"):
        client.review(change)
    path = root/"pricing.rs"
    path.write_text(path.read_text().replace("_legacy: i64", "_legacy: String"))
    _, change = proposal(client, RefactorRequest.remove_parameter(2))
    with pytest.raises(FrRuntimeError, match="primitive scalar"):
        client.review(change)


def test_added_consumer_and_mapping_drift_refuse_old_review(tmp_path):
    root, client = fixture(tmp_path)
    _, change = proposal(client, RefactorRequest.rename("sum_values"))
    review = client.review(change)
    (root/"new.rs").write_text("fn added() -> i64 { crate::pricing::subtotal(1, 2, 0) }\n")
    with pytest.raises(FrRuntimeError):
        client.execute(review)
    assert "subtotal" in (root/"pricing.rs").read_text()


def test_overlapping_structural_and_body_edits_refuse_in_both_orders(tmp_path):
    root, client = fixture(tmp_path)
    _, change = proposal(client, RefactorRequest.rename("sum_values"))
    handle = client.project("find", "quote").definition_target().handle
    body = TaskTarget("body", handle, "replace-body", fragment="{ crate::pricing::subtotal(1, 4, 0) }")
    for targets in [(change.targets[0], body), (body, change.targets[0])]:
        with pytest.raises(FrRuntimeError, match="overlap"):
            client.review(replace(change, targets=targets))
    assert "subtotal" in (root/"api.rs").read_text()


@pytest.mark.parametrize("args", [("rename", ""), ("rename", "x", 1), ("remove-parameter", True),
    ("remove-parameter", 64), ("remove-parameter", 0, 1), ("move-parameter", 0, 0),
    ("move-parameter", 0, True), ("unknown", 0)])
def test_typed_refactors_reject_malformed_fields(args):
    with pytest.raises(IrError):
        RefactorRequest(*args)


def test_native_task_limit_matches_sdk_and_author_batches(tmp_path):
    root, client = fixture(tmp_path)
    (root/"many.rs").write_text("\n".join(f"pub fn f{i}() -> i64 {{ 0 }}" for i in range(32)))
    selection = client.project("select", *(f"f{i}" for i in range(32)), "--limit", "32")
    data = selection.to_data()
    rows = [dict(zip(data["columns"], row)) for row in data["rows"]]
    assert len(rows) == 32
    targets = [TaskTarget(row["name"], row["handle"], "replace-body", fragment="{ 1 }") for row in rows]
    change = TaskChange([], targets, {"files-changed": 1, "edits": 32}, ["behavior"])
    result = client.execute(client.review(change))
    assert result.passed and (root/"many.rs").read_text().count("{ 1 }") == 32
    with pytest.raises(IrError, match="32 targets"):
        replace(change, targets=[*targets, TaskTarget("extra", rows[0]["handle"], "replace-body", fragment="{ 2 }")])


def test_parameter_movement_preserves_noncommutative_results(tmp_path):
    root, client = fixture(tmp_path)
    path = root/"pricing.rs"
    path.write_text(path.read_text().replace("a + b", "a * 10 + b"))
    checker = tmp_path/"check.py"
    checker.write_text(checker.read_text().replace("=='5'", "=='32'"))
    _, change = proposal(client, RefactorRequest.move_parameter(0, 1))
    assert client.execute(client.review(change)).passed
    assert "subtotal(2, 3, 0)" in (root/"api.rs").read_text()


def test_function_value_and_unresolved_consumer_refuse_signature_delivery(tmp_path):
    root, client = fixture(tmp_path)
    path = root/"api.rs"
    path.write_text("pub fn quote() -> i64 { let f = crate::pricing::subtotal; f(3, 2, 0) }\n")
    _, change = proposal(client, RefactorRequest.remove_parameter(2))
    with pytest.raises(FrRuntimeError, match="value"):
        client.review(change)
    path.write_text("pub fn quote() -> i64 { missing::subtotal(3, 2, 0) }\n")
    handle = client.project("find", "subtotal").definition_target().handle
    scope = ChangeScope.inspect(client, [handle])
    assert not scope.ready and scope.report.at("/unresolved")


def test_native_request_rejects_unknown_fields_and_clipped_reviews(tmp_path):
    root, client = fixture(tmp_path)
    _, change = proposal(client, RefactorRequest.rename("sum_values"))
    target = replace(change.targets[0], fragment='{"operation":"rename","name":"sum_values","unexpected":true}')
    with pytest.raises(FrRuntimeError, match="unknown field"):
        client.review(replace(change, targets=[target]))
    manifest = root/".fr/migration.json"
    change.write(manifest)
    result = subprocess.run([str(FR), "--json", "-C", str(root), "task-change", "--from", str(manifest),
                             "--diff-bytes", "1"], capture_output=True, text=True)
    assert result.returncode != 0 or json.loads(result.stdout).get("task_change_basis") is None
    assert "subtotal" in (root/"pricing.rs").read_text()
