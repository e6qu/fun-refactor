#!/usr/bin/env python3
"""Exercise native skill discovery with isolated clients and no live model calls."""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import selectors
import shutil
import subprocess
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

from agent_eval.bounded_host import run

ROOT = Path(__file__).resolve().parents[1]
SKILL = ROOT / "skills/fr"
CASES = ("absent", "project", "user")
LOCATIONS = {
    "claude": (".claude/skills/fr", ".claude/skills/fr"),
    "codex": (".agents/skills/fr", ".agents/skills/fr"),
    "opencode": (".opencode/skills/fr", ".config/opencode/skills/fr"),
}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def save(path, value):
    path.write_text(json.dumps(value, indent=2, allow_nan=False) + "\n")


def environment(home):
    result = {key: os.environ[key] for key in ("PATH", "LANG", "LC_ALL") if key in os.environ}
    result.update(HOME=str(home), CODEX_HOME=str(home / ".codex"),
                  CLAUDE_CONFIG_DIR=str(home / ".claude"),
                  XDG_CONFIG_HOME=str(home / ".config"), XDG_DATA_HOME=str(home / ".local/share"),
                  XDG_CACHE_HOME=str(home / ".cache"), XDG_STATE_HOME=str(home / ".local/state"),
                  OPENCODE_DISABLE_MODELS_FETCH="true", OPENCODE_DISABLE_AUTOUPDATE="true",
                  DISABLE_AUTOUPDATER="1", DISABLE_TELEMETRY="1",
                  CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC="1", CI="true")
    return result


def codex(binary, project, env, output):
    errors = output / "client.stderr"
    with errors.open("wb") as stderr:
        process = subprocess.Popen([str(binary), "app-server", "--listen", "stdio://"],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=stderr, cwd=project, env=env)
        selector = selectors.DefaultSelector()
        selector.register(process.stdout, selectors.EVENT_READ)
        transcript, buffer = [], b""

        def request(identity, method, params):
            nonlocal buffer
            process.stdin.write((json.dumps({"id": identity, "method": method, "params": params}) + "\n").encode())
            process.stdin.flush()
            deadline = time.monotonic() + 20
            while time.monotonic() < deadline:
                if b"\n" not in buffer:
                    require(selector.select(max(0, deadline - time.monotonic())), "Codex discovery reply timed out")
                    chunk = os.read(process.stdout.fileno(), 65536)
                    require(chunk, "Codex exited during discovery")
                    buffer += chunk
                    require(len(buffer) <= 1048576, "Codex discovery reply exceeds 1 MiB")
                    continue
                line, buffer = buffer.split(b"\n", 1)
                value = json.loads(line)
                transcript.append(value)
                if value.get("id") == identity:
                    require("error" not in value, "Codex discovery returned an error: " + str(value))
                    return value["result"]
            raise ValueError("Codex discovery reply timed out")

        try:
            request(1, "initialize", {"clientInfo": {"name": "fr_skill_discovery", "version": "1"}})
            process.stdin.write(b'{"method":"initialized","params":{}}\n')
            process.stdin.flush()
            result = request(2, "skills/list", {"cwds": [str(project)], "forceReload": True})
            save(output / "discovery.json", result)
            entries, = result["data"]
            require(not entries["errors"], "Codex reported invalid skills")
            return [item for item in entries["skills"] if item["name"] == "fr"]
        finally:
            save(output / "client.json", transcript)
            process.terminate()
            process.wait(timeout=5)
            process.stdin.close()
            process.stdout.close()
            selector.close()


def opencode(binary, project, env, output):
    result = subprocess.run([str(binary), "debug", "skill"], cwd=project, env=env,
        capture_output=True, timeout=40)
    (output / "client.stdout").write_bytes(result.stdout)
    (output / "client.stderr").write_bytes(result.stderr)
    require(result.returncode == 0, "OpenCode discovery failed")
    skills = json.loads(result.stdout)
    save(output / "discovery.json", skills)
    return [item for item in skills if item["name"] == "fr"]


