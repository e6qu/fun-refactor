"""Capture configured code changes under one process budget, without local grading."""
import io
import json
from pathlib import Path
import shutil
import sys

from . import bounded_host, native_changes, native_mcp as mcp, source_reviews
from . import structured_probe, terminal_changes as changes, terminal_review_runner as reviews
from . import terminal_reviews, terminal_transport
from .study import encode, require


def environment(root, model, config, catalog):
    env = reviews.configured_environment(root, model, config)
    settings = json.loads(env["OPENCODE_CONFIG_CONTENT"])
    settings["agent"]["fr-submission"]["prompt"] = "Make the requested code change using the permitted tools, then submit a structured summary."
    settings["mcp"]["rehearsal"]["command"][2] = str(Path(__file__).parents[1] / "terminal-changes.py")
    path = config.parent / "catalog.json"
    path.write_bytes(encode(catalog))
    env.update(OPENCODE_CONFIG_CONTENT=json.dumps(settings), OPENCODE_MODELS_PATH=str(path),
               BUN_OPTIONS="", RAYON_NUM_THREADS="1")
    return env


def capture(frozen, snapshots, cell, root, binary, client):
    plan = changes.checked(frozen, snapshots, execution=True)
    require(cell in plan["cells"], "unplanned cell")
    require(source_reviews.identity(binary) == plan["binary_sha256"]
            and source_reviews.identity(client) == plan["opencode_sha256"], "executable changed")
    task = next(t for t in plan["tasks"] if t["id"] == cell["task"])
    work = root / "work"
    work.mkdir()
    try:
        config = work / "server.json"
        config.write_bytes(encode({"files": snapshots[cell["task"]], "arm": cell["arm"],
                                  "binary": str(binary), "tools_schema_version": 2}))
        env = environment(root, plan["models"][cell["model"]], config, plan["catalog"])
        captured = terminal_transport.capture(client, root, env, changes.request(plan, task, cell))
        (root / "identity.json").write_bytes(encode({"schema": changes.SCHEMA, "plan_sha256": frozen["sha256"],
            "cell": cell, "opencode_version": captured["opencode_version"],
            "binary_sha256": plan["binary_sha256"], "opencode_sha256": plan["opencode_sha256"]}))
    finally:
        shutil.rmtree(work)


def serve(config, log):
    with log.open("xb") as destination:
        server = native_changes.Server(mcp.decode(terminal_reviews.read(config)), destination)
        del server.tools["submit_patch"]
        try:
            for _ in range(128):
                line = sys.stdin.buffer.readline(mcp.MAX_LINE + 1)
                if not line:
                    return
                require(len(line) <= mcp.MAX_LINE and line.endswith(b"\n"), "invalid MCP line")
                response = server.dispatch(mcp.decode(line))
                if response is not None:
                    sys.stdout.buffer.write(encode(response) + b"\n")
                    sys.stdout.buffer.flush()
            raise ValueError("MCP message budget exhausted")
        finally:
            server.machine.close()


def seal(frozen, snapshots, cell, root, process):
    reviews.cleanup(root)
    for path in root.glob("fr-change-source-*"):
        require(path.is_dir() and not path.is_symlink(), "invalid disposable source directory")
        shutil.rmtree(path)
    (root / "process.json").write_bytes(encode(process))
    truncated = reviews.retain_prefixes(root)
    record = {"cell": cell, "plan_sha256": frozen["sha256"], "status": "failed", "failure": None,
              "truncated_artifacts": truncated}
    try:
        require(not truncated, "capture artifact exceeded retention limit")
        structured_probe.checked_process(process)
        task = next(t for t in frozen["plan"]["tasks"] if t["id"] == cell["task"]) | {"files": snapshots[cell["task"]]}
        audited = changes.audit(frozen["plan"], task, cell, root)
        files = audited.pop("files")
        require(len(encode(files)) <= terminal_reviews.MAX_BYTES, "submission exceeds retention budget")
        (root / "submission.json").write_bytes(encode(files))
        record.update(status="completed", audit=audited)
    except (OSError, ValueError, KeyError, TypeError, IndexError, UnicodeError) as error:
        record["failure"] = str(error)[:512]
    (root / "record.json").write_bytes(encode(record))
    require({p.name for p in root.iterdir()} <= changes.ARTIFACTS | {"record.json"}, "unexpected capture file")
    (root / "manifest.json").write_bytes(encode({p.name: source_reviews.identity(p) for p in root.iterdir()}))
    return record


def collect(frozen, snapshots, cell_id, inputs, output, binary, client):
    plan = changes.checked(frozen, snapshots, execution=True)
    require(source_reviews.identity(binary) == plan["binary_sha256"]
            and source_reviews.identity(client) == plan["opencode_sha256"], "executable changed")
    prior = changes.report(frozen, snapshots, output)
    require(all(a.get("process", {}).get("stop_reason") is None for a in prior["attempts"]), "resource stop closes collection")
    cell = source_reviews.eligible(plan, cell_id, output, frozen["sha256"])
    root = output / cell_id
    root.mkdir(parents=True, exist_ok=False)
    out, err = io.BytesIO(), io.BytesIO()
    limits = {k: v for k, v in plan["limits"].items() if k not in {"steps", "tool_calls"}}
    limits["cpu_limit_seconds"] = limits.pop("cpu_seconds")
    process = bounded_host.run([sys.executable, "-B", str(Path(__file__).parents[1] / "terminal-changes.py"),
        "capture", str(inputs), cell_id, str(root), "--fr", str(binary), "--opencode", str(client),
        "--plan-sha", frozen["sha256"]], b"", out, err, root, **limits)
    (root / "host.stdout").write_bytes(out.getvalue())
    (root / "host.stderr").write_bytes(err.getvalue())
    return seal(frozen, snapshots, cell, root, process)
