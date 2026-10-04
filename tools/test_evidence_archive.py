"""Lossless restoration, bounded decoding and refusal of altered historical evidence."""
import contextlib
import copy
import gzip
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import evidence_archive as archive

SPEC = importlib.util.spec_from_file_location("archive_cli", Path(__file__).with_name("evidence-archive.py"))
CLI = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CLI)


def fixture(root, raw=b'{"retained": "exact bytes"}\n'):
    packed = gzip.compress(raw, mtime=0)
    (root / "result.json.gz").write_bytes(packed)
    entry = {"path": "result.json", "archive": "result.json.gz", "bytes": len(raw),
             "sha256": hashlib.sha256(raw).hexdigest(),
             "git_blob": hashlib.sha1(b"blob " + str(len(raw)).encode() + b"\0" + raw).hexdigest(),
             "archive_bytes": len(packed), "archive_sha256": hashlib.sha256(packed).hexdigest()}
    value = {"schema": "fr-evidence-archives-1", "source_revision": "a" * 40, "entries": [entry]}
    (root / "catalog.json").write_text(json.dumps(value))
    return value


class Archives(unittest.TestCase):
    def test_restore_preserves_original_bytes_and_refuses_existing_destination(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            raw = b'{ "unicode": "\xce\xba", "spaces": [ 1, 2 ] }\r\n'
            entry = fixture(root, raw)["entries"][0]
            self.assertEqual(archive.transfer(root, entry), len(raw))
            destination = root / "restored.json"
            archive.restore(root, entry, destination)
            self.assertEqual(destination.read_bytes(), raw)
            with self.assertRaisesRegex(ValueError, "already exists"):
                archive.restore(root, entry, destination)
            self.assertEqual(destination.read_bytes(), raw)
            self.assertFalse(list(root.glob(".fr-evidence-*")))

    def test_altered_compressed_or_original_identities_leave_no_partial_restore(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            original = fixture(root)["entries"][0]
            for field in ("archive_sha256", "sha256", "git_blob"):
                entry = {**original, field: "0" * len(original[field])}
                with self.subTest(field=field), self.assertRaises(ValueError):
                    archive.restore(root, entry, root / "restored.json")
                self.assertFalse((root / "restored.json").exists())
                self.assertFalse(list(root.glob(".fr-evidence-*")))
            changed = gzip.compress(b'{"retained": "changed"}\n', mtime=0)
            (root / original["archive"]).write_bytes(changed)
            entry = {**original, "archive_bytes": len(changed), "archive_sha256": hashlib.sha256(changed).hexdigest()}
            with self.assertRaises(ValueError):
                archive.transfer(root, entry)

    def test_decoding_is_bounded_by_declared_size_and_refuses_truncated_gzip(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            entry = fixture(root, b"x" * (archive.CHUNK + 123))["entries"][0]
            with self.assertRaisesRegex(ValueError, "exceeds declared size"):
                archive.transfer(root, {**entry, "bytes": 100}, io.BytesIO())
            with self.assertRaisesRegex(ValueError, "size differs"):
                archive.transfer(root, {**entry, "bytes": entry["bytes"] + 1})
            path = root / entry["archive"]
            truncated = path.read_bytes()[:-4]
            path.write_bytes(truncated)
            entry.update(archive_bytes=len(truncated), archive_sha256=hashlib.sha256(truncated).hexdigest())
            with self.assertRaises((EOFError, OSError)):
                archive.restore(root, entry, root / "restored.json")
            self.assertFalse((root / "restored.json").exists())

    def test_catalog_refuses_paths_duplicates_sizes_and_malformed_identities(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            original = fixture(root)
            self.assertEqual(archive.catalog(root, root / "catalog.json"), original)
            bad = [("path", "../result.json"), ("path", "/result.json"), ("path", "a/../result.json"),
                   ("archive", "different.gz"), ("bytes", True), ("bytes", archive.MAX_ORIGINAL + 1),
                   ("archive_bytes", archive.MAX_ARCHIVE + 1), ("sha256", "bad"), ("git_blob", "f" * 64)]
            for key, value in bad:
                changed = copy.deepcopy(original)
                changed["entries"][0][key] = value
                (root / "catalog.json").write_text(json.dumps(changed))
                with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                    archive.catalog(root, root / "catalog.json")
            original["entries"] *= 2
            (root / "catalog.json").write_text(json.dumps(original))
            with self.assertRaisesRegex(ValueError, "duplicate"):
                archive.catalog(root, root / "catalog.json")

    def test_symlink_archive_and_escaping_parent_refuse(self):
        with tempfile.TemporaryDirectory() as temporary, tempfile.TemporaryDirectory() as outside:
            root = Path(temporary)
            entry = fixture(root)["entries"][0]
            path = root / entry["archive"]
            other = Path(outside) / path.name
            path.replace(other)
            path.symlink_to(other)
            with self.assertRaisesRegex(ValueError, "escapes"):
                archive.transfer(root, entry)
            (root / "outside").symlink_to(outside, target_is_directory=True)
            with self.assertRaisesRegex(ValueError, "escapes"):
                archive.safe(root, "outside/result.json.gz")

    def test_cli_checks_and_restores_only_registered_archives(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            fixture(root)
            common = ["evidence-archive.py", "--catalog", str(root / "catalog.json")]
            with patch.object(CLI, "ROOT", root), patch("sys.argv", common + ["check"]), contextlib.redirect_stdout(io.StringIO()) as output:
                CLI.main()
            self.assertEqual(json.loads(output.getvalue())["verified"], 1)
            with patch.object(CLI, "ROOT", root), patch("sys.argv", common + ["restore", "absent.json", str(root / "out.json")]):
                with self.assertRaisesRegex(ValueError, "not registered"):
                    CLI.main()
            (root / "result.json").write_bytes(b"unexpected second copy")
            with patch.object(CLI, "ROOT", root), patch("sys.argv", common + ["check"]):
                with self.assertRaisesRegex(ValueError, "both archived and original"):
                    CLI.main()


if __name__ == "__main__":
    unittest.main()
