"""A trusted host path from a frozen request profile to retained provider usage."""
from __future__ import annotations

import copy
import hashlib
import http.client
import json
import os
from pathlib import Path
import re
import time

from .study import digest, encode, number, require, text
from .provider_usage import normalize

MAX_JSON = 16 * 1024 * 1024
ENDPOINTS = {
    "openai": ("api.openai.com", "/v1/responses", "/v1/responses/input_tokens", "OPENAI_API_KEY", "openai-response-1"),
    "anthropic": ("api.anthropic.com", "/v1/messages", "/v1/messages/count_tokens", "ANTHROPIC_API_KEY", "anthropic-message-1"),
}


def decode(data):
    def unique(pairs):
        result = {}
        for key, value in pairs:
            require(key not in result, "duplicate provider JSON key")
            result[key] = value
        return result
    return json.loads(data, object_pairs_hook=unique,
                      parse_constant=lambda value: require(False, "nonfinite provider JSON"))


def write_json(path, value):
    data = encode(value)
    require(len(data) <= MAX_JSON, "host artifact exceeds size limit")
    with path.open("xb") as output:
        os.chmod(path, 0o600)
        output.write(data)
        output.flush()
        os.fsync(output.fileno())
    return {"path": path.name, "sha256": hashlib.sha256(data).hexdigest()}


class HTTPS:
    """Fixed origins, no redirects or retries, socket timeouts and bounded bodies."""
    def __call__(self, provider, path, payload, timeout):
        host, generate, count, variable, _ = ENDPOINTS[provider]
        require(path in {generate, count}, "unexpected provider route")
        key = os.environ.get(variable)
        require(key and "\n" not in key and "\r" not in key, f"missing or invalid {variable}")
        headers = {"Content-Type": "application/json", "Accept": "application/json"}
        if provider == "openai":
            headers["Authorization"] = f"Bearer {key}"
        else:
            headers.update({"x-api-key": key, "anthropic-version": "2023-06-01"})
        connection = http.client.HTTPSConnection(host, timeout=timeout)
        started = time.monotonic()
        try:
            connection.request("POST", path, body=encode(payload), headers=headers)
            response = connection.getresponse()
            require(response.status == 200, f"provider HTTP status {response.status}; no automatic retry")
            data = bytearray()
            while True:
                remaining = timeout - (time.monotonic() - started)
                if remaining <= 0:
                    raise TimeoutError("provider deadline exceeded")
                if connection.sock is not None:
                    connection.sock.settimeout(remaining)
                block = response.read1(min(65536, MAX_JSON + 1 - len(data)))
                if not block:
                    break
                data.extend(block)
                require(len(data) <= MAX_JSON, "provider response exceeds size limit")
            return decode(data)
        except (OSError, http.client.HTTPException) as error:
            raise RuntimeError(f"provider transport failed ({type(error).__name__}); charge may be unknown") from None
        finally:
            connection.close()


def text_blocks(content, provider):
    if isinstance(content, str):
        return
    require(isinstance(content, list), "message content must be text or blocks")
    for block in content:
        require(isinstance(block, dict), "invalid content block")
        kind = block.get("type")
        if kind in {"text", "input_text", "output_text"}:
            require(isinstance(block.get("text"), str), "invalid text block")
            allowed = {"type", "text", "cache_control", "annotations"}
            if provider == "openai" and kind == "output_text":
                allowed.add("logprobs")
            require(set(block) <= allowed, "unsupported text block fields")
        elif provider == "anthropic" and kind == "tool_use":
            require(set(block) <= {"type", "id", "name", "input"}, "unsupported tool-use fields")
        elif provider == "anthropic" and kind == "tool_result":
            require(set(block) <= {"type", "tool_use_id", "content", "is_error", "cache_control"}, "unsupported tool-result fields")
            text_blocks(block.get("content", ""), provider)
        else:
            raise ValueError("only text and client tool messages are supported")
        if "cache_control" in block:
            require(block["cache_control"] in ({"type": "ephemeral"}, {"type": "ephemeral", "ttl": "5m"}),
                    "only five-minute cache writes have a pricing profile")


