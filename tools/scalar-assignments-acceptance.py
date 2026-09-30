#!/usr/bin/env python3
"""Retain discovered repair through swaps, unpacking and chained scalar assignments."""
from __future__ import annotations

import importlib.util
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("package_flow", ROOT / "tools/package-flow-acceptance.py")
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)
package.FIXTURE = ROOT / "tests/agent-eval/scalar-assignments"
package.BINDINGS = [path.replace("tests/agent-eval/package-flow/", "tests/agent-eval/scalar-assignments/")
                    for path in package.BINDINGS] + ["tools/scalar-assignments-acceptance.py"]
original_audit = package.audit


def audit(value):
    original_audit(value)
    for name in ("positive", "negative", "after"):
        report = value[name]
        summaries = package.FunctionSummaries.from_report(package.FrReport(report, ()))
        assert report["semantics"] == "python-scalar-summaries-5"
        assert summaries.assignment_control is not None
        assert summaries.assignment_control.implicit_exceptions is False
        assert report["inputs"]["assignment_control"] == report["assignment_control"]
    assert value["baseline"]["fr_complete"] is False
    assert value["baseline"]["fr_cutoffs"]


def main():
    package.audit = audit
    package.main()


if __name__ == "__main__":
    main()
