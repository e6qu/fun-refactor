"""Exercise request admission with fake providers; no credentials or network needed."""
import copy
from concurrent.futures import ThreadPoolExecutor
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval.request_gateway import HTTPS, admit_agent, prepare, send
from agent_eval.study import plan
from agent_eval.study_budget import Budget
from agent_eval.test_study import manifest
from agent_eval.test_host import response
from agent_eval import test_study


def api_manifest(provider="openai"):
    value = manifest()
    for model in value["models"]:
        model.update(provider=provider, harness="fr-study-api-1", settings={
            "request": {}, "max_output_tokens": 20, "input_token_ceiling": 1000,
            "input_bound_source": "Synthetic fixture ceiling; not a real model context bound"})
    return value


class Gateway(unittest.TestCase):
    def test_competing_parent_and_child_cannot_both_spend_the_same_budget(self):
        value = api_manifest()
        value["budgets"]["attempt_cap_usd"] = 0.0012
        frozen = plan(value)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            ledger = Budget(root / "ledger", frozen)
            cell = next(cell["id"] for cell in frozen["cells"] if cell["mode"] == "delegated")
            ledger.begin(cell)
            admit_agent(ledger, cell, "root", None)
            sends = []
            def transport(provider, path, payload, timeout):
                if path.endswith("input_tokens"):
                    return {"object": "response.input_tokens", "input_tokens": 350}
                sends.append(path)
                raw = response()
                raw["model"] = ledger.cells[cell]["model"]
                return raw
            def invoke(index):
                try:
                    return send(ledger, cell, "root" if index == 0 else "child", f"request-{index}",
                                {"input": "x"}, root / "evidence", parent=None if index == 0 else "root",
                                transport=transport)["state"]
                except ValueError as error:
                    self.assertIn("spend cap", str(error))
                    return "refused"
            with ThreadPoolExecutor(max_workers=2) as pool:
                self.assertEqual(sorted(pool.map(invoke, (0, 1))), ["refused", "settled"])
            self.assertEqual(len(sends), 1)

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.frozen = plan(api_manifest())
        self.ledger = Budget(self.root / "host.db", self.frozen)
        self.cell = next(row["id"] for row in self.frozen["cells"] if row["mode"] == "delegated")
        self.ledger.begin(self.cell)
        self.calls = []
        self.raw = response()
        self.raw["model"] = self.ledger.cells[self.cell]["model"]

    def transport(self, provider, path, payload, timeout):
        self.calls.append((provider, path, copy.deepcopy(payload)))
        if path.endswith("input_tokens"):
            return {"object": "response.input_tokens", "input_tokens": 350}
        self.assertEqual(self.ledger.snapshot()["calls"][-1]["state"], "dispatched")
        return self.raw

    def dispatch(self, identity="request", **kwargs):
        return send(self.ledger, self.cell, "parent", identity, {"input": "a bounded question"},
                    self.root / "evidence", transport=self.transport, **kwargs)

    def test_actual_send_is_reserved_and_export_matches_raw_auditor(self):
        result = self.dispatch()
        self.assertEqual(result["state"], "settled")
        self.assertEqual(len(self.calls), 2)
        request = self.calls[1][2]
        self.assertEqual(request["max_output_tokens"], 20)
        self.assertFalse(request["store"])
        self.assertFalse(request["stream"])
        self.assertEqual(self.ledger.snapshot()["calls"][0]["spent_nano_usd"], 465000)
        from agent_eval.provider_usage import verify
        artifact = self.root / "evidence" / result["invocation"]["raw_usage"]["path"]
        self.assertTrue(verify(result["invocation"], artifact.read_bytes(),
                               self.ledger.models[self.ledger.cells[self.cell]["model"]])["billable_complete"])
        self.assertEqual((artifact.stat().st_mode & 0o777), 0o600)

    def test_unknown_charge_stops_next_request_before_network(self):
        del self.raw["usage"]["input_tokens_details"]["cache_write_tokens"]
        self.assertEqual(self.dispatch()["state"], "unknown")
        with self.assertRaisesRegex(ValueError, "unknown charge"):
            self.dispatch("another")
        self.assertEqual(len(self.calls), 2)

    def test_transport_failure_keeps_hold_and_redacts_error_detail(self):
        def failure(provider, path, payload, timeout):
            if path.endswith("input_tokens"):
                return {"object": "response.input_tokens", "input_tokens": 350}
            raise TimeoutError("sensitive body must not enter the receipt")
        with self.assertRaises(TimeoutError):
            send(self.ledger, self.cell, "parent", "lost", {"input": "x"}, self.root / "evidence", transport=failure)
        row = self.ledger.snapshot()["calls"][0]
        self.assertEqual(row["state"], "unknown")
        self.assertIsNone(row["spent_nano_usd"])
        receipt = (self.root / "evidence/lost/receipt.json").read_text()
        self.assertNotIn("sensitive body", receipt)
        self.assertIn("TimeoutError", receipt)

    def test_crash_after_send_preserves_unknown_and_response(self):
        with patch.object(self.ledger, "settle_response", side_effect=KeyboardInterrupt), self.assertRaises(KeyboardInterrupt):
            self.dispatch()
        self.assertEqual(self.ledger.snapshot()["calls"][0]["state"], "unknown")
        self.assertTrue((self.root / "evidence/request/response.json").is_file())

    def test_insufficient_budget_never_generates(self):
        self.ledger.reserve(self.cell, "held", "parent", 9990, 0)
        with self.assertRaisesRegex(ValueError, "token cap"):
            self.dispatch()
        self.assertEqual(len(self.calls), 1)
        self.assertTrue((self.root / "evidence/request/receipt.json").is_file())

    def test_substituted_model_and_overrun_remain_visible(self):
        self.raw["model"] = "substituted"
        with self.assertRaisesRegex(ValueError, "model"):
            self.dispatch()
        self.assertEqual(self.ledger.snapshot()["calls"][0]["state"], "unknown")

    def test_duplicate_request_never_sends_twice(self):
        self.dispatch()
        with self.assertRaises(FileExistsError):
            self.dispatch()
        self.assertEqual(len(self.calls), 2)

    def test_parent_child_admission_is_persistent_and_bounded(self):
        admit_agent(self.ledger, self.cell, "parent", None)
        admit_agent(self.ledger, self.cell, "one", "parent")
        admit_agent(self.ledger, self.cell, "two", "one")
        resumed = Budget(self.root / "host.db", self.frozen)
        for agent, parent, message in (("three", "parent", "child admission"), ("orphan", "absent", "parent must"),
                                        ("two", "parent", "parent changed"), ("second-root", None, "root agent")):
            with self.subTest(agent=agent), self.assertRaisesRegex(ValueError, message):
                admit_agent(resumed, self.cell, agent, parent)
        single = next(row["id"] for row in self.frozen["cells"] if row["mode"] == "single")
        self.ledger.begin(single)
        admit_agent(self.ledger, single, "single-root", None)
        with self.assertRaisesRegex(ValueError, "child admission"):
            admit_agent(self.ledger, single, "child", "single-root")

    def test_inputs_cannot_override_model_or_enable_external_services(self):
        model = self.ledger.models[self.ledger.cells[self.cell]["model"]]
        for extra in ({"model": "other"}, {"max_output_tokens": 999}, {"previous_response_id": "old"},
                      {"tools": [{"type": "web_search"}]}, {"stream": True}):
            with self.subTest(extra=extra), self.assertRaisesRegex(ValueError, "override"):
                prepare(model, {"input": "x", **extra})
        with self.assertRaisesRegex(ValueError, "only text"):
            prepare(model, {"input": [{"role": "user", "content": [{"type": "input_image", "image_url": "https://example.invalid"}]}]})
        model = copy.deepcopy(model)
        model["settings"]["request"]["tools"] = [{"type": "web_search"}]
        with self.assertRaisesRegex(ValueError, "client function"):
            prepare(model, {"input": "x"})

    def test_anthropic_estimate_reserves_full_frozen_ceiling(self):
        frozen = plan(api_manifest("anthropic"))
        ledger = Budget(self.root / "claude.db", frozen)
        cell = frozen["cells"][0]["id"]
        ledger.begin(cell)
        def provider(name, path, payload, timeout):
            if path.endswith("count_tokens"):
                return {"input_tokens": 351}
            self.assertEqual(ledger.snapshot()["calls"][0]["token_limit"], 1020)
            self.assertEqual(payload["max_tokens"], 20)
            return {"type": "message", "id": "message-1", "model": ledger.cells[cell]["model"], "usage": {
                "input_tokens": 100, "cache_creation_input_tokens": 50, "cache_read_input_tokens": 200, "output_tokens": 10}}
        result = send(ledger, cell, "root", "claude", {"messages": [{"role": "user", "content": "x"}]},
                      self.root / "evidence", transport=provider)
        self.assertEqual(result["state"], "settled")
        self.assertEqual(ledger.snapshot()["calls"][0]["tokens"], 360)

    def test_cli_requires_spend_ack_before_reading_files_or_credentials(self):
        result = subprocess.run([sys.executable, "tools/agent-eval-host.py", "send", "absent", "absent", "cell", "agent", "request", "absent", "absent"],
                                capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 2)
        self.assertIn("confirm-agent-spend", result.stderr)
        self.assertNotIn("No such file", result.stderr)


