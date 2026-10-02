"""Freeze native editing trials and retain submissions for separate container grading."""
import io
import json
from pathlib import Path
import re
import shutil
import sys
import tempfile
import time

from . import isolated_grade, native_changes as changes, native_discovery as discovery
from . import native_mcp as mcp, opencode_native as native, opencode_rehearsal as legacy
from .bounded_host import disk_size, run as bounded_run
from .study import digest, encode, load, require
from .workspace_bundle import unpack, validate

SCHEMA = "fr-opencode-changes-1"
LIMITS = native.LIMITS


def implementation():
    names = (*native.IMPLEMENTATION, "native_changes.py", "opencode_changes.py",
             "isolated_grade.py", "request_gateway.py", "provider_usage.py")
    return {**{name: legacy.identity(Path(__file__).with_name(name)) for name in names},
            "native-changes.py": legacy.identity(Path(__file__).parents[1] / "native-changes.py")}


def retain_runner(frozen, destination):
    require(frozen["plan"]["implementation"] == implementation(), "runner changed before retention")
    destination.mkdir(parents=True, exist_ok=False)
    (destination / "agent_eval").mkdir()
    for name, sha in implementation().items():
        cli = name == "native-changes.py"
        source = Path(__file__).parents[1] / name if cli else Path(__file__).with_name(name)
        target = destination / name if cli else destination / "agent_eval" / name
        shutil.copyfile(source, target)
        require(legacy.identity(target) == sha, "retained runner differs")


def freeze(manifest, base, binary):
    source = legacy.freeze(manifest, base, binary)
    graders = {}
    for task in source["plan"]["tasks"]:
        require(task["kind"] in {"fix", "feature"}, "change trials need fix or feature tasks")
        raw = (base / task["grader"]).read_text()
        isolated_grade.validate(mcp.decode(raw))
        graders[task["id"]] = raw
    plan = {"schema": SCHEMA, "source_plan": source, "limits": LIMITS,
            "prompt": changes.PROMPT, "tools": {arm: changes.schemas(arm) for arm in ("files", "fr")},
            "graders": graders, "implementation": implementation()}
    frozen = {"plan": plan, "sha256": digest(plan)}
    checked(frozen)
    return frozen


def checked(frozen):
    plan = frozen["plan"]
    require(plan["schema"] == SCHEMA and digest(plan) == frozen["sha256"], "changed code-change plan")
    require(plan["limits"] == LIMITS and plan["prompt"] == changes.PROMPT, "unsupported change protocol")
    require(plan["tools"] == {arm: changes.schemas(arm) for arm in ("files", "fr")}, "change schemas differ")
    source = legacy.checked(plan["source_plan"])
    require(source["cells"] == discovery.cells(source["manifest"], available=("files", "fr")), "change allocation differs")
    require(len(source["tasks"]) == len(source["manifest"]["tasks"]), "task count differs")
    require(set(plan["graders"]) == {t["id"] for t in source["tasks"]}, "grader task set differs")
    for task, declared in zip(source["tasks"], source["manifest"]["tasks"]):
        require(all(task[k] == v for k, v in declared.items()), "task differs from manifest")
        require(task["kind"] in {"fix", "feature"} and task["private_criteria"] is None, "change task kind differs")
        validate(task["files"], legacy.MAX_WORKSPACE)
        require(digest(task["files"]) == task["source_sha256"], "source identity differs")
        raw = plan["graders"][task["id"]].encode()
        import hashlib
        require(len(raw) <= 65536 and hashlib.sha256(raw).hexdigest() == task["grader_sha256"], "private grader differs")
        isolated_grade.validate(mcp.decode(raw))
    return source


def environment(config, log):
    env = native.environment(config, log, changes.PROMPT)
    settings = json.loads(env["OPENCODE_CONFIG_CONTENT"])
    settings["mcp"]["rehearsal"]["command"][2] = str(Path(__file__).parents[1] / "native-changes.py")
    env["OPENCODE_CONFIG_CONTENT"] = json.dumps(settings)
    return env


