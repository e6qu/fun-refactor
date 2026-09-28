#!/usr/bin/env python3
"""Retain checked delivery through explicit Python module aliases and child lookup."""
from __future__ import annotations

import importlib.util
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('package_flow', ROOT / 'tools/package-flow-acceptance.py')
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)
package.FIXTURE = ROOT / 'tests/agent-eval/module-aliases'
package.BINDINGS = [path.replace('tests/agent-eval/package-flow/', 'tests/agent-eval/module-aliases/')
                    for path in package.BINDINGS] + ['tools/module-aliases-acceptance.py']
original_audit = package.audit


def audit(value):
    original_audit(value)
    lookups = value['positive']['inputs']['modules']['lookups']
    exported = next(item for item in lookups if item['importer'] == 'app.py' and item['alias'] == 'api')
    assert exported['resolution'] == 'module' and exported['terminal_module'] == 'portal/api.py'
    assert exported['binding_chain'] == [{'path': 'portal/__init__.py', 'name': 'transport'}]
    child = next(item for item in lookups if item['importer'] == 'portal/__init__.py')
    assert child['submodule']['module'] == 'portal.api'
    assert child['submodule']['target'] == 'portal/api.py'
    assert child['terminal_module'] == 'portal/api.py' and not child['binding_chain']
    assert value['baseline']['fr_complete'] is False


def main():
    package.audit = audit
    package.main()


if __name__ == '__main__':
    main()
