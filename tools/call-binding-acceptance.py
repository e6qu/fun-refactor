#!/usr/bin/env python3
"""Retain discovered repair through required Python call binding."""
from __future__ import annotations

import importlib.util
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("package_flow", ROOT / "tools/package-flow-acceptance.py")
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)
package.FIXTURE = ROOT / "tests/agent-eval/call-binding"
package.BINDINGS = [path.replace("tests/agent-eval/package-flow/", "tests/agent-eval/call-binding/")
                    for path in package.BINDINGS] + ["tools/call-binding-acceptance.py"]
original_audit = package.audit


def audit(value):
    original_audit(value)
    for name in ("positive", "negative", "after"):
        report = value[name]
        summaries = package.FunctionSummaries.from_report(package.FrReport(report, ()))
        assert report["semantics"] in {"python-scalar-summaries-3", "python-scalar-summaries-4", "python-scalar-summaries-5"}
        assert summaries.call_binding is not None
        assert summaries.call_binding.implicit_exceptions is False
        assert report["inputs"]["call_binding"] == report["call_binding"]
    assert value["baseline"]["fr_complete"] is False
    assert "unsupported-expression:keyword_argument" in value["baseline"]["fr_cutoffs"]


def main():
    package.audit = audit
    package.main()


if __name__ == "__main__":
    main()