def run_attempt(frozen, cell_id, output, binary, opencode):
    source = checked(frozen)
    require(frozen["plan"]["implementation"] == implementation(), "runner changed; freeze again")
    require(legacy.identity(binary) == source["binary_sha256"], "binary changed")
    cell = next((c for c in source["cells"] if c["id"] == cell_id), None)
    require(cell is not None, "unknown cell")
    task = next(t for t in source["tasks"] if t["id"] == cell["task"])
    directory = output / cell_id
    directory.mkdir(parents=True, exist_ok=False)
    config = directory / "server.json"
    config.write_bytes(encode({"files": task["files"], "arm": cell["arm"], "binary": str(binary)}))
    prompt = changes.PROMPT + "\nTask:\n" + task["requirement"]
    (directory / "prompt.txt").write_text(prompt)
    (directory / "schemas.json").write_bytes(encode(changes.schemas(cell["arm"])))
    record = {"cell": cell, "plan_sha256": frozen["sha256"], "status": "failed", "failure": None,
              "grade_status": "pending", "provider_usage_verified": False, "actual_usd": None}
    started, processes = time.monotonic(), []
    with tempfile.TemporaryDirectory(prefix="fr-change-agent-") as isolated:
        env = environment(config, directory / "tools.jsonl")

        def execute(command, data, name):
            remaining = LIMITS["wall_seconds"] - (time.monotonic() - started)
            require(remaining > 0, "attempt wall budget exhausted")
            out, err = io.BytesIO(), io.BytesIO()
            result = bounded_run(command, data, out, err, directory, wall_seconds=remaining,
                cpu_limit_seconds=LIMITS["cpu_seconds"], rss_bytes=LIMITS["rss_bytes"],
                disk_bytes=LIMITS["disk_bytes"], transcript_bytes=LIMITS["transcript_bytes"], env=env, cwd=isolated)
            (directory / (name + ".stdout")).write_bytes(out.getvalue())
            (directory / (name + ".stderr")).write_bytes(err.getvalue())
            processes.append({"name": name, **result})
            require(disk_size(directory) <= LIMITS["disk_bytes"], "retained attempt exceeds disk budget")
            require(result["exit_code"] == 0 and result["stop_reason"] is None, f"{name} failed: {result['stop_reason'] or result['exit_code']}")
            return out.getvalue()

        try:
            record["opencode_version"] = execute([str(opencode), "--version"], b"", "version").decode().strip()
            raw = execute([str(opencode), "run", "--pure", "--model", cell["model"], "--agent", native.AGENT,
                "--format", "json", "--title", "fr changes " + cell_id, "--dir", isolated], prompt.encode(), "opencode")
            events = [mcp.decode(line) for line in raw.splitlines() if line.strip()]
            session = events[0]["sessionID"]
            require(re.fullmatch(r"ses_[A-Za-z0-9]+", session), "invalid session")
            exported = mcp.decode(execute([sys.executable, "-I", "-B", str(Path(__file__).with_name("opencode_export.py")),
                str(opencode), "export", session, str(directory)], b"", "export"))
            rows = [mcp.decode(line) for line in (directory / "tools.jsonl").read_bytes().splitlines()]
            result = changes.audit(raw, exported, rows, task, cell)
            (directory / "submission.json").write_bytes(encode(result.pop("files")))
            record.update(status="submitted", audit=result)
        except (OSError, ValueError, KeyError, TypeError, IndexError) as error:
            record["failure"] = str(error)
    record.update(wall_seconds=time.monotonic() - started, processes=processes)
    # The monitored group is stopped before cleanup. Preserve transcripts and
    # bundles; remove only disposable fr materializations left by a killed MCP.
    for path in directory.glob("fr-change-source-*"):
        require(path.is_dir() and not path.is_symlink(), "unexpected disposable workspace")
        shutil.rmtree(path)
    (directory / "record.json").write_bytes(encode(record))
    inventory = {p.name: legacy.identity(p) for p in directory.iterdir() if p.is_file()}
    (directory / "manifest.json").write_bytes(encode({"files": inventory}))
    return record


