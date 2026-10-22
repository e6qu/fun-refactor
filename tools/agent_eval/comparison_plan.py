"""Prepare the small, familiar-project comparison; never launch model calls."""
import hashlib
import json
from pathlib import Path

from .study import digest, load, number, require, text
from .comparison_clients import inputs as client_inputs

ROOT = Path(__file__).resolve().parents[2]
HOSTS = ("claude", "codex", "opencode")
SETUPS = ("files", "codegraph", "fr")
TASKS = ("unicode-dice", "regex-escape-len")
# Bind the independent behavior checks, not just their descriptions.
INPUTS = (
    "tests/agent-eval/unknown-target/task.json",
    "tests/agent-eval/agent-first-use/hosts.json",
    "tests/agent-eval/agent-first-use/codegraph.json",
    "tests/agent-eval/agent-comparison/tools.json",
    "tools/agent-eval.py", "tools/agent_eval/oracle.py",
    "tools/agent_eval/regex_workspace.py", "tools/agent_eval/regex_escape_len.py",
    "tools/agent_eval/comparison_plan.py",
    "tools/agent-comparison-plan.py", "tools/agent-comparison-rehearsal.py",
    "tools/agent_eval/comparison_probe.py", "tools/agent_eval/container_resources.py",
    "tools/agent_eval/bounded_host.py", "tools/agent_eval/study.py",
    "tools/agent-eval-codex.py", "tools/agent_eval/isolated_grade.py",
    "tools/agent_eval/docker_info.py", "tools/agent_eval/request_gateway.py",
    "tools/agent_eval/provider_usage.py",
    "tests/agent-eval/agent-comparison/Dockerfile", ".github/workflows/agent-comparison.yml",
    "tests/agent-eval/agent-comparison/Native.Dockerfile",
    "tools/agent_eval/comparison_clients.py", "tools/agent_eval/comparison_native.py",
    "tools/agent-comparison-native.py",
    "tools/agent-comparison-grade.py", "tools/agent_eval/comparison_grade.py",
    "tools/agent_eval/investigation_rehearsal.py",
)


def bindings(root):
    paths = sorted(set(INPUTS) | {
        str(path.relative_to(root)) for path in (root / "skills/fr").rglob("*") if path.is_file()
    })
    require(any(path.startswith("skills/fr/") for path in paths), "missing complete fr skill")
    return {path: hashlib.sha256((root / path).read_bytes()).hexdigest() for path in paths}


def prepare(root):
    task = load(root / INPUTS[0])
    budget = task["budgets"]
    attempts = []
    # Each setup occupies each position twice across six task/application groups.
    # There is one first attempt per cell; this is not a repeated-agent study.
    for task_index, task_id in enumerate(TASKS):
        for host_index, host in enumerate(HOSTS):
            offset = (task_index + host_index) % len(SETUPS)
            order = SETUPS[offset:] + SETUPS[:offset]
            for position, setup in enumerate(order):
                attempts.append({"id": f"{task_id}-{host}-{setup}", "task": task_id,
                                 "host": host, "setup": setup, "position": position + 1,
                                 "status": "unstarted", "outcome": None,
                                 "usage": None, "cost_usd": None, "tool_used": None})
    result = {
        "purpose": "Preparation for 18 first attempts on two projects already familiar to development.",
        "live_calls_enabled": False,
        "missing_before_live_calls": [
            "Agreed exact models, reasoning settings, prices and spending cap",
            "Agreed live native application configurations and full attempt rehearsal",
            "Full-attempt resource and credential admission on the chosen runner",
        ],
        "bindings": bindings(root),
        "clients": load(root / INPUTS[1]),
        "codegraph": load(root / INPUTS[2]),
        "tools": load(root / INPUTS[3]),
        "native_rehearsal_inputs": client_inputs(root),
        "tasks": {name: task["tasks"][name] for name in TASKS},
        "limits": {name: budget[name] for name in (
            "phases_per_trial", "seconds_per_phase", "calls_per_trial",
            "visible_bytes_per_trial", "parallel_workers")},
        "attempts": attempts,
    }
    return {**result, "sha256": digest(result)}


def verify(plan, root):
    require(plan == prepare(root), "comparison preparation changed; regenerate and review it")
    return plan


def settings(value):
    """Review explicit settings without treating review as permission to spend."""
    require(set(value) == {"applications", "spend_cap_usd", "attempt_cap_usd"},
            "supply applications and both spending caps explicitly")
    total = number(value["spend_cap_usd"], "total spending cap", positive=True)
    per_attempt = number(value["attempt_cap_usd"], "attempt spending cap", positive=True)
    require(per_attempt <= total, "attempt cap exceeds total cap")
    require(set(value["applications"]) == set(HOSTS), "settings need all three applications")
    for host, entry in value["applications"].items():
        require(set(entry) == {"provider", "model", "reasoning", "pricing"}, f"incomplete {host} settings")
        for key in ("provider", "model"):
            text(entry[key], f"{host} {key}")
        require(isinstance(entry["reasoning"], dict) and entry["reasoning"], f"explicit {host} reasoning settings required")
        price = entry["pricing"]
        require(set(price) == {"source", "checked_at", "usd_per_million_tokens"}, "incomplete pricing")
        for key in ("source", "checked_at"):
            text(price[key], key)
        require(set(price["usd_per_million_tokens"]) == {"uncached_input", "cache_read", "cache_write", "output"},
                "supply all four token prices; missing prices are unknown, not zero")
        for key, amount in price["usd_per_million_tokens"].items():
            number(amount, key)
    # Serialise now so NaN, non-JSON reasoning values and invalid keys fail review.
    json.dumps(value, allow_nan=False)
    return value
