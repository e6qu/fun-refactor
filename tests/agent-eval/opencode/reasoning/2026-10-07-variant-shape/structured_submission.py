"""Audit a terminal OpenCode answer against events, messages and MCP records."""
import re

from . import native_mcp as mcp
from .study import encode, number, require

SCHEMA = "fr-structured-submission-1"
VERSION = "1.18.34"
PROMPT = "Read module.py using read_source, then return its value with StructuredOutput."
FORMAT = {"type": "json_schema", "retryCount": 0, "schema": {
    "type": "object", "properties": {"answer": {"type": "object"}},
    "required": ["answer"], "additionalProperties": False}}


def variant(value):
    require(isinstance(value, str) and re.fullmatch(r"[a-zA-Z0-9][a-zA-Z0-9_-]{0,63}", value),
            "invalid model variant")
    return value


def audit(request, terminal, messages, events, rows):
    """This format has no synthetic stop turn or synthetic MCP submission."""
    require(set(request) - {"variant"} == {"model", "agent", "format", "parts"}
            and request["agent"] == "fr-submission" and request["format"] == FORMAT, "request differs")
    if "variant" in request:
        variant(request["variant"])
    require(set(request["model"]) == {"providerID", "modelID"}
            and all(isinstance(v, str) and re.fullmatch(r"[\w./-]+", v) for v in request["model"].values()),
            "invalid requested model")
    require(len(request["parts"]) == 1 and set(request["parts"][0]) == {"type", "text"}
            and request["parts"][0]["type"] == "text" and isinstance(request["parts"][0]["text"], str)
            and 0 < len(request["parts"][0]["text"].encode()) <= 16384, "invalid requested prompt")
    require(2 <= len(messages) <= 13, "message budget exceeded")
    user, *assistants = messages
    session = user["info"]["sessionID"]
    require(re.fullmatch(r"ses_[A-Za-z0-9]+", session) is not None, "invalid session")
    require(user["info"]["role"] == "user" and user["info"]["format"] == FORMAT
            and user["info"]["model"] == request["model"] and user["info"]["agent"] == request["agent"],
            "user format differs")
    require([{"type": p["type"], "text": p.get("text")} for p in user["parts"]]
            == request["parts"], "user prompt differs")
    require(user["info"].get("variant") == request.get("variant"), "user variant differs")
    require(terminal == assistants[-1], "terminal response and messages differ")
    observed_parts, observed_info, idle = {}, {}, False
    for event in events:
        kind, props = event["type"], event.get("properties", {})
        if kind == "message.part.updated":
            part = props["part"]
            require(part["sessionID"] == session, "event session differs")
            observed_parts[part["id"]] = part
        elif kind == "message.updated":
            info = props["info"]
            require(info["sessionID"] == session, "event session differs")
            observed_info[info["id"]] = info
        elif kind == "session.status" and props.get("sessionID") == session:
            idle = props["status"]["type"] == "idle"
        elif kind in {"session.error", "message.removed", "message.part.removed"}:
            raise ValueError("error or removed evidence in event stream")
    require(idle, "session never reached idle")
    require(set(observed_info) == {m["info"]["id"] for m in messages}, "unaccounted messages")
    parts = [p for message in messages for p in message["parts"]]
    require(len({p["id"] for p in parts}) == len(parts), "duplicate part identity")
    require(set(observed_parts) == {p["id"] for p in parts}, "unaccounted parts")
    tokens, cost, tools, ids = [], 0, [], set()
    for index, message in enumerate(messages):
        info = message["info"]
        require(info["id"] not in ids and info["sessionID"] == session, "message identity differs")
        ids.add(info["id"])
        require(observed_info[info["id"]] == info, "event message differs")
        for part in message["parts"]:
            require(part["messageID"] == info["id"] and part["sessionID"] == session,
                    "part identity differs")
            require(observed_parts[part["id"]] == part, "event part differs")
        if index == 0:
            continue
        require(info["role"] == "assistant" and "error" not in info, "failed assistant response")
        require({key: info[key] for key in ("providerID", "modelID")} == request["model"], "model differs")
        require(info["parentID"] == user["info"]["id"], "unexpected parent message")
        require(info["finish"] in {"stop", "tool-calls"} and info["time"].get("completed"),
                "incomplete assistant response")
        require(all(p["type"] in {"step-start", "step-finish", "text", "reasoning", "tool"}
                    for p in message["parts"]), "unexpected assistant part")
        starts = [p for p in message["parts"] if p["type"] == "step-start"]
        finishes = [p for p in message["parts"] if p["type"] == "step-finish"]
        require(len(starts) == len(finishes) == 1, "step count differs")
        finish = finishes[0]
        require(finish["reason"] == info["finish"] and finish["tokens"] == info["tokens"]
                and finish["cost"] == info["cost"], "usage or finish differs")
        usage = info["tokens"]
        for count in [usage["input"], usage["output"], usage["reasoning"],
                      usage["cache"]["read"], usage["cache"]["write"]]:
            number(count, "tokens", integer=True)
        if "total" in usage:
            number(usage["total"], "total tokens", integer=True)
        tokens.append(usage)
        cost += number(info["cost"], "reported cost")
        tools.extend((index, p) for p in message["parts"] if p["type"] == "tool")
    require(len(tools) <= mcp.MAX_CALLS and len({p["callID"] for _, p in tools}) == len(tools),
            "duplicate or excessive calls")
    submissions = [(i, p) for i, p in tools if p["tool"] == "StructuredOutput"]
    require(len(submissions) == 1, "expected exactly one structured submission")
    submitted_at, submission = submissions[0]
    require(submitted_at == len(messages) - 1, "assistant continued after submission")
    require(sum(i == submitted_at for i, _ in tools) == 1, "other calls alongside submission")
    state = submission["state"]
    require(state["status"] == "completed" and state["output"] == "Structured output captured successfully.",
            "submission failed")
    mcp.arguments(FORMAT["schema"], state["input"])
    require(len(encode(state["input"])) <= 16384, "answer exceeds budget")
    require(state["input"] == terminal["info"].get("structured"), "structured answer differs")
    require(all("structured" not in m["info"] for m in assistants[:-1]), "earlier structured answer")
    ordinary = [(i, p) for i, p in tools if p["tool"] != "StructuredOutput"]
    require(len(ordinary) == len(rows), "host call count differs")
    for sequence, row in enumerate(rows, 1):
        require(row["sequence"] == sequence, "host sequence differs")
        response = {"content": [{"type": "text", "text": encode(row["result"]).decode()}],
                    "isError": "error" in row["result"]}
        require(row["response"] == response, "host response differs")
        matches = [(i, p) for i, p in ordinary if p["tool"] == "rehearsal_" + row["params"]["name"]
                   and p["state"].get("input") == row["params"].get("arguments", {})
                   and p["state"].get("status") == ("error" if response["isError"] else "completed")
                   and p["state"].get("error" if response["isError"] else "output", "").strip()
                   == response["content"][0]["text"]]
        require(matches, "host call absent from session")
        match = matches[0]
        require(match[0] < submitted_at, "source unavailable before submission")
        ordinary.remove(match)
    return {"schema": SCHEMA, "session": session, "answer": state["input"]["answer"],
            "tool_calls": len(tools), "assistant_responses": len(assistants),
            "tokens": tokens, "reported_cost": cost, "provider_usage_verified": False}
