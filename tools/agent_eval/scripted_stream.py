"""Equivalent complete and incremental OpenAI-compatible replies for CPU controls."""
import copy
import time

from .study import encode, require

MODES = ("whole", "chunked")
DELIVERY_MODES = ("whole-burst", "whole-paced", "chunked-burst", "chunked-paced")
WIDTH = 12
DELAY = 0.002


def schedule(reply, mode):
    require(mode in DELIVERY_MODES, "unknown delivery mode")
    shape, pacing = mode.split("-")
    payload = frames(reply, shape)
    duration = (len(frames(reply, "chunked")) - 1) * DELAY if pacing == "paced" else 0
    deadlines = ([index * DELAY for index in range(len(payload))]
                 if mode == "chunked-paced" else [duration] * len(payload))
    return payload, deadlines


def write_scheduled(handler, reply, mode, *, clock=time.monotonic, sleep=time.sleep):
    payload, deadlines = schedule(reply, mode)
    handler.send_response(200)
    handler.send_header("Content-Type", "text/event-stream")
    handler.send_header("Content-Length", str(sum(map(len, payload))))
    handler.end_headers()
    start, late = clock(), 0
    for part, deadline in zip(payload, deadlines):
        remaining = start + deadline - clock()
        if remaining > 0:
            sleep(remaining)
        late = max(late, clock() - start - deadline)
        handler.wfile.write(part)
        handler.wfile.flush()
    return {"mode": mode, "frames": len(payload), "scheduled_seconds": deadlines[-1],
            "elapsed_seconds": clock() - start, "max_lateness_seconds": late}


def frames(reply, mode):
    mode = mode.split("-")[0] if mode in DELIVERY_MODES else mode
    require(mode in MODES, "unknown streaming mode")
    choice = reply["choices"][0]
    message = copy.deepcopy(choice["message"])
    common = {k: v for k, v in reply.items() if k not in {"choices", "usage"}}
    common["object"] = "chat.completion.chunk"
    calls = message.pop("tool_calls", [])
    if mode == "whole":
        message["tool_calls"] = [{"index": i, **call} for i, call in enumerate(calls)]
        deltas = [message]
    else:
        content = message.pop("content")
        deltas = [message]
        deltas.extend({"content": content[i:i + WIDTH]} for i in range(0, len(content), WIDTH))
        for index, call in enumerate(calls):
            arguments = call["function"].pop("arguments")
            deltas.append({"tool_calls": [{"index": index, **call, "function": {
                **call["function"], "arguments": ""}}]})
            deltas.extend({"tool_calls": [{"index": index, "function": {
                "arguments": arguments[i:i + WIDTH]}}]} for i in range(0, len(arguments), WIDTH))
    values = [{**common, "choices": [{"index": 0, "delta": delta, "finish_reason": None}]} for delta in deltas]
    values.append({**common, "choices": [{"index": 0, "delta": {},
                   "finish_reason": choice["finish_reason"]}], "usage": reply["usage"]})
    return [b"data: " + encode(value) + b"\n\n" for value in values] + [b"data: [DONE]\n\n"]


def write(handler, reply, mode):
    if mode in DELIVERY_MODES:
        return write_scheduled(handler, reply, mode)
    payload = frames(reply, mode)
    handler.send_response(200)
    handler.send_header("Content-Type", "text/event-stream")
    handler.send_header("Content-Length", str(sum(map(len, payload))))
    handler.end_headers()
    for part in payload:
        handler.wfile.write(part)
        handler.wfile.flush()
        if mode == "chunked":
            time.sleep(DELAY)
