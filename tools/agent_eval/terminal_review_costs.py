"""Replay source reads and count retained work without requiring a session export."""
from . import native_costs, native_discovery as discovery, native_mcp as mcp
from .study import encode, require


def source(rows, task):
    require(len(rows) <= mcp.MAX_CALLS, "host call budget exceeded")
    machine = native_costs.ReadOnlyReplay(task["files"], "fr", 6)
    del machine.server.tools["submit_answer"]
    spans, produced, fr_calls = [], 0, 0
    for sequence, row in enumerate(rows, 1):
        require(type(row["sequence"]) is int and row["sequence"] == sequence, "host sequence differs")
        params, result = row["params"], row["result"]
        require(isinstance(result, dict) and set(params) <= {"name", "arguments", "_meta"}, "invalid host record")
        require(row["response"] == {"content": [{"type": "text", "text": encode(result).decode()}],
                "isError": "error" in result}, "host response differs")
        require(machine.call(params, result) == result, "source replay differs")
        produced += len(encode(result))
        fr_calls += params["name"].startswith("fr_")
        if "error" not in result:
            spans.extend(discovery.disclosed(task["files"],
                mcp.request(params["name"], params.get("arguments", {})), result))
    return {"spans": spans, "host_calls": len(rows), "fr_calls": fr_calls,
            "produced_result_bytes": produced}


def observed(raw, host, task):
    events, stream_tail = native_costs.prefix(raw, 1024**2)
    rows, host_tail = native_costs.prefix(host, mcp.MAX_LOG)
    replayed = source(rows, task)
    infos, parts, session = {}, {}, None
    for event in events:
        kind, props = event["type"], event.get("properties", {})
        value = props.get("info") if kind == "message.updated" else props.get("part") if kind == "message.part.updated" else None
        if value is None:
            continue
        require(isinstance(value, dict) and isinstance(value["id"], str), "invalid event identity")
        session = session or value["sessionID"]
        require(value["sessionID"] == session, "mixed partial sessions")
        (infos if kind == "message.updated" else parts)[value["id"]] = value
    tools = [p for p in parts.values() if p["type"] == "tool"]
    require(len({p["callID"] for p in tools}) == len(tools), "duplicate partial tool identity")
    finished = [p for p in parts.values() if p["type"] == "step-finish"]
    remaining = [p for p in tools if p["tool"] != "StructuredOutput"]
    confirmed = 0
    for row in rows:
        response = row["response"]
        match = next((p for p in remaining if p["tool"] == "rehearsal_" + row["params"]["name"]
            and p["state"].get("input") == row["params"].get("arguments", {})
            and p["state"].get("status") == ("error" if response["isError"] else "completed")
            and p["state"].get("error" if response["isError"] else "output", "").strip()
                == response["content"][0]["text"]), None)
        if match is not None:
            remaining.remove(match)
            confirmed += 1
    return {"assistant_messages": sum(m["role"] == "assistant" for m in infos.values()),
            "tool_calls": len(tools), "terminal_calls": sum(p["tool"] == "StructuredOutput" for p in tools),
            "host_calls": len(rows), "fr_calls": replayed["fr_calls"],
            "native_confirmed_results": confirmed, "produced_only_results": len(rows) - confirmed,
            "unmatched_native_results": len(remaining), "produced_result_bytes": replayed["produced_result_bytes"],
            "produced_source_bytes": sum(s["end"] - s["start"] for s in replayed["spans"]),
            "usage": native_costs.usage(finished), "unparsed_stream_tail_bytes": stream_tail,
            "unparsed_host_tail_bytes": host_tail, "complete_context_accounting": False,
            "scope": "Retained event and host prefixes only; missing evidence is unobserved work, not zero cost."}
