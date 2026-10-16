"""Check frozen source without requiring a pre-squash commit in local history."""
import copy
import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('flow_freeze', ROOT / 'tools/python-repository-flow.py')
flow = importlib.util.module_from_spec(spec)
spec.loader.exec_module(flow)


def git(root, *arguments):
    return subprocess.run(['git', *arguments], cwd=root, check=True, text=True,
                          capture_output=True, timeout=5).stdout.strip()


def repository(root, subject):
    root.mkdir()
    git(root, 'init', '-q', '-b', 'main')
    git(root, 'config', 'user.name', 'Freeze Test')
    git(root, 'config', 'user.email', 'freeze@example.test')
    for path in flow.ANALYZER_PATHS:
        target = root / path
        if path not in ('Cargo.toml', 'Cargo.lock'):
            target /= 'sample.txt'
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text('frozen source\n')
    git(root, 'add', '.')
    git(root, 'commit', '-qm', subject)
    return {'analyzer_revision': git(root, 'rev-parse', 'HEAD'),
            'analyzer_trees': {path: git(root, 'rev-parse', 'HEAD:' + path)
                               for path in flow.ANALYZER_PATHS}}


class FrozenAnalyzer(unittest.TestCase):
    def test_identical_squashed_source_needs_no_original_commit_object(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            frozen = repository(root / 'original', 'original source')
            repository(root / 'squashed', 'squashed source')
            absent = subprocess.run(['git', 'cat-file', '-e', frozen['analyzer_revision']],
                                    cwd=root / 'squashed', capture_output=True, timeout=5)
            self.assertNotEqual(absent.returncode, 0)
            flow.verify_analyzer(root / 'squashed', frozen)

    def test_dirty_and_committed_changes_in_every_frozen_path_refuse(self):
        for path in flow.ANALYZER_PATHS:
            with self.subTest(path=path), tempfile.TemporaryDirectory() as directory:
                root = Path(directory) / 'repo'
                frozen = repository(root, 'frozen source')
                target = root / path
                if target.is_dir():
                    target /= 'sample.txt'
                target.write_text('changed source\n')
                with self.assertRaises(ValueError):
                    flow.verify_analyzer(root, frozen)
                git(root, 'add', '.')
                git(root, 'commit', '-qm', 'changed source')
                with self.assertRaisesRegex(ValueError, 'fingerprints changed'):
                    flow.verify_analyzer(root, frozen)

    def test_documentation_changes_preserve_the_source_freeze(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / 'repo'
            frozen = repository(root, 'frozen source')
            (root / 'README.md').write_text('New explanation.\n')
            git(root, 'add', '.')
            git(root, 'commit', '-qm', 'explain the source')
            flow.verify_analyzer(root, frozen)

    def test_missing_incomplete_or_malformed_source_fingerprints_refuse(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / 'repo'
            frozen = repository(root, 'frozen source')
            for mutation in ('missing', 'entry', 'extra', 'list', 'boolean', 'digest'):
                with self.subTest(mutation=mutation):
                    changed = copy.deepcopy(frozen)
                    if mutation == 'missing':
                        del changed['analyzer_trees']
                    elif mutation == 'entry':
                        del changed['analyzer_trees']['src']
                    elif mutation == 'extra':
                        changed['analyzer_trees']['README.md'] = 'a' * 40
                    elif mutation == 'list':
                        changed['analyzer_trees'] = []
                    elif mutation == 'boolean':
                        changed['analyzer_trees']['src'] = True
                    else:
                        changed['analyzer_trees']['src'] = 'unknown'
                    with self.assertRaises((KeyError, ValueError)):
                        flow.verify_analyzer(root, changed)


if __name__ == '__main__':
    unittest.main()
