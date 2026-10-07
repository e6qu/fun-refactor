#!/usr/bin/env python3
"""Reconstruct a two-provider catalog from pinned public TOML, without auth."""
import argparse
import hashlib
import json
from pathlib import Path
import tomllib

HERE = Path(__file__).resolve().parent / "2026-10-07"
PAIRS = {"kimi-code-plan-global": "k3", "zai-coding-plan": "glm-5.3-flash"}
MODEL_FIELDS = {"name", "family", "release_date", "attachment", "reasoning", "temperature", "tool_call",
                "reasoning_options", "interleaved", "cost", "limit", "modalities", "provider", "status"}


def prepare():
    manifest = json.loads((HERE / "manifest.json").read_bytes())
    sources = {}
    for name, expected in manifest["files"].items():
        raw = (HERE / name).read_bytes()
        assert hashlib.sha256(raw).hexdigest() == expected, "public source changed"
        sources[name] = tomllib.loads(raw.decode())
    result = {}
    for provider, model_id in PAIRS.items():
        metadata = sources[f"providers/{provider}/provider.toml"]
        override = sources[f"providers/{provider}/models/{model_id}.toml"]
        model = sources["models/" + override["base_model"] + ".toml"] | override
        result[provider] = {key: metadata[key] for key in ("name", "npm", "api", "env")}
        result[provider].update(id=provider, models={model_id: {"id": model_id,
            **{key: value for key, value in model.items() if key in MODEL_FIELDS}}})
    return json.dumps(result, sort_keys=True, separators=(",", ":")).encode() + b"\n"


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("write", "check"))
    args = parser.parse_args()
    data = prepare()
    destination = HERE / "catalog.json"
    if args.command == "write":
        with destination.open("xb") as stream:
            stream.write(data)
    else:
        assert destination.read_bytes() == data, "catalog changed"
    print(f"Verified {len(PAIRS)} provider/model pairs in {len(data)} catalog bytes")
