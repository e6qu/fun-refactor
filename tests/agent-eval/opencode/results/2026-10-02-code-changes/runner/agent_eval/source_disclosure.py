"""Bounded host-owned source pages and reproducible disclosure accounting.

This measures text delivered by read_source, not filesystem I/O inside commands.
The trace is a trusted-host attestation, not proof of what an arbitrary tool read.
"""
import base64
import hashlib

from .study import encode, number, require, sha
from .workspace_bundle import validate as validate_bundle


def validate_read(arguments):
    require(isinstance(arguments, dict) and set(arguments) == {"path", "offset", "bytes", "sha256"},
            "read_source needs path, offset, bytes and sha256")
    require(isinstance(arguments["path"], str), "source path must be text")
    validate_bundle({arguments["path"]: {"data": "", "executable": False}})
    number(arguments["offset"], "source offset", integer=True)
    require(number(arguments["bytes"], "source bytes", integer=True, positive=True) <= 65536,
            "source page exceeds 65536 bytes")
    require(arguments["sha256"] == "" or sha(arguments["sha256"], "source"), "invalid source identity")
    require(arguments["offset"] == 0 or arguments["sha256"], "continuation needs source sha256")


def read_source(files, arguments, output_bytes):
    """Read the in-memory export only; no agent path is opened on the host."""
    validate_read(arguments)
    require(number(output_bytes, "source output bytes", integer=True) >= 128, "source output budget too small")
    path, offset = arguments["path"], arguments["offset"]
    if path not in files:
        return {"error": "missing_file"}
    raw = base64.b64decode(files[path]["data"], validate=True)
    identity = hashlib.sha256(raw).hexdigest()
    if arguments["sha256"] and arguments["sha256"] != identity:
        return {"error": "stale_source", "sha256": identity}
    if offset > len(raw):
        return {"error": "offset_out_of_range"}
    try:
        raw.decode("utf-8")
        raw[:offset].decode("utf-8")
    except UnicodeDecodeError:
        return {"error": "invalid_utf8_or_boundary"}
    # A page ends at a complete character, never at a replacement character.
    text = raw[offset:offset + arguments["bytes"]].decode("utf-8", errors="ignore")
    def page(length):
        content = text[:length]
        end = offset + len(content.encode())
        return {"path": path, "sha256": identity, "size_bytes": len(raw), "offset": offset,
                "end_offset": end, "next_offset": end if end < len(raw) else None, "text": content}
    low, high = 0, len(text)
    while low < high:
        middle = (low + high + 1) // 2
        if len(encode(page(middle))) <= output_bytes:
            low = middle
        else:
            high = middle - 1
    result = page(low)
    if len(encode(result)) > output_bytes or (not low and offset < len(raw)):
        return {"error": "page_budget_too_small"}
    return result


def overlap(intervals, start, end):
    repeated = sum(max(0, min(end, right) - max(start, left)) for left, right in intervals)
    merged = []
    for left, right in sorted([*intervals, (start, end)]):
        if merged and left <= merged[-1][1]:
            merged[-1] = (merged[-1][0], max(merged[-1][1], right))
        else:
            merged.append((left, right))
    intervals[:] = merged
    return repeated


