#!/usr/bin/env python3
"""Reject unsafe or substituted historical source snapshots."""
import importlib.util
import io
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('frozen', ROOT / 'tools/frozen-repository-flow.py')
frozen = importlib.util.module_from_spec(spec)
spec.loader.exec_module(frozen)


class FrozenSource(unittest.TestCase):
    def archive(self, names, kind=tarfile.REGTYPE):
        output = io.BytesIO()
        with tarfile.open(fileobj=output, mode='w:gz') as archive:
            for name in names:
                member = tarfile.TarInfo(name)
                member.type = kind
                member.linkname = '/tmp/outside'
                member.size = 4 if kind == tarfile.REGTYPE else 0
                archive.addfile(member, io.BytesIO(b'code'))
        return output.getvalue()

    def test_safe_snapshot_preserves_exact_source_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            frozen.unpack(self.archive(['tools/audit.py']), root)
            self.assertEqual((root / 'tools/audit.py').read_bytes(), b'code')

    def test_traversal_links_and_duplicate_paths_refuse(self):
        for names, kind in [(['../outside'], tarfile.REGTYPE), (['/outside'], tarfile.REGTYPE),
                            (['tools/a.py'] * 2, tarfile.REGTYPE), (['tools/link'], tarfile.SYMTYPE),
                            (['tools/link'], tarfile.LNKTYPE)]:
            with self.subTest(names=names, kind=kind), tempfile.TemporaryDirectory() as directory:
                with self.assertRaises(ValueError):
                    frozen.unpack(self.archive(names, kind), Path(directory))

    def test_archive_substitution_refuses_before_extraction(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'source.tar.gz'
            path.write_bytes(self.archive(['tools/audit.py']))
            with patch.object(frozen, 'SNAPSHOT', path), patch.object(frozen, 'SNAPSHOT_SHA', '0' * 64):
                with self.assertRaisesRegex(ValueError, 'digest changed'):
                    frozen.source_root()
            self.assertIsNone(frozen._temporary)


if __name__ == '__main__':
    unittest.main()
