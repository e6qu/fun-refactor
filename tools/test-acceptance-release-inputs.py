"""Release labels must not invalidate structural checks or hide changed SDK code."""
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import evidence_basis


class AcceptanceReleaseInputs(unittest.TestCase):
    def test_structural_and_scope_collectors_use_complete_release_independent_sdk_inputs(self):
        for name in ('structural-change-acceptance', 'change-scope-acceptance'):
            with self.subTest(collector=name), tempfile.TemporaryDirectory() as temporary:
                spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(name + '.py'))
                collector = importlib.util.module_from_spec(spec)
                spec.loader.exec_module(collector)
                root = Path(temporary)
                for relative in (*collector.BINDINGS, 'sdk/python/pyproject.toml'):
                    path = root / relative
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_text('fixed input\n')
                manifest = root / 'Cargo.toml'
                version = root / 'sdk/python/src/fr_ir/_version.py'

                def release(number):
                    manifest.write_text('[package]\nversion = "' + number + '"\n')
                    version.write_text('VERSION = "' + number + '"  # x-release-please-version\n')

                release('1.0.0')
                with patch.object(collector, 'ROOT', root), \
                     patch.object(evidence_basis, 'native_source_bindings', return_value={'native': 'fixed'}):
                    baseline = collector.bindings()
                    self.assertIn('sdk/python/pyproject.toml', baseline)
                    release('2.0.0')
                    self.assertEqual(collector.bindings(), baseline)
                    runtime = root / 'sdk/python/src/fr_ir/runtime.py'
                    runtime.write_text('changed behavior\n')
                    self.assertNotEqual(collector.bindings(), baseline)
                    runtime.write_text('fixed input\n')
                    extra = version.with_name('new_behavior.py')
                    extra.write_text('new behavior\n')
                    self.assertNotEqual(collector.bindings(), baseline)
                    extra.unlink()
                    version.write_text('VERSION = "3.0.0"  # x-release-please-version\n')
                    with self.assertRaisesRegex(ValueError, 'versions must match'):
                        collector.bindings()


if __name__ == '__main__':
    unittest.main()
