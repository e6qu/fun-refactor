"""Explain real import gaps while preserving unknown answers and disclosure limits."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import re
import zipfile

import pytest

from fr_ir.flow_import_explanation import ImportExplanation, read_report
from fr_ir.runtime import FrReport, FrRuntimeError
from test_flow_imports import analyze, workspace

ROOT = Path(__file__).resolve().parents[3]
ARCHIVE = ROOT / "tests/agent-eval/python-symbolic/2026-10-17-frozen-dependencies.zip"


def archived_report(context="full-package"):
    with zipfile.ZipFile(ARCHIVE) as archive:
        return FrReport(json.loads(archive.read(f"numeric-results/{context}-trace/trace.stdout")), ())


def rehash(data):
    data["input_digest"] = hashlib.sha256(json.dumps(data["inputs"], sort_keys=True,
        ensure_ascii=False, separators=(",", ":")).encode()).hexdigest()
    return FrReport(data, ())


def test_pydash_explanation_distinguishes_unread_source_from_missing_bindings():
    report = archived_report()
    before = report.to_data()
    result = ImportExplanation.from_report(report)
    assert result.files_read == 8 and result.unchecked_statements == 420
    assert not result.import_check_complete and not result.analysis_complete
    assert any("256 KiB" in message for message in result.limit_explanations)
    unread = [gap for gap in result.gaps if gap.kind == "unread-source"]
    assert unread and all(gap.unread_paths for gap in unread)
    assert all("does not establish a missing name" in gap.explanation for gap in unread)
    assert result.unresolved_requests == sum(len(gap.names) for gap in result.gaps)
    assert report.to_data() == before
    text = result.render(max_bytes=2048)
    assert "420 import-time statements unchecked" in text and "unfinished" in text
    assert "256 KiB" in text and "workspace has changed" in text
    assert len(text.encode()) <= 2048


def test_isolated_success_is_only_a_claim_about_the_declared_model():
    result = ImportExplanation.from_report(archived_report("isolated-declaration"))
    assert result.import_check_complete and result.analysis_complete
    assert not result.gaps and not result.initialization.by_file
    text = result.render()
    assert "within its stated rules" in text and "not a proof of runtime behavior" in text


@pytest.mark.parametrize("max_bytes", [1024, 1536, 2048, 4096])
@pytest.mark.parametrize("max_items", [1, 3, 8, 32])
def test_bounded_disclosure_keeps_totals_and_omission_counts(max_bytes, max_items):
    result = ImportExplanation.from_report(archived_report())
    text = result.render(max_bytes=max_bytes, max_items=max_items)
    assert len(text.encode()) <= max_bytes
    assert "420 import-time statements unchecked" in text
    assert f"/{len(result.gaps)} import groups" in text
    assert f"/{len(result.initialization.by_file)} startup file summaries" in text
    assert f"{result.initialization.omitted} additional locations omitted" in text
    assert len([line for line in text.splitlines() if line.startswith(("Import in ", "During import, "))]) <= max_items


@pytest.mark.parametrize("option,value", [("max_bytes", 1023), ("max_bytes", 65537),
                                         ("max_bytes", True), ("max_items", 0),
                                         ("max_items", 33), ("max_items", True)])
def test_invalid_disclosure_limits_refuse(option, value):
    with pytest.raises(FrRuntimeError):
        ImportExplanation.from_report(archived_report()).render(**{option: value})


@pytest.mark.parametrize("mutation", ["old-alias-format", "false-complete", "missing-cutoffs",
                                     "false-startup-count", "false-digest", "nontext-cutoff", "unknown-module-reason",
                                     "unexpected-budget-detail", "missing-module-name"])
def test_malformed_reports_cannot_become_reassuring_explanations(mutation):
    value = archived_report().to_data()
    if mutation == "old-alias-format":
        del value["inputs"]["modules"]["value_aliases"]
    elif mutation == "false-complete":
        value["complete"] = True
    elif mutation == "missing-cutoffs":
        value["cutoffs"] = ["module-effects-unchecked"]
    elif mutation == "false-startup-count":
        value["initialization"]["by_file"]["pydash/numerical.py"]["statements"] = 1
    elif mutation == "false-digest":
        value["input_digest"] = "0" * 64
    elif mutation in {"unknown-module-reason", "unexpected-budget-detail", "missing-module-name"}:
        reason = {"unknown-module-reason": "unrecognized-module-condition",
                  "unexpected-budget-detail": "import-source-byte-budget:unknown",
                  "missing-module-name": "unresolved-local-import:"}[mutation]
        value["inputs"]["modules"]["cutoffs"].append(reason)
        value["cutoffs"] = sorted([*value["cutoffs"], reason])
    else:
        value["cutoffs"].append(17)
    report = FrReport(value, ()) if mutation == "false-digest" else rehash(value)
    with pytest.raises(FrRuntimeError):
        ImportExplanation.from_report(report)


@pytest.mark.parametrize("kind", ["missing", "aliased", "external", "unchecked-startup", "ordinary", "unknown-call"])
def test_fresh_analysis_explains_different_gaps_without_guessing_runtime(tmp_path, kind):
    client, rules = workspace(tmp_path)
    if kind in {"missing", "aliased"}:
        (tmp_path / "leaf.py").write_text("def other(value):\n    return value\n")
        if kind == "aliased":
            (tmp_path / "relay.py").write_text("from leaf import identity as forwarded\ndef forward(value):\n    return forwarded(value)\n")
    elif kind == "external":
        (tmp_path / "leaf.py").write_text("import math\ndef identity(value):\n    return value\n")
    elif kind == "unchecked-startup":
        (tmp_path / "leaf.py").write_text("metadata = opaque()\ndef identity(value):\n    return value\n")
    elif kind == "unknown-call":
        (tmp_path / "app.py").write_text("def positive():\n    return sink(opaque(source()))\n")
    result = ImportExplanation.from_report(analyze(client, rules).report)
    if kind in {"missing", "aliased"}:
        assert any(gap.kind == "missing-binding" for gap in result.gaps)
        assert not result.import_check_complete
        if kind == "aliased":
            assert '"identity" as "forwarded"' in result.render()
    elif kind == "external":
        assert any(gap.kind == "local-module" and gap.module == "math" for gap in result.gaps)
        assert not result.import_check_complete
    elif kind == "unchecked-startup":
        assert result.module_search_complete and not result.import_check_complete
        assert result.unchecked_statements == 1 and not result.gaps
        assert "leaf.py" in result.render()
    elif kind == "unknown-call":
        assert result.import_check_complete and not result.analysis_complete
        assert result.render().startswith("Recorded Python value trace: unfinished.")
        assert "Other operations still prevent" in result.render()
    else:
        assert result.import_check_complete and result.analysis_complete


def test_saved_report_command_does_not_read_the_original_workspace(tmp_path):
    path = tmp_path / "answer.json"
    path.write_text(json.dumps(archived_report().to_data()))
    process = subprocess.run([sys.executable, "-m", "fr_ir.flow_import_explanation", str(path),
                              "--max-bytes", "2048"], capture_output=True, text=True, timeout=10)
    assert process.returncode == 0, process.stderr
    assert len(process.stdout.encode()) <= 2048 and "saved report" in process.stdout


def test_complete_reader_example_and_fresh_analysis_after_edit(tmp_path):
    guide = (ROOT / "docs/explain-python-imports.md").read_text()
    programs = re.findall(r"```python\n(.*?)```", guide, re.S)
    assert len(programs) == 4
    for name, source in zip(("helpers.py", "app.py", "explain.py", "run_example.py"), programs):
        (tmp_path / name).write_text(source)
    (tmp_path / "rules.json").write_text(re.findall(r"```json\n(.*?)```", guide, re.S)[0])
    environment = dict(os.environ, PATH=str(ROOT / "target/debug") + os.pathsep + os.environ["PATH"])
    def run(name):
        process = subprocess.run([sys.executable, name], cwd=tmp_path, env=environment,
                                 capture_output=True, text=True, timeout=20)
        assert process.returncode == 0, process.stderr
        return process.stdout
    before = run("explain.py")
    assert "1 import-time statement unchecked" in before and "line 4" in before
    assert "unfinished" in before
    runtime = subprocess.run([sys.executable, "run_example.py"], cwd=tmp_path,
                             capture_output=True, text=True, timeout=10)
    assert runtime.returncode == 0 and runtime.stdout.strip() == "17"
    (tmp_path / "helpers.py").write_text(programs[0].replace("metadata = start()", "metadata = 0"))
    after = run("explain.py")
    assert "finished within its stated rules" in after and "0 import-time statements unchecked" in after


@pytest.mark.parametrize("content", [b'[]', b'{"schema":1,"schema":2}', b'\xff', b' ' * 1048577],
                         ids=["non-object", "duplicate-field", "invalid-utf8", "oversized"])
def test_saved_report_requires_one_bounded_current_json_object(tmp_path, content):
    path = tmp_path / "answer.json"
    path.write_bytes(content)
    with pytest.raises((FrRuntimeError, ValueError, UnicodeDecodeError)):
        read_report(path)