def prepare(model, supplied):
    require(model["harness"] == "fr-study-api-1", "model needs the fr-study-api-1 host profile")
    provider = model["provider"]
    require(provider in ENDPOINTS, "unsupported provider")
    settings = model["settings"]
    require(set(settings) == {"request", "max_output_tokens", "input_token_ceiling", "input_bound_source"},
            "API host settings differ from the supported profile")
    maximum = number(settings["max_output_tokens"], "output limit", integer=True, positive=True)
    ceiling = number(settings["input_token_ceiling"], "input ceiling", integer=True, positive=True)
    text(settings["input_bound_source"], "input bound source")
    options = copy.deepcopy(settings["request"])
    require(isinstance(options, dict), "request settings must be an object")
    allowed = ({"reasoning", "temperature", "top_p", "service_tier", "tools", "tool_choice", "parallel_tool_calls"}
               if provider == "openai" else {"temperature", "top_p", "top_k", "thinking", "output_config", "tools", "tool_choice", "service_tier"})
    require(set(options) <= allowed, "unsupported frozen provider option")
    # Reasoning summaries cannot reconstruct Anthropic signed thinking blocks yet.
    require(provider != "anthropic" or "thinking" not in options, "Anthropic thinking replay needs a separate adapter")
    tools = options.get("tools", [])
    require(isinstance(tools, list), "tools must be a list")
    for tool in tools:
        require(isinstance(tool, dict), "invalid tool")
        if provider == "openai":
            require(tool.get("type") == "function", "only client function tools are supported")
        else:
            require("type" not in tool and "cache_control" not in tool and "input_schema" in tool,
                    "only uncached client tools are supported")
    require(isinstance(supplied, dict), "request input must be an object")
    allowed_input = {"input", "instructions"} if provider == "openai" else {"messages", "system"}
    require(set(supplied) <= allowed_input, "request cannot override frozen settings or reference external state")
    messages = supplied["input" if provider == "openai" else "messages"]
    if isinstance(messages, str):
        require(provider == "openai", "Anthropic messages must be a list")
    else:
        require(isinstance(messages, list), "messages must be a list")
        for message in messages:
            require(isinstance(message, dict), "invalid message")
            if provider == "openai" and message.get("type") in {"function_call", "function_call_output"}:
                require(set(message) <= {"type", "call_id", "name", "arguments", "output", "id", "status"}, "unsupported function item")
                if "output" in message:
                    require(isinstance(message["output"], str), "function output must be text")
            elif provider == "openai" and message.get("type") == "reasoning":
                require(set(message) <= {"type", "id", "summary", "content", "encrypted_content", "status"}, "unsupported reasoning fields")
                require(isinstance(message.get("encrypted_content"), str) and message["encrypted_content"],
                        "stateless reasoning replay requires encrypted content")
                require(isinstance(message.get("summary"), list), "invalid reasoning summary")
            else:
                allowed_message = {"role", "content", "type", "id", "status", "phase"} if provider == "openai" else {"role", "content", "type"}
                require(set(message) <= allowed_message, "unsupported message fields")
                require(message.get("type", "message") == "message", "unsupported input item")
                text_blocks(message["content"], provider)
    if "system" in supplied:
        text_blocks(supplied["system"], provider)
    if "instructions" in supplied:
        require(isinstance(supplied["instructions"], str), "instructions must be text")
    request = {**copy.deepcopy(supplied), **options, "model": model["model"]}
    count_request = copy.deepcopy(request)
    if provider == "openai":
        count_request = {key: value for key, value in count_request.items()
                         if key in {"model", "input", "instructions", "tools", "tool_choice"}}
        request.update(max_output_tokens=maximum, store=False, stream=False, background=False)
    else:
        count_request = {key: value for key, value in count_request.items() if key in {"model", "messages", "system", "tools", "tool_choice"}}
        request.update(max_tokens=maximum, stream=False)
    require(len(encode(request)) <= MAX_JSON, "request exceeds size limit")
    return request, count_request, ceiling, maximum