def claude(binary, project, env, output):
    requests = []

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):
            return

        def do_POST(self):
            value = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            requests.append({"path": self.path, "body": value})
            save(output / "requests.json", requests)
            if "count_tokens" in self.path:
                payload = json.dumps({"input_tokens": 1}).encode()
                content_type = "application/json"
            else:
                require(self.path.split("?")[0] == "/v1/messages", "Unexpected Claude endpoint")
                message = {"id": "msg_fr_discovery", "type": "message", "role": "assistant",
                    "model": value["model"], "content": [], "stop_reason": None,
                    "stop_sequence": None, "usage": {"input_tokens": 1, "output_tokens": 1}}
                if value.get("stream"):
                    events = [
                        ("message_start", {"type": "message_start", "message": message}),
                        ("content_block_start", {"type": "content_block_start", "index": 0,
                            "content_block": {"type": "text", "text": ""}}),
                        ("content_block_delta", {"type": "content_block_delta", "index": 0,
                            "delta": {"type": "text_delta", "text": "Scripted discovery control completed."}}),
                        ("content_block_stop", {"type": "content_block_stop", "index": 0}),
                        ("message_delta", {"type": "message_delta", "delta": {"stop_reason": "end_turn",
                            "stop_sequence": None}, "usage": {"output_tokens": 1}}),
                        ("message_stop", {"type": "message_stop"}),
                    ]
                    payload = "".join("event: " + name + "\ndata: " + json.dumps(item) + "\n\n"
                                      for name, item in events).encode()
                    content_type = "text/event-stream"
                else:
                    message.update(content=[{"type": "text", "text": "Scripted discovery control completed."}],
                                   stop_reason="end_turn")
                    payload, content_type = json.dumps(message).encode(), "application/json"
            self.send_response(200)
            self.send_header("Content-Type", content_type)
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    env.update(ANTHROPIC_BASE_URL="http://127.0.0.1:" + str(server.server_port),
               ANTHROPIC_API_KEY="fr-scripted-control-no-live-access")
    try:
        result = subprocess.run([str(binary), "-p", "Report readiness.", "--output-format", "stream-json",
            "--verbose", "--max-turns", "1", "--model", "claude-sonnet-4-6", "--tools", "",
            "--strict-mcp-config", "--mcp-config", '{"mcpServers":{}}', "--no-session-persistence"],
            cwd=project, env=env, capture_output=True, timeout=40)
        (output / "client.stdout").write_bytes(result.stdout)
        (output / "client.stderr").write_bytes(result.stderr)
        require(result.returncode == 0, "Claude discovery failed")
        events = [json.loads(line) for line in result.stdout.splitlines() if line.strip()]
        save(output / "discovery.json", events)
        initial = [item for item in events if item.get("type") == "system" and item.get("subtype") == "init"]
        require(len(initial) == 1, "Missing native Claude initialization event")
        return [{"name": "fr"}] if "fr" in initial[0].get("slash_commands", []) else []
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=2)


def capture(args):
    output = args.output.resolve()
    home, project = output / "home", output / "project"
    home.mkdir()
    project.mkdir()
    subprocess.run(["git", "init", "-q", "-b", "main", str(project)], check=True, timeout=10)
    location = None
    if args.case != "absent":
        relative = LOCATIONS[args.host][args.case == "user"]
        location = (home if args.case == "user" else project) / relative
        shutil.copytree(SKILL, location)
    entries = {"codex": codex, "opencode": opencode, "claude": claude}[args.host](
        args.binary.resolve(), project, environment(home), output)
    require(len(entries) == (0 if args.case == "absent" else 1), "Wrong native fr discovery result")
    if location is not None:
        require((location / "references/author.md").is_file(), "Skill references were not installed")
    save(output / "result.json", {"host": args.host, "case": args.case, "entries": entries,
        "installed": str(location) if location else None,
        "scope": "Native discovery with isolated homes. Scripted responses only; no live-agent adoption or savings claim."})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("check", "capture"))
    parser.add_argument("--host", choices=tuple(LOCATIONS), required=True)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--case", choices=CASES)
    args = parser.parse_args()
    if args.command == "capture":
        require(args.case is not None, "Capture requires an explicit case")
        capture(args)
        return
    args.output.mkdir(parents=True, exist_ok=False)
    results = []
    for case in CASES:
        folder = args.output / case
        folder.mkdir()
        stdout, stderr = io.BytesIO(), io.BytesIO()
        process = run([sys.executable, "-B", str(Path(__file__).resolve()), "capture", "--host", args.host,
            "--binary", str(args.binary.resolve()), "--output", str(folder.resolve()), "--case", case],
            b"", stdout, stderr, folder, wall_seconds=60, cpu_limit_seconds=20,
            rss_bytes=768 * 1024**2, disk_bytes=32 * 1024**2, transcript_bytes=1048576)
        (folder / "host.stdout").write_bytes(stdout.getvalue())
        (folder / "host.stderr").write_bytes(stderr.getvalue())
        save(folder / "process.json", process)
        require(process["exit_code"] == 0 and process["stop_reason"] is None,
                f"{args.host}/{case} failed: {process['stop_reason'] or process['exit_code']}; inspect retained diagnostics")
        results.append(json.loads((folder / "result.json").read_text()))
        save(args.output / "result.json", {"host": args.host, "cases": results,
             "binary_sha256": hashlib.sha256(args.binary.read_bytes()).hexdigest()})
        print(args.host + "/" + case + ": passed", flush=True)


if __name__ == "__main__":
    main()
