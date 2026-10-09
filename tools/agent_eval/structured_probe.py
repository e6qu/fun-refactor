"""Scripted loopback provider for testing OpenCode's terminal submission protocol."""
import base64
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import secrets
from socketserver import TCPServer
import sys
import threading

from . import native_mcp as mcp
from . import structured_submission as protocol
from . import terminal_transport
from .opencode_rehearsal import settings
from .study import encode, number, require

SOURCE = b"def value():\n    return 42\n"
ANSWER = {"answer": {"value": {"value": 42, "citations": [{
    "path": "module.py", "quote": SOURCE.decode()}]}}}
CASES = ("one-answer", "missing-answer", "duplicate-answer", "adjacent-tool")
MAX_BYTES = 1024**2


class LoopbackServer(ThreadingHTTPServer):
    def server_bind(self):
        # The provider uses a numeric loopback endpoint and needs no reverse DNS.
        TCPServer.server_bind(self)
        self.server_name, self.server_port = self.server_address[:2]


def observed_work(messages, requests):
    """Count failed responses too; these are client counters, not verified billing."""
    assistants = [m for m in messages if m["info"]["role"] == "assistant"]
    tokens, cost = [], 0
    for message in assistants:
        info = message["info"]
        usage = info["tokens"]
        for count in (usage["input"], usage["output"], usage["reasoning"],
                      usage["cache"]["read"], usage["cache"]["write"]):
            number(count, "observed tokens", integer=True)
        tokens.append(usage)
        cost += number(info["cost"], "observed cost")
    return {"assistant_responses": len(assistants), "provider_requests": len(requests),
            "tool_calls": sum(p["type"] == "tool" for m in assistants for p in m["parts"]),
            "tokens": tokens, "reported_cost": cost, "provider_usage_verified": False}


def checked_process(process):
    limits = {"wall_seconds": 120, "cpu_seconds": 20, "rss_bytes": 768 * 1024**2,
              "disk_bytes": 16 * 1024**2, "transcript_bytes": MAX_BYTES}
    require(process["limits"] == limits, "capture limits changed")
    require(process["exit_code"] == process["process_exit_code"] == 0
            and process["stop_reason"] is None and process["timed_out"] is False
            and process["launch_error"] is None and process["monitor_error"] is None,
            "capture process did not complete")
    for field, limit in (("elapsed_seconds", "wall_seconds"), ("sampled_cpu_seconds", "cpu_seconds"),
                         ("sampled_aggregate_rss_bytes", "rss_bytes"),
                         ("sampled_disk_growth_bytes", "disk_bytes"), ("transcript_bytes", "transcript_bytes")):
        require(number(process[field], field) <= limits[limit], "capture exceeded resource limits")


