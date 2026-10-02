"""Match native OpenCode events, exported messages and serialized host calls."""
import re
from . import native_mcp as mcp
from .study import encode, number, require

LIMITS = {"steps": 12}


def audit(raw, exported, rows, prompt, task, cell):
    events = [mcp.decode(line) for line in raw.splitlines() if line.strip()]
    require(events and events[0]["type"] == "step_start" and events[-1]["type"] == "step_finish", "incomplete native stream")
    session = exported["info"]["id"]
    require(re.fullmatch(r"ses_[A-Za-z0-9]+", session) is not None, "invalid session")
    require(all(row.get("sessionID") == session for row in events), "mixed sessions")
    require(all(row["type"] in {"step_start", "step_finish", "tool_use", "text", "reasoning"} for row in events), "unexpected native event")
    users = [row for row in exported["messages"] if row["info"]["role"] == "user"]
    require(len(users) == 1 and exported["messages"][0] is users[0], "unexpected user messages")
    require(all(part["type"] == "text" for part in users[0]["parts"])
            and "".join(part["text"] for part in users[0]["parts"]) == prompt + "\nTask:\n" + task["requirement"], "exported task prompt differs")
    assistants = [row for row in exported["messages"] if row["info"]["role"] == "assistant"]
    require(len(exported["messages"]) == len(assistants) + 1, "unexpected message role")
    finishes = [row["part"] for row in events if row["type"] == "step_finish"]
    starts = [row["part"] for row in events if row["type"] == "step_start"]
    require(0 < len(assistants) == len(finishes) == len(starts) <= LIMITS["steps"], "assistant count differs")
    ids, tokens, cost = [], [], 0
    for index, (message, finish, start) in enumerate(zip(assistants, finishes, starts)):
        info = message["info"]
        require(all(part["messageID"] == info["id"] for part in message["parts"]), "exported part belongs to another message")
        require(info["id"] == start["messageID"] == finish["messageID"] and info["id"] not in ids, "message identity differs")
        require(info["providerID"] + "/" + info["modelID"] == cell["model"], "model differs")
        require(info.get("finish") == finish["reason"] and finish["reason"] in {"tool-calls", "stop"}, "unfinished model response")
        require(info["tokens"] == finish["tokens"] and info["cost"] == finish["cost"], "usage differs")
        usage = info["tokens"]
        for count in [usage["input"], usage["output"], usage["reasoning"], usage["cache"]["read"], usage["cache"]["write"]]:
            number(count, "tokens", integer=True)
        cost += number(info["cost"], "reported cost")
        ids.append(info["id"])
        tokens.append(usage)
    require(finishes[-1]["reason"] == "stop", "native session did not finish")
    require(all(row["part"]["messageID"] in ids for row in events), "unaccounted message")
    emitted = [row["part"] for row in events if row["type"] == "tool_use"]
    saved = [part for message in assistants for part in message["parts"] if part["type"] == "tool"]
    require(len(emitted) == len(saved) == len(rows) <= mcp.MAX_CALLS, "tool call count differs")
    require(len({part["callID"] for part in saved}) == len(saved), "duplicate call identity")
    by_id = {part["callID"]: part for part in saved}
    require(len({part["callID"] for part in emitted}) == len(emitted) and {part["callID"] for part in emitted} == set(by_id), "stream call identities differ")
    for part in emitted:
        require(part == by_id.get(part["callID"]), "stream and export tool differ")
    # Match by arguments and exact result, allowing parallel calls to complete in
    # a different order. Duplicate calls still consume distinct host records.
    remaining = list(saved)
    matched = []
    for sequence, row in enumerate(rows, 1):
        require(row["sequence"] == sequence, "host sequence differs")
        response = {"content": [{"type": "text", "text": encode(row["result"]).decode()}], "isError": "error" in row["result"]}
        require(row["response"] == response, "host response differs")
        match = next((part for part in remaining if part["tool"] == "rehearsal_" + row["params"]["name"]
                      and part["state"].get("input") == row["params"].get("arguments", {})
                      and part["state"].get("status") == ("error" if response["isError"] else "completed")
                      and part["state"].get("error" if response["isError"] else "output", "").strip() == response["content"][0]["text"]), None)
        require(match is not None, "host result absent from native export")
        remaining.remove(match)
        matched.append((row, ids.index(match["messageID"])))
    return matched, {"tokens": tokens, "reported_cost": cost, "session": session}
