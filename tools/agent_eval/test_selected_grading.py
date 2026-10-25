"""Check source isolation before any generated code can execute."""
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval import selected_grading as grading


class SelectedGradingTests(unittest.TestCase):
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


if __name__ == '__main__':
    unittest.main()
