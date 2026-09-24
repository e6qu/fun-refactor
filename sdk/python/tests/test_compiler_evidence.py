import copy
import hashlib
import json
from pathlib import Path
import subprocess
import sys

import pytest

from fr_ir.compiler_evidence import CompilerEvidence
from fr_ir.context import MemoryObjectStore
from fr_ir.investigation import TaskPlan, TaskStep
from fr_ir.runtime import FrClient, FrReport, FrRuntimeError

ROOT = Path(__file__).resolve().parents[3]
FIXTURE = ROOT / "tests/agent-eval/compiler-evidence"


def workspace(tmp_path, monkeypatch):
    root = tmp_path / "project"
    root.mkdir()
    (root / "subject.rs").write_bytes((FIXTURE / "subject.rs").read_bytes())
    (root / ".fr").mkdir()
    (root / "target").mkdir()
    (root / ".gitignore").write_text("target/\n")
    identity = tmp_path / "compiler-policy.txt"
    identity.write_text("policy version 1")
    monkeypatch.setenv("FR_COMPILER_TEST_SCOPE", "private-value-never-disclosed")
    compiler = subprocess.check_output(["rustup", "which", "rustc"], text=True).strip()
    config = {"schema":1,"checks":[{"name":name,"argv":[compiler,"--crate-type","lib","--emit","metadata","--out-dir","target",
        "--error-format","json",*flags,"subject.rs"],"cwd":".","timeout_seconds":30,"covers":[name],
        "identity_files":[str(identity)],"environment":["FR_COMPILER_TEST_SCOPE"]}
        for name,flags in [("default",[]),("strict",["--cfg","fr_strict"])]]}
    (root / ".fr/checks.json").write_text(json.dumps(config))
    client = FrClient(root, executable=str(ROOT / "target/debug/fr"), max_output_bytes=1_048_576)
    return client, identity


def execute(client, name="strict", output_bytes=65536):
    listing = client.call("checks", "--toolchain")
    try:
        return client.call("checks", "--toolchain", "--run", name, "--basis", listing.at("/basis"), "--output-bytes", str(output_bytes))
    except FrRuntimeError as error:
        assert error.report is not None
        return FrReport(error.report, ())


def test_real_compiler_disagreement_matches_independent_runtime_and_coordinate_oracle(tmp_path, monkeypatch):
    client, _ = workspace(tmp_path, monkeypatch)
    checks = execute(client)
    first = CompilerEvidence.inspect(client, checks, check="strict", limit=1)
    assert not first.complete and first.report.at("/capture/complete")
    assert not first.report.at("/outcome/passed")
    assert first.report.at("/disagreements/0/kind") == "syntax-accepted-compiler-error"
    collection = first.collect(client)
    assert collection.complete and collection.pages > 1
    oracle = json.loads(subprocess.check_output([sys.executable, str(FIXTURE / "oracle.py")]))
    diagnostic = next(item for item in collection.items if item.code == oracle["code"])
    primary = next(span for span in diagnostic.spans if span.primary)
    assert primary.syntax == "accepted" and primary.status == "exact"
    assert [primary.occurrence.location.span.start, primary.occurrence.location.span.end] == oracle["primary_span"]
    assert primary.reveal(client).at("/revision") == first.report.at("/revision")
    assert "private-value-never-disclosed" not in json.dumps(first.report.to_data())
    passed = CompilerEvidence.inspect(client, execute(client, "default"), check="default")
    assert passed.complete and passed.report.at("/outcome/passed") and not passed.items
    assert not first.collect(client, max_pages=1).complete
    assert not first.next(client).collect(client).disclosure_complete


@pytest.mark.parametrize("drift", ["source", "configuration", "identity_file", "environment", "unset_environment", "executable", "new_cargo_configuration", "new_toolchain_file"])
def test_retained_compiler_evidence_rejects_input_drift(tmp_path, monkeypatch, drift):
    client, identity = workspace(tmp_path, monkeypatch)
    if drift == "executable":
        config_path = client.root / ".fr/checks.json"
        config = json.loads(config_path.read_text())
        wrapper = tmp_path / "rustc-wrapper"
        wrapper.write_text(f'#!/bin/sh\nexec "{config["checks"][0]["argv"][0]}" "$@"\n')
        wrapper.chmod(0o755)
        for check in config["checks"]:
            check["argv"][0] = str(wrapper)
        config_path.write_text(json.dumps(config))
    checks = execute(client)
    first = CompilerEvidence.inspect(client, checks, check="strict", limit=1)
    if drift == "source":
        path = client.root / "subject.rs"
        path.write_text(path.read_text() + "\n// changed\n")
    elif drift == "configuration":
        path = client.root / ".fr/checks.json"
        path.write_text(path.read_text() + "\n")
    elif drift == "identity_file":
        identity.write_text("policy version 2")
    elif drift == "environment":
        monkeypatch.setenv("FR_COMPILER_TEST_SCOPE", "changed")
    elif drift == "unset_environment":
        monkeypatch.delenv("FR_COMPILER_TEST_SCOPE")
    elif drift == "new_cargo_configuration":
        (client.root / ".cargo").mkdir()
        (client.root / ".cargo/config.toml").write_text('[build]\nrustflags=[]\n')
    elif drift == "new_toolchain_file":
        (client.root / "rust-toolchain.toml").write_text('[toolchain]\nchannel="stable"\n')
    else:
        wrapper.write_text(wrapper.read_text() + "# changed\n")
    with pytest.raises(FrRuntimeError):
        first.next(client)


