"""Scripted loopback provider for testing OpenCode's terminal submission protocol."""
import base64
import http.client
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import secrets
import socket
import subprocess
import sys
import threading
import time

from . import native_mcp as mcp
from . import structured_submission as protocol
from .opencode_rehearsal import settings
from .study import encode, require

SOURCE = b"def value():\n    return 42\n"
ANSWER = {"answer": {"value": {"value": 42, "citations": [{
    "path": "module.py", "quote": SOURCE.decode()}]}}}
CASES = ("one-answer", "missing-answer", "duplicate-answer", "adjacent-tool")
MAX_BYTES = 1024**2


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


def provider(root, case):
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
                require(request["model"] == "protocol", "unexpected provider model")
                require(request.get("tool_choice") == "required", "structured tool choice missing")
                names = {t["function"]["name"] for t in request["tools"]}
                require("StructuredOutput" in names and "rehearsal_submit_answer" not in names,
                        "submission tool surface differs")
                reply = response(len(self.server.requests), case)
                if request.get("stream"):
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
    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
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
    env = {k: v for k, v in os.environ.items() if k in {"PATH", "TMPDIR", "LANG", "SYSTEMROOT"}}
    for name in ("CONFIG", "CACHE", "DATA", "STATE"):
        env[f"XDG_{name}_HOME"] = str(root / name.lower())
    env.update(OPENCODE_CONFIG_CONTENT=json.dumps(config), OPENCODE_CONFIG_DIR=str(root / "config"),
               OPENCODE_SERVER_PASSWORD=secrets.token_hex(24), OPENCODE_SERVER_USERNAME="probe")
    for name in ("PROJECT_CONFIG", "AUTOUPDATE", "EXTERNAL_SKILLS", "CLAUDE_CODE", "MODELS_FETCH", "LSP_DOWNLOAD"):
        env[f"OPENCODE_DISABLE_{name}"] = "1"
    return env


def capture(binary, root, case):
    """Run only beneath bounded_host.run; all children remain in its process group."""
    require(case in CASES, "unknown scripted case")
    root.mkdir(parents=True, exist_ok=True)
    with provider(root, case) as fake:
        worker = threading.Thread(target=fake.serve_forever, daemon=True)
        worker.start()
        env = environment(root, f"http://127.0.0.1:{fake.server_port}/v1")
        with socket.socket() as reservation:
            reservation.bind(("127.0.0.1", 0))
            port = reservation.getsockname()[1]
        auth = "Basic " + base64.b64encode(("probe:" + env["OPENCODE_SERVER_PASSWORD"]).encode()).decode()
        deadline = time.monotonic() + 100
        def request(method, route, data=None):
            connection = http.client.HTTPConnection("127.0.0.1", port, timeout=max(0.1, deadline - time.monotonic()))
            try:
                connection.request(method, route, body=encode(data) if data is not None else None,
                                   headers={"Authorization": auth, "Content-Type": "application/json"})
                reply = connection.getresponse()
                raw = reply.read(MAX_BYTES + 1)
                require(reply.status == 200 and len(raw) <= MAX_BYTES, f"invalid server response: {reply.status} {raw[:256]!r}")
                return mcp.decode(raw)
            finally:
                connection.close()
        with (root / "server.stdout").open("wb") as out, (root / "server.stderr").open("wb") as err:
            child = subprocess.Popen([str(binary), "--pure", "serve", "--hostname", "127.0.0.1", "--port", str(port)],
                                     cwd=root, env=env, stdin=subprocess.DEVNULL, stdout=out, stderr=err)
            stream = None
            try:
                while True:
                    require(child.poll() is None and time.monotonic() < deadline, "server did not start")
                    try:
                        health = request("GET", "/global/health")
                        break
                    except (ConnectionError, OSError):
                        time.sleep(0.05)
                require(health["version"] == protocol.VERSION, "OpenCode version differs")
                session = request("POST", "/session", {"title": "Scripted submission control"})["id"]
                stream = http.client.HTTPConnection("127.0.0.1", port, timeout=100)
                stream.request("GET", "/event", headers={"Authorization": auth})
                incoming = stream.getresponse()
                require(incoming.status == 200, "event stream failed")
                ready, idle = threading.Event(), threading.Event()
                errors = []
                def collect():
                    size = 0
                    try:
                        with (root / "events.jsonl").open("xb") as retained:
                            while not idle.is_set():
                                line = incoming.readline(MAX_BYTES + 1)
                                require(line and len(line) <= MAX_BYTES, "incomplete or oversized event stream")
                                size += len(line)
                                require(size <= MAX_BYTES, "event stream exceeds budget")
                                if not line.startswith(b"data: "):
                                    continue
                                event = mcp.decode(line[6:])
                                retained.write(encode(event) + b"\n")
                                retained.flush()
                                ready.set()
                                if event["type"] == "session.status" and event["properties"].get("sessionID") == session:
                                    if event["properties"]["status"]["type"] == "idle":
                                        idle.set()
                    except (ValueError, KeyError, OSError) as error:
                        errors.append(str(error))
                        ready.set()
                        idle.set()
                reader = threading.Thread(target=collect, daemon=True)
                reader.start()
                require(ready.wait(5) and not errors, "event stream never became ready")
                body = {"model": {"providerID": "scripted", "modelID": "protocol"},
                        "agent": "fr-submission", "format": protocol.FORMAT,
                        "parts": [{"type": "text", "text": protocol.PROMPT}]}
                (root / "request.json").write_bytes(encode(body))
                terminal = request("POST", f"/session/{session}/message", body)
                (root / "terminal.json").write_bytes(encode(terminal))
                require(idle.wait(5) and not errors, "event stream did not finish cleanly")
                reader.join(timeout=1)
                # The pinned HTTP message encoder rejects its persisted structured format.
                # Export the unchanged database through the client after stopping the server.
                child.terminate()
                child.wait(timeout=3)
                with (root / "export.json").open("xb") as saved, (root / "export.stderr").open("wb") as errors_out:
                    exported = subprocess.run([str(binary), "--pure", "export", session],
                        cwd=root, env=env, stdin=subprocess.DEVNULL, stdout=saved, stderr=errors_out,
                        timeout=max(0.1, deadline - time.monotonic()))
                require(exported.returncode == 0, "session export failed")
                require((root / "export.json").stat().st_size <= MAX_BYTES, "session export exceeds budget")
                exported = mcp.decode((root / "export.json").read_bytes())
                require(exported["info"]["id"] == session, "export session differs")
                messages = exported["messages"]
                (root / "messages.json").write_bytes(encode(messages))
                require(len(fake.requests) == 2 and not fake.errors, "unexpected provider requests")
                (root / "identity.json").write_bytes(encode({"schema": protocol.SCHEMA,
                    "opencode_version": health["version"], "case": case, "provider_requests": len(fake.requests)}))
            finally:
                if stream:
                    stream.close()
                child.terminate()
                try:
                    child.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait(timeout=3)
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
