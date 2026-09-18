import asyncio
import json

from api import app


async def invoke(url, body):
    messages = []
    encoded = json.dumps(body).encode()

    async def receive():
        return {"type": "http.request", "body": encoded, "more_body": False}

    async def send(message):
        messages.append(message)

    path, _, query = url.partition("?")
    await app({
        "type": "http", "asgi": {"version": "3.0"}, "http_version": "1.1",
        "method": "POST", "scheme": "http", "path": path, "raw_path": path.encode(),
        "query_string": query.encode(), "root_path": "",
        "headers": [(b"content-type", b"application/json")],
        "client": ("127.0.0.1", 4000), "server": ("local", 80),
    }, receive, send)
    status = next(message["status"] for message in messages if message["type"] == "http.response.start")
    payload = b"".join(message.get("body", b"") for message in messages if message["type"] == "http.response.body")
    return {"status": status, "body": json.loads(payload)}


async def main():
    valid = await invoke("/records/chosen?limit=12", {"visible": True})
    bad_query = await invoke("/records/chosen?limit=word", {"visible": True})
    missing_body = await invoke("/records/chosen?limit=12", {})
    print(json.dumps({
        "valid": valid,
        "bad_query_status": bad_query["status"],
        "missing_body_status": missing_body["status"],
    }, separators=(",", ":")))


asyncio.run(main())
