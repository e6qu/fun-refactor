"""Observed native tool work, including prefixes retained after a resource stop."""
import json
import io
import re

from . import native_changes as changes, native_discovery as discovery, native_mcp as mcp
from .source_disclosure import overlap
from .study import encode, number, require


def prefix(raw, limit):
    require(isinstance(raw, bytes) and len(raw) <= limit, "retained stream exceeds budget")
    rows, tail = [], 0
    lines = raw.split(b"\n")
    for index, line in enumerate(lines):
        if not line.strip():
            continue
        try:
            row = mcp.decode(line)
        except (json.JSONDecodeError, UnicodeError):
            require(index == len(lines) - 1 and not raw.endswith(b"\n"), "invalid complete JSONL record")
            tail = len(line)
            break
        require(isinstance(row, dict), "JSONL record must be an object")
        rows.append(row)
    return rows, tail


def usage(finishes):
    totals = {"input": 0, "output": 0, "reasoning": 0, "cache_read": 0, "cache_write": 0}
    cost = 0
    for finish in finishes:
        tokens = finish["tokens"]
        values = {"input": tokens["input"], "output": tokens["output"], "reasoning": tokens["reasoning"],
                  "cache_read": tokens["cache"]["read"], "cache_write": tokens["cache"]["write"]}
        for key, count in values.items():
            totals[key] += number(count, "reported tokens", integer=True)
        cost += number(finish["cost"], "reported cost")
    return {"finished_steps": len(finishes), "reported_tokens": totals, "reported_cost": cost,
            "actual_usd": None, "provider_usage_verified": False,
            "scope": "CLI-reported completed steps only. Fields retain provider semantics; do not add reasoning to output."}


class ReadOnlyReplay:
    """Reuse the native read server while independently rebuilding source results."""
    def __init__(self, files, arm, version):
        self.files, self.edits, self.checks = files, 0, None
        self.retained = None
        self.server = mcp.Server({"files": files, "arm": arm, "binary": "fr", "workspace": "/unused",
                                  "tools_schema_version": version}, io.BytesIO(), self.execute)

    @property
    def finished(self):
        return self.server.finished

    def execute(self, *args):
        return encode({key: value for key, value in self.retained.items() if key != "source_refs"})

    def call(self, params, retained):
        self.retained = retained
        response = self.server.call(params)
        return mcp.decode(response["content"][0]["text"])


def observed(raw, host, task, arm, version, *, read_only=False):
    events, stream_tail = prefix(raw, 1024**2)
    rows, host_tail = prefix(host, mcp.MAX_LOG)
    require(len(rows) <= mcp.MAX_CALLS, "host call budget exceeded")
    session, starts, finishes, tools, finished = None, [], [], [], set()
    for event in events:
        kind, sid = event["type"], event["sessionID"]
        require(re.fullmatch(r"ses_[A-Za-z0-9]+", sid), "invalid partial session")
        if session is None:
            session = sid
        require(sid == session, "mixed partial sessions")
        require(kind in {"step_start", "step_finish", "tool_use", "text", "reasoning"}, "unexpected partial event")
        part, mid = event["part"], event["part"]["messageID"]
        require(isinstance(mid, str) and mid, "missing partial message identity")
        if "sessionID" in part:
            require(part["sessionID"] == session, "partial part session differs")
        if kind == "step_start":
            require(mid not in starts and len(starts) < 12, "duplicate or excess step start")
            starts.append(mid)
        else:
            require(mid in starts and mid not in finished, "event outside an active step")
        if kind == "step_finish":
            require(part["reason"] in {"tool-calls", "stop"}, "unfinished reported step")
            finished.add(mid)
            finishes.append(part)
        elif kind == "tool_use":
            require(isinstance(part["callID"], str) and part["callID"], "missing partial call identity")
            require(part["callID"] not in {p["callID"] for p in tools}, "duplicate partial call identity")
            tools.append(part)
    remaining, matched = list(tools), set()
    require(type(read_only) is bool, "invalid native tool mode")
    machine = (ReadOnlyReplay(task["files"], arm, version) if read_only else
               changes.Machine(task["files"], arm, replay=True, version=version, public_check=task.get("public_check")))
    ranges, delivered_ranges = {}, {}
    counters = {"host_calls": len(rows), "native_confirmed_results": 0, "fr_calls": 0, "author_applies": 0,
                "refused_calls": 0, "produced_result_bytes": 0, "native_confirmed_result_bytes": 0,
                "edit_argument_bytes": 0, "source_page_bytes": 0, "repeated_source_page_bytes": 0,
                "native_confirmed_source_page_bytes": 0, "native_confirmed_repeated_source_page_bytes": 0}
    for sequence, row in enumerate(rows, 1):
        require(type(row["sequence"]) is int and row["sequence"] == sequence, "partial host sequence differs")
        result, params = row["result"], row["params"]
        require(isinstance(result, dict), "partial host result must be an object")
        response = {"content": [{"type": "text", "text": encode(result).decode()}], "isError": "error" in result}
        require(row["response"] == response, "partial host response differs")
        require(machine.call(params, result) == result, "partial host replay differs")
        match = next((part for part in remaining if part["tool"] == "rehearsal_" + params["name"]
            and part["state"].get("input") == params.get("arguments", {})
            and part["state"].get("status") == ("error" if response["isError"] else "completed")
            and part["state"].get("error" if response["isError"] else "output", "").strip() == response["content"][0]["text"]), None)
        if match is not None:
            remaining.remove(match)
            matched.add(sequence)
        name, size = params["name"], len(encode(result))
        counters["produced_result_bytes"] += size
        counters["native_confirmed_result_bytes"] += size if match is not None else 0
        counters["fr_calls"] += name.startswith("fr_")
        counters["refused_calls"] += "error" in result
        counters["author_applies"] += name == "fr_apply_preview" and "error" not in result
        if name in {"replace_source", "fr_preview_body", "fr_apply_preview"}:
            counters["edit_argument_bytes"] += len(encode(params.get("arguments", {})))
        elif name not in {"submit_patch", "submit_answer", "describe_checks", "run_checks"} and "error" not in result:
            for span in discovery.disclosed(machine.files, mcp.request(name, params.get("arguments", {})), result):
                key, size = (span["path"], span["sha256"]), span["end"] - span["start"]
                counters["source_page_bytes"] += size
                counters["repeated_source_page_bytes"] += overlap(ranges.setdefault(key, []), span["start"], span["end"])
                if match is not None:
                    counters["native_confirmed_source_page_bytes"] += size
                    counters["native_confirmed_repeated_source_page_bytes"] += overlap(delivered_ranges.setdefault(key, []), span["start"], span["end"])
    require(not remaining, "native result absent from retained host calls")
    counters.update(native_confirmed_results=len(matched), produced_only_results=len(rows) - len(matched),
                    accepted_edits=machine.edits, submission_observed=machine.finished)
    if machine.checks is not None:
        counters["public_checks"] = machine.checks.summary()
    return {"session": session, "observed": counters, "usage": usage(finishes),
            "coverage": {"started_steps": len(starts), "unfinished_steps": len(starts) - len(finishes),
                         "unparsed_stream_tail_bytes": stream_tail, "unparsed_host_tail_bytes": host_tail,
                         "source_pages_exclude_author_diffs": not read_only, "complete_context_accounting": False,
                         "export_and_model_identity_verified": False},
            "scope": "Retained host work and native stream confirmations. Incomplete streams provide observed lower bounds, never zero-cost failures."}