def counts(events, tools, *, completed=False, output_bytes=65536):
    """Recompute delivered-byte counters; interrupted tools never imply zero reads."""
    require(isinstance(events, list) and len(events) <= 1024, "invalid or excessive trace events")
    measured = dict(tool_calls=0, tool_result_bytes=0, instruction_bytes=0, handoff_bytes=0, retries=0)
    disclosure = dict(source_read_bytes=0, repeated_read_bytes=0, same_agent_repeated_bytes=0,
                      pages=0, opaque_command_calls=0, failed_read_calls=0, unfinished_read_calls=0)
    agents, pending, seen, ranges, local_ranges = {}, {}, set(), {}, {}
    requests, finished, request_ids = {}, set(), set()
    for event in events:
        require(isinstance(event, dict), "invalid trace event")
        require(sum(key in event for key in ("instructions", "tool_call", "tool_result", "request", "final", "failure_type")) == 1,
                "ambiguous trace event")
        if "instructions" in event:
            agent = event["agent"]
            require(agent not in agents and (event["parent"] is None or event["parent"] in agents),
                    "duplicate or unparented trace agent")
            agents[agent] = event["parent"]
            requests[agent] = []
            measured["instruction_bytes"] += len(event["instructions"].encode()) + len(event["question"].encode()) + len(encode(tools))
        elif "tool_call" in event:
            agent, call = event["agent"], event["tool_call"]
            key = (agent, call["id"])
            require(agent in agents and agent not in finished and key not in seen, "unknown agent or replayed tool call")
            require(call["name"] in {"command", "delegate", "read_source"}, "unknown trace tool")
            seen.add(key)
            pending[key] = call
            measured["tool_calls"] += 1
            if call["name"] == "command":
                disclosure["opaque_command_calls"] += 1
            elif call["name"] == "delegate":
                measured["handoff_bytes"] += len(call["arguments"]["question"].encode())
            else:
                validate_read(call["arguments"])
        elif "tool_result" in event:
            key = (event["agent"], event["tool_result"])
            require(key in pending and event["agent"] not in finished, "orphan or repeated tool result")
            call, result = pending.pop(key), event["result"]
            measured["tool_result_bytes"] += len(encode(result))
            if call["name"] == "delegate":
                measured["handoff_bytes"] += len(result["findings"].encode())
            if call["name"] != "read_source":
                continue
            if "error" in result:
                require(result["error"] in {"missing_file", "stale_source", "offset_out_of_range",
                                           "invalid_utf8_or_boundary", "page_budget_too_small"}, "unknown source refusal")
                require(set(result) == ({"error", "sha256"} if result["error"] == "stale_source" else {"error"}),
                        "source refusal contains disclosure")
                if result["error"] == "stale_source":
                    sha(result["sha256"], "changed source")
                disclosure["failed_read_calls"] += 1
                continue
            args = call["arguments"]
            require(set(result) == {"path", "sha256", "size_bytes", "offset", "end_offset", "next_offset", "text"},
                    "source page fields differ")
            sha(result["sha256"], "source page")
            for field in ("size_bytes", "offset", "end_offset"):
                number(result[field], field, integer=True)
            start, end = result["offset"], result["end_offset"]
            length = len(result["text"].encode())
            require(result["path"] == args["path"] and start == args["offset"]
                    and (not args["sha256"] or args["sha256"] == result["sha256"]), "source page identity differs")
            require(start <= end <= result["size_bytes"] and end - start == length <= args["bytes"]
                    and (length or end == result["size_bytes"]), "source page extent differs")
            if result["next_offset"] is not None:
                number(result["next_offset"], "next offset", integer=True)
            require(result["next_offset"] == (end if end < result["size_bytes"] else None)
                    and len(encode(result)) <= output_bytes, "source continuation or output bound differs")
            identity = (result["path"], result["sha256"])
            disclosure["source_read_bytes"] += length
            disclosure["pages"] += 1
            disclosure["repeated_read_bytes"] += overlap(ranges.setdefault(identity, []), start, end)
            disclosure["same_agent_repeated_bytes"] += overlap(local_ranges.setdefault((key[0], identity), []), start, end)
        elif "request" in event or "final" in event:
            agent = event["agent"]
            require(agent in agents and agent not in finished, "unknown or finished trace agent")
            if "request" in event:
                require(event["request"] not in request_ids, "replayed trace request")
                request_ids.add(event["request"])
                requests[agent].append(event["request"])
            else:
                require(not any(key[0] == agent for key in pending), "agent finished with pending tools")
                finished.add(agent)
        else:
            require(set(event) == {"failure_type"}, "unknown trace event")
    require(not completed or not pending, "completed attempt has unfinished tools")
    require(not completed or set(agents) == finished, "completed attempt has unfinished agents")
    disclosure["unfinished_read_calls"] = sum(call["name"] == "read_source" for call in pending.values())
    disclosure["complete"] = not (disclosure["opaque_command_calls"] or disclosure["unfinished_read_calls"])
    for field in ("source_read_bytes", "repeated_read_bytes", "pages"):
        measured[field] = disclosure[field] if disclosure["complete"] else None
    return {"measurements": measured, "source_disclosure": disclosure,
            "agent_parents": agents, "requests": requests, "completed_agents": sorted(finished)}
