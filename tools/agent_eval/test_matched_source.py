import importlib.util
from pathlib import Path
import tempfile

import pytest


ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    "matched_agent_source", ROOT / "tools/matched-agent-source.py"
)
SOURCE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SOURCE)


def test_program_validation_admits_only_the_guided_delivery_surface():
    SOURCE.validate_program(SOURCE.sdk_program())
    with pytest.raises(ValueError, match="outside the admitted surface"):
        SOURCE.validate_program("from pathlib import Path\nPath('src/lib.rs').read_text()\n")
    with pytest.raises(ValueError, match="outside the admitted surface"):
        SOURCE.validate_program("from fr_ir.runtime import FrClient\nclient.call('guide')\n")


def test_both_instrumented_arms_produce_the_exact_source_change():
    binary = ROOT / "target/debug/fr"
    if not binary.is_file():
        pytest.skip("target/debug/fr is required for the source-writing harness integration")
    with tempfile.TemporaryDirectory(prefix="fr-matched-source-test-") as directory:
        root = Path(directory) / "cohort"
        SOURCE.prepare(root, binary)
        fr = root / "fr"
        result = SOURCE.step(fr, {"tool": "sdk", "program_lines": SOURCE.sdk_program().splitlines()})
        assert result["exit_code"] == 0
        assert result["answers"] == SOURCE.EXPECTED
        assert (fr / "project/src/lib.rs").read_text() == SOURCE.EXPECTED_SOURCE
        lifecycle = SOURCE.sdk_lifecycle(fr)
        assert lifecycle["reviewed_writes"] == 1
        assert lifecycle["passed"] is True
        assert len(lifecycle["stages"]) == 8

        duplicate = SOURCE.step(fr, {"tool": "sdk", "program": SOURCE.sdk_program()})
        assert duplicate["exit_code"] == 1

        files = root / "files"
        assert SOURCE.step(files, {"tool": "read", "path": "src/lib.rs"})["text"] == SOURCE.SOURCE
        assert SOURCE.step(files, {"tool": "replace", "path": "src/lib.rs",
                                   "old": "+ 7", "new": "+ 9"})["changed"] is True
        assert SOURCE.step(files, {"tool": "submit", "answers": SOURCE.EXPECTED})["accepted"] is True
        assert (files / "project/src/lib.rs").read_text() == SOURCE.EXPECTED_SOURCE


def test_retained_source_writing_cohort_is_bound_and_passing():
    evidence = ROOT / "tests/agent-eval/results/2026-09-18-guided-delivery-acceptance"
    report = SOURCE.audit(evidence)
    assert report["acceptance_evidence"] is True
    assert report["passed"] is True
    assert report["usage_difference_fr_minus_files"]["input_tokens"] == -26_596
    fr, files = report["results"]
    assert (fr["tool_calls"], files["tool_calls"]) == (2, 4)
    assert fr["coverage"]["reviewed_writes"] == 1
    assert len(fr["coverage"]["stages"]) == 8