class HTTPSContract(unittest.TestCase):
    def test_fixed_origin_headers_and_no_redirect_following(self):
        with patch.dict("os.environ", {"OPENAI_API_KEY": "synthetic-key"}), patch("http.client.HTTPSConnection") as connection:
            response = connection.return_value.getresponse.return_value
            response.status = 302
            with self.assertRaisesRegex(ValueError, "302"):
                HTTPS()("openai", "/v1/responses", {}, 3)
            connection.assert_called_once_with("api.openai.com", timeout=3)
            self.assertEqual(connection.return_value.request.call_args.kwargs["headers"]["Authorization"], "Bearer synthetic-key")
            connection.return_value.close.assert_called_once()
            self.assertEqual(connection.return_value.request.call_count, 1)

    def test_response_limit_and_duplicate_json_keys_refuse(self):
        for body, cap, message in ((b"x" * 11, 10, "size limit"), (b'{"x":1,"x":2}', 100, "duplicate")):
            with self.subTest(body=body), patch.dict("os.environ", {"ANTHROPIC_API_KEY": "synthetic-key"}), patch("http.client.HTTPSConnection") as connection:
                result = connection.return_value.getresponse.return_value
                result.status = 200
                result.read1.side_effect = [body, b""]
                with patch("agent_eval.request_gateway.MAX_JSON", cap), self.assertRaisesRegex(ValueError, message):
                    HTTPS()("anthropic", "/v1/messages", {}, 3)
                self.assertEqual(connection.return_value.request.call_count, 1)


if __name__ == "__main__":
    unittest.main()