def review(root, case):
    """Replay a scripted capture without starting OpenCode or executing source."""
    require(case in CASES, "unknown scripted case")
    names = ("request.json", "terminal.json", "export.json", "messages.json", "events.jsonl",
             "tools.jsonl", "provider.json", "identity.json", "process.json")
    data = {}
    for name in names:
        path = root / name
        require(path.is_file() and not path.is_symlink() and path.stat().st_size <= MAX_BYTES,
                "missing or oversized capture")
        data[name] = path.read_bytes()
    parsed = {name: mcp.decode(raw) for name, raw in data.items() if name.endswith(".json")}
    process, identity = parsed["process.json"], parsed["identity.json"]
    checked_process(process)
    require(identity == {"schema": protocol.SCHEMA, "opencode_version": protocol.VERSION,
                         "case": case, "provider_requests": 2}, "capture identity differs")
    messages = parsed["messages.json"]
    require(parsed["export.json"]["messages"] == messages, "saved messages differ from export")
    require(parsed["export.json"]["info"]["id"] == messages[0]["info"]["sessionID"], "export session differs")
    rows = [mcp.decode(line) for line in data["tools.jsonl"].splitlines()]
    requests = parsed["provider.json"]
    require(len(requests) == 2, "provider request count differs")
    for request in requests:
        require(request["model"] == "protocol" and request["tool_choice"] == "required", "provider model or choice differs")
        tools = request["tools"]
        names = [t["function"]["name"] for t in tools]
        require(len(names) == len(set(names)) and set(names) == {
            "rehearsal_list_files", "rehearsal_search_source", "rehearsal_read_source", "StructuredOutput"},
            "provider tool surface differs")
        output = next(t["function"] for t in tools if t["function"]["name"] == "StructuredOutput")
        require(output["parameters"] == protocol.FORMAT["schema"], "provider output schema differs")
    before_answer = [m for m in requests[1]["messages"] if m["role"] == "tool"]
    require(len(before_answer) == 1 and before_answer[0]["tool_call_id"] == "call_1_0", "source handoff differs")
    require(rows and set(rows[0]["params"]) <= {"name", "arguments", "_meta"}
            and rows[0]["params"]["name"] == "read_source" and rows[0]["params"]["arguments"] == {
            "path": "module.py", "offset": 0, "bytes": 128, "sha256": ""}, "scripted read differs")
    require(before_answer[0]["content"] == rows[0]["response"]["content"][0]["text"], "provider did not receive source result")
    require(rows[0]["result"]["text"] == SOURCE.decode()
            and rows[0]["result"]["sha256"] == hashlib.sha256(SOURCE).hexdigest(), "scripted source differs")
    verdict, refusal = None, None
    try:
        verdict = protocol.audit(parsed["request.json"], parsed["terminal.json"], messages,
            [mcp.decode(line) for line in data["events.jsonl"].splitlines()], rows)
    except ValueError as error:
        refusal = str(error)
    expected = {"missing-answer": "failed assistant response",
                "duplicate-answer": "expected exactly one structured submission",
                "adjacent-tool": "other calls alongside submission"}
    if case == "one-answer":
        require(verdict is not None, f"valid submission refused: {refusal}")
        require(verdict["answer"] == ANSWER["answer"] and verdict["assistant_responses"] == 2,
                "scripted answer or model call count differs")
    else:
        require(verdict is None and refusal == expected[case], f"unexpected {case} verdict: {refusal}")
        if case == "missing-answer":
            require(parsed["terminal.json"]["info"]["error"]["name"] == "StructuredOutputError",
                    "missing answer failed for another reason")
    return {"case": case, "accepted": verdict is not None, "refusal": refusal, "audit": verdict,
            "observed_work": observed_work(messages, requests),
            "evidence_sha256": {name: hashlib.sha256(raw).hexdigest() for name, raw in data.items()}}


def response(turn, case):
    if turn == 1:
        calls = [("rehearsal_read_source", {"path": "module.py", "offset": 0, "bytes": 128, "sha256": ""})]
    elif turn == 2:
        calls = [] if case == "missing-answer" else [("StructuredOutput", ANSWER)]
        if case == "duplicate-answer":
            calls.append(("StructuredOutput", ANSWER))
        if case == "adjacent-tool":
            calls.append(("rehearsal_list_files", {}))
    else:
        raise ValueError("client made an unexpected extra provider request")
    message = {"role": "assistant", "content": None if calls else "The value is 42."}
    if calls:
        message["tool_calls"] = [{"id": f"call_{turn}_{i}", "type": "function",
                                  "function": {"name": name, "arguments": encode(args).decode()}}
                                 for i, (name, args) in enumerate(calls)]
    return {"id": f"chatcmpl_{turn}", "object": "chat.completion", "created": 1,
            "model": "protocol", "choices": [{"index": 0, "message": message,
            "finish_reason": "tool_calls" if calls else "stop"}],
            "usage": {"prompt_tokens": 100, "completion_tokens": 10, "total_tokens": 110}}


def provider(root, case, *, model="protocol"):
    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_POST(self):
            try:
                require(self.path == "/v1/chat/completions", "unexpected provider route")
                length = int(self.headers.get("Content-Length", "0"))
                require(0 < length <= MAX_BYTES, "provider request exceeds budget")
                request = mcp.decode(self.rfile.read(length))
                self.server.requests.append(request)
                raw = encode(self.server.requests)
                require(len(raw) <= MAX_BYTES, "provider transcript exceeds budget")
                (root / "provider.json").write_bytes(raw)
                require(request["model"] == model, "unexpected provider model")
                require(request.get("tool_choice") == "required", "structured tool choice missing")
                names = {t["function"]["name"] for t in request["tools"]}
                require("StructuredOutput" in names and "rehearsal_submit_answer" not in names,
                        "submission tool surface differs")
                reply = response(len(self.server.requests), case)
                reply["model"] = model
                if request.get("stream"):
                    writer = getattr(self.server, "stream_writer", None)
                    if writer is not None:
                        writer(self, reply)
                        return
                    choice = reply["choices"][0]
                    delta = dict(choice["message"])
                    if "tool_calls" in delta:
                        delta["tool_calls"] = [{"index": i, **call} for i, call in enumerate(delta["tool_calls"])]
                    common = {k: v for k, v in reply.items() if k not in {"choices", "usage"}}
                    common["object"] = "chat.completion.chunk"
                    chunks = [{**common, "choices": [{"index": 0, "delta": delta, "finish_reason": None}]},
                              {**common, "choices": [{"index": 0, "delta": {},
                               "finish_reason": choice["finish_reason"]}], "usage": reply["usage"]}]
                    payload = b"".join(b"data: " + encode(c) + b"\n\n" for c in chunks) + b"data: [DONE]\n\n"
                    content_type = "text/event-stream"
                else:
                    payload, content_type = encode(reply), "application/json"
                self.send_response(200)
                self.send_header("Content-Type", content_type)
                self.send_header("Content-Length", str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)
            except (ValueError, KeyError, TypeError) as error:
                self.server.errors.append(str(error))
                self.send_error(400, "scripted provider refused request")
    server = LoopbackServer(("127.0.0.1", 0), Handler)
    server.requests, server.errors = [], []
    return server


