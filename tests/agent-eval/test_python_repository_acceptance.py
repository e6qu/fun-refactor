"""Adversarial archive and independent-oracle checks; no fr build or generation."""
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools'))
spec = importlib.util.spec_from_file_location('repository_acceptance', ROOT / 'tools/python-repository-acceptance.py')
acceptance = importlib.util.module_from_spec(spec)
spec.loader.exec_module(acceptance)


class RepositoryAcceptanceTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)

    def archive(self, names, *, link=False):
        path = self.root / 'fixture.tar.gz'
        with tarfile.open(path, 'w:gz') as stream:
            for name in names:
                info = tarfile.TarInfo(name)
                if link:
                    info.type = tarfile.SYMTYPE
                    info.linkname = '/etc/passwd'
                    stream.addfile(info)
                else:
                    info.size = 4
                    stream.addfile(info, io.BytesIO(b'test'))
        return {'archive': path.name, 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}

    def test_archive_rejects_traversal_links_and_duplicate_paths(self):
        for index, (names, link) in enumerate([
            (['root/../outside'], False), (['/absolute/file'], False),
            (['root/link'], True), (['root/file', 'root/file'], False),
            (['root/file', 'other/file'], False),
        ]):
            task = self.archive(names, link=link)
            with patch.object(acceptance, 'FIXTURE', self.root), self.assertRaises(AssertionError):
                acceptance.unpack(task, self.root / f'extracted-{index}')

    def test_archive_rejects_digest_and_expansion_budget_drift(self):
        task = self.archive(['root/file'])
        task['sha256'] = '0' * 64
        with patch.object(acceptance, 'FIXTURE', self.root), self.assertRaises(AssertionError):
            acceptance.unpack(task, self.root / 'bad-digest')
        task = self.archive(['root/file'])
        config = {'budgets': {**acceptance.TASKS['budgets'], 'expanded_bytes': 3}}
        with patch.object(acceptance, 'FIXTURE', self.root), patch.object(acceptance, 'TASKS', config), self.assertRaises(AssertionError):
            acceptance.unpack(task, self.root / 'too-large')

    def run_oracle(self, module, function, name):
        package = self.root / module
        package.mkdir(exist_ok=True)
        (package / '__init__.py').write_text(function)
        return acceptance.oracle(self.root, name)

    def test_tail_oracle_detects_eager_storage_and_overconsumption(self):
        eager = '''
def take(n, iterable):
    from operator import index
    if n is None: return list(iterable)
    n = index(n)
    values = list(iterable)
    return values[n:] if n < 0 else values[:n]
'''
        result = self.run_oracle('more_itertools', eager, 'take-tail-counts')
        self.assertFalse(result['passed'])
        failures = {f[0] for f in result['failures']}
        self.assertIn('bounded tail storage', failures)
        self.assertIn('consumption:0', failures)
        self.assertIn('consumption:2', failures)

    def test_tail_oracle_detects_wrong_tail_order(self):
        wrong = '''
def take(n, iterable):
    from collections import deque
    from itertools import islice
    from operator import index
    if n is None: return list(iterable)
    n = index(n)
    return list(reversed(deque(iterable, maxlen=-n))) if n < 0 else list(islice(iterable, n))
'''
        result = self.run_oracle('more_itertools', wrong, 'take-tail-counts')
        self.assertFalse(result['passed'])
        self.assertTrue(any(f[0] == 'index protocol' for f in result['failures']))

    def test_pinned_original_behavior_fails_without_modifying_archives(self):
        for name, task in acceptance.TASKS['tasks'].items():
            root = self.root / name
            acceptance.unpack(task, root)
            value = acceptance.oracle(root, name)
            self.assertFalse(value['passed'])
            self.assertGreater(value['cases'], 50)
            self.assertTrue(value['failures'])


if __name__ == '__main__':
    unittest.main()
