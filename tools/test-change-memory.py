"""Replay original change admission failures without starting a client."""
import hashlib
import json
from pathlib import Path
import runpy
import tempfile
import unittest
import zipfile
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
HERE = ROOT / "tests/agent-eval/opencode/changes/2026-10-08-memory"


class Memory(unittest.TestCase):
    def test_original_platform_evidence_keeps_failed_admission(self):
        checker = runpy.run_path(str(ROOT / "tools/check-terminal-changes.py"))
        provenance = json.loads((HERE / "provenance.json").read_bytes())
        self.assertFalse(provenance["admitted"])
        results = {}
        for artifact in provenance["artifacts"]:
            path = HERE / (artifact["platform"] + ".zip")
            self.assertEqual(hashlib.sha256(path.read_bytes()).hexdigest(), artifact["sha256"])
            with tempfile.TemporaryDirectory() as temporary, zipfile.ZipFile(path) as archive:
                root = Path(temporary).resolve()
                self.assertLess(sum(i.file_size for i in archive.infolist()), 8 * 1024**2)
                self.assertTrue(all((root / n).resolve().is_relative_to(root) for n in archive.namelist()))
                archive.extractall(root)
                saved = json.loads((root / "result.json").read_bytes())
                with patch.object(checker["runner"], "capture", side_effect=AssertionError("offline only")):
                    actual = [checker["report"](root / str(i)) for i in range(len(saved))]
                self.assertEqual(actual, saved)
                results[artifact["platform"]] = actual
        self.assertEqual(len(results["ubuntu-latest"]), 5)
        self.assertTrue(all(r["headroom_admitted"] for r in results["ubuntu-latest"]))
        mac = results["macos-14"]
        self.assertEqual(len(mac), 2)
        self.assertEqual([r["record"]["status"] for r in mac], ["completed", "completed"])
        self.assertEqual([r["headroom_admitted"] for r in mac], [True, False])
        self.assertGreater(mac[1]["process"]["sampled_aggregate_rss_bytes"], 640 * 1024**2)
        self.assertLess(mac[1]["process"]["sampled_aggregate_rss_bytes"], 768 * 1024**2)


if __name__ == "__main__":
    unittest.main()
