"""Inside-container native application controls. All provider replies are scripted."""
import argparse
from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import selectors
import shutil
import subprocess
import sys
import threading
import time

from agent_eval.comparison_clients import HOSTS, MODEL, SETUPS, WORKERS, launch_command, native_config, require

CAP = 1024**2
QUERY = {"query": "normalize_total", "maxFiles": 2, "projectPath": "/workspace"}
READ = "python3 -c 'from pathlib import Path; print(\"app.py\"); print(Path(\"app.py\").read_text())'"
FIND = "fr --json project find normalize_total --signature"


def toml(value):
    if isinstance(value, dict):
        return "{" + ", ".join(json.dumps(k) + " = " + toml(v) for k, v in value.items()) + "}"
    if isinstance(value, list):
        return "[" + ", ".join(toml(v) for v in value) + "]"
    return json.dumps(value)


def trace(stage):
    processes = []
    for status in Path("/proc").glob("[0-9]*/status"):
        try:
            fields = dict(line.split(":", 1) for line in status.read_text().splitlines() if ":" in line)
            processes.append({"name": fields["Name"].strip(), "threads": int(fields["Threads"]),
                              "rss_kib": int(fields.get("VmRSS", "0 kB").split()[0]),
                              "command": (status.parent / "cmdline").read_bytes()[:160].replace(b"\0", b" ").decode(errors="replace")})
        except FileNotFoundError:
            continue
    print(json.dumps({"stage": stage, "processes": processes}), file=sys.stderr, flush=True)


def command(argv, env):
    trace("before " + " ".join(argv[:2]))
    result = subprocess.run(argv, cwd="/workspace", env=env, capture_output=True, timeout=60)
    trace("after " + " ".join(argv[:2]))
    Path("/workspace/native.stdout").write_bytes(result.stdout[:CAP])
    Path("/workspace/native.stderr").write_bytes(result.stderr[:CAP])
    require(len(result.stdout) + len(result.stderr) <= CAP, "native output exceeds 1 MiB")
    require(result.returncode == 0, f"{argv!r}: {result.returncode}: " +
            result.stdout[-4096:].decode(errors="replace") + result.stderr[-4096:].decode(errors="replace"))
    return {"argv": argv, "stdout": result.stdout.decode(), "stderr": result.stderr.decode()}


def call_for(host, setup):
    if setup == "codegraph":
        name = "mcp__codegraph__codegraph_explore" if host == "claude" else "codegraph_codegraph_explore"
        return name, QUERY
    name = "Bash" if host == "claude" else "bash"
    return name, {"command": FIND if setup == "fr" else READ, "description": "Inspect the fixture function"}