def test_truncated_capture_never_claims_complete_diagnostics(tmp_path, monkeypatch):
    client, _ = workspace(tmp_path, monkeypatch)
    page = CompilerEvidence.inspect(client, execute(client, output_bytes=64), check="strict")
    assert not page.complete and page.report.at("/capture/omitted_bytes") > 0
    assert "compiler-stream-truncated" in page.report.at("/capture/cutoffs")


def altered_report(checks, text, *, cargo=False):
    data = checks.to_data()
    stream = data["results"][0]["stdout" if cargo else "stderr"]
    stream.update(text=text, retained_bytes=len(text.encode()), omitted_bytes=0)
    return FrReport(data, ())


def test_cargo_envelopes_parent_diagnostics_and_unknown_protocol(tmp_path, monkeypatch):
    client, _ = workspace(tmp_path, monkeypatch)
    checks = execute(client)
    original = json.loads(checks.at("/results/0/stderr/text").splitlines()[0])
    original["children"] = [{"message":"declared child note", "level":"note", "spans":[], "children":[], "code":None}]
    text = json.dumps({"reason":"compiler-message","message":original}) + '\n' + json.dumps({"reason":"build-finished","success":False}) + '\n'
    page = CompilerEvidence.inspect(client, altered_report(checks, text, cargo=True), check="strict", format="cargo-json", limit=64)
    assert page.complete and page.items[0].code == "E0308"
    assert any(item.parent == 0 for item in page.items)
    missing = CompilerEvidence.inspect(client, altered_report(checks, text.splitlines()[0], cargo=True), check="strict", format="cargo-json")
    assert not missing.complete and "missing-build-outcome" in missing.report.at("/capture/cutoffs")
    for bad in ['{"reason":"new-protocol-event"}\n', 'not json\n']:
        page = CompilerEvidence.inspect(client, altered_report(checks, text + bad, cargo=True), check="strict", format="cargo-json")
        assert not page.complete


def test_real_cargo_diagnostics_bind_manifest_and_compiler_environment(tmp_path, monkeypatch):
    client, _ = workspace(tmp_path, monkeypatch)
    (client.root / "Cargo.toml").write_text('[package]\nname="compiler_evidence_fixture"\nversion="0.1.0"\nedition="2021"\n[lib]\npath="subject.rs"\n[lints.rust]\nunexpected_cfgs="allow"\n')
    cargo = subprocess.check_output(["rustup", "which", "cargo"], text=True).strip()
    compiler = subprocess.check_output(["rustup", "which", "rustc"], text=True).strip()
    monkeypatch.setenv("RUSTC", compiler)
    monkeypatch.setenv("RUSTFLAGS", "--cfg fr_strict")
    subprocess.run([cargo, "generate-lockfile", "--offline"], cwd=client.root, check=True, capture_output=True)
    path = client.root / ".fr/checks.json"
    config = json.loads(path.read_text())
    config["checks"].append({"name":"cargo-strict","argv":[cargo,"check","--offline","--message-format=json"],
        "cwd":".","timeout_seconds":30,"covers":["Cargo strict configuration"],"identity_files":[compiler],"environment":["RUSTC","RUSTFLAGS"]})
    path.write_text(json.dumps(config))
    checks = execute(client, "cargo-strict")
    page = CompilerEvidence.inspect(client, checks, check="cargo-strict", format="cargo-json", limit=64)
    assert page.complete and page.report.at("/outcome/cargo_success") is False
    assert any(item.code == "E0308" for item in page.items)
    assert page.report.at("/disagreements")
    (client.root / "Cargo.toml").write_text((client.root / "Cargo.toml").read_text() + '\n[features]\nstrict=[]\n')
    with pytest.raises(FrRuntimeError):
        CompilerEvidence.inspect(client, checks, check="cargo-strict", format="cargo-json")