def environment(root, endpoint):
    config = settings()
    permission = {"*": "deny", "rehearsal_*": "allow", "StructuredOutput": "allow"}
    config.update(enabled_providers=["scripted"], plugin=[],
                  provider={"scripted": {"npm": "@ai-sdk/openai-compatible", "name": "Scripted",
                    "options": {"baseURL": endpoint, "apiKey": "scripted-no-credential"},
                    "models": {"protocol": {"name": "Protocol", "limit": {"context": 8192, "output": 512}}}}},
                  agent={"fr-submission": {"mode": "primary", "steps": 12,
                    "permission": permission,
                    "prompt": "Use the available tools, then return the requested structured answer."}},
                  permission=permission,
                  mcp={"rehearsal": {"type": "local", "enabled": True,
                    "command": [sys.executable, "-B", str(Path(__file__).parents[1] / "check-native-submission.py"),
                                "serve", str(root / "tools.jsonl")]}})
    # Empty XDG roots keep the scripted probe away from configured providers and credentials.
    env = {k: v for k, v in os.environ.items() if k in {"PATH", "TMPDIR", "LANG", "SYSTEMROOT", "LD_LIBRARY_PATH"}}
    for name in ("CONFIG", "CACHE", "DATA", "STATE"):
        env[f"XDG_{name}_HOME"] = str(root / name.lower())
    env.update(OPENCODE_CONFIG_CONTENT=json.dumps(config), OPENCODE_CONFIG_DIR=str(root / "config"),
               OPENCODE_SERVER_PASSWORD=secrets.token_hex(24), OPENCODE_SERVER_USERNAME="probe")
    for name in ("PROJECT_CONFIG", "AUTOUPDATE", "EXTERNAL_SKILLS", "CLAUDE_CODE", "MODELS_FETCH", "LSP_DOWNLOAD", "DEFAULT_PLUGINS"):
        env[f"OPENCODE_DISABLE_{name}"] = "1"
    return env


def capture(binary, root, case):
    """Run the scripted provider and shared capture under one process budget."""
    require(case in CASES, "unknown scripted case")
    root.mkdir(parents=True, exist_ok=True)
    with provider(root, case) as fake:
        worker = threading.Thread(target=fake.serve_forever, daemon=True)
        worker.start()
        try:
            env = environment(root, f"http://127.0.0.1:{fake.server_port}/v1")
            body = {"model": {"providerID": "scripted", "modelID": "protocol"},
                    "agent": "fr-submission", "format": protocol.FORMAT,
                    "parts": [{"type": "text", "text": protocol.PROMPT}]}
            captured = terminal_transport.capture(binary, root, env, body)
            require(len(fake.requests) == 2 and not fake.errors, "unexpected provider requests")
            (root / "identity.json").write_bytes(encode({"schema": protocol.SCHEMA,
                "opencode_version": captured["opencode_version"], "case": case,
                "provider_requests": len(fake.requests)}))
        finally:
            fake.shutdown()


def serve(log):
    config = {"files": {"module.py": {"data": base64.b64encode(SOURCE).decode(), "executable": False}},
              "arm": "files", "binary": "unused", "workspace": "/unused", "tools_schema_version": 6}
    with log.open("xb") as destination:
        server = mcp.Server(config, destination)
        del server.tools["submit_answer"]
        for _ in range(128):
            line = sys.stdin.buffer.readline(mcp.MAX_LINE + 1)
            if not line:
                return
            require(len(line) <= mcp.MAX_LINE and line.endswith(b"\n"), "invalid MCP line")
            result = server.dispatch(mcp.decode(line))
            if result is not None:
                sys.stdout.buffer.write(encode(result) + b"\n")
                sys.stdout.buffer.flush()
