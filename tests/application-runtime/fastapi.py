import asyncio
import json

from fastapi import FastAPI
from routes import router

app = FastAPI()
app.include_router(router)


async def invoke(case):
    messages = []
    body = json.dumps(case.get("body"), ensure_ascii=False).encode() if "body" in case else b""

    async def receive():
        return {"type": "http.request", "body": body, "more_body": False}

    async def send(message):
        messages.append(message)

    await app({
        "type": "http", "asgi": {"version": "3.0"}, "http_version": "1.1",
        "method": case["method"], "scheme": "http", "path": case["url"].split("?", 1)[0],
        "raw_path": case["url"].split("?", 1)[0].encode(),
        "query_string": case["url"].partition("?")[2].encode(), "root_path": "",
        "headers": [(b"content-type", b"application/json")] if "body" in case else [],
        "client": ("127.0.0.1", 4000), "server": ("local", 80),
    }, receive, send)
    status = next(message["status"] for message in messages if message["type"] == "http.response.start")
    body = b"".join(message.get("body", b"") for message in messages if message["type"] == "http.response.body")
    return {"status": status, "body": json.loads(body)}


async def main():
    with open("cases.json", encoding="utf-8") as source:
        cases = json.load(source)
    print(json.dumps([await invoke(case) for case in cases], ensure_ascii=False))


asyncio.run(main())