@pytest.mark.parametrize("kind", ["external", "invalid", "expanded", "missing", "unicode_boundary"])
def test_unmapped_compiler_spans_preserve_gaps(tmp_path, monkeypatch, kind):
    client, _ = workspace(tmp_path, monkeypatch)
    checks = execute(client)
    row = json.loads(checks.at("/results/0/stderr/text").splitlines()[0])
    row["children"] = []
    row["spans"] = [row["spans"][0]]
    span = row["spans"][0]
    expected = "unavailable"
    if kind == "external":
        span["file_name"] = "/external/subject.rs"
    elif kind == "invalid":
        span["byte_end"] = 999999
        expected = "invalid"
    elif kind == "expanded":
        span["expansion"] = {"macro_decl_name":"example!"}
        expected = "expanded"
    elif kind == "unicode_boundary":
        span["byte_start"] = (client.root / "subject.rs").read_bytes().index("é".encode()) + 1
        span["byte_end"] = span["byte_start"] + 1
        expected = "invalid"
    else:
        span["file_name"] = "missing.rs"
    page = CompilerEvidence.inspect(client, altered_report(checks, json.dumps(row)), check="strict")
    point = page.items[0].spans[0]
    assert page.complete and point.status == expected and point.occurrence is None
    assert point.syntax == "unknown"
    with pytest.raises(FrRuntimeError):
        point.reveal(client)


def test_verified_persistence_and_plan_attachment_keep_compiler_failure(tmp_path, monkeypatch):
    client, identity = workspace(tmp_path, monkeypatch)
    checks = execute(client)
    page = CompilerEvidence.inspect(client, checks, check="strict", limit=64)
    store = MemoryObjectStore()
    restored = CompilerEvidence.restore(store, page.persist(store))
    assert restored.report.to_data() == page.report.to_data()
    plan = TaskPlan("validate strict mode", ("strict compiles",), (TaskStep.checked("compile", "does strict mode compile?", checks=("strict",), satisfies=("strict compiles",)),))
    started = plan.resume(client, transition="compile:start")
    attached = restored.attach(started.plan, client, "compile")
    assert not attached.complete
    assert any(item.kind.value == "check" and not item.passed for item in attached.plan.steps[0].evidence)
    assert any(item.id == "compiler:strict" and item.passed for item in attached.plan.steps[0].evidence)
    identity.write_text("changed")
    assert attached.plan.resume(client).invalidated == ("compile",)
    with pytest.raises(FrRuntimeError):
        restored.attach(started.plan, client, "compile")


def test_diagnostic_byte_budget_keeps_every_row_reachable(tmp_path, monkeypatch):
    client, _ = workspace(tmp_path, monkeypatch)
    checks = execute(client)
    row = json.loads(checks.at("/results/0/stderr/text").splitlines()[0])
    row["children"] = []
    report = altered_report(checks, '\n'.join(json.dumps(row) for _ in range(12)))
    page = CompilerEvidence.inspect(client, report, check="strict", limit=64, max_bytes=8192)
    assert len(json.dumps(page.report.to_data(),ensure_ascii=False,separators=(",",":")).encode()) <= 8192
    assert not page.complete and page.report.at("/page/remaining") > 0
    collection = page.collect(client)
    assert collection.complete and len(collection.items) == 12 and collection.pages > 1
    assert len({item.id for item in collection.items}) == 12


def test_compiler_persistence_detects_corrupt_objects(tmp_path, monkeypatch):
    client, _ = workspace(tmp_path, monkeypatch)
    page = CompilerEvidence.inspect(client, execute(client), check="strict")
    store = MemoryObjectStore()
    digest = page.persist(store)

    class Corrupt:
        def get(self, key):
            return {"kind":"scalar","value":"forged"} if key == digest else store.get(key)
        def put(self, key, value):
            store.put(key, value)

    with pytest.raises((FrRuntimeError, ValueError)):
        CompilerEvidence.restore(Corrupt(), digest)


@pytest.mark.parametrize("field", ["identity", "coverage", "outcome", "disagreement", "occurrence", "parent"])
def test_sdk_rejects_tampered_compiler_evidence(tmp_path, monkeypatch, field):
    client, _ = workspace(tmp_path, monkeypatch)
    checks = execute(client)
    page = CompilerEvidence.inspect(client, checks, check="strict")
    value = copy.deepcopy(page.report.to_data())
    if field == "identity":
        value["inputs"]["check"] = "other"
    elif field == "coverage":
        value["page"]["total"] += 1
    elif field == "outcome":
        value["outcome"]["passed"] = True
    elif field == "disagreement":
        value["disagreements"] = []
    else:
        row = value["items"][0]
        if field == "parent":
            row["core"]["parent"] = 0
        else:
            row["core"]["spans"][0]["occurrence"]["location"]["span"]["end"] += 1
        row["id"] = "frcd1:" + hashlib.sha256(json.dumps([value["basis"],row["core"]],sort_keys=True,ensure_ascii=False,separators=(",",":")).encode()).hexdigest()
    with pytest.raises(FrRuntimeError):
        CompilerEvidence.from_report(FrReport(value, ()), checks=checks)
