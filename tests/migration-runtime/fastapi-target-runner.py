import asyncio
import importlib.util
import json
import sys
import types


class APIRouter:
    def get(self, path):
        return lambda function: function


fastapi = types.ModuleType("fastapi")
fastapi.APIRouter = APIRouter
sys.modules["fastapi"] = fastapi
spec = importlib.util.spec_from_file_location("generated", "services/telemetry.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
body = asyncio.run(module.get())
print(json.dumps({"status": 200, "body": body}, sort_keys=True))
