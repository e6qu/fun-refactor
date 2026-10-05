"""Before/after context must distinguish file bytes, mode and behavior claims."""
import base64
import copy
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from agent_eval import source_packets as packets


def entry(raw, executable=False):
    return {"data": base64.b64encode(raw).decode(), "executable": executable}


class SourceComparison(unittest.TestCase):
    def test_exact_bytes_modes_and_shared_snippets_have_distinct_results(self):
        files = {"old/a.py": entry("value = 'α'\n".encode()),
                 "new/a.py": entry("value = 'α'\n".encode()),
                 "new/b.py": entry("value = 'α'\nother = 1\n".encode()),
                 "new/c.py": entry("value = 'α'\n".encode(), True)}
        result = packets.compare(files, [{"before": "old/a.py", "after": p}
                                        for p in ("new/a.py", "new/b.py", "new/c.py")])
        self.assertEqual([(r["same_content"], r["same_executable"]) for r in result["pairs"]],
                         [(True, True), (False, True), (True, False)])
        self.assertEqual(result["pairs"][0]["before"]["bytes"], 13)
        self.assertIn("do not establish equal behavior", result["scope"])
        self.assertEqual(packets.check_comparison(files, result), result)
        for side in ("before", "after"):
            changed = copy.deepcopy(files)
            changed[result["pairs"][0][side]["path"]] = entry(b"changed\n")
            with self.assertRaisesRegex(ValueError, "differs from frozen"):
                packets.check_comparison(changed, result)

    def test_forged_identity_flags_and_unbounded_context_are_rejected(self):
        files = {"a": entry(b"a"), "b": entry(b"b")}
        pair = {"before": "a", "after": "b"}
        original = packets.compare(files, [pair])
        for field, value in (("sha256", "0" * 64), ("bytes", 99), ("executable", True)):
            forged = copy.deepcopy(original)
            forged["pairs"][0]["after"][field] = value
            with self.assertRaises(ValueError):
                packets.check_comparison(files, forged)
        forged = copy.deepcopy(original)
        forged["pairs"][0]["same_content"] = True
        with self.assertRaises(ValueError):
            packets.check_comparison(files, forged)
        for pairs in ([], [pair] * 17, [pair, pair], [{"before": "a", "after": "missing"}],
                      [{"before": "a", "after": "a"}], [{"before": "a"}]):
            with self.assertRaises(ValueError):
                packets.compare(files, pairs)
        files["../escape"] = entry(b"x")
        with self.assertRaises(ValueError):
            packets.compare(files, [pair])

    def test_numeric_substitutes_for_boolean_evidence_are_rejected(self):
        files = {"before": entry(b"same"), "after": entry(b"same")}
        result = packets.compare(files, [{"before": "before", "after": "after"}])
        result["pairs"][0]["same_content"] = 1
        with self.assertRaisesRegex(ValueError, "differs from frozen"):
            packets.check_comparison(files, result)


if __name__ == "__main__":
    unittest.main()