def admit_agent(ledger, cell, agent, parent):
    text(agent, "agent identity")
    if parent is not None:
        text(parent, "parent identity")
    with ledger.transaction() as db:
        ledger.ready(db)
        require(db.execute("SELECT state FROM attempts WHERE cell=?", (cell,)).fetchone() == ("open",), "attempt is not open")
        db.execute("CREATE TABLE IF NOT EXISTS host_agents (cell TEXT, id TEXT, parent TEXT, PRIMARY KEY(cell,id))")
        existing = db.execute("SELECT parent FROM host_agents WHERE cell=? AND id=?", (cell, agent)).fetchone()
        if existing is not None:
            require(existing == (parent,), "agent parent changed")
            return
        require(db.execute("SELECT 1 FROM host_agents WHERE id=?", (agent,)).fetchone() is None,
                "agent session reused across attempts")
        roster = dict(db.execute("SELECT id,parent FROM host_agents WHERE cell=?", (cell,)))
        if parent is None:
            require(not roster, "attempt already has a root agent")
        else:
            require(parent in roster, "parent must be admitted before its child")
        maximum = ledger.frozen["manifest"]["max_children"] if ledger.cells[cell]["mode"] == "delegated" else 0
        require(len(roster) < maximum + 1, "child admission limit exceeded")
        db.execute("INSERT INTO host_agents VALUES (?,?,?)", (cell, agent, parent))


def send(ledger, cell, agent, identity, supplied, directory, *, parent=None, transport=None, timeout=60):
    """All retries require new identities; an ambiguous send retains its full hold."""
    require(re.fullmatch(r"[a-zA-Z0-9][a-zA-Z0-9_-]{0,95}", identity) is not None, "invalid request identity")
    number(timeout, "request deadline", positive=True)
    require(timeout <= 120, "request deadline exceeds 120 seconds")
    require(cell in ledger.cells, "unplanned cell")
    model = ledger.models[ledger.cells[cell]["model"]]
    request, count_request, ceiling, output = prepare(model, supplied)
    admit_agent(ledger, cell, agent, parent)
    transport = transport or HTTPS()
    provider = model["provider"]
    _, endpoint, count_endpoint, _, format_name = ENDPOINTS[provider]
    folder = Path(directory) / identity
    folder.mkdir(parents=True, mode=0o700, exist_ok=False)
    state = "not_dispatched"
    started = time.monotonic()
    evidence = {"schema": "fr-agent-study-request-1", "cell": cell, "agent": agent, "request": identity,
                "provider": provider, "model": model["model"], "format": format_name,
                "plan_sha256": digest(ledger.frozen), "parent": parent,
                "harness": model["harness"], "settings": copy.deepcopy(model["settings"]),
                "started_at": time.time()}
    reserved = False
    def retain(name, value):
        reference = write_json(folder / name, value)
        reference["path"] = f"{identity}/{name}"
        return reference
    try:
        evidence["payload"] = retain("request.json", request)
        counted = transport(provider, count_endpoint, count_request, timeout)
        evidence["count"] = retain("count.json", counted)
        estimate = number(counted["input_tokens"], "provider input count", integer=True)
        require(estimate <= ceiling, "input count exceeds frozen ceiling")
        if provider == "openai":
            require(counted.get("object") == "response.input_tokens", "invalid OpenAI input count response")
        bound = estimate if provider == "openai" else ceiling
        evidence["reservation"] = ledger.reserve(cell, identity, agent, bound, output)
        reserved = True
        ledger.dispatch(identity)
        state = "dispatched"
        response = transport(provider, endpoint, request, timeout)
        evidence["response"] = retain("response.json", response)
        state = ledger.settle_response(identity, format_name, response)
        observed = normalize(format_name, response, model["model"])
        evidence["invocation"] = {"id": observed["id"], "format": format_name,
                                  "tokens": observed["tokens"], "raw_usage": evidence["response"]}
        evidence["state"] = state
        return evidence
    except BaseException as error:
        if state == "dispatched":
            with ledger.transaction() as db:
                db.execute("UPDATE calls SET state='unknown' WHERE id=? AND state='dispatched'", (identity,))
            state = "unknown"
        elif reserved and state == "not_dispatched":
            ledger.cancel(identity)
        evidence.update(state=state, error_type=type(error).__name__)
        raise
    finally:
        evidence["elapsed_seconds"] = time.monotonic() - started
        evidence["state"] = state
        write_json(folder / "receipt.json", evidence)
