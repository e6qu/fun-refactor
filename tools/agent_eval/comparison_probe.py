"""Inside-container controls using real tools, never an agent or model."""
import argparse
import ast
import json
import os
from pathlib import Path
import selectors
import shutil
import subprocess
import time


def require(condition, message):
    if not condition:
        raise ValueError(message)


def command(argv):
    result = subprocess.run(argv, capture_output=True, timeout=30)
    require(len(result.stdout) + len(result.stderr) <= 1024**2, "tool output exceeds 1 MiB")
    require(result.returncode == 0, f"{argv!r}: exit={result.returncode}\n"
            f"stdout: {result.stdout.decode(errors='replace')}\nstderr: {result.stderr.decode(errors='replace')}")
    return result.stdout.decode()


def codegraph():
    command(["codegraph", "init", "--yes"])
    transcript = []
    with Path("/workspace/mcp.stderr").open("wb") as errors:
        server = subprocess.Popen(["codegraph", "serve", "--mcp"], stdin=subprocess.PIPE,
                                  stdout=subprocess.PIPE, stderr=errors, start_new_session=True)
        selector = selectors.DefaultSelector()
        selector.register(server.stdout, selectors.EVENT_READ)
        buffer = b""

        def request(identity, method, params):
            nonlocal buffer
            server.stdin.write((json.dumps({"jsonrpc": "2.0", "id": identity,
                                           "method": method, "params": params}) + "\n").encode())
            server.stdin.flush()
            deadline = time.monotonic() + 30
            while time.monotonic() < deadline:
                if b"\n" not in buffer:
                    require(selector.select(max(0, deadline - time.monotonic())), "MCP reply timed out")
                    block = os.read(server.stdout.fileno(), 65536)
                    require(block, "MCP server exited before its reply")
                    buffer += block
                    require(len(buffer) <= 1024**2, "MCP reply exceeds 1 MiB")
                    continue
                line, buffer = buffer.split(b"\n", 1)
                row = json.loads(line)
                transcript.append(row)
                require(len(json.dumps(transcript)) <= 1024**2, "MCP transcript exceeds 1 MiB")
                if row.get("id") == identity:
                    require("error" not in row, f"MCP error: {row}")
                    return row["result"]
            raise ValueError("MCP reply timed out")

        try:
            request(1, "initialize", {"protocolVersion": "2024-11-05", "capabilities": {},
                                      "clientInfo": {"name": "fr-comparison-rehearsal", "version": "1"}})
            server.stdin.write(b'{"jsonrpc":"2.0","method":"notifications/initialized"}\n')
            server.stdin.flush()
            tools = request(2, "tools/list", {})
            require("codegraph_explore" in {row["name"] for row in tools["tools"]}, "explore tool missing")
            answer = request(3, "tools/call", {"name": "codegraph_explore", "arguments": {
                "query": "normalize_total", "maxFiles": 2, "projectPath": "/workspace"}})
            require(not answer.get("isError", False), f"CodeGraph explore failed: {answer}")
            require("normalize_total" in json.dumps(answer) and "app.py" in json.dumps(answer),
                    "CodeGraph did not find the fixture function")
            lock = json.loads(Path(".codegraph/daemon.pid").read_text())
            daemon = lock["pid"]
            require(lock["version"] == "1.6.2", "daemon version differs from pinned release")
            require(os.getpgid(daemon) != os.getpgid(server.pid), "daemon did not leave the proxy process group")
            membership = {label: Path(f"/proc/{pid}/cgroup").read_text() for label, pid in (
                ("host", os.getpid()), ("proxy", server.pid), ("daemon", daemon))}
            require(len(set(membership.values())) == 1, "daemon and host belong to different resource groups")
            result = {"tools": tools, "answer": answer, "daemon_pid": daemon,
                      "proxy_pgid": os.getpgid(server.pid), "daemon_pgid": os.getpgid(daemon),
                      "resource_group_membership": membership}
        finally:
            Path("/workspace/mcp.json").write_text(json.dumps(transcript, indent=2) + "\n")
            server.stdin.close()
            try:
                server.wait(timeout=5)
            except subprocess.TimeoutExpired:
                server.kill()
                server.wait(timeout=2)
                raise ValueError("CodeGraph proxy failed to stop after stdin closed")
            finally:
                server.stdout.close()
                selector.close()
        require(server.returncode == 0, "CodeGraph proxy exited unsuccessfully")
        os.kill(daemon, 0)
        require(Path(f"/proc/{daemon}/stat").read_text().split(") ", 1)[1].split()[0] != "Z",
                "daemon is a zombie rather than a running program")
        require(b"codegraph" in Path(f"/proc/{daemon}/cmdline").read_bytes(), "daemon PID does not identify CodeGraph")
        result["daemon_alive_after_proxy_exit"] = True
        return result


