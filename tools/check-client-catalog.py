#!/usr/bin/env python3
"""Check resolved model endpoints with isolated configuration and no model calls."""
import argparse
import io
import json
import os
from pathlib import Path

from agent_eval import bounded_host, source_reviews, structured_probe
from agent_eval.study import encode, require


def models(raw):
    remaining, result = raw.decode().strip(), {}
    decoder = json.JSONDecoder()
    while remaining:
        name, remaining = remaining.split("\n", 1)
        model, end = decoder.raw_decode(remaining)
        require(name not in result, "duplicate model metadata")
        result[name] = model
        remaining = remaining[end:].strip()
    return result


def check(root, client, catalog):
    require(os.environ.get("GITHUB_ACTIONS") == "true", "run model resolution controls on GitHub")
    expected = json.loads(catalog.read_bytes())
    require(len(expected) == 2 and all(len(p["models"]) == 1 for p in expected.values()), "expected two selected models")
    root.mkdir(parents=True, exist_ok=False)
    results = []
    for mode in ("empty", "providers"):
        folder = root / mode
        folder.mkdir()
        selected = folder / "catalog.json"
        selected.write_bytes(b"{}\n" if mode == "empty" else catalog.read_bytes())
        env = structured_probe.environment(folder, "http://127.0.0.1:1/v1")
        settings = json.loads(env["OPENCODE_CONFIG_CONTENT"])
        settings.update(mcp={}, enabled_providers=list(expected), provider={provider: {
            "options": {"apiKey": "scripted-no-credential"}, "models": {model: {"name": model,
                "limit": {"context": 32768, "output": 2048}} for model in metadata["models"]}}
            for provider, metadata in expected.items()})
        env.update(OPENCODE_CONFIG_CONTENT=json.dumps(settings), OPENCODE_MODELS_PATH=str(selected), BUN_OPTIONS="")
        out, err = io.BytesIO(), io.BytesIO()
        process = bounded_host.run([str(client), "--pure", "models", "--verbose"], b"", out, err, folder,
            env=env, cwd=folder, wall_seconds=120, cpu_limit_seconds=20, rss_bytes=768 * 1024**2,
            disk_bytes=16 * 1024**2, transcript_bytes=1024**2)
        (folder / "process.json").write_bytes(encode(process))
        (folder / "models.stdout").write_bytes(out.getvalue())
        (folder / "models.stderr").write_bytes(err.getvalue())
        structured_probe.checked_process(process)
        actual = models(out.getvalue())
        require(set(actual) == {provider + "/" + model for provider, metadata in expected.items() for model in metadata["models"]},
                "resolved model set differs")
        for provider, metadata in expected.items():
            for model in metadata["models"]:
                resolved = actual[provider + "/" + model]
                if mode == "empty":
                    require(not resolved["api"].get("url"), "negative control unexpectedly resolved an endpoint")
                else:
                    require(resolved["api"] == {"id": model, "url": metadata["api"], "npm": metadata["npm"]},
                            "model endpoint or adapter differs")
                    require(resolved["capabilities"]["reasoning"] and resolved["capabilities"]["toolcall"],
                            "model capabilities lost")
                    require(resolved["capabilities"]["interleaved"] == {"field": "reasoning_content"},
                            "interleaved reasoning metadata lost")
                require(resolved["limit"]["context"] == 32768 and resolved["limit"]["output"] == 2048,
                        "configured model limits changed")
        results.append({"mode": mode, "process": process, "catalog_sha256": source_reviews.identity(selected),
                        "models_sha256": source_reviews.identity(folder / "models.stdout")})
    result = {"cases": results, "opencode_sha256": source_reviews.identity(client),
              "scope": "Isolated metadata resolution only; no provider request or configured authentication."}
    (root / "result.json").write_bytes(encode(result))
    print(encode(result).decode())


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--opencode", required=True, type=Path)
    parser.add_argument("--catalog", required=True, type=Path)
    args = parser.parse_args()
    check(args.output.resolve(), args.opencode.resolve(), args.catalog.resolve())
