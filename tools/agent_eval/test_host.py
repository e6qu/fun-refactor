"""Provider and host regressions using synthetic responses and small subprocesses."""
import copy
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval.bounded_host import run
from agent_eval.provider_usage import normalize, verify
from agent_eval.study import plan
from agent_eval.study_budget import Budget
from agent_eval.test_study import manifest
from agent_eval import test_study


def response():
    return {"object": "response", "id": "resp-1", "model": "model-a",
            "usage": {"input_tokens": 350, "input_tokens_details": {"cached_tokens": 200, "cache_write_tokens": 50},
                      "output_tokens": 10, "output_tokens_details": {"reasoning_tokens": 3}, "total_tokens": 360}}


class ProviderUsage(unittest.TestCase):
    def test_extra_priced_modalities_and_hosted_tools_remain_unpriced(self):
        raw = response()
        raw["tools"] = [{"type": "web_search"}]
        self.assertFalse(normalize("openai-response-1", raw, "model-a")["billable_complete"])
        raw.pop("tools")
        raw["usage"]["input_tokens_details"]["audio_tokens"] = 1
        self.assertFalse(normalize("openai-response-1", raw, "model-a")["billable_complete"])

    def test_disjoint_openai_input_and_reasoning_subset(self):
        observed = normalize("openai-response-1", response(), "model-a")
        self.assertEqual(observed["tokens"], dict(uncached_input=100, cache_read=200, cache_write=50, output=10, reasoning=3))
        self.assertEqual(observed["total_tokens"], 360)
        self.assertTrue(observed["billable_complete"])

    def test_missing_cache_writes_are_unknown(self):
        raw = response()
        del raw["usage"]["input_tokens_details"]["cache_write_tokens"]
        result = normalize("openai-response-1", raw, "model-a")
        self.assertIsNone(result["tokens"]["uncached_input"])
        self.assertFalse(result["billable_complete"])

    def test_codex_totals_do_not_invent_model_observation_or_cache_writes(self):
        raw = {"type": "turn.completed", "thread_id": "x", "turn_index": 0, "model": "model-a",
               "usage": {"input_tokens": 350, "cached_input_tokens": 200, "output_tokens": 10}}
        result = normalize("codex-turn-1", raw, "model-a")
        self.assertEqual(result["total_tokens"], 360)
        self.assertFalse(result["model_observed"])
        self.assertFalse(result["billable_complete"])

    def test_anthropic_input_is_already_uncached(self):
        raw = {"type": "message", "id": "msg-1", "model": "model-a", "usage": {
            "input_tokens": 100, "cache_read_input_tokens": 200, "cache_creation_input_tokens": 50, "output_tokens": 10}}
        result = normalize("anthropic-message-1", raw, "model-a")
        self.assertEqual(result["tokens"]["uncached_input"], 100)
        self.assertEqual(result["total_tokens"], 360)
        self.assertTrue(result["billable_complete"])
        for extra in ({"cache_creation": {"ephemeral_1h_input_tokens": 50}}, {"server_tool_use": {"web_search_requests": 1}}):
            raw["usage"].update(extra)
            self.assertFalse(normalize("anthropic-message-1", raw, "model-a")["pricing_compatible"])

    def test_malformed_and_inconsistent_counts_refuse(self):
        for change in ({"input_tokens": 10}, {"output_tokens": True}, {"total_tokens": 359},
                       {"input_tokens_details": []}, {"output_tokens": 2}):
            raw = response()
            raw["usage"].update(change)
            if change == {"input_tokens_details": []}:
                raw["usage"]["input_tokens_details"] = [1]
            with self.subTest(change=change), self.assertRaises(ValueError):
                normalize("openai-response-1", raw, "model-a")
        with self.assertRaisesRegex(ValueError, "model"):
            normalize("openai-response-1", response(), "substituted")

    def test_raw_json_verification_rejects_tampering_and_duplicate_keys(self):
        observed = normalize("openai-response-1", response(), "model-a")
        invocation = {"format": "openai-response-1", "id": observed["id"], "tokens": observed["tokens"]}
        profile = {"model": "model-a", "provider": "openai"}
        verify(invocation, json.dumps(response()), profile)
        invocation["tokens"]["output"] = 9
        with self.assertRaisesRegex(ValueError, "counters"):
            verify(invocation, json.dumps(response()), profile)
        with self.assertRaisesRegex(ValueError, "duplicate"):
            verify(invocation, '{"id":"a","id":"b"}', profile)

    def test_auditor_recomputes_counters(self):
        fixture = test_study.StudyTests()
        fixture.setUp()
        try:
            value = manifest()
            for model in value["models"]:
                model["provider"] = "openai"
            fixture.frozen = plan(value)
            record = fixture.record()
            raw = response()
            raw["model"] = record["agents"][0]["model"]
            invocation = record["agents"][0]["invocations"][0]
            invocation.update(id=raw["id"], format="openai-response-1", raw_usage=fixture.artifact("provider.json", json.dumps(raw).encode()))
            self.assertTrue(fixture.audit(record)["provider_usage_verified"])
            invocation["tokens"]["output"] = 9
            with self.assertRaisesRegex(ValueError, "counters"):
                fixture.audit(record)
        finally:
            fixture.doCleanups()


