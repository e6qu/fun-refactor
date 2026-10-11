#!/usr/bin/env python3
"""Release versions are inert; dependency and workflow changes invalidate evidence."""
import importlib.util
from pathlib import Path
import tempfile
import unittest
from evidence_basis import cargo_lock_basis

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('routes', ROOT / 'tools/workflow-route-context.py')
routes = importlib.util.module_from_spec(spec)
spec.loader.exec_module(routes)

LOCK = '''version = 4
[[package]]
name = "workspace-root"
version = "0.1.0"
dependencies = ["workspace-child 0.1.0", "remote"]
[[package]]
name = "workspace-child"
version = "0.1.0"
[[package]]
name = "remote"
version = "1.0.0"
source = "registry+https://example.invalid/index"
checksum = "first"
'''


class WorkflowBindings(unittest.TestCase):
    def test_release_version_does_not_invalidate_routes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in routes.BOUND_FILES:
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(LOCK if name == 'Cargo.lock' else name)
            original = routes.ROOT
            try:
                routes.ROOT = root
                baseline = routes.bindings()
                (root / 'Cargo.lock').write_text(LOCK.replace('0.1.0', '0.2.0'))
                self.assertEqual(routes.bindings(), baseline)
                (root / 'src/workflow.rs').write_text('changed workflow behavior')
                self.assertNotEqual(routes.bindings(), baseline)
                (root / 'src/workflow.rs').write_text('src/workflow.rs')
                (root / 'tools/evidence_basis.py').write_text('changed normalization')
                self.assertNotEqual(routes.bindings(), baseline)
            finally:
                routes.ROOT = original

    def test_dependencies_and_package_identity_remain_bound(self):
        baseline = cargo_lock_basis(LOCK.encode())
        for before, after in [('1.0.0', '1.0.1'), ('first', 'second'),
                              ('example.invalid', 'another.invalid'),
                              ('workspace-child', 'another-child'),
                              ('"workspace-child 0.1.0", "remote"', '"remote"')]:
            with self.subTest(change=(before, after)):
                self.assertNotEqual(cargo_lock_basis(LOCK.replace(before, after).encode()), baseline)


if __name__ == '__main__':
    unittest.main()
