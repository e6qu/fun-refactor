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
from . import native_checks, container_resources
from .bounded_host import disk_size, run as bounded_run
from .study import digest, encode, load, require
from .workspace_bundle import unpack, validate

SCHEMA = "fr-opencode-changes-1"
LIMITS = native.LIMITS


def implementation():
    names = (*native.IMPLEMENTATION, "native_changes.py", "native_author.py", "opencode_changes.py",
             "isolated_grade.py", "request_gateway.py", "provider_usage.py", "native_checks.py",
             "container_resources.py", "docker_info.py")
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


def freeze(manifest, base, binary, *, public_edits=False, public_checks=None):
    source = legacy.freeze(manifest, base, binary)
    graders = {}
    for task in source["plan"]["tasks"]:
        require(task["kind"] in {"fix", "feature"}, "change trials need fix or feature tasks")
        raw = (base / task["grader"]).read_text()
        isolated_grade.validate(mcp.decode(raw))
        graders[task["id"]] = raw
    version = 3 if public_checks is not None else 2 if public_edits else 1
    plan = {"schema": SCHEMA, "source_plan": source, "limits": LIMITS,
            "prompt": changes.prompt("files", version), "tools": {arm: changes.schemas(arm, version) for arm in ("files", "fr")},
            "graders": graders, "implementation": implementation()}
    if version >= 2:
        plan.update(tools_schema_version=version, edit_guidance=changes.native_author.GUIDANCE)
    if version == 3:
        plan.update(public_checks=public_checks, public_check_resources=dict(native_checks.RESOURCES))
    frozen = {"plan": plan, "sha256": digest(plan)}
    checked(frozen)
    return frozen


def checked(frozen):
    plan = frozen["plan"]
    require(plan["schema"] == SCHEMA and digest(plan) == frozen["sha256"], "changed code-change plan")
    version = plan.get("tools_schema_version", 1)
    require(plan["limits"] == LIMITS and plan["prompt"] == changes.prompt("files", version), "unsupported change protocol")
    require(plan["tools"] == {arm: changes.schemas(arm, version) for arm in ("files", "fr")}, "change schemas differ")
    require(plan.get("edit_guidance") == (changes.native_author.GUIDANCE if version >= 2 else None), "edit guidance differs")
    source = legacy.checked(plan["source_plan"])
    require(source["cells"] == discovery.cells(source["manifest"], available=("files", "fr")), "change allocation differs")
    require(len(source["tasks"]) == len(source["manifest"]["tasks"]), "task count differs")
    require(set(plan["graders"]) == {t["id"] for t in source["tasks"]}, "grader task set differs")
    if version == 3:
        require(plan["public_check_resources"] == native_checks.RESOURCES, "public check resource profile differs")
        require(set(plan["public_checks"]) == set(plan["graders"]), "public check task set differs")
        for spec in plan["public_checks"].values():
            native_checks.validate(spec)
    else:
        require("public_checks" not in plan and "public_check_resources" not in plan, "public checks require protocol 3")
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


def environment(config, log, instructions=changes.PROMPT):
    env = native.environment(config, log, instructions)
    settings = json.loads(env["OPENCODE_CONFIG_CONTENT"])
    settings["mcp"]["rehearsal"]["command"][2] = str(Path(__file__).parents[1] / "native-changes.py")
    env["OPENCODE_CONFIG_CONTENT"] = json.dumps(settings)
    return env


def runtime_task(frozen, task):
    profiles = frozen["plan"].get("public_checks")
    return {**task, "public_check": profiles[task["id"]]} if profiles is not None else task


def run_attempt(frozen, cell_id, output, binary, opencode, *, container_slice=None):
    source = checked(frozen)
    version = frozen["plan"].get("tools_schema_version", 1)
    require((version == 3) == (container_slice is not None), "public checks and --container-slice must be supplied together")
    require(frozen["plan"]["implementation"] == implementation(), "runner changed; freeze again")
    require(legacy.identity(binary) == source["binary_sha256"], "binary changed")
    require(any(c["id"] == cell_id for c in source["cells"]), "unknown cell")
    if version == 3:
        native_checks.require_runner()
    directory = output / cell_id
    directory.mkdir(parents=True, exist_ok=False)
    started = time.monotonic()
    resources = (container_resources.ContainerResources(native_checks.RESOURCES, container_slice, directory).start()
                 if version == 3 else None)
    record, evidence, cleanup_failure = None, None, None
    try:
        record = _collect_attempt(frozen, cell_id, directory, binary, opencode, resources, container_slice, started)
    finally:
        if resources is not None:
            evidence = resources.finish()
            try:
                native_checks.DockerChecks(container_slice, directory).cleanup()
            except (OSError, ValueError, KeyError) as error:
                cleanup_failure = str(error)
        for prefix in ("fr-change-source-", "fr-public-check-", "fr-grade-"):
            for path in directory.glob(prefix + "*"):
                require(path.is_dir() and not path.is_symlink(), "unexpected disposable workspace")
                shutil.rmtree(path)
    if evidence is not None:
        record["public_check_resources"] = evidence
        record["public_check_cleanup_failure"] = cleanup_failure
        observed = container_resources.audit(evidence, native_checks.RESOURCES)
        if observed["stopped"] or cleanup_failure:
            record.update(status="failed", failure=cleanup_failure or "public check resource budget stopped the attempt")
    require(disk_size(directory) <= LIMITS["disk_bytes"], "retained attempt exceeds disk budget")
    (directory / "record.json").write_bytes(encode(record))
    inventory = {p.name: legacy.identity(p) for p in directory.iterdir() if p.is_file()}
    (directory / "manifest.json").write_bytes(encode({"files": inventory}))
    return record


