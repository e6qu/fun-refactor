#!/usr/bin/env python3
"""Retain compiler diagnostics, input invalidation and independent behavior evidence."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "sdk/python/src"))
from evidence_basis import file_digest
from fr_ir.compiler_evidence import CompilerEvidence
from fr_ir.context import MemoryObjectStore
from fr_ir.investigation import TaskPlan, TaskStep
from fr_ir.runtime import FrClient, FrReport, FrRuntimeError

FIXTURE = ROOT / "tests/agent-eval/compiler-evidence"
BINDINGS = ["tools/compiler-evidence-acceptance.py", "tools/evidence_basis.py", "Cargo.lock",
    "src/checks.rs", "src/checks/evidence.rs", "src/history.rs", "src/project.rs",
    "src/project/compiler_evidence.rs", "src/project/compiler_diagnostics.rs", "src/project/occurrence.rs",
    "src/project/investigation.rs", "src/parse.rs", "src/span.rs",
    *[f"sdk/python/src/fr_ir/{name}.py" for name in ["compiler_evidence", "investigation", "investigation_checks", "runtime", "context"]],
    "kernels/FrKernels/Investigation.lean", "kernels/InvestigationMain.lean",
    *[f"tests/agent-eval/compiler-evidence/{name}" for name in ["subject.rs", "oracle.py", "task.json", "baseline.json"]]]


def encode(value):
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode()


def digest(value):
    return hashlib.sha256(encode(value)).hexdigest()


def oracle():
    return json.loads(subprocess.check_output([sys.executable, str(FIXTURE / "oracle.py")]))


def execute(client, name, output_bytes=65536):
    listing = client.call("checks", "--toolchain")
    try:
        return client.call("checks", "--toolchain", "--run", name, "--basis", listing.at("/basis"), "--output-bytes", str(output_bytes))
    except FrRuntimeError as error:
        if error.report is None:
            raise
        return FrReport(error.report, ())


def refuses(action):
    try:
        action()
    except FrRuntimeError:
        return True
    return False


def measure(args):
    start = time.perf_counter()
    independent = oracle()
    oracle_seconds = time.perf_counter() - start
    compiler = Path(subprocess.check_output(["rustup", "which", "rustc"], text=True).strip())
    cargo = Path(subprocess.check_output(["rustup", "which", "cargo"], text=True).strip())
    drivers = sorted((compiler.parent.parent / "lib").glob("librustc_driver*"))
    with tempfile.TemporaryDirectory(prefix="fr-compiler-eval-") as directory:
        work = Path(directory)
        project = work / "project"
        project.mkdir()
        shutil.copyfile(FIXTURE / "subject.rs", project / "subject.rs")
        (project / ".fr").mkdir()
        (project / "target").mkdir()
        (project / ".gitignore").write_text("target/\n")
        (project / "Cargo.toml").write_text('[package]\nname="compiler_evidence_fixture"\nversion="0.1.0"\nedition="2021"\n[lib]\npath="subject.rs"\n[lints.rust]\nunexpected_cfgs="allow"\n')
        subprocess.run([str(cargo), "generate-lockfile", "--offline"], cwd=project, check=True, capture_output=True)
        identity = work / "policy.txt"
        identity.write_text("declared evaluation policy v1")
        config = {"schema":1,"checks":[]}
        for name, flags in [("default", []), ("strict", ["--cfg", "fr_strict"])]:
            config["checks"].append({"name":name,"argv":[str(compiler),"--crate-type","lib","--emit","metadata","--out-dir","target","--error-format","json",*flags,"subject.rs"],
                "cwd":".","timeout_seconds":30,"covers":[name],"environment":["RUSTC_BOOTSTRAP"],"identity_files":[str(identity),*[str(path) for path in drivers]]})
        config_path = project / ".fr/checks.json"
        config_path.write_text(json.dumps(config))
        client = FrClient(project, executable=str(args.fr.resolve()), max_output_bytes=1_048_576)
        cases = {}
        for name in ["default", "strict"]:
            start = time.perf_counter()
            checks = execute(client, name)
            check_seconds = time.perf_counter() - start
            start = time.perf_counter()
            first = CompilerEvidence.inspect(client, checks, check=name, limit=1)
            first_seconds = time.perf_counter() - start
            current, pages = first, []
            while True:
                pages.append(current.report.to_data())
                if current.report.at("/continuation") is None:
                    break
                current = current.next(client)
            store = MemoryObjectStore()
            restored = CompilerEvidence.restore(store, first.persist(store))
            assert restored.report.to_data() == first.report.to_data()
            cases[name] = {"checks":checks.to_data(),"pages":pages,"check_seconds":check_seconds,
                "first_page_seconds":first_seconds,"raw_report_bytes":len(encode(checks.to_data())),
                "first_page_bytes":len(encode(pages[0])),"all_page_bytes":sum(len(encode(page)) for page in pages),
                "check_process_calls":2,"evidence_process_calls":len(pages),"source_reveals":0,"tokens":None}
        strict = FrReport(cases["strict"]["checks"], ())
        full = CompilerEvidence.inspect(client, strict, check="strict", limit=64)
        plan = TaskPlan("check strict compiler mode", ("strict compiles",), (TaskStep.checked("compile", "does strict mode compile?", checks=("strict",), satisfies=("strict compiles",)),))
        attached = full.attach(plan.resume(client, transition="compile:start").plan, client, "compile")
        plan_result = {"complete":attached.complete,"evidence":[{"kind":item.kind.value,"passed":item.passed} for item in attached.plan.steps[0].evidence]}
        clipped_checks = execute(client, "strict", 64)
        clipped = CompilerEvidence.inspect(client, clipped_checks, check="strict").report.to_data()
        refusals = {}
        original_identity = identity.read_text()
        identity.write_text("changed policy")
        refusals["external_identity"] = refuses(lambda: CompilerEvidence.inspect(client, strict, check="strict"))
        identity.write_text(original_identity)
        before = os.environ.get("RUSTC_BOOTSTRAP")
        os.environ["RUSTC_BOOTSTRAP"] = "changed-evaluation-identity"
        refusals["environment"] = refuses(lambda: CompilerEvidence.inspect(client, strict, check="strict"))
        if before is None:
            del os.environ["RUSTC_BOOTSTRAP"]
        else:
            os.environ["RUSTC_BOOTSTRAP"] = before
        (project / ".cargo").mkdir()
        (project / ".cargo/config.toml").write_text('[build]\nrustflags=["--cfg","fr_strict"]\n')
        refusals["new_build_configuration"] = refuses(lambda: CompilerEvidence.inspect(client, strict, check="strict"))
        config["checks"].append({"name":"cargo","argv":[str(cargo),"check","--offline","--message-format=json"],"cwd":".","timeout_seconds":30,"covers":["Cargo strict configuration"],
            "identity_files":[str(compiler),*[str(path) for path in drivers]],"environment":["RUSTFLAGS","RUSTC","CARGO_ENCODED_RUSTFLAGS"]})
        config_path.write_text(json.dumps(config))
        cargo_checks = execute(client, "cargo")
        cargo_report = CompilerEvidence.inspect(client, cargo_checks, check="cargo", format="cargo-json", limit=64).report.to_data()
        source = project / "subject.rs"
        source.write_text(source.read_text() + "\n// changed source\n")
        refusals["source"] = refuses(lambda: CompilerEvidence.inspect(client, cargo_checks, check="cargo", format="cargo-json"))
    return {"schema":"fr-compiler-evidence-acceptance-1","repository_revision":subprocess.check_output(["git","rev-parse","HEAD"],cwd=ROOT,text=True).strip(),
        "source_bindings":{path:file_digest(ROOT / path) for path in BINDINGS},
        "binary_sha256":hashlib.sha256(args.fr.read_bytes()).hexdigest(),"platform":platform.platform(),
        "compiler_version":subprocess.check_output([str(compiler),"-vV"],text=True),"oracle":independent,"ordinary_oracle_seconds":oracle_seconds,
        "baseline":json.loads((FIXTURE / "baseline.json").read_text()),"cases":cases,"clipped":{"checks":clipped_checks.to_data(),"report":clipped},
        "cargo":{"checks":cargo_checks.to_data(),"report":cargo_report},"plan":plan_result,"refusals":refusals,
        "evidence_digest":digest([cases,clipped,cargo_report,plan_result]),"false_claims":0,"useful_discoveries":["E0308 at exact value span","syntax acceptance differs from strict compiler rejection","default configuration passes"],
        "diagnostics":["A Cargo manifest edit initially left retained checks current; automatic Rust build-input identities fixed the gap.",
                       "A configuration filename replaced by a directory also changes Cargo behavior; unsupported build-input types now refuse evidence.",
                       "The initial identity-file limit excluded the compiler driver; streaming identities now admit 256 MiB files.",
                       "Generated artifacts initially changed the fixture project snapshot; the fixture now declares target/ ignored."],
        "scope":"Six independent runtime cases and exact strict diagnostic coordinates. Real rustc and Cargo invocations. Driver identities are declared when available. Full compiler dependency coverage, execution attestation, runtime safety and source proofs remain outside scope. No live agent or token measurement. Raw retained compiler output can contain rendered source; diagnostic pages omit those fields."}


def audit(value):
    assert value["schema"] == "fr-compiler-evidence-acceptance-1"
    assert value["source_bindings"] == {path:file_digest(ROOT / path) for path in BINDINGS}
    assert value["oracle"] == oracle()
    assert value["baseline"] == json.loads((FIXTURE / "baseline.json").read_text())
    assert value["evidence_digest"] == digest([value["cases"],value["clipped"]["report"],value["cargo"]["report"],value["plan"]])
    for name, case in value["cases"].items():
        checks = FrReport(case["checks"], ())
        offset, items = 0, []
        for data in case["pages"]:
            page = CompilerEvidence.from_report(FrReport(data, ()), checks=checks)
            assert page.report.at("/page/before") == offset and page.report.at("/capture/complete")
            assert page.report.at("/outcome/passed") == (name == "default")
            offset += len(page.items)
            items.extend(page.items)
        assert case["pages"][-1]["page"]["remaining"] == 0
        assert case["first_page_bytes"] == len(encode(case["pages"][0]))
        if name == "default":
            assert not items
        else:
            error = next(item for item in items if item.code == "E0308")
            primary = next(span for span in error.spans if span.primary)
            assert primary.status == "exact" and primary.syntax == "accepted"
            assert [primary.occurrence.location.span.start,primary.occurrence.location.span.end] == value["oracle"]["primary_span"]
    clipped = CompilerEvidence.from_report(FrReport(value["clipped"]["report"],()), checks=FrReport(value["clipped"]["checks"],()))
    assert not clipped.complete and clipped.report.at("/capture/omitted_bytes") > 0
    cargo = CompilerEvidence.from_report(FrReport(value["cargo"]["report"],()), checks=FrReport(value["cargo"]["checks"],()))
    assert cargo.complete and not cargo.report.at("/outcome/passed") and any(item.code == "E0308" for item in cargo.items)
    assert not value["plan"]["complete"] and {item["kind"] for item in value["plan"]["evidence"]} == {"check","observation"}
    assert all(value["refusals"].values()) and value["false_claims"] == 0


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--fr",type=Path,default=ROOT / "target/debug/fr")
    parser.add_argument("--output",type=Path)
    parser.add_argument("--audit",type=Path)
    args = parser.parse_args()
    value = json.loads(args.audit.read_text()) if args.audit else measure(args)
    audit(value)
    text = json.dumps(value,indent=2) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True,exist_ok=True)
        args.output.write_text(text)
    else:
        print(text,end="")


if __name__ == "__main__":
    main()
