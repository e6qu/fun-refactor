"""Reject broken or substituted packages before any consumer command runs."""
import importlib.util
import io
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch


def load(name):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(name + '.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


release = load('check-release-package')
sdk = load('check-sdk-package')


class ReleasePackages(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.archive = self.root / 'fr-v1.2.3-test.tar.gz'
        self.checksum = self.root / (self.archive.name + '.sha256')
        self.prefix = self.archive.name.removesuffix('.tar.gz')

    def package(self, names, link=None):
        with tarfile.open(self.archive, 'w:gz') as stream:
            for name in names:
                item = tarfile.TarInfo(name)
                item.size = 4
                stream.addfile(item, io.BytesIO(b'test'))
            if link:
                item = tarfile.TarInfo(self.prefix + '/link')
                item.type = tarfile.SYMTYPE
                item.linkname = link
                stream.addfile(item)
        self.checksum.write_text(release.digest(self.archive) + '  ' + self.archive.name + '\n')

    def test_exact_package_extracts(self):
        self.package([self.prefix + '/README.md'])
        extracted = release.unpack(self.archive, self.checksum, self.root / 'consumer')
        self.assertEqual((extracted / 'README.md').read_text(), 'test')

    def test_changed_bytes_and_wrong_checksum_name_are_rejected(self):
        self.package([self.prefix + '/README.md'])
        self.checksum.write_text(release.digest(self.archive) + '  another.tar.gz\n')
        with self.assertRaisesRegex(ValueError, 'checksum'):
            release.unpack(self.archive, self.checksum, self.root / 'consumer')
        self.package([self.prefix + '/README.md'])
        with self.archive.open('ab') as stream:
            stream.write(b'changed')
        with self.assertRaisesRegex(ValueError, 'checksum'):
            release.unpack(self.archive, self.checksum, self.root / 'consumer')

    def test_paths_duplicates_and_links_are_rejected(self):
        for names, link in [(['../outside'], None), (['/absolute'], None),
                            (['wrong-root/file'], None), ([self.prefix + '/a'] * 2, None),
                            ([self.prefix + '/a'], '../outside')]:
            with self.subTest(names=names, link=link):
                self.package(names, link)
                with self.assertRaisesRegex(ValueError, 'path'):
                    release.unpack(self.archive, self.checksum, self.root / 'consumer')
                self.assertFalse((self.root / 'consumer').exists())

    def test_missing_native_and_browser_members_never_run_consumers(self):
        self.package([self.prefix + '/' + name for name in ['LICENSE', 'README.md']])
        with patch.object(release.subprocess, 'run') as run:
            for kind in ('native', 'wasm'):
                with self.subTest(kind=kind), self.assertRaisesRegex(ValueError, 'lacks'):
                    release.check(self.archive, self.checksum, '1.2.3', kind)
            run.assert_not_called()

    def test_consumer_failure_is_not_an_architecture_only_pass(self):
        self.package([self.prefix + '/' + name for name in ['LICENSE', 'README.md', 'fr', 'CLI.md', 'skills/fr/SKILL.md']])
        failure = release.subprocess.CalledProcessError(1, ['fr'])
        with patch.object(release.subprocess, 'run', side_effect=failure):
            with self.assertRaises(release.subprocess.CalledProcessError):
                release.check(self.archive, self.checksum, '1.2.3', 'native')

    def test_sdk_requires_one_distribution_of_each_kind(self):
        with self.assertRaisesRegex(ValueError, 'exactly one'):
            sdk.distributions(self.root)
        wheel = self.root / 'fun_refactor_ir-1.2.3-py3-none-any.whl'
        wheel.touch()
        self.archive.unlink(missing_ok=True)
        sdist = self.root / 'fun_refactor_ir-1.2.3.tar.gz'
        sdist.touch()
        self.assertEqual(sdk.distributions(self.root), (wheel, sdist))
        (self.root / 'fun_refactor_ir-1.2.2-py3-none-any.whl').touch()
        with self.assertRaisesRegex(ValueError, 'exactly one'):
            sdk.distributions(self.root)


if __name__ == '__main__':
    unittest.main()
