"""Normalize retained provider responses; missing accounting remains unknown."""
from __future__ import annotations

import json

from .study import number, require, text

FORMATS = ("openai-response-1", "anthropic-message-1", "codex-turn-1")


def count(value, key):
    require(isinstance(value, dict), "usage details must be an object")
    amount = value.get(key)
    return None if amount is None else number(amount, key, integer=True)


def details(value, key):
    result = value.get(key)
    require(result is None or isinstance(result, dict), "usage details must be an object or null")
    return result or {}


def normalize(format_name, response, expected_model):
    require(format_name in FORMATS, "unsupported provider usage format")
    require(isinstance(response, dict), "provider response must be an object")
    require(response.get("model") == expected_model, "provider model differs from frozen model")
    reported = response.get("usage")
    require(reported is None or isinstance(reported, dict), "provider usage must be an object or null")
    usage = reported or {}
    output = count(usage, "output_tokens")
    issues = []
    pricing_compatible = True
    if format_name == "openai-response-1":
        require(response.get("object") == "response", "expected an OpenAI response")
        identity = text(response["id"], "response id")
        input_details = details(usage, "input_tokens_details")
        total_input = count(usage, "input_tokens")
        read, write = count(input_details, "cached_tokens"), count(input_details, "cache_write_tokens")
        uncached = total_input - read - write if None not in (total_input, read, write) else None
        output_details = details(usage, "output_tokens_details")
        reasoning = count(output_details, "reasoning_tokens")
        if any(tool.get("type") not in {"function", "custom"} for tool in response.get("tools", [])):
            issues.append("hosted tools need separate pricing")
            pricing_compatible = False
        if any(count(part, key) for part in (input_details, output_details)
               for key in ("audio_tokens", "image_tokens", "video_tokens")):
            issues.append("multimodal usage needs separate pricing")
            pricing_compatible = False
    elif format_name == "anthropic-message-1":
        require(response.get("type") == "message", "expected an Anthropic message")
        identity = text(response["id"], "message id")
        uncached = count(usage, "input_tokens")
        read, write = count(usage, "cache_read_input_tokens"), count(usage, "cache_creation_input_tokens")
        total_input = sum((uncached, read, write)) if None not in (uncached, read, write) else None
        reasoning = None
        # The study has one frozen cache-write rate; mixed TTLs need separate rates.
        creation = details(usage, "cache_creation")
        hour = count(creation, "ephemeral_1h_input_tokens")
        if hour:
            issues.append("one-hour cache writes need a separate pricing profile")
            pricing_compatible = False
        server_tools = details(usage, "server_tool_use")
        if any(count(server_tools, key) for key in server_tools):
            issues.append("server tool charges are outside token-only pricing")
            pricing_compatible = False
    else:
        require(response.get("type") == "turn.completed", "expected a completed Codex turn")
        thread = text(response["thread_id"], "thread id")
        index = number(response["turn_index"], "turn index", integer=True)
        identity = f"codex:{thread}:{index}"
        total_input, read = count(usage, "input_tokens"), count(usage, "cached_input_tokens")
        write, uncached = None, None
        reasoning = count(usage, "reasoning_output_tokens")
        issues.append("Codex turn totals do not expose cache writes or observed model identity")
    for label, value in (("uncached input", uncached), ("cache read", read), ("cache write", write)):
        if value is not None:
            number(value, label, integer=True)
    if total_input is not None:
        require(read is None or read <= total_input, "cached input exceeds total input")
        require(write is None or write <= total_input, "cache writes exceed total input")
    if output is not None and reasoning is not None:
        require(reasoning <= output, "reasoning exceeds output")
    total = total_input + output if total_input is not None and output is not None else None
    if "total_tokens" in usage and usage["total_tokens"] is not None:
        stated = count(usage, "total_tokens")
        require(total is None or stated == total, "provider token total is inconsistent")
    tokens = {"uncached_input": uncached, "cache_read": read, "cache_write": write,
              "output": output, "reasoning": reasoning}
    complete = pricing_compatible and all(tokens[key] is not None for key in tokens if key != "reasoning")
    return {"id": identity, "model": expected_model, "tokens": tokens, "total_tokens": total,
            "pricing_compatible": pricing_compatible, "billable_complete": complete, "issues": issues,
            "model_observed": format_name != "codex-turn-1"}


def verify(invocation, data, model):
    """Recompute supplied counters from pinned raw JSON rather than trusting a copy."""
    def unique(pairs):
        result = {}
        for key, value in pairs:
            require(key not in result, "duplicate key in provider response")
            result[key] = value
        return result
    raw = json.loads(data, object_pairs_hook=unique,
                     parse_constant=lambda value: require(False, f"nonfinite provider value: {value}"))
    result = normalize(invocation["format"], raw, model["model"])
    require(invocation["id"] == result["id"], "provider invocation identity differs")
    for value in invocation["tokens"].values():
        if value is not None:
            number(value, "normalized token count", integer=True)
    require(invocation["tokens"] == result["tokens"], "normalized counters differ from provider response")
    provider = "anthropic" if invocation["format"] == "anthropic-message-1" else "openai"
    require(model["provider"] == provider, "provider format differs from frozen provider")
    return result