def collection_cpu(processes, version, *, allow_zero=False):
    reservation = [{"sampled_cpu_seconds": native_checks.RESOURCES["cpu_seconds"]}] if version == 3 else []
    return native.cpu_remaining(processes + reservation, allow_zero=allow_zero)


def _collect_attempt(frozen, cell_id, directory, binary, opencode, resources, container_slice, started):
    source = checked(frozen)
    version = frozen["plan"].get("tools_schema_version", 1)
    require(frozen["plan"]["implementation"] == implementation(), "runner changed; freeze again")
    require(legacy.identity(binary) == source["binary_sha256"], "binary changed")
    cell = next((c for c in source["cells"] if c["id"] == cell_id), None)
    require(cell is not None, "unknown cell")
    task = runtime_task(frozen, next(t for t in source["tasks"] if t["id"] == cell["task"]))
    config = directory / "server.json"
    settings = {"files": task["files"], "arm": cell["arm"], "binary": str(binary)}
    if version != 1:
        settings["tools_schema_version"] = version
    if version == 3:
        settings.update(public_check=task["public_check"], container_slice=container_slice)
    config.write_bytes(encode(settings))
    instructions = changes.prompt(cell["arm"], version)
    prompt = instructions + "\nTask:\n" + task["requirement"]
    (directory / "prompt.txt").write_text(prompt)
    (directory / "schemas.json").write_bytes(encode(changes.schemas(cell["arm"], version)))
    record = {"cell": cell, "plan_sha256": frozen["sha256"], "status": "failed", "failure": None,
              "grade_status": "pending", "provider_usage_verified": False, "actual_usd": None}
    processes = []
    with tempfile.TemporaryDirectory(prefix="fr-change-agent-") as isolated:
        env = environment(config, directory / "tools.jsonl", instructions)

        def execute(command, data, name):
            if resources is not None:
                resources.check()
            remaining = LIMITS["wall_seconds"] - (time.monotonic() - started)
            require(remaining > 0, "attempt wall budget exhausted")
            out, err = io.BytesIO(), io.BytesIO()
            result = bounded_run(command, data, out, err, directory, wall_seconds=remaining,
                cpu_limit_seconds=collection_cpu(processes, version), rss_bytes=LIMITS["rss_bytes"],
                disk_bytes=LIMITS["disk_bytes"], transcript_bytes=LIMITS["transcript_bytes"], env=env, cwd=isolated)
            (directory / (name + ".stdout")).write_bytes(out.getvalue())
            (directory / (name + ".stderr")).write_bytes(err.getvalue())
            processes.append({"name": name, **result})
            collection_cpu(processes, version, allow_zero=True)
            if resources is not None:
                resources.check()
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
            result = changes.audit(raw, exported, rows, task, cell, version)
            (directory / "submission.json").write_bytes(encode(result.pop("files")))
            record.update(status="submitted", audit=result)
        except (OSError, ValueError, KeyError, TypeError, IndexError) as error:
            record["failure"] = str(error)
    record.update(wall_seconds=time.monotonic() - started, processes=processes)
    return record


def replay(frozen, output):
    source, records = checked(frozen), []
    version = frozen["plan"].get("tools_schema_version", 1)
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
        task = runtime_task(frozen, next(t for t in source["tasks"] if t["id"] == cell["task"]))
        require((folder / "prompt.txt").read_text() == changes.prompt(cell["arm"], version) + "\nTask:\n" + task["requirement"], "prompt differs")
        require(load(folder / "schemas.json") == changes.schemas(cell["arm"], version), "schemas differ")
        if version == 3:
            observed = container_resources.audit(record["public_check_resources"], native_checks.RESOURCES)
            cleanup = record["public_check_cleanup_failure"]
            require(cleanup is None or isinstance(cleanup, str) and cleanup, "invalid public check cleanup result")
            if record["status"] == "submitted":
                require(not observed["stopped"] and cleanup is None, "resource-stopped attempt cannot be submitted")
                require([p["name"] for p in record["processes"]] == ["version", "opencode", "export"], "collection process set differs")
                collection_cpu(record["processes"], version, allow_zero=True)
        else:
            require("public_check_resources" not in record and "public_check_cleanup_failure" not in record,
                    "public check resources require protocol 3")
        if record["status"] == "submitted":
            result = changes.audit((folder / "opencode.stdout").read_bytes(), load(folder / "export.stdout"),
                [mcp.decode(line) for line in (folder / "tools.jsonl").read_bytes().splitlines()], task, cell, version)
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
