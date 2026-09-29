#!/usr/bin/env python3
"""Retain discovered repair through immutable literal defaults and signature origins."""
from __future__ import annotations

import importlib.util
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("package_flow", ROOT / "tools/package-flow-acceptance.py")
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)
package.FIXTURE = ROOT / "tests/agent-eval/literal-defaults"
package.BINDINGS = [path.replace("tests/agent-eval/package-flow/", "tests/agent-eval/literal-defaults/")
                    for path in package.BINDINGS] + ["tools/literal-defaults-acceptance.py"]
original_audit = package.audit


def audit(value):
    original_audit(value)
    for name in ("positive", "negative", "after"):
        report = value[name]
        summaries = package.FunctionSummaries.from_report(package.FrReport(report, ()))
        assert report["semantics"] == "python-scalar-summaries-4"
        assert summaries.call_binding.defaults is not None
        assert report["inputs"]["call_binding"] == report["call_binding"]
        for function in summaries.functions:
            assert function.signature is not None
            assert len(function.signature) == len(function.parameters)
            assert all(parameter.site.revision == report["revision"] for parameter in function.signature)
    assert value["baseline"]["fr_complete"] is False
    assert "module-effects-unchecked" in value["baseline"]["fr_cutoffs"]


def main():
    package.audit = audit
    package.main()


if __name__ == "__main__":
    main()
