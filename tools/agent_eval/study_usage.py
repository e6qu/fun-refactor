"""Account for all declared agent invocations without guessing missing usage."""
from __future__ import annotations

from .study import TOKEN_FIELDS, artifact, number, require, text

MEASUREMENTS = (
    "tool_calls", "tool_result_bytes", "source_read_bytes", "repeated_read_bytes",
    "fallback_calls", "retries", "pages", "instruction_bytes", "handoff_bytes",
    "integration_tokens", "peak_context_tokens", "tool_cpu_seconds",
    "sampled_aggregate_rss_bytes", "disk_growth_bytes", "cache_growth_bytes",
)


def usage(agents, model, root, max_children, wall_seconds):
    require(isinstance(agents, list) and agents, "attempt needs a parent agent")
    by_id = {}
    for agent in agents:
        identity = text(agent["id"], "agent id")
        require(identity not in by_id, "duplicate agent identity")
        by_id[identity] = agent
    parents = [agent for agent in agents if agent["parent"] is None]
    require(len(parents) == 1, "attempt needs exactly one root agent")
    require(len(agents) - 1 <= max_children, "child budget exceeded")
    expected_children = {identity: [] for identity in by_id}
    for agent in agents:
        if agent["parent"] is not None:
            require(agent["parent"] in by_id, "missing parent agent")
            expected_children[agent["parent"]].append(agent["id"])
        visited, cursor = set(), agent
        while cursor["parent"] is not None:
            require(cursor["id"] not in visited, "cycle in agent tree")
            visited.add(cursor["id"])
            require(cursor["parent"] in by_id, "missing parent agent")
            cursor = by_id[cursor["parent"]]
    missing, turns, seconds = [], set(), 0
    totals = dict.fromkeys(TOKEN_FIELDS, 0)
    reasoning = 0
    for agent in agents:
        identity = agent["id"]
        require(sorted(agent["children"]) == sorted(expected_children[identity]), "missing or undeclared child agent")
        require(agent["status"] in {"completed", "failed", "cancelled"}, "agent is not terminal")
        for field in ("provider", "model", "harness", "settings"):
            require(agent[field] == model[field], f"agent {field} differs from frozen settings")
        duration = number(agent["seconds"], "agent seconds")
        require(duration <= wall_seconds, "agent duration exceeds attempt wall time")
        seconds += duration
        require(type(agent["usage_complete"]) is bool, "usage completeness must be boolean")
        if not agent["usage_complete"]:
            missing.append(f"{identity}: incomplete invocation ledger")
        require(isinstance(agent["invocations"], list), "invocations must be a list")
        if not agent["invocations"]:
            missing.append(f"{identity}: no provider usage")
        for invocation in agent["invocations"]:
            turn = text(invocation["id"], "invocation id")
            require(turn not in turns, "duplicate invocation (possible double counting)")
            turns.add(turn)
            artifact(root, invocation["raw_usage"])
            counters = invocation["tokens"]
            for field in (*TOKEN_FIELDS, "reasoning"):
                value = counters.get(field)
                if value is None:
                    missing.append(f"{identity}/{turn}: {field}")
                    if field != "reasoning":
                        totals[field] = None
                    else:
                        reasoning = None
                else:
                    number(value, field, integer=True)
                    if field == "reasoning":
                        if reasoning is not None:
                            reasoning += value
                    elif totals[field] is not None:
                        totals[field] += value
            if counters.get("reasoning") is not None and counters.get("output") is not None:
                require(counters["reasoning"] <= counters["output"], "reasoning must be a subset of output")
    complete = all(agent["usage_complete"] and agent["invocations"] for agent in agents)
    complete = bool(complete and all(value is not None for value in totals.values()))
    rates = model["pricing"]["usd_per_million"]
    estimate = sum(totals[key] * rates[key] / 1_000_000 for key in TOKEN_FIELDS) if complete else None
    return {"tokens": {**totals, "reasoning": reasoning}, "usage_complete": complete,
            "missing": missing, "estimated_usd": estimate, "agent_seconds": seconds,
            "agents": len(agents), "invocations": len(turns)}


def measurements(value):
    require(isinstance(value, dict) and set(value) == set(MEASUREMENTS), "measurement fields differ")
    for key, amount in value.items():
        if amount is not None:
            number(amount, key, integer=key != "tool_cpu_seconds")
    return [key for key, amount in value.items() if amount is None]