def replay(frozen, output):
    source, records = checked(frozen), []
    require(not output.exists() or all(p.name in {c["id"] for c in source["cells"]} and not p.is_symlink() for p in output.iterdir()), "unplanned attempt")
    for cell in source["cells"]:
        folder = output / cell["id"]
        if not (folder / "manifest.json").exists():
            records.append({"cell": cell, "status": "interrupted" if folder.exists() else "pending"})
            continue
        require(not (folder / "manifest.json").is_symlink(), "linked inventory")
        inventory = load(folder / "manifest.json")["files"]
        require({p.name for p in folder.iterdir()} == set(inventory) | {"manifest.json"}, "unplanned artifact")
        require(all(Path(name).name == name and (folder / name).is_file() and not (folder / name).is_symlink()
                    and (folder / name).stat().st_size <= LIMITS["disk_bytes"] and legacy.identity(folder / name) == sha
                    for name, sha in inventory.items()), "changed artifact")
        require(disk_size(folder) <= LIMITS["disk_bytes"], "retained attempt exceeds disk budget")
        record = load(folder / "record.json")
        require(record["cell"] == cell and record["plan_sha256"] == frozen["sha256"], "record identity differs")
        require(record["status"] in {"submitted", "failed"} and record["grade_status"] == "pending", "invalid ungraded outcome")
        task = next(t for t in source["tasks"] if t["id"] == cell["task"])
        require((folder / "prompt.txt").read_text() == changes.PROMPT + "\nTask:\n" + task["requirement"], "prompt differs")
        require(load(folder / "schemas.json") == changes.schemas(cell["arm"]), "schemas differ")
        if record["status"] == "submitted":
            result = changes.audit((folder / "opencode.stdout").read_bytes(), load(folder / "export.stdout"),
                [mcp.decode(line) for line in (folder / "tools.jsonl").read_bytes().splitlines()], task, cell)
            require(load(folder / "submission.json") == result.pop("files"), "submitted files differ from replay")
            require(result == record["audit"], "change audit differs")
            require(all(p["exit_code"] == 0 and p["stop_reason"] is None for p in record["processes"]), "submitted attempt has failed process")
        else:
            require(isinstance(record["failure"], str) and record["failure"], "missing failure")
        records.append(record)
    return {"schema": SCHEMA, "plan_sha256": frozen["sha256"], "planned": len(records),
        "submitted": sum(r["status"] == "submitted" for r in records), "attempts": records,
        "provider_usage_verified": False, "complete_context_accounting": False,
        "scope": "Transcript and patch replay only. Submitted does not mean behavior passed."}


def grade_attempts(frozen, output, *, grader=isolated_grade.grade):
    require(frozen["plan"]["implementation"] == implementation(), "grading runner changed; use the frozen runner")
    report = replay(frozen, output)
    source = checked(frozen)
    outcomes = []
    for record in report["attempts"]:
        cell = record["cell"]
        if record["status"] != "submitted":
            outcomes.append({"cell": cell, "outcome": record["status"]})
            continue
        task = next(t for t in source["tasks"] if t["id"] == cell["task"])
        with tempfile.TemporaryDirectory(prefix="fr-change-grade-") as tmp:
            root = Path(tmp)
            spec = root / "grader.json"
            spec.write_text(frozen["plan"]["graders"][task["id"]])
            files = load(output / cell["id"] / "submission.json")
            unpack(files, root / "submission", legacy.MAX_WORKSPACE)
            result = grader(root / "submission", spec, task["grader_sha256"])
        outcomes.append({"cell": cell, "submission_sha256": digest(files), "outcome": result["outcome"], "grade": result})
    return {"schema": "fr-opencode-change-grades-1", "plan_sha256": frozen["sha256"],
            "passed": sum(r["outcome"] == "passed" for r in outcomes), "outcomes": outcomes,
            "scope": "Pinned black-box behavior cases only; not a proof or general efficiency result."}
