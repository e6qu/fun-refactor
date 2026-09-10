import asyncio
import importlib.util
import json
import sys
import types


class APIRouter:
    def post(self, path):
        return lambda function: function


class Request:
    async def json(self):
        return {
            "sensor_id": "sensor-4",
            "measuredAt": "2026-09-10T10:30:00Z",
            "values": [3.25, 5.5, 8.75],
        }


class BaseModel:
    @classmethod
    def model_validate(cls, fields):
        value = cls()
        value.__dict__.update(fields)
        return value


fastapi = types.ModuleType("fastapi")
fastapi.APIRouter = APIRouter
fastapi.Request = Request
sys.modules["fastapi"] = fastapi
pydantic = types.ModuleType("pydantic")
pydantic.BaseModel = BaseModel
sys.modules["pydantic"] = pydantic
spec = importlib.util.spec_from_file_location("generated", "services/readings.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
reading = module.ReadingEnvelope.model_validate(asyncio.run(Request().json()))
body = asyncio.run(module.post(Request(), reading))
print(json.dumps({"status": 200, "body": body}, sort_keys=True))
