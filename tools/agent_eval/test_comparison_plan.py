"""Preparation must not silently become a paid study or hide missing attempts."""
import copy
from collections import Counter
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval.comparison_plan import HOSTS, ROOT, SETUPS, TASKS, prepare, settings, verify
from agent_eval.study import load


def explicit_settings():
    # Deliberately fictitious: this fixture is not a suggested live configuration.
    return {"spend_cap_usd": 18, "attempt_cap_usd": 1, "applications": {
        host: {"provider": "fixture", "model": "fixture-exact-model", "reasoning": {"effort": "medium"},
               "pricing": {"source": "https://example.invalid/fixture-prices", "checked_at": "2026-10-22",
                           "usd_per_million_tokens": {"uncached_input": 1, "cache_read": 0.1,
                                                      "cache_write": 2, "output": 3}}}
        for host in HOSTS}}


class ComparisonPlan(unittest.TestCase):
    def test_complete_first_attempt_matrix_with_balanced_order_and_unknown_costs(self):
        plan = prepare(ROOT)
        self.assertFalse(plan["live_calls_enabled"])
        attempts = plan["attempts"]
        self.assertEqual(len(attempts), 18)
        self.assertEqual(len({row["id"] for row in attempts}), 18)
        self.assertEqual({(r["host"], r["task"], r["setup"]) for r in attempts},
                         {(h, t, s) for h in HOSTS for t in TASKS for s in SETUPS})
        self.assertEqual(Counter((r["setup"], r["position"]) for r in attempts),
                         Counter({(s, p): 2 for s in SETUPS for p in (1, 2, 3)}))
        for row in attempts:
            self.assertEqual(row["status"], "unstarted")
            for key in ("outcome", "usage", "cost_usd", "tool_used"):
                self.assertIsNone(row[key])
        old = load(ROOT / "tests/agent-eval/unknown-target/task.json")["budgets"]
        self.assertEqual(plan["limits"], {key: old[key] for key in plan["limits"]})
        self.assertNotIn("max_live_attempts", plan["limits"])
        self.assertEqual(verify(plan, ROOT), plan)

    def test_first_failure_cannot_be_overwritten_by_a_success_in_preparation(self):
        plan = prepare(ROOT)
        for change in ({"status": "passed"}, {"cost_usd": 0}, {"tool_used": False}):
            altered = copy.deepcopy(plan)
            altered["attempts"][0].update(change)
            with self.subTest(change=change), self.assertRaises(ValueError):
                verify(altered, ROOT)

    def test_changed_grader_task_client_or_skill_requires_new_review(self):
        original = prepare(ROOT)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for relative in original["bindings"]:
                target = root / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes((ROOT / relative).read_bytes())
            self.assertEqual(verify(original, root), original)
            for relative in ("tools/agent_eval/oracle.py", "tests/agent-eval/unknown-target/task.json",
                             "tests/agent-eval/agent-first-use/hosts.json", "skills/fr/SKILL.md"):
                target = root / relative
                previous = target.read_bytes()
                target.write_bytes(previous + b"\n")
                with self.subTest(path=relative), self.assertRaises(ValueError):
                    verify(original, root)
                target.write_bytes(previous)
            (root / "skills/fr/new-instructions.md").write_text("New instructions")
            with self.assertRaises(ValueError):
                verify(original, root)

    def test_all_settings_are_explicit_and_shared_within_each_application(self):
        value = explicit_settings()
        self.assertEqual(settings(value), value)
        for key in ("provider", "model", "reasoning", "pricing"):
            incomplete = copy.deepcopy(value)
            del incomplete["applications"]["codex"][key]
            with self.subTest(key=key), self.assertRaises(ValueError):
                settings(incomplete)
        for invalid in ({}, {"files": value["applications"]["codex"]}):
            altered = copy.deepcopy(value)
            altered["applications"]["codex"] = invalid
            with self.assertRaises(ValueError):
                settings(altered)

    def test_missing_prices_and_invalid_caps_never_become_free_attempts(self):
        for amount in (0, -1, True, float("nan"), float("inf"), 19):
            value = explicit_settings()
            value["attempt_cap_usd"] = amount
            with self.subTest(amount=amount), self.assertRaises(ValueError):
                settings(value)
        value = explicit_settings()
        del value["applications"]["claude"]["pricing"]["usd_per_million_tokens"]["cache_write"]
        with self.assertRaises(ValueError):
            settings(value)
        value = explicit_settings()
        value["applications"]["opencode"]["reasoning"] = {}
        with self.assertRaises(ValueError):
            settings(value)

    def test_duplicate_keys_and_nonfinite_json_fail_before_review(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "settings.json"
            for source in ('{"spend_cap_usd":1,"spend_cap_usd":2}', '{"spend_cap_usd":NaN}'):
                path.write_text(source)
                with self.subTest(source=source), self.assertRaises(ValueError):
                    load(path)


if __name__ == "__main__":
    unittest.main()
