"""Audit retained review attempts without model calls or candidate execution."""
import hashlib
import json
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval.study import digest, load

ROOT = Path(__file__).resolve().parents[2]
HERE = ROOT / "tests/agent-eval/opencode/reviews/2026-10-03-candidates"
PACK = ROOT / "tests/agent-eval/opencode/candidates"


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


class CandidateReviewEvidence(unittest.TestCase):
    def test_frozen_inputs_and_original_graders(self):
        envelope = load(HERE / "plan.json")
        plan = envelope["plan"]
        self.assertEqual(digest(plan), envelope["sha256"])
        self.assertEqual(sha(HERE / "collect.py"), plan["collector_sha256"])
        for name, expected in plan["inputs"].items():
            self.assertEqual(sha(HERE / name), expected)
        public = load(PACK / "public-checks.json")
        for task in plan["tasks"]:
            baseline = PACK / "review-baseline" / (task["id"] + "-grader.json")
            self.assertEqual(sha(baseline), task["grader_sha256"])
            self.assertEqual(digest(public[task["id"]]), task["public_sha256"])
        provenance = load(HERE / "public-input-provenance.json")
        self.assertEqual(provenance["revision"], plan["baseline"])
        for source in provenance["sources"]:
            path = ROOT / source["path"]
            if path.name.endswith("-grader.json"):
                path = PACK / "review-baseline" / path.name
            self.assertEqual(sha(path), source["sha256"])
            self.assertEqual(source["url"], "https://github.com/e6qu/fun-refactor/blob/"
                             + plan["baseline"] + "/" + source["path"])

    def test_every_retained_byte_matches_inventory(self):
        inventory = load(HERE / "evidence-sha256.json")
        actual = {str(path.relative_to(HERE)) for path in HERE.rglob("*") if path.is_file()
                  and path.name not in {"README.md", "evidence-sha256.json"}
                  and "__pycache__" not in path.parts}
        self.assertEqual(actual, set(inventory))
        for name, expected in inventory.items():
            self.assertEqual(sha(HERE / name), expected, name)

    def test_timeouts_are_not_completed_reviews_or_zero_cost_claims(self):
        envelope = load(HERE / "plan.json")
        plan = envelope["plan"]
        planned = {(task["id"], model) for task in plan["tasks"] for model in plan["models"]}
        attempted = set()
        cpu, wall, peak = 0, 0, 0
        for directory in sorted((HERE / "attempts").iterdir()):
            manifest = load(directory / "manifest.json")
            self.assertEqual(set(manifest), {p.name for p in directory.iterdir()
                                            if p.name != "manifest.json"})
            for name, expected in manifest.items():
                self.assertEqual(sha(directory / name), expected)
            record = load(directory / "record.json")
            cell = (record["task"], record["model"])
            self.assertIn(cell, planned)
            self.assertNotIn(cell, attempted)
            attempted.add(cell)
            self.assertEqual(record["plan_sha256"], envelope["sha256"])
            self.assertEqual(record["status"], "failed")
            self.assertEqual(record["failure"], "review failed: wall_seconds")
            self.assertNotIn("review", record)
            self.assertFalse(record["provider_usage_verified"])
            self.assertFalse(record["complete_context_accounting"])
            self.assertEqual([p["name"] for p in record["processes"]], ["version", "review"])
            process = record["processes"][-1]
            self.assertEqual(process["stop_reason"], "wall_seconds")
            self.assertEqual(process["exit_code"], 124)
            cpu += sum(p["sampled_cpu_seconds"] for p in record["processes"])
            wall += record["wall_seconds"]
            peak = max(peak, *(p["sampled_aggregate_rss_bytes"] for p in record["processes"]))
            events = [json.loads(line) for line in (directory / "review.stdout").read_text().splitlines()]
            self.assertEqual([event["type"] for event in events], ["step_start"])
            exported = load(HERE / "diagnostics" / (directory.name + ".stdout"))
            session = events[0]["sessionID"]
            self.assertEqual(exported["info"]["id"], session)
            assistants = [m for m in exported["messages"] if m["info"]["role"] == "assistant"]
            self.assertEqual(len(assistants), 1)
            info = assistants[0]["info"]
            self.assertEqual(info["sessionID"], session)
            self.assertEqual(info["id"], events[0]["part"]["messageID"])
            self.assertEqual(info["providerID"] + "/" + info["modelID"], record["model"])
            self.assertNotIn("finish", info)
            self.assertNotIn("error", info)
            self.assertFalse(any(p.get("text") for p in assistants[0]["parts"] if p["type"] == "text"))
            self.assertFalse(any(p["type"] == "tool" for m in exported["messages"] for p in m["parts"]))
        self.assertEqual(len(planned), 6)
        self.assertEqual(attempted, {("dotenv-alternate", model) for model in plan["models"]})
        self.assertEqual(len(planned - attempted), 4)
        self.assertAlmostEqual(cpu, 9.86)
        self.assertAlmostEqual(wall, 240.11310049996246)
        self.assertEqual(peak, 685457408)


if __name__ == "__main__":
    unittest.main()
