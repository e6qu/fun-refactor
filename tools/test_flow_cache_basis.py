#!/usr/bin/env python3
"""Keep release metadata out of evidence identities without hiding build changes."""

import importlib.util
from pathlib import Path
import tempfile
import unittest

from evidence_basis import cargo_lock_basis, file_digest

spec = importlib.util.spec_from_file_location(
    "flow_cache_acceptance", Path(__file__).with_name("flow-cache-acceptance.py"))
evaluator = importlib.util.module_from_spec(spec)
spec.loader.exec_module(evaluator)


def manifest(version="0.34.0"):
    return f'''[package]
name = "example"
version = "{version}"
edition = "2021"
[features]
default = ["cli"]
[dependencies]
remote = "1.0"
local = {{ path = "local", version = "{version}", features = ["one"] }}
[dev-dependencies]
helper = {{ path = "helper", version = "{version}" }}
[build-dependencies]
builder = {{ path = "builder", version = "{version}" }}
[target.'cfg(unix)'.dependencies]
platform = {{ path = "platform", version = "{version}", optional = true }}
'''.encode()


def lock(version="0.34.0"):
    return f'''version = 4
[[package]]
name = "example"
version = "{version}"
dependencies = ["local {version}", "remote"]
[[package]]
name = "local"
version = "{version}"
[[package]]
name = "remote"
version = "1.0.0"
source = "registry+https://example.invalid/index"
checksum = "first"
'''.encode()


class FlowCacheBasisTests(unittest.TestCase):
    def test_coordinated_release_is_stable(self):
        self.assertEqual(evaluator.manifest_basis(manifest()),
                         evaluator.manifest_basis(manifest("0.34.1")))
        self.assertEqual(cargo_lock_basis(lock()), cargo_lock_basis(lock("0.34.1")))

    def test_dependency_and_build_changes_remain_visible(self):
        baseline = evaluator.manifest_basis(manifest())
        changes = [
            (b'remote = "1.0"', b'remote = "1.1"'),
            (b'path = "local"', b'path = "elsewhere"'),
            (b'features = ["one"]', b'features = ["two"]'),
            (b'default = ["cli"]', b'default = []'),
            (b'edition = "2021"', b'edition = "2024"'),
            (b'optional = true', b'optional = false'),
            (b'version = "0.34.0", features', b'version = "0.35.0", features'),
            (b'version = "0.34.0", features', b'version = "=0.34.0", features'),
        ]
        for old, new in changes:
            with self.subTest(change=new):
                self.assertNotEqual(baseline, evaluator.manifest_basis(manifest().replace(old, new)))

    def test_remote_lock_changes_remain_visible(self):
        for old, new in [(b'1.0.0', b'1.0.1'), (b'first', b'second'),
                         (b'example.invalid', b'elsewhere.invalid')]:
            with self.subTest(change=new):
                self.assertNotEqual(cargo_lock_basis(lock()), cargo_lock_basis(lock().replace(old, new)))

    def test_bindings_use_normalization_and_keep_source_identity(self):
        previous = evaluator.ROOT, evaluator.BINDINGS
        try:
            with tempfile.TemporaryDirectory() as directory:
                evaluator.ROOT = root = Path(directory)
                evaluator.BINDINGS = ["Cargo.toml", "Cargo.lock", "source.rs"]
                (root / "Cargo.toml").write_bytes(manifest())
                (root / "Cargo.lock").write_bytes(lock())
                (root / "source.rs").write_text("fn answer() -> u8 { 1 }")
                baseline = evaluator.bindings()
                (root / "Cargo.toml").write_bytes(manifest("0.34.1"))
                (root / "Cargo.lock").write_bytes(lock("0.34.1"))
                self.assertEqual(baseline, evaluator.bindings())
                self.assertEqual(baseline["Cargo.lock"], file_digest(root / "Cargo.lock"))
                (root / "source.rs").write_text("fn answer() -> u8 { 2 }")
                self.assertNotEqual(baseline["source.rs"], evaluator.bindings()["source.rs"])
        finally:
            evaluator.ROOT, evaluator.BINDINGS = previous


if __name__ == "__main__":
    unittest.main()
