import asyncio
import importlib.util
import json
import sys
import types


class APIRouter:
    def post(self, path):
        return lambda function: function


class BaseModel:
    def __init__(self, **fields):
        self.__dict__.update(fields)


fastapi = types.ModuleType("fastapi")
fastapi.APIRouter = APIRouter
sys.modules["fastapi"] = fastapi
pydantic = types.ModuleType("pydantic")
pydantic.BaseModel = BaseModel
sys.modules["pydantic"] = pydantic
spec = importlib.util.spec_from_file_location("source", "events.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
event = module.EventEnvelope(
    event_id="evt-9",
    sentAt=1757500200,
    labels=["accepted", "priority"],
)
body = asyncio.run(module.publish_event(event))
print(json.dumps({"status": 200, "body": body}, sort_keys=True))
