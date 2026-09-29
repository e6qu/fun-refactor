#!/usr/bin/env python3
"""Retain discovered repair through ordered scalar expression transfers."""
from __future__ import annotations

import importlib.util
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("package_flow", ROOT / "tools/package-flow-acceptance.py")
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)
package.FIXTURE = ROOT / "tests/agent-eval/expression-control"
package.BINDINGS = [path.replace("tests/agent-eval/package-flow/", "tests/agent-eval/expression-control/")
                    for path in package.BINDINGS] + ["tools/expression-control-acceptance.py"]
original_audit = package.audit


def audit(value):
    original_audit(value)
    for name in ("positive", "negative", "after"):
        report = value[name]
        summaries = package.FunctionSummaries.from_report(package.FrReport(report, ()))
        assert report["semantics"] == "python-scalar-summaries-3"
        assert summaries.expression_control is not None
        assert summaries.expression_control.path_feasibility is False
        assert report["inputs"]["expression_control"] == report["expression_control"]
    assert value["baseline"]["fr_complete"] is False
    assert "unsupported-expression:conditional_expression" in value["baseline"]["fr_cutoffs"]
    assert "short-circuit-call-control-unchecked" in value["baseline"]["fr_cutoffs"]


def main():
    package.audit = audit
    package.main()


if __name__ == "__main__":
    main()