def probe(setup):
    paths = {name: shutil.which(name) for name in ("fr", "codegraph")}
    expected = {"files": set(), "fr": {"fr"}, "codegraph": {"codegraph"}}[setup]
    require({name for name, path in paths.items() if path} == expected, "specialized tools leaked into the setup")
    versions = {name: command([name, "--version"]).strip() for name in expected}
    for name, wanted in (("fr", "0.80.0"), ("codegraph", "1.6.2")):
        if name in expected:
            require(wanted in versions[name], f"{name} version differs from the pinned release")
    Path("app.py").write_text("def normalize_total(value):\n    return max(value, 0)\n\ndef checkout(value):\n    return normalize_total(value)\n")
    shutil.copyfile("/probe/comparison_plan.py", "comparison_plan.py")
    shutil.copyfile("/probe/comparison_handoff_actions.py", "comparison_handoff_actions.py")
    command(["git", "init", "-q", "-b", "main"])
    if setup == "files":
        answer = [node.name for node in ast.walk(ast.parse(Path("app.py").read_text()))
                  if isinstance(node, ast.FunctionDef) and node.name == "normalize_total"]
        require(answer == ["normalize_total"], "ordinary source inspection failed")
        result = {"answer": answer}
    elif setup == "fr":
        answer = json.loads(command(["fr", "--json", "project", "find", "normalize_total", "--signature"]))
        rows = [dict(zip(answer["columns"], row)) for row in answer["rows"]]
        require(any(row["name"] == "normalize_total" for row in rows), "fr did not find fixture function")
        own_code = json.loads(command(["fr", "--json", "project", "find", "settings", "--signature"]))
        require(own_code["rows"], "fr did not find the settings validator in its own comparison code")
        own_edits = json.loads(command(["fr", "--json", "project", "find", "patched", "--in", "comparison_handoff_actions.py", "--signature"]))
        require(own_edits["rows"], "fr did not find this PR's handoff edit function")
        result = {"answer": answer, "dogfood_comparison_settings": own_code, "dogfood_handoff_edits": own_edits}
    else:
        result = codegraph()
    controls = {name: os.environ[name] for name in (
        "CODEGRAPH_TELEMETRY", "CODEGRAPH_PARSE_WORKERS", "CODEGRAPH_QUERY_POOL_SIZE",
        "CODEGRAPH_RESOLVE_WORKERS", "RAYON_NUM_THREADS")}
    require(controls == {"CODEGRAPH_TELEMETRY": "0", "CODEGRAPH_PARSE_WORKERS": "1",
                         "CODEGRAPH_QUERY_POOL_SIZE": "1", "CODEGRAPH_RESOLVE_WORKERS": "1",
                         "RAYON_NUM_THREADS": "1"}, "worker settings differ from the rehearsal")
    return {"setup": setup, "available_tools": paths, "versions": versions, "worker_settings": controls, **result}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--setup", choices=("files", "fr", "codegraph"), required=True)
    args = parser.parse_args()
    print(json.dumps(probe(args.setup), indent=2))