@contextmanager
def provider(host, setup):
    requests, errors = [], []
    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_POST(self):
            try:
                length = int(self.headers["Content-Length"])
                require(0 < length <= CAP, "scripted request exceeds 1 MiB")
                value = json.loads(self.rfile.read(length))
                require(len(json.dumps(requests)) + length <= CAP, "provider transcript exceeds 1 MiB")
                requests.append({"path": self.path, "body": value})
                if host == "codex":
                    raise ValueError("Codex control must not request inference")
                require(value["model"] == MODEL, "client substituted the scripted model")
                if self.path.split("?")[0] == "/v1/messages/count_tokens" and host == "claude":
                    payload, content_type = b'{"input_tokens":1}', "application/json"
                else:
                    expected = "/v1/messages" if host == "claude" else "/v1/chat/completions"
                    require(self.path.split("?")[0] == expected, "unexpected provider route")
                    turns = [r for r in requests if r["path"].split("?")[0] == expected]
                    require(len(turns) <= 2, "unexpected additional inference request")
                    first = len(turns) == 1
                    name, arguments = call_for(host, setup)
                    names = {t["name"] if host == "claude" else t["function"]["name"] for t in value["tools"]}
                    require(name in names, f"native client did not offer {name}: {sorted(names)}")
                    require(any("codegraph_explore" in n for n in names) == (setup == "codegraph"),
                            "CodeGraph tools leaked or were not connected")
                    if not first:
                        if host == "claude":
                            results = [block for message in value["messages"] if isinstance(message["content"], list)
                                       for block in message["content"] if block.get("type") == "tool_result"]
                            require(len(results) == 1 and not results[0].get("is_error", False), "native tool failed")
                            content = json.dumps(results[0]["content"], ensure_ascii=False)
                        else:
                            results = [message for message in value["messages"] if message["role"] == "tool"]
                            require(len(results) == 1, "missing native tool result")
                            content = json.dumps(results[0]["content"], ensure_ascii=False)
                        require("normalize_total" in content and "app.py" in content,
                                "the native tool result was not returned to the provider")
                    if host == "claude":
                        payload = self.anthropic(value, first, name, arguments)
                    else:
                        payload = self.openai(value, first, name, arguments)
                    content_type = "text/event-stream"
                self.send_response(200)
                self.send_header("Content-Type", content_type)
                self.send_header("Content-Length", str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)
            except (KeyError, ValueError, TypeError) as error:
                errors.append(str(error))
                self.send_error(400, "scripted control refused request")

        def anthropic(self, value, first, name, arguments):
            message = {"id": "msg_control", "type": "message", "role": "assistant", "model": MODEL,
                       "content": [], "stop_reason": None, "stop_sequence": None,
                       "usage": {"input_tokens": 1, "output_tokens": 1}}
            block = {"type": "tool_use", "id": "call_control", "name": name, "input": {}} if first else {"type": "text", "text": ""}
            delta = {"type": "input_json_delta", "partial_json": json.dumps(arguments)} if first else {
                "type": "text_delta", "text": "Scripted native tool control completed."}
            events = [("message_start", {"message": message}),
                      ("content_block_start", {"index": 0, "content_block": block}),
                      ("content_block_delta", {"index": 0, "delta": delta}),
                      ("content_block_stop", {"index": 0}),
                      ("message_delta", {"delta": {"stop_reason": "tool_use" if first else "end_turn", "stop_sequence": None},
                                         "usage": {"output_tokens": 1}}), ("message_stop", {})]
            return "".join("event: " + key + "\ndata: " + json.dumps({"type": key, **body}) + "\n\n"
                           for key, body in events).encode()

        def openai(self, value, first, name, arguments):
            message = {"role": "assistant", "content": None if first else "Scripted native tool control completed."}
            if first:
                message["tool_calls"] = [{"index": 0, "id": "call_control", "type": "function",
                    "function": {"name": name, "arguments": json.dumps(arguments)}}]
            common = {"id": "chatcmpl_control", "object": "chat.completion.chunk", "created": 1, "model": MODEL}
            chunks = [{**common, "choices": [{"index": 0, "delta": message, "finish_reason": None}]},
                      {**common, "choices": [{"index": 0, "delta": {}, "finish_reason": "tool_calls" if first else "stop"}],
                       "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}}]
            return b"".join(b"data: " + json.dumps(c).encode() + b"\n\n" for c in chunks) + b"data: [DONE]\n\n"

    class Server(ThreadingHTTPServer):
        def service_actions(self):
            trace("provider resource sample")

    server = Server(("127.0.0.1", 0), Handler)
    worker = threading.Thread(target=server.serve_forever, daemon=True)
    worker.start()
    try:
        yield "http://127.0.0.1:" + str(server.server_port), requests, errors
    finally:
        Path("/workspace/provider-summary.json").write_text(json.dumps({"errors": errors, "requests": [
            {"path": r["path"], "model": r["body"].get("model"),
             "tools": [t.get("name") if host == "claude" else t.get("function", {}).get("name")
                       for t in r["body"].get("tools", [])]} for r in requests]}) + "\n")
        Path("/workspace/provider.json").write_text(json.dumps({"requests": requests, "errors": errors}) + "\n")
        server.shutdown()
        server.server_close()
        worker.join(timeout=2)


def codex(config, env, setup):
    Path(env["CODEX_HOME"], "config.toml").write_text("\n".join(k + " = " + toml(v) for k, v in config.items()) + "\n")
    transcript = []
    with Path("/workspace/native.stderr").open("wb") as stderr:
        process = subprocess.Popen(launch_command("codex", setup, config), cwd="/workspace", env=env,
                                   stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=stderr)
        selector = selectors.DefaultSelector()
        selector.register(process.stdout, selectors.EVENT_READ)
        buffer = b""
        def request(method, params):
            nonlocal buffer
            trace("before " + method)
            identity = len(transcript) + 1
            process.stdin.write((json.dumps({"id": identity, "method": method, "params": params}) + "\n").encode())
            process.stdin.flush()
            deadline = time.monotonic() + 30
            while time.monotonic() < deadline:
                if b"\n" not in buffer:
                    require(selector.select(max(0, deadline - time.monotonic())), "native Codex reply timed out")
                    block = os.read(process.stdout.fileno(), 65536)
                    require(block, "Codex exited before replying")
                    buffer += block
                    require(len(buffer) <= CAP, "Codex reply exceeds 1 MiB")
                    continue
                line, buffer = buffer.split(b"\n", 1)
                value = json.loads(line)
                transcript.append(value)
                require(len(json.dumps(transcript)) <= CAP, "Codex transcript exceeds 1 MiB")
                require(not ("method" in value and "id" in value), "unexpected Codex approval request")
                if value.get("id") == identity:
                    require("error" not in value, str(value))
                    return value["result"]
            raise ValueError("native Codex reply timed out")
        try:
            request("initialize", {"clientInfo": {"name": "fr_comparison_control", "version": "1"},
                                   "capabilities": {"experimentalApi": True}})
            process.stdin.write(b'{"method":"initialized","params":{}}\n')
            process.stdin.flush()
            effective = request("config/read", {"includeLayers": False})
            require(effective["config"]["model"] == MODEL, "Codex substituted the configured model")
            discovered = request("skills/list", {"cwds": ["/workspace"], "forceReload": True})["data"]
            require(len(discovered) == 1 and not discovered[0]["errors"], "native skill discovery failed")
            require(any(s["name"] == "fr" for s in discovered[0]["skills"]) == (setup == "fr"), "wrong Codex skill availability")
            thread = request("thread/start", {"cwd": "/workspace", "model": MODEL, "modelProvider": "rehearsal",
                "approvalPolicy": "never", "sandbox": "workspace-write", "ephemeral": True})["thread"]["id"]
            status = request("mcpServerStatus/list", {"threadId": thread, "limit": 100, "detail": "toolsAndAuthOnly"})
            require({r["name"] for r in status["data"]} == ({"codegraph"} if setup == "codegraph" else set()),
                    "Codex loaded the wrong MCP servers")
            if setup == "codegraph":
                require(any(t["name"] == "codegraph_explore" for t in status["data"][0]["tools"].values()),
                        "Codex did not discover the configured CodeGraph tool")
                answer = request("mcpServer/tool/call", {"threadId": thread, "server": "codegraph",
                    "tool": "codegraph_explore", "arguments": QUERY})
                require(not answer.get("isError", False), "native MCP tool failed")
            else:
                answer = request("command/exec", {"command": ["/bin/sh", "-c", FIND if setup == "fr" else READ],
                    "cwd": "/workspace", "timeoutMs": 10000,
                    "sandboxPolicy": {"type": "externalSandbox", "networkAccess": "restricted"}})
                require(answer["exitCode"] == 0, "native command failed")
            require("normalize_total" in json.dumps(answer) and "app.py" in json.dumps(answer), "Codex query did not succeed")
            return {"effective_config": effective, "mcp": status, "answer": answer, "transcript": transcript}
        finally:
            process.terminate()
            process.wait(timeout=5)
            process.stdin.close()
            process.stdout.close()
            selector.close()


def probe(host, setup):
    # A CPU quota throttles time but leaves the runner's cores visible to worker pools.
    os.sched_setaffinity(0, {min(os.sched_getaffinity(0))})
    available = {n for n in ("fr", "codegraph") if shutil.which(n)}
    require(available == ({setup} if setup != "files" else set()), "specialized executable leaked into a setup")
    require({n for n in HOSTS if shutil.which(n)} == {host}, "wrong native application installed")
    Path("app.py").write_text("def normalize_total(value):\n    return max(value, 0)\n\ndef checkout(value):\n    return normalize_total(value)\n")
    env = {"PATH": os.environ["PATH"], "HOME": "/tmp/home", "LANG": "C.UTF-8", **WORKERS,
        "CODEX_HOME": "/tmp/home/.codex", "CLAUDE_CONFIG_DIR": "/tmp/home/.claude",
        "XDG_CONFIG_HOME": "/tmp/home/.config", "XDG_DATA_HOME": "/tmp/home/.local/share",
        "XDG_CACHE_HOME": "/tmp/home/.cache", "XDG_STATE_HOME": "/tmp/home/.local/state",
        "FR_REHEARSAL_KEY": "scripted-no-live-credential", "CI": "true", "DISABLE_AUTOUPDATER": "1",
        "DISABLE_TELEMETRY": "1", "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC": "1"}
    for name in ("AUTOUPDATE", "CLAUDE_CODE", "MODELS_FETCH", "LSP_DOWNLOAD", "DEFAULT_PLUGINS"):
        env["OPENCODE_DISABLE_" + name] = "1"
    for folder in (".codex", ".claude", ".config/opencode", ".local/share", ".cache", ".local/state"):
        Path("/tmp/home", folder).mkdir(parents=True, exist_ok=True)
    identity = json.loads(Path("/opt/client/identity.json").read_text())
    version = command([host, "--version"], env)
    require(identity["version"] in version["stdout"], "native release differs from its frozen version")
    command(["git", "init", "-q", "-b", "main"], env)
    if setup == "fr":
        location = {"codex": ".agents", "claude": ".claude", "opencode": ".opencode"}[host] + "/skills/fr"
        shutil.copytree("/opt/skill/fr", location)
    if setup == "codegraph":
        command(["codegraph", "init", "--yes"], env)
    with provider(host, setup) as (endpoint, requests, errors):
        config = native_config(host, setup, endpoint)
        Path("native-config.json").write_text(json.dumps(config, indent=2) + "\n")
        if host == "codex":
            result = codex(config, env, setup)
            require(not requests, "Codex contacted the scripted inference endpoint")
        elif host == "claude":
            env.update(ANTHROPIC_BASE_URL=endpoint, ANTHROPIC_API_KEY="scripted-no-live-credential")
            Path("mcp.json").write_text(json.dumps(config["mcp"]))
            result = command(launch_command(host, setup, config), env)
            events = [json.loads(line) for line in result["stdout"].splitlines() if line.strip()]
            initial, = [row for row in events if row.get("type") == "system" and row.get("subtype") == "init"]
            require(("fr" in initial.get("slash_commands", [])) == (setup == "fr"), "wrong Claude skill availability")
        else:
            env.update(OPENCODE_CONFIG_CONTENT=json.dumps(config))
            discovered = json.loads(command(["opencode", "debug", "skill"], env)["stdout"])
            require(any(s["name"] == "fr" for s in discovered) == (setup == "fr"), "wrong OpenCode skill availability")
            result = command(launch_command(host, setup, config), env)
        require(not errors, "; ".join(errors))
        if host != "codex":
            route = "/v1/messages" if host == "claude" else "/v1/chat/completions"
            require(len([r for r in requests if r["path"].split("?")[0] == route]) == 2, "scripted tool loop did not complete")
        return {"host": host, "setup": setup, "client_release": identity, "version": version,
                "config": config, "result": result, "provider_requests": requests,
                "cpu_affinity": sorted(os.sched_getaffinity(0)),
                "live_model_calls": 0, "scope": "Scripted native integration control; not agent adoption, performance or billing evidence."}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", choices=HOSTS, required=True)
    parser.add_argument("--setup", choices=SETUPS, required=True)
    args = parser.parse_args()
    print(json.dumps(probe(args.host, args.setup), indent=2))
