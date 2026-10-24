"""Keep retained source evidence tied to the same native inputs as the build."""
from pathlib import Path
import shutil
import tempfile
import unittest

from evidence_basis import checker_bindings, native_source_bindings, native_source_paths, native_source_roots

ROOT = Path(__file__).resolve().parents[1]


def fixture(root):
    shutil.copyfile(ROOT / 'native-source-inputs.txt', root / 'native-source-inputs.txt')
    for name in ('src', 'grammars/python/src', 'crates/local/src', 'queries/python', 'catalogs', 'kernels/FrKernels'):
        (root / name).mkdir(parents=True, exist_ok=True)
    for path in ('src/rules.rs', 'grammars/python/src/parser.c', 'grammars/python/src/scanner.c',
                 'queries/python/calls.scm', 'crates/local/src/lib.rs', 'build.rs',
                 'catalogs/python.yaml', 'kernels/FrKernels/PureKernel.lean'):
        (root / path).write_text('initial input\n')
    (root / 'Cargo.lock').write_text('version=4\n[[package]]\nname="fixture"\nversion="1.0.0"\n'
        '[[package]]\nname="remote"\nversion="2.0.0"\nsource="registry+https://example.invalid/index"\nchecksum="first"\n')
    (root / 'Cargo.toml').write_text('[workspace]\nmembers=["."]\n[package]\nname="fixture"\nversion="1.0.0"\n'
        '[dependencies]\nlocal={path="crates/local",version="1.0.0"}\nremote="2.0.0"\n')


class NativeInputs(unittest.TestCase):
    def test_client_module_changes_additions_and_removals_expire_checker_evidence(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            fixture(root)
            client = root / 'sdk/python/src/fr_ir'
            client.mkdir(parents=True)
            (root / 'sdk/python/pyproject.toml').write_text('[project]\nname="client"\nversion="1.0.0"\n')
            (client / '__init__.py').write_text('')
            (client / 'flow.py').write_text('from .flow_completion import validate\n')
            module = client / 'flow_completion.py'
            module.write_text('def validate(): return False\n')
            native = native_source_bindings(root)
            before = checker_bindings(root, ['build.rs'])
            module.write_text('def validate(): return True\n')
            self.assertNotEqual(before, checker_bindings(root, ['build.rs']))
            self.assertEqual(native, native_source_bindings(root))
            changed = checker_bindings(root, ['build.rs'])
            added = client / 'new_rule.py'
            added.write_text('rule = True\n')
            self.assertNotEqual(changed, checker_bindings(root, ['build.rs']))
            added.unlink()
            self.assertEqual(changed, checker_bindings(root, ['build.rs']))
            module.unlink()
            self.assertNotEqual(changed, checker_bindings(root, ['build.rs']))

    def test_missing_or_symlinked_client_source_cannot_inherit_native_only_evidence(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            fixture(root)
            with self.assertRaises(ValueError):
                checker_bindings(root, ['build.rs'])
            client = root / 'sdk/python/src/fr_ir'
            client.mkdir(parents=True)
            (root / 'sdk/python/pyproject.toml').write_text('[project]\nname="client"\nversion="1.0.0"\n')
            (client / 'flow.py').symlink_to(root / 'build.rs')
            with self.assertRaises(ValueError):
                checker_bindings(root, ['build.rs'])

    def test_current_policy_covers_build_parsing_grammar_and_flow_rules(self):
        paths = set(native_source_paths(ROOT))
        self.assertTrue({'build.rs', 'Cargo.toml', 'Cargo.lock', 'native-source-inputs.txt',
                         'src/native_fingerprint.rs', 'src/project/flow_initializers.rs',
                         'src/project/flow_import_order.rs', 'grammars/python/src/parser.c',
                         'grammars/python/src/scanner.c', 'src/model.rs', 'src/lang.rs',
                         'catalogs/python.yaml', 'kernels/FrKernels/PureKernel.lean'} <= paths)
        self.assertIn('native-source-inputs.txt', native_source_roots(ROOT))

    def test_each_input_content_changes_its_retained_binding(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            fixture(root)
            before = native_source_bindings(root)
            for name in ('src/rules.rs', 'grammars/python/src/parser.c', 'queries/python/calls.scm',
                         'crates/local/src/lib.rs', 'build.rs', 'Cargo.lock',
                         'catalogs/python.yaml', 'kernels/FrKernels/PureKernel.lean'):
                old = (root / name).read_bytes()
                (root / name).write_bytes(old.replace(b'first', b'second') if name == 'Cargo.lock' else old + b'changed\n')
                self.assertNotEqual(before, native_source_bindings(root), name)
                (root / name).write_bytes(old)

    def test_additions_deletions_and_renames_change_the_recorded_file_set(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            fixture(root)
            before = native_source_bindings(root)
            path = root / 'src/new.rs'
            path.write_text('new rule')
            self.assertNotEqual(before, native_source_bindings(root))
            after = native_source_bindings(root)
            path.rename(root / 'src/renamed.rs')
            self.assertNotEqual(after, native_source_bindings(root))
            (root / 'src/renamed.rs').unlink()
            self.assertEqual(before, native_source_bindings(root))
            (root / 'src/rules.rs').unlink()
            self.assertNotEqual(before, native_source_bindings(root))

    def test_release_labels_keep_existing_normalization_but_dependencies_stay_bound(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            fixture(root)
            before = native_source_bindings(root)
            path = root / 'Cargo.toml'
            original = path.read_text()
            path.write_text(original.replace('1.0.0', '1.1.0'))
            self.assertEqual(before, native_source_bindings(root))
            path.write_text(original.replace('remote="2.0.0"', 'remote="2.0.1"'))
            self.assertNotEqual(before, native_source_bindings(root))

    def test_generated_outputs_and_documentation_do_not_enter_evidence(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            fixture(root)
            before = native_source_bindings(root)
            for name in ('docs/lesson.md', 'target/binary', 'grammars/python/node_modules/package/cache'):
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text('generated output')
            self.assertEqual(before, native_source_bindings(root))

    def test_missing_roots_and_unsupported_policy_fail(self):
        for policy in ('../outside\nnative-source-inputs.txt\n', 'src/\nsrc/\nnative-source-inputs.txt\n',
                       'src/\n', 'src/\n\nnative-source-inputs.txt\n', '!bad/path/\nnative-source-inputs.txt\n'):
            with self.subTest(policy=policy), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                fixture(root)
                (root / 'native-source-inputs.txt').write_text(policy)
                with self.assertRaises((ValueError, OSError)):
                    native_source_paths(root)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            fixture(root)
            (root / 'Cargo.lock').unlink()
            with self.assertRaises(OSError):
                native_source_paths(root)


if __name__ == '__main__':
    unittest.main()
