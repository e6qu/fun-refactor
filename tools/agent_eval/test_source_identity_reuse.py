"""Exact duplicate files add measurable rereads even when their paths differ."""
import base64
import io
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from agent_eval import native_costs, native_mcp, source_reviews
from agent_eval.study import encode, load


def files(original=b"0123456789", changed=b"012345678X"):
    return {path: {"data": base64.b64encode(raw).decode(), "executable": False}
            for path, raw in (("before/a.py", original), ("after/a.py", original),
                              ("changed/a.py", changed))}


def log_reads(source, requests):
    log = io.BytesIO()
    server = native_mcp.Server({"files": source, "arm": "files", "binary": "fr",
                               "workspace": "/unused", "tools_schema_version": 6}, log)
    for path, offset, size in requests:
        server.call({"name": "read_source", "arguments": {
            "path": path, "offset": offset, "bytes": size, "sha256": ""}})
    return log.getvalue()


class SourceIdentityReuse(unittest.TestCase):
    def test_overlapping_aliases_preserve_path_counts_and_exclude_changed_files(self):
        source = files()
        host = log_reads(source, [("before/a.py", 0, 8), ("after/a.py", 4, 6),
                                  ("after/a.py", 0, 6), ("changed/a.py", 0, 8)])
        result = native_costs.read_identity_reuse(b"", host, {"files": source}, "files", 6)
        self.assertEqual(result["observed"], {"source_bytes": 28, "same_path_reread_bytes": 2,
            "identical_file_reread_bytes": 10, "additional_cross_path_reread_bytes": 8})
        self.assertEqual(result["reads"][-1]["identical_file_reread_bytes"], 0)
        self.assertFalse(result["all_host_results_native_confirmed"])
        self.assertFalse(result["complete_context_accounting"])

    def test_unicode_uses_bytes_and_partial_tail_cannot_become_a_read(self):
        source = files("αβγ".encode())
        host = log_reads(source, [("before/a.py", 0, 3), ("after/a.py", 0, 4)])
        result = native_costs.read_identity_reuse(b"", host + b'{"sequence":', {"files": source}, "files", 6)
        self.assertEqual(result["observed"]["source_bytes"], 6)
        self.assertEqual(result["observed"]["additional_cross_path_reread_bytes"], 2)
        self.assertEqual(result["unparsed_host_tail_bytes"], 12)

    def test_error_results_add_no_source_and_forged_identity_refuses(self):
        source = files()
        host = log_reads(source, [("missing.py", 0, 8)])
        result = native_costs.read_identity_reuse(b"", host, {"files": source}, "files", 6)
        self.assertEqual(result["observed"]["source_bytes"], 0)
        rows, _ = native_costs.prefix(log_reads(source, [("before/a.py", 0, 8)]), native_mcp.MAX_LOG)
        rows[0]["result"]["sha256"] = "0" * 64
        rows[0]["response"]["content"][0]["text"] = encode(rows[0]["result"]).decode()
        with self.assertRaises(ValueError):
            native_costs.read_identity_reuse(b"", encode(rows[0]) + b"\n", {"files": source}, "files", 6)

    def test_retained_timeouts_stop_four_cells_and_reveal_exact_cross_path_reads(self):
        root = Path(__file__).resolve().parents[2]
        directory = root / "tests/agent-eval/opencode/reviews/2026-10-05-boundaries"
        frozen = load(directory / "plan.json")
        snapshots = source_reviews.read_inputs(directory)
        original = load(directory / "report.json")
        self.assertEqual(source_reviews.report(frozen, snapshots, directory / "attempts"), original)
        self.assertEqual((original["completed"], original["failed"], original["not_started"]), (0, 2, 4))
        with self.assertRaisesRegex(ValueError, "stop rule"):
            source_reviews.eligible(frozen["plan"], frozen["plan"]["cells"][2]["id"],
                                    directory / "attempts", frozen["sha256"])
        result = source_reviews.source_reuse(frozen, snapshots, directory / "attempts")
        self.assertEqual(result, load(directory / "source-reuse.json"))
        kimi = result["attempts"][0]["source_reuse"]
        self.assertTrue(kimi["all_host_results_native_confirmed"])
        self.assertEqual(kimi["observed"]["same_path_reread_bytes"], 0)
        self.assertEqual(kimi["observed"]["additional_cross_path_reread_bytes"], 4096)
        self.assertTrue(all(row["source_reuse"] is None for row in result["attempts"][2:]))


if __name__ == "__main__":
    unittest.main()
