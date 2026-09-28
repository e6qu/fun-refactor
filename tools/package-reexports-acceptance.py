#!/usr/bin/env python3
"""Retain checked delivery through explicit Python package function re-exports."""
from __future__ import annotations

import importlib.util
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('package_flow', ROOT / 'tools/package-flow-acceptance.py')
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)
package.FIXTURE = ROOT / 'tests/agent-eval/package-reexports'
package.BINDINGS = [path.replace('tests/agent-eval/package-flow/', 'tests/agent-eval/package-reexports/')
                    for path in package.BINDINGS] + ['tools/package-reexports-acceptance.py']
original_audit = package.audit


def audit(value):
    original_audit(value)
    lookups = value['positive']['inputs']['modules']['lookups']
    exported = next(item for item in lookups if item['importer'] == 'app.py' and item['alias'] == 'dispatch')
    assert exported['resolution'] == 'function'
    assert exported['binding_chain'] == [
        {'path': 'portal/__init__.py', 'name': 'dispatch'}, {'path': 'portal/api.py', 'name': 'send'}]
    assert value['baseline']['fr_complete'] is False


def main():
    package.audit = audit
    package.main()


if __name__ == '__main__':
    main()
