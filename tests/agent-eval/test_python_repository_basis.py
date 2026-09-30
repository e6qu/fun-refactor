"""A release bump is metadata; other SDK changes still invalidate evidence."""
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "tools"))
from python_repository_basis import file_digest


class ReleaseBasisTests(unittest.TestCase):
    def test_only_the_known_release_literal_is_normalized(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "sdk/python/src/fr_ir/_version.py"
            path.parent.mkdir(parents=True)
            original = '"""Distribution version."""\n\nVERSION = "0.41.0"  # x-release-please-version\n'
            path.write_text(original)
            expected = file_digest(path)
            for version in ("0.41.1", "1.0.0", "1.0.0-rc.1"):
                path.write_text(original.replace("0.41.0", version))
                self.assertEqual(file_digest(path), expected)
            for changed in (original + "OTHER = 1\n", original.replace("Distribution", "Changed"),
                            original.replace('"0.41.0"', "compute_version()"),
                            original.replace("0.41.0", "not-a-release")):
                path.write_text(changed)
                self.assertNotEqual(file_digest(path), expected)

    def test_an_unrelated_version_file_keeps_its_exact_identity(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "_version.py"
            path.write_text('VERSION = "1.0.0"  # x-release-please-version\n')
            before = file_digest(path)
            path.write_text('VERSION = "2.0.0"  # x-release-please-version\n')
            self.assertNotEqual(file_digest(path), before)


if __name__ == "__main__":
    unittest.main()