class HostBudget(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        value = manifest()
        for model in value["models"]:
            model["provider"] = "openai"
        self.frozen = plan(value)
        self.cell = self.frozen["cells"][0]["id"]
        self.ledger = Budget(self.root / "budget.db", self.frozen)
        self.ledger.begin(self.cell)

    def reserve(self, identity="request"):
        self.ledger.reserve(self.cell, identity, "parent", 400, 20)
        self.ledger.dispatch(identity)

    def test_cli_workflow_reserves_dispatches_and_settles(self):
        frozen = self.root / "plan.json"
        frozen.write_text(json.dumps(self.frozen))
        request = self.root / "action.json"
        raw_path = self.root / "response.json"
        raw = response()
        raw["model"] = self.ledger.models[self.ledger.cells[self.cell]["model"]]["model"]
        raw_path.write_text(json.dumps(raw))
        actions = [{"action": "reserve", "cell": self.cell, "request": "cli", "agent": "child", "input_limit": 400, "output_limit": 20},
                   {"action": "dispatch", "request": "cli"},
                   {"action": "settle", "request": "cli", "format": "openai-response-1", "response": str(raw_path)},
                   {"action": "finish", "cell": self.cell}]
        for action in actions:
            request.write_text(json.dumps(action))
            result = subprocess.run([sys.executable, "tools/agent-eval-host.py", "budget", str(frozen),
                                     str(self.root / "budget.db"), str(request)], capture_output=True, text=True, timeout=10)
            self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.ledger.snapshot()["attempts"][0]["state"], "closed")
        self.assertEqual(self.ledger.snapshot()["calls"][0]["spent_nano_usd"], 465000)

    def test_strict_report_refuses_host_supplied_usage(self):
        fixture = test_study.StudyTests()
        fixture.setUp()
        try:
            fixture.save(fixture.record())
            frozen = self.root / "legacy-plan.json"
            frozen.write_text(json.dumps(fixture.frozen))
            command = [sys.executable, "tools/agent-eval-study.py", "report", str(frozen), str(fixture.root)]
            result = subprocess.run(command + ["--require-provider-usage"], capture_output=True, text=True, timeout=10)
            self.assertEqual(result.returncode, 1)
            self.assertIn("verified provider usage", result.stderr)
            self.assertEqual(json.loads(result.stdout)["executed"], 1)
        finally:
            fixture.doCleanups()

    def test_resume_and_unknown_charge_block_all_agents(self):
        self.reserve()
        self.ledger.unknown("request")
        resumed = Budget(self.root / "budget.db", self.frozen)
        with self.assertRaisesRegex(ValueError, "unknown charge"):
            resumed.reserve(self.cell, "child", "child", 1, 1)
        with self.assertRaises(ValueError):
            resumed.cancel("request")
        resumed.settle("request", 0.000465, 360, "resp-1")
        resumed.finish(self.cell)
        self.assertEqual(resumed.snapshot()["calls"][0]["spent_nano_usd"], 465000)

    def test_overrun_is_retained_and_never_releases_the_hold(self):
        self.reserve()
        self.assertEqual(self.ledger.settle("request", 2, 360, "response"), "overrun")
        self.assertEqual(self.ledger.snapshot()["calls"][0]["spent_nano_usd"], 2_000_000_000)
        with self.assertRaisesRegex(ValueError, "overrun"):
            self.ledger.reserve(self.cell, "next", "parent", 1, 1)
        with self.assertRaises(ValueError):
            self.ledger.finish(self.cell)

    def test_actual_provider_settlement_and_missing_usage(self):
        self.reserve()
        raw = response()
        raw["model"] = self.ledger.models[self.ledger.cells[self.cell]["model"]]["model"]
        del raw["usage"]["input_tokens_details"]["cache_write_tokens"]
        self.assertEqual(self.ledger.settle_response("request", "openai-response-1", raw), "unknown")
        raw["usage"]["input_tokens_details"]["cache_write_tokens"] = 50
        self.assertEqual(self.ledger.settle_response("request", "openai-response-1", raw), "settled")
        self.assertEqual(self.ledger.snapshot()["calls"][0]["spent_nano_usd"], 465000)

    def test_malformed_response_leaves_charge_unknown(self):
        self.reserve()
        with self.assertRaises(ValueError):
            self.ledger.settle_response("request", "invalid", response())
        self.assertEqual(self.ledger.snapshot()["calls"][0]["state"], "unknown")

    def test_attempt_holds_and_no_silent_retry(self):
        cells = self.frozen["cells"]
        for cell in cells[1:10]:
            self.ledger.begin(cell["id"])
        with self.assertRaisesRegex(ValueError, "spend cap"):
            self.ledger.begin(cells[10]["id"])
        with self.assertRaisesRegex(ValueError, "already admitted"):
            self.ledger.begin(self.cell)
        self.ledger.finish(self.cell)
        self.ledger.begin(cells[10]["id"])

    def test_cross_process_requests_share_token_limit(self):
        path = self.root / "plan.json"
        path.write_text(json.dumps(self.frozen))
        script = ('import sys; from pathlib import Path; sys.path.insert(0,"tools"); '
                  'from agent_eval.study import load; from agent_eval.study_budget import Budget; '
                  'b=Budget(sys.argv[1],load(Path(sys.argv[2]))); '
                  'b.reserve(sys.argv[3],sys.argv[4],sys.argv[4],6000,0)')
        processes = [subprocess.Popen([sys.executable, "-c", script, str(self.root / "budget.db"), str(path), self.cell, name],
                                      stdout=subprocess.PIPE, stderr=subprocess.PIPE) for name in ("parent", "child")]
        codes = []
        for process in processes:
            _, error = process.communicate(timeout=10)
            codes.append(process.returncode)
            if process.returncode:
                self.assertIn(b"aggregate token cap", error)
        self.assertEqual(sorted(codes), [0, 1])
        self.assertEqual(len(self.ledger.snapshot()["calls"]), 1)

    def test_dispatch_once_cancel_only_before_dispatch_and_plan_binding(self):
        self.ledger.reserve(self.cell, "unused", "parent", 1, 1)
        self.ledger.cancel("unused")
        with self.assertRaises(ValueError):
            self.ledger.dispatch("unused")
        self.reserve()
        with self.assertRaises(ValueError):
            self.ledger.dispatch("request")
        changed = copy.deepcopy(self.frozen["manifest"])
        changed["seed"] += 1
        with self.assertRaisesRegex(ValueError, "another frozen plan"):
            Budget(self.root / "budget.db", plan(changed))


