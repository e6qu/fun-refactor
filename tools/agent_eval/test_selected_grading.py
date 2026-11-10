"""Check source isolation before any generated code can execute."""
from pathlib import Path
import json
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval import selected_grading as grading
from agent_eval import selected_grade_probe as probe, proof_writing_tasks as proof
from agent_eval.selected_grade_probe import restore_dependency_cache


class SelectedGradingTests(unittest.TestCase):
    def test_algorithm_grader_accepts_exact_named_snapshots_and_rejects_package_changes(self):
        from agent_eval import proof_algorithms as algorithms
        for directory in ('fixtures', 'interrupted-results', 'retained-fixtures'):
            with self.subTest(snapshot=directory):
                inputs = self.root / directory
                original = inputs / 'original'
                fixture = algorithms.ROOT / algorithms.DIRECTORY / directory
                for target, name in {algorithms.SPEC: 'Euclid.lean', **{
                        'specs/' + name: name for name in algorithms.PACKAGE_FILES}}.items():
                    path = original / target
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_bytes((fixture / name).read_bytes())
                prepared = {'task': 'euclid', 'fixture': directory + '/Euclid.lean',
                            'obligations': list(algorithms.OBLIGATIONS),
                            'protected_source': algorithms.masked((original / algorithms.SPEC).read_text()),
                            'original': grading.tasks.sources(original)}
                (inputs / 'project.json').write_text(json.dumps(prepared))
                (inputs / 'delivery.json').write_text('{"provided": false}')
                (inputs / 'change.patch').write_bytes(b'')
                workspace = inputs / 'workspace'
                workspace.mkdir()
                container = inputs / '.dockerenv'
                container.touch()
                paths = {'/.dockerenv': container, '/workspace': workspace,
                         **{'/inputs/' + name: inputs / name for name in
                            ('original', 'project.json', 'delivery.json', 'change.patch')}}
                with mock.patch.object(probe, 'Path', side_effect=lambda value: paths.get(value, Path(value))), \
                        mock.patch.dict(probe.os.environ, {}, clear=True):
                    result = probe.run('euclid', 'M3-algorithm-facts')
                    self.assertEqual(result['reason'], 'the agent did not deliver a nonempty patch')
                    package = original / 'specs/lakefile.toml'
                    package.write_text(package.read_text() + '\n# changed package\n')
                    prepared['original'] = grading.tasks.sources(original)
                    (inputs / 'project.json').write_text(json.dumps(prepared))
                    with self.assertRaisesRegex(ValueError, 'private executable algorithm'):
                        probe.run('euclid', 'M3-algorithm-facts')

    def test_page_count_grader_uses_its_settings_and_checks_exact_patch_delivery(self):
        inputs = self.root / 'inputs'
        original = inputs / 'original'
        original.mkdir(parents=True)
        spec = 'specs/Fixed.lean'
        (original / 'specs').mkdir()
        (original / spec).write_text('old\n')
        prepared = {'task': 'page-count', 'spec': spec, 'original': grading.tasks.sources(original),
                    'property': proof.property_for('page-count', {'object_digest': 'fixed'})}
        (inputs / 'project.json').write_text(json.dumps(prepared))
        (inputs / 'delivery.json').write_text(json.dumps({'provided': True}))
        (inputs / 'change.patch').write_text(
            'diff --git a/specs/Fixed.lean b/specs/Fixed.lean\n'
            '--- a/specs/Fixed.lean\n+++ b/specs/Fixed.lean\n@@ -1 +1 @@\n-old\n+new\n')
        workspace = self.root / 'workspace'
        workspace.mkdir()
        container = self.root / '.dockerenv'
        container.touch()

        def path(value):
            return {'/.dockerenv': container, '/inputs/original': original,
                    '/inputs/project.json': inputs / 'project.json',
                    '/inputs/delivery.json': inputs / 'delivery.json',
                    '/inputs/change.patch': inputs / 'change.patch',
                    '/workspace': workspace}.get(value, Path(value))

        versions = {'lean': 'Lean (version 4.28.0, test)', 'lake': 'Lake version test',
                    'rustc': 'rustc 1.99.1 test', 'python3': 'Python 3.12.3'}
        settings = self.root / 'settings' / probe.CASE_DIRECTORIES['M3-page-count']
        settings.mkdir(parents=True)
        (settings / 'checkers.json').write_bytes(
            (probe.ROOT / probe.CASE_DIRECTORIES['M3-page-count'] / 'checkers.json').read_bytes())
        actual_check_output = probe.subprocess.check_output

        def version_or_git(argv, **kwargs):
            if len(argv) == 2 and argv[1] == '--version' and argv[0] in versions:
                return versions[argv[0]]
            return actual_check_output(argv, **kwargs)

        with mock.patch.object(probe, 'Path', side_effect=path), \
                mock.patch.object(probe, 'ROOT', self.root / 'settings'), \
                mock.patch.dict(probe.os.environ, {}, clear=True), \
                mock.patch.object(probe.subprocess, 'check_output', side_effect=version_or_git), \
                mock.patch.object(proof, 'check', return_value={'passed': True}) as checking:
            result = probe.run('page-count', 'M3-page-count')
        self.assertTrue(result['passed'])
        self.assertTrue(result['undo_exact'])
        self.assertTrue(result['redo_exact'])
        self.assertEqual(checking.call_count, 2)
        self.assertEqual((workspace / 'receiver' / spec).read_text(), 'new\n')

    def test_image_timeout_retains_the_build_log_without_extending_the_deadline(self):
        output = self.root / 'image'

        def timeout(argv, *, stdout, stderr, timeout):
            self.assertEqual(timeout, grading.IMAGE_BUILD_SECONDS)
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
