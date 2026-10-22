"""Explicit native-client inputs for model-free comparison rehearsals."""
import hashlib
import json
from pathlib import Path

HOSTS = ("claude", "codex", "opencode")
SETUPS = ("files", "fr", "codegraph")
MODEL = "fr-scripted-control"
ORDINARY = ("read", "search", "edit", "run checks", "export a Git patch")
WORKERS = {"CODEGRAPH_TELEMETRY": "0", "CODEGRAPH_PARSE_WORKERS": "1",
           "CODEGRAPH_QUERY_POOL_SIZE": "1", "CODEGRAPH_RESOLVE_WORKERS": "1", "RAYON_NUM_THREADS": "1",
           "TOKIO_WORKER_THREADS": "1", "UV_THREADPOOL_SIZE": "1", "NODE_OPTIONS": "--v8-pool-size=1"}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def native_config(host, setup, endpoint):
    require(host in HOSTS and setup in SETUPS, "unknown application or tool setup")
    require(endpoint.startswith("http://127.0.0.1:") and endpoint.rsplit(":", 1)[1].isdigit(),
            "rehearsal provider must be an explicit loopback endpoint")
    require(endpoint.rsplit(":", 1)[1].isascii() and 1 <= int(endpoint.rsplit(":", 1)[1]) <= 65535,
            "rehearsal port must be between 1 and 65535")
    server = {"command": "/opt/tool/bin/codegraph", "args": ["serve", "--mcp"],
              "env": dict(WORKERS), "cwd": "/workspace"}
    if host == "claude":
        return {"model": MODEL, "tools": ["Bash", "Read", "Edit", "Write", "Glob", "Grep", "Skill"],
                "mcp": {"mcpServers": {"codegraph": server} if setup == "codegraph" else {}},
                "settings": {"disableAllHooks": True, "autoMemoryEnabled": False},
                "provider": endpoint, "max_turns": 3}
    if host == "codex":
        return {"model": MODEL, "model_provider": "rehearsal", "model_reasoning_effort": "low",
                "service_tier": "default", "approval_policy": "never", "sandbox_mode": "workspace-write",
                "web_search": "disabled", "model_providers": {"rehearsal": {
                    "name": "Scripted control only", "base_url": endpoint + "/v1", "wire_api": "responses",
                    "env_key": "FR_REHEARSAL_KEY", "requires_openai_auth": False}},
                "mcp_servers": {"codegraph": {**server, "enabled": True, "required": True,
                    "startup_timeout_sec": 10, "tool_timeout_sec": 30,
                    "enabled_tools": ["codegraph_explore"], "default_tools_approval_mode": "auto"}} if setup == "codegraph" else {}}
    permission = {"*": "deny", "read": "allow", "glob": "allow", "grep": "allow",
                  "list": "allow", "edit": "allow", "bash": "allow", "skill": "allow"}
    if setup == "codegraph":
        permission["codegraph_codegraph_explore"] = "allow"
    return {"model": "rehearsal/" + MODEL, "small_model": "rehearsal/" + MODEL,
            "autoupdate": False, "share": "disabled", "snapshot": False, "lsp": False, "plugin": [],
            "compaction": {"auto": False, "prune": False}, "permission": permission,
            "enabled_providers": ["rehearsal"], "provider": {"rehearsal": {
                "npm": "@ai-sdk/openai-compatible", "name": "Scripted control only",
                "options": {"baseURL": endpoint + "/v1", "apiKey": "scripted-no-live-credential"},
                "models": {MODEL: {"name": MODEL, "limit": {"context": 32768, "output": 1024}}}}},
            "mcp": {"codegraph": {"type": "local", "enabled": True,
                "command": [server["command"], *server["args"]], "environment": dict(WORKERS),
                "cwd": "/workspace", "timeout": 10000}} if setup == "codegraph" else {}}


def prompts(requirement):
    common = ("Use the supplied project to complete this task:\n\n" + requirement +
              "\n\nOrdinary file reading, search, editing, checks and Git patch export are available in every setup. "
              "Use an offered specialized tool when useful. Do not browse, delegate, commit, push, change build "
              "scripts or dependency manifests, or inspect the private evaluator or other attempts. "
              "An incomplete check is not proof that the code is correct. Report failed checks and unsupported behavior.\n")
    return {"discover": common + "\nFirst phase: diagnose and inspect without changing implementation source. "
            "Write HANDOFF.md with your diagnosis, concrete source references, checks already run and remaining work.\n",
            "deliver": common + "\nSecond phase: read HANDOFF.md and inspect the current source again. "
            "A relevant source may have changed while work was paused. Implement the change, run the project checks, "
            "export change.patch with Git, and explain the result and any remaining limitations.\n"}


def launch_command(host, setup, config):
    require(host in HOSTS and setup in SETUPS, "unknown application or tool setup")
    if host == "codex":
        return ["codex", "app-server", "--listen", "stdio://"]
    if host == "opencode":
        return ["opencode", "run", "--format", "json", "--title", "Scripted native tool control", "--model", config["model"],
                "Inspect the fixture and report readiness."]
    allowed = config["tools"] + (["mcp__codegraph__codegraph_explore"] if setup == "codegraph" else [])
    return ["claude", "-p", "Inspect the fixture and report readiness.", "--output-format", "stream-json",
            "--verbose", "--model", config["model"], "--max-turns", str(config["max_turns"]),
            "--no-session-persistence", "--setting-sources", "project", "--settings", json.dumps(config["settings"]),
            "--tools", ",".join(config["tools"]), "--allowedTools", *allowed,
            "--strict-mcp-config", "--mcp-config", "/workspace/mcp.json"]


def inputs(root):
    root = Path(root)
    tasks = json.loads((root / "tests/agent-eval/unknown-target/task.json").read_text())["tasks"]
    result = {"live_calls_enabled": False, "ordinary_operations": list(ORDINARY), "cpu_affinity_count": 1,
              "worker_environment": dict(WORKERS),
              "rehearsal_model": MODEL, "applications": {}, "commands": {}, "tasks": {}}
    for host in HOSTS:
        result["applications"][host] = {setup: native_config(host, setup, "http://127.0.0.1:1") for setup in SETUPS}
        result["commands"][host] = {setup: launch_command(host, setup, result["applications"][host][setup]) for setup in SETUPS}
    for name in ("unicode-dice", "regex-escape-len"):
        result["tasks"][name] = {"prompts": prompts(tasks[name]["requirement"]),
            "grader": ["python3", "-B", "tools/agent-comparison-grade.py", "grade", "--task", name,
                       "--candidate", "CANDIDATE_DIRECTORY", "--output", "PRIVATE_RESULT_DIRECTORY"],
            "controls": ["unchanged", "incomplete", "reference", "independent patch receiver"]}
    encoded = json.dumps(result, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()
    return {**result, "sha256": hashlib.sha256(encoded).hexdigest()}
