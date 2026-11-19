"""Keep the hosted memory diagnostic separate from admission and live work."""
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from . import product_memory as memory


class ProofMemoryTests(unittest.TestCase):
    def test_refuses_local_measurement_before_creating_files(self):
        with tempfile.TemporaryDirectory() as temporary, patch.dict(memory.os.environ, {}, clear=True):
            root = Path(temporary)
            with self.assertRaisesRegex(ValueError, 'on GitHub'):
                memory.measure(root, root / 'run', root, root / 'output')
            self.assertFalse((root / 'output').exists())

    def test_direct_proof_and_package_keep_the_original_source_and_check(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            proof = root / memory.tasks.PROOF
            proof.parent.mkdir(parents=True)
            proof.write_text('protected source')
            self.assertEqual(memory.command('package', Path('/lean'), root),
                             ['/lean/bin/lake', '-d', 'specs', 'build', '--wfail'])
            self.assertEqual(memory.command('direct-proof', Path('/lean'), root),
                             ['/lean/bin/lean', '-j1', '-DwarningAsError=true', memory.tasks.PROOF])
            for case, module in [('std-import', 'Std'), ('init-import', 'Init')]:
                memory.command(case, Path('/lean'), root)
                self.assertEqual((root / 'Import.lean').read_text(),
                                 f'import {module}\nexample : True := by trivial\n')
            self.assertEqual(proof.read_text(), 'protected source')
            with self.assertRaisesRegex(ValueError, 'unknown'):
                memory.command('narrowed-proof', Path('/lean'), root)

    def test_resource_and_monitor_stops_cannot_count_as_completed(self):
        good = {'exit_code': 0, 'stop_reason': None, 'monitor_error': None, 'launch_error': None}
        self.assertTrue(memory.completed(good))
        for key, value in [('exit_code', 125), ('stop_reason', 'rss_bytes'),
                           ('monitor_error', 'missing sample'), ('launch_error', 'missing Lean')]:
            self.assertFalse(memory.completed({**good, key: value}))

    def test_refuses_unsuccessful_or_unrelated_qualification(self):
        good = {'status': 'completed', 'conclusion': 'success',
                'path': '.github/workflows/product-validation.yml',
                'head_repository': {'full_name': 'e6qu/fun-refactor'}}
        for key, value in [('status', 'in_progress'), ('conclusion', 'failure'),
                           ('path', 'another-workflow.yml'),
                           ('head_repository', {'full_name': 'another/repository'})]:
            with self.subTest(key=key), self.assertRaisesRegex(ValueError, 'successful product qualification'):
                memory.qualified_inputs(Path('/missing'), {**good, key: value})


if __name__ == '__main__':
    unittest.main()
