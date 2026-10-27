"""Check source isolation before any generated code can execute."""
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval import selected_grading as grading
from agent_eval.selected_grade_probe import restore_dependency_cache


class SelectedGradingTests(unittest.TestCase):
    def test_image_timeout_retains_the_build_log_without_extending_the_deadline(self):
        output = self.root / 'image'

        def timeout(argv, *, stdout, stderr, timeout):
            self.assertEqual(timeout, 120)
            self.assertIn('--progress=plain', argv)
            stdout.write(b'build output before timeout\n')
            stderr.write(b'package download did not finish\n')
            raise grading.subprocess.TimeoutExpired(argv, timeout)

        with mock.patch.object(grading.subprocess, 'run', side_effect=timeout), \
                self.assertRaises(grading.subprocess.TimeoutExpired):
            grading.build(output)
        self.assertEqual((output / 'build.stdout').read_bytes(), b'build output before timeout\n')
        self.assertEqual((output / 'build.stderr').read_bytes(), b'package download did not finish\n')

    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.source = self.root / 'source'
        self.source.mkdir()

    def test_source_snapshot_preserves_files_modes_and_ignores_agent_state(self):
        (self.source / 'main.py').write_text('print("é")\n')
        (self.source / 'run').write_text('exit 0\n')
        (self.source / 'run').chmod(0o755)
        for name in ('.git', '.agents', '.lake', 'target', '.fr-history'):
            path = self.source / name
            path.mkdir()
            (path / 'state').write_text('not a source input')
        receipt = grading.stage(self.source, self.root / 'snapshot')
        self.assertEqual(set(receipt['files']), {'main.py', 'run'})
        self.assertEqual(receipt['files'], grading.tasks.sources(self.root / 'snapshot'))
        self.assertFalse((self.root / 'snapshot/.git').exists())

    def test_symlinks_and_oversized_sources_refuse_before_container_creation(self):
        (self.source / 'outside').symlink_to(self.root / 'private-credential')
        with self.assertRaisesRegex(ValueError, 'regular file'):
            grading.stage(self.source, self.root / 'symlink-snapshot')
        (self.source / 'outside').unlink()
        with (self.source / 'too-large').open('wb') as stream:
            stream.truncate(grading.SOURCE_LIMIT + 1)
        with self.assertRaisesRegex(ValueError, 'exceeds 32 MiB'):
            grading.stage(self.source, self.root / 'large-snapshot')

    def test_private_file_modes_that_a_git_patch_cannot_deliver_refuse(self):
        source = self.source / 'main.py'
        source.write_text('print("hello")\n')
        source.chmod(0o600)
        with self.assertRaisesRegex(ValueError, 'normal Git checkout modes'):
            grading.stage(self.source, self.root / 'private-mode-snapshot')

    def test_prepared_dependencies_share_bytes_but_rebuilt_outputs_stay_writable(self):
        files = {'debug/deps/libserde-abc.rlib': b'unchanged compiled dependency',
                 'debug/deps/libserde-abc.rmeta': b'unchanged metadata',
                 'debug/deps/libserde_derive-abc.so': b'unchanged compiler plugin',
                 'debug/deps/libregex_automata-abc.rlib': b'depends on changed code',
                 'debug/deps/libregex_automata-abc.rmeta': b'depends on changed metadata',
                 'debug/.fingerprint/serde-abc/lib-serde': b'cargo state'}
        for name, content in files.items():
            path = self.source / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(content)
        target = self.root / 'target'
        restore_dependency_cache(self.source, target)
        for name, content in files.items():
            self.assertEqual((target / name).read_bytes(), content)
            shared = name.startswith('debug/deps/libserde')
            self.assertEqual((target / name).is_symlink(), shared)
            if shared:
                self.assertEqual((target / name).resolve(), (self.source / name).resolve())
            else:
                (target / name).write_bytes(b'rebuilt state')
                self.assertEqual((self.source / name).read_bytes(), content)


if __name__ == '__main__':
    unittest.main()
