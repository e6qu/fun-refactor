import asyncio
import json

from fastapi import FastAPI
from routes import router

app = FastAPI()
app.include_router(router)


async def invoke(url):
    messages = []

    async def receive():
        return {"type": "http.request", "body": b"", "more_body": False}

    async def send(message):
        messages.append(message)

    await app({
        "type": "http", "asgi": {"version": "3.0"}, "http_version": "1.1",
        "method": "GET", "scheme": "http", "path": url.split("?", 1)[0],
        "raw_path": url.split("?", 1)[0].encode(),
        "query_string": url.partition("?")[2].encode(), "root_path": "",
        "headers": [], "client": ("127.0.0.1", 4000), "server": ("local", 80),
    }, receive, send)
    status = next(message["status"] for message in messages if message["type"] == "http.response.start")
    body = b"".join(message.get("body", b"") for message in messages if message["type"] == "http.response.body")
    return {"status": status, "body": json.loads(body)}


async def main():
    print(json.dumps([await invoke("/guarded"), await invoke("/guarded?token=present")]))


asyncio.run(main())
