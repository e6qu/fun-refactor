import asyncio
import importlib.util
import json

from fastapi import FastAPI


spec = importlib.util.spec_from_file_location("source", "events.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
app = FastAPI()
app.include_router(module.router)


async def invoke(payload):
    request_body = json.dumps(payload).encode()
    request_sent = False
    messages = []

    async def receive():
        nonlocal request_sent
        if request_sent:
            return {"type": "http.disconnect"}
        request_sent = True
        return {"type": "http.request", "body": request_body, "more_body": False}

    async def send(message):
        messages.append(message)

    await app(
        {
            "type": "http",
            "asgi": {"version": "3.0", "spec_version": "2.3"},
            "http_version": "1.1",
            "method": "POST",
            "scheme": "http",
            "path": "/events",
            "raw_path": b"/events",
            "query_string": b"",
            "root_path": "",
            "headers": [(b"content-type", b"application/json")],
            "client": ("127.0.0.1", 4000),
            "server": ("local", 80),
        },
        receive,
        send,
    )
    status = next(
        message["status"]
        for message in messages
        if message["type"] == "http.response.start"
    )
    body = b"".join(
        message.get("body", b"")
        for message in messages
        if message["type"] == "http.response.body"
    )
    return {"status": status, "body": json.loads(body)}


async def main():
    valid = await invoke(
        {
            "event_id": "evt-9",
            "sentAt": 1757500200,
            "labels": ["accepted", "priority"],
        }
    )
    invalid = await invoke(
        {
            "event_id": "evt-9",
            "sentAt": 1757500200,
            "labels": [7],
        }
    )
    print(json.dumps({"valid": valid, "invalid": invalid}, sort_keys=True))


asyncio.run(main())
