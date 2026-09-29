#!/usr/bin/env python3
"""Retain discovered repair and checked delivery through single-root namespaces."""
from __future__ import annotations

import importlib.util
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("package_flow", ROOT / "tools/package-flow-acceptance.py")
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)
package.FIXTURE = ROOT / "tests/agent-eval/namespace-packages"
package.SOURCES = tuple(path for path in package.SOURCES if not path.endswith('/__init__.py'))
package.BINDINGS = [path.replace("tests/agent-eval/package-flow/", "tests/agent-eval/namespace-packages/")
                    for path in package.BINDINGS if not path.endswith('/__init__.py')] + ["tools/namespace-packages-acceptance.py"]
original_audit = package.audit


def audit(value):
    original_audit(value)
    for name in ("positive", "negative", "after"):
        report = value[name]
        dependencies = package.FlowDependencies.from_report(package.FrReport(report, ()))
        assert report['inputs']['modules']['schema'] == 'fr-flow-modules-5'
        assert dependencies.namespaces and 'portal' in dependencies.namespaces
        assert not any(path.endswith('/__init__.py') for path, _ in dependencies.files)
        assert len(dependencies.files) + len(dependencies.namespaces) <= 16
    assert value['baseline']['fr_complete'] is False
    assert any(reason.startswith('unresolved-local-import:') for reason in value['baseline']['fr_cutoffs'])
    changed = package.FlowDependencies.from_report(package.FrReport(value['mutations']['initializer']['analysis'], ()))
    assert 'portal' not in changed.namespaces and 'portal/__init__.py' in dict(changed.files)


def main():
    package.audit = audit
    package.main()


if __name__ == '__main__':
    main()
