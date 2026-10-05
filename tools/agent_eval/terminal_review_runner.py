"""Run a frozen terminal review under one process-group resource budget."""
import io
import json
import os
from pathlib import Path
import shutil
import sys
from urllib.parse import urlsplit

from . import bounded_host, native_mcp as mcp, source_reviews, structured_probe
from . import terminal_reviews as review, terminal_transport
from .study import encode, require
from .workspace_bundle import unpack


def environment(root, model, config):
    env = structured_probe.environment(root, model["baseURL"])
    settings = json.loads(env["OPENCODE_CONFIG_CONTENT"])
    provider = settings["provider"]["scripted"]
    provider["models"] = {model["modelID"]: {"name": model["modelID"],
        "limit": {"context": model["context"], "output": model["output"]}}}
    if urlsplit(model["baseURL"]).hostname != "127.0.0.1":
        require(os.environ.get("GITHUB_ACTIONS") == "true", "live reviews require the remote bounded runner")
        require(os.environ.get("FR_REVIEW_API_KEY"), "remote provider credential is not configured")
        provider["options"]["apiKey"] = "{env:FR_REVIEW_API_KEY}"
        env["FR_REVIEW_API_KEY"] = os.environ["FR_REVIEW_API_KEY"]
    settings.update(enabled_providers=[model["providerID"]], provider={model["providerID"]: provider})
    settings["agent"]["fr-submission"]["prompt"] = review.PROMPT
    settings["mcp"]["rehearsal"]["command"] = [sys.executable, "-B",
        str(Path(__file__).parents[1] / "terminal-reviews.py"), "serve", str(config), str(root / "tools.jsonl")]
    env["OPENCODE_CONFIG_CONTENT"] = json.dumps(settings)
    return env


def capture(frozen, snapshots, cell, root, binary, opencode):
    plan = review.checked(frozen, snapshots, execution=True)
    require(cell in plan["cells"], "unplanned cell")
    require(source_reviews.identity(binary) == plan["binary_sha256"]
            and source_reviews.identity(opencode) == plan["opencode_sha256"], "executable changed")
    task = {**next(t for t in plan["tasks"] if t["id"] == cell["task"]), "files": snapshots[cell["task"]]}
    work = root / "work"
    work.mkdir()
    try:
        unpack(task["files"], work / "source", source_reviews.legacy.MAX_WORKSPACE)
        config = work / "server.json"
        config.write_bytes(encode({"files": task["files"], "arm": "fr", "binary": str(binary),
            "workspace": str(work / "source"), "tools_schema_version": 6}))
        env = environment(root, plan["models"][cell["model"]], config)
        captured = terminal_transport.capture(opencode, root, env, review.request(plan, task, cell))
        (root / "identity.json").write_bytes(encode({"schema": review.SCHEMA, "plan_sha256": frozen["sha256"],
            "cell": cell, "opencode_version": captured["opencode_version"],
            "binary_sha256": plan["binary_sha256"], "opencode_sha256": plan["opencode_sha256"]}))
    finally:
        shutil.rmtree(work)


def seal(frozen, snapshots, cell, root, process):
    """Seal observed failures too; replay never upgrades them to successful reviews."""
    plan = frozen["plan"]
    task = {**next(t for t in plan["tasks"] if t["id"] == cell["task"]), "files": snapshots[cell["task"]]}
    (root / "process.json").write_bytes(encode(process))
    record = {"cell": cell, "plan_sha256": frozen["sha256"], "status": "failed", "failure": None}
    try:
        structured_probe.checked_process(process)
        record.update(audit=review.audit(plan, task, cell, root), status="completed")
    except (OSError, ValueError, KeyError, TypeError, IndexError, UnicodeError) as error:
        record["failure"] = str(error)[:512]
    (root / "record.json").write_bytes(encode(record))
    require({p.name for p in root.iterdir()} <= review.ARTIFACTS | {"record.json"}, "unexpected retained artifact")
    (root / "manifest.json").write_bytes(encode({p.name: source_reviews.identity(p) for p in root.iterdir()}))
    return record


def collect(frozen, snapshots, cell_id, output, binary, opencode, inputs):
    plan = review.checked(frozen, snapshots, execution=True)
    require(source_reviews.identity(binary) == plan["binary_sha256"]
            and source_reviews.identity(opencode) == plan["opencode_sha256"], "executable changed")
    review.report(frozen, snapshots, output)
    cell = source_reviews.eligible(plan, cell_id, output, frozen["sha256"])
    folder = output / cell_id
    folder.mkdir(parents=True, exist_ok=False)
    out, err = io.BytesIO(), io.BytesIO()
    limits = plan["limits"]
    process = bounded_host.run([sys.executable, "-B", str(Path(__file__).parents[1] / "terminal-reviews.py"),
        "capture", str(inputs), cell_id, str(folder), "--fr", str(binary), "--opencode", str(opencode)],
        b"", out, err, folder, wall_seconds=limits["wall_seconds"], cpu_limit_seconds=limits["cpu_seconds"],
        rss_bytes=limits["rss_bytes"], disk_bytes=limits["disk_bytes"], transcript_bytes=limits["transcript_bytes"])
    # Only this attempt's temporary source/configuration and isolated client stores are removed.
    for name in ("work", "config", "cache", "data", "state"):
        path = folder / name
        if path.exists():
            require(path.is_dir() and not path.is_symlink(), "invalid temporary client directory")
            shutil.rmtree(path)
    (folder / "host.stdout").write_bytes(out.getvalue())
    (folder / "host.stderr").write_bytes(err.getvalue())
    return seal(frozen, snapshots, cell, folder, process)


def serve(config, log):
    settings = mcp.decode(review.read(config))
    with log.open("xb") as destination:
        server = mcp.Server(settings, destination)
        del server.tools["submit_answer"]
        for _ in range(128):
            line = sys.stdin.buffer.readline(mcp.MAX_LINE + 1)
            if not line:
                return
            require(len(line) <= mcp.MAX_LINE and line.endswith(b"\n"), "invalid MCP line")
            result = server.dispatch(mcp.decode(line))
            if result is not None:
                sys.stdout.buffer.write(encode(result) + b"\n")
                sys.stdout.buffer.flush()
