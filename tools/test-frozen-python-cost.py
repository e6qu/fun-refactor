"""Reject substituted source or rewritten records in completed cost studies."""
import importlib.util
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('history', ROOT / 'tools/frozen-python-cost.py')
history = importlib.util.module_from_spec(spec)
spec.loader.exec_module(history)


class History(unittest.TestCase):
    def test_exact_pinned_source_is_read_without_modification(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path, metadata = root / 'source.tar.gz', root / 'manifest.json'
            path.write_bytes(b'original source')
            identity = history.digest(path.read_bytes())
            metadata.write_text(json.dumps(history.manifest(identity)))
            self.assertEqual(history.checked_snapshot(path, metadata, identity), b'original source')

    def test_unpinned_and_substituted_source_refuse(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path, metadata = root / 'source.tar.gz', root / 'manifest.json'
            path.write_bytes(b'swapped source')
            for identity in ('', '0' * 64):
                with self.assertRaisesRegex(ValueError, 'not pinned|archive changed'):
                    history.checked_snapshot(path, metadata, identity)

    def test_changed_producer_archive_name_and_report_set_refuse(self):
        for field in ('source_commit', 'snapshot', 'reports'):
            with self.subTest(field=field), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                path, metadata = root / 'source.tar.gz', root / 'manifest.json'
                path.write_bytes(b'original source')
                identity = history.digest(path.read_bytes())
                value = history.manifest(identity)
                value[field] = 'changed'
                metadata.write_text(json.dumps(value))
                with self.assertRaisesRegex(ValueError, 'manifest changed'):
                    history.checked_snapshot(path, metadata, identity)

    def test_published_reports_must_keep_their_original_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            source, published = Path(directory) / 'source', Path(directory) / 'published'
            source.mkdir()
            published.mkdir()
            for root in (source, published):
                (root / 'result.json').write_bytes(b'{"original":true}\n')
            history.check_published_reports(source, published, ('result.json',))
            (published / 'result.json').write_bytes(b'{"original":false}\n')
            with self.assertRaisesRegex(ValueError, 'historical report changed'):
                history.check_published_reports(source, published, ('result.json',))

    def test_even_correctly_fingerprinted_unsafe_archive_refuses(self):
        for name, kind in (('../escape', tarfile.REGTYPE), ('tools/link', tarfile.SYMTYPE)):
            with self.subTest(name=name), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                output = io.BytesIO()
                with tarfile.open(fileobj=output, mode='w:gz') as archive:
                    member = tarfile.TarInfo(name)
                    member.type = kind
                    member.linkname = '/tmp/escape'
                    archive.addfile(member, io.BytesIO(b''))
                path, metadata = root / 'source.tar.gz', root / 'manifest.json'
                path.write_bytes(output.getvalue())
                identity = history.digest(output.getvalue())
                metadata.write_text(json.dumps(history.manifest(identity)))
                with self.assertRaisesRegex(ValueError, 'unsafe frozen source'):
                    history.audit(path, metadata, identity, root)


if __name__ == '__main__':
    unittest.main()
