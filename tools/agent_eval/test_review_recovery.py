"""Reference reviews preserve refusal evidence with the opted-in tool protocol."""
import copy
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from agent_eval import source_reviews as reviews
from agent_eval.study import digest, encode
from agent_eval.test_source_reviews import FILES, question, transcript


def frozen():
    return reviews.freeze_recovery_reference(
        [question()], ["provider/model"], Path(__file__), Path(__file__), {})


class RecoveryReviews(unittest.TestCase):
    def test_explicit_opt_in_preserves_limits_and_rejects_schema_substitution(self):
        current, snapshots = frozen()
        old, _ = reviews.freeze_reference(
            [question()], ["provider/model"], Path(__file__), Path(__file__), {})
        self.assertEqual(current["plan"]["limits"], old["plan"]["limits"])
        self.assertEqual(old["plan"]["tools_schema_version"], 4)
        self.assertEqual(current["plan"]["tools_schema_version"], 6)
        reviews.checked(current, snapshots, execution=True)
        for version in (4, 5, True, 6.0, "6"):
            altered = copy.deepcopy(current)
            altered["plan"]["tools_schema_version"] = version
            altered["sha256"] = digest(altered["plan"])
            with self.assertRaises(ValueError):
                reviews.checked(altered, snapshots)
        altered = copy.deepcopy(current)
        altered["plan"].update(schema=reviews.SCHEMA, reference_repairs_disclosed=False)
        altered["sha256"] = digest(altered["plan"])
        with self.assertRaisesRegex(ValueError, "reference review protocol"):
            reviews.checked(altered, snapshots)

    def test_real_collector_passes_version_and_replays_timeout_refusal_costs(self):
        current, snapshots = frozen()
        seen = []

        def timeout(command, data, out, err, directory, **kwargs):
            version = command[-1] == "--version"
            if version:
                out.write(b"test-version")
            else:
                config = reviews.load(Path(kwargs["cwd"]) / "server.json")
                seen.append(config["tools_schema_version"])
                with (directory / "tools.jsonl").open("wb") as log:
                    server = reviews.mcp.Server(config, log)
                    response = server.call({"name": "fr_explore", "arguments": {
                        "term": "value", "path": "module.py", "mode": "behavior"}})
                    result = json.loads(response["content"][0]["text"])
                    self.assertEqual(result["next"]["arguments"], {
                        "term": "value", "path": "module.py", "mode": "names"})
            return {"exit_code": 0 if version else -15,
                    "stop_reason": None if version else "wall",
                    "sampled_cpu_seconds": 0.01, "sampled_aggregate_rss_bytes": 4096}

        with tempfile.TemporaryDirectory() as temporary, patch.object(
                reviews.bounded_host, "run", side_effect=timeout):
            output = Path(temporary)
            record = reviews.collect(current, snapshots, current["plan"]["cells"][0]["id"],
                                     output, Path(__file__), Path(__file__))
            self.assertEqual(record["status"], "failed")
            result = reviews.report(current, snapshots, output)
            row = result["attempts"][0]
            self.assertEqual(seen, [6])
            self.assertEqual(row["costs"]["observed"]["refused_calls"], 1)
            self.assertEqual(row["costs"]["observed"]["source_page_bytes"], 0)
            self.assertGreater(row["costs"]["observed"]["produced_result_bytes"], 0)
            self.assertFalse(row["initial_packet_export_verified"])
            self.assertIsNone(row["review"])
            self.assertIsNone(row["actual_usd"])

    def test_completed_review_replays_current_tools_without_accepting_claims(self):
        current, snapshots = frozen()
        plan = current["plan"]
        task = {**plan["tasks"][0], "files": FILES}
        submitted = {"findings": [], "limitations": "Only this synthetic declaration was reviewed."}
        raw, exported, log = transcript(plan, task, submitted)
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            (directory / "opencode.stdout").write_bytes(raw)
            (directory / "export.stdout").write_bytes(encode(exported))
            (directory / "tools.jsonl").write_bytes(log)
            result = reviews.audit(plan, task, plan["cells"][0], directory)
        self.assertFalse(result["review"]["claims_verified"])
        self.assertEqual(result["review"]["findings"], [])


if __name__ == "__main__":
    unittest.main()