class BoundedHost(unittest.TestCase):
    def execute(self, code, **limits):
        with tempfile.TemporaryDirectory() as directory:
            out, err = io.BytesIO(), io.BytesIO()
            result = run([sys.executable, "-c", code], b"", out, err, Path(directory),
                         wall_seconds=limits.pop("wall_seconds", 3), **limits)
            return result, out.getvalue()

    def test_success_and_capped_output(self):
        result, data = self.execute('print("retained")')
        self.assertEqual(result["exit_code"], 0)
        self.assertIsNone(result["stop_reason"])
        self.assertEqual(data, b"retained\n")
        result, data = self.execute('print("x"*10000)', transcript_bytes=100)
        self.assertEqual(result["stop_reason"], "transcript_bytes")
        self.assertNotEqual(result["exit_code"], 0)
        self.assertEqual(len(data), 100)

    def test_timeout_kills_child_even_when_parent_already_exits(self):
        for parent in ('time.sleep(5)', 'pass'):
            with self.subTest(parent=parent), tempfile.TemporaryDirectory() as directory:
                sentinel = str(Path(directory) / "survived")
                child = f'import time; from pathlib import Path; time.sleep(0.8); Path({sentinel!r}).touch()'
                code = f'import subprocess,sys,time; subprocess.Popen([sys.executable,"-c",{child!r}]); {parent}'
                result, _ = self.execute(code, wall_seconds=0.3)
                if parent != 'pass':
                    self.assertEqual(result["stop_reason"], "wall_seconds")
                subprocess.run([sys.executable, "-c", "import time; time.sleep(0.9)"], check=True, timeout=2)
                self.assertFalse(Path(sentinel).exists())

    def test_monitor_failure_closes_group(self):
        with patch("agent_eval.bounded_host.sample", side_effect=OSError("ps unavailable")):
            result, _ = self.execute('import time; time.sleep(5)')
        self.assertEqual(result["stop_reason"], "monitor_error")
        self.assertIn("ps unavailable", result["monitor_error"])

    def test_cpu_limit_counts_samples_from_departed_children(self):
        with patch("agent_eval.bounded_host.sample", side_effect=[(100, {1: 0.6}), (100, {2: 0.6})]):
            result, _ = self.execute('import time; time.sleep(5)', cpu_limit_seconds=1)
        self.assertEqual(result["stop_reason"], "cpu_seconds")
        self.assertEqual(result["sampled_cpu_seconds"], 1.2)

    def test_memory_and_disk_limits(self):
        result, _ = self.execute('import time; x=bytearray(2*1024*1024); time.sleep(2)', rss_bytes=1024*1024)
        self.assertEqual(result["stop_reason"], "rss_bytes")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            command = [sys.executable, "-c", f'from pathlib import Path; import time; Path({str(root / "growth")!r}).write_bytes(b"x"*4096); time.sleep(2)']
            result = run(command, b"", io.BytesIO(), io.BytesIO(), root, wall_seconds=3, disk_bytes=1024)
            self.assertEqual(result["stop_reason"], "disk_bytes")


if __name__ == "__main__":
    unittest.main()
