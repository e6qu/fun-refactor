"""Replay the retained review records offline; never invoke a model or candidate."""
import json
import gzip
import io
from pathlib import Path
import runpy
import sys

HERE = Path(__file__).resolve().parent
COLLECT = runpy.run_path(str(HERE / "collect.py"))
from agent_eval import native_costs
load, require, sha = (COLLECT[name] for name in ("load", "require", "sha"))


def snapshots(plan):
    path = HERE / "inputs.json.gz"
    require(path.stat().st_size <= 1024**2, "review snapshot archive exceeds budget")
    with gzip.GzipFile(fileobj=io.BytesIO(path.read_bytes())) as stream:
        raw = stream.read(4 * 1024**2 + 1)
    require(len(raw) <= 4 * 1024**2, "review snapshots exceed budget")
    files = COLLECT["native_mcp"].decode(raw)
    require(set(files) == {t["id"] for t in plan["tasks"]}, "review snapshot tasks differ")
    for task in plan["tasks"]:
        COLLECT["validate"](files[task["id"]], COLLECT["legacy"].MAX_WORKSPACE)
        require(COLLECT["digest"](files[task["id"]]) == task["files_sha256"], "review source identity differs")
    return files


def audit(plan, cell, directory, files):
    task = next(t for t in plan["tasks"] if t["id"] == cell["task"])
    result = COLLECT["native"].audit((directory / "opencode.stdout").read_bytes(), load(directory / "export.stdout"),
        [COLLECT["native_mcp"].decode(line) for line in (directory / "tools.jsonl").read_bytes().splitlines()],
        {**task, "files": files[task["id"]]}, cell, plan)
    return {**result, "review": COLLECT["findings"](result["answer"], result["disclosed"])}


def report():
    frozen = COLLECT["checked"]()
    files = snapshots(frozen["plan"])
    cells = frozen["plan"]["cells"]
    attempts = HERE / "attempts"
    require(not attempts.exists() or {p.name for p in attempts.iterdir()} <= {c["id"] for c in cells}, "unplanned review")
    rows = []
    for cell in cells:
        directory = attempts / cell["id"]
        if not directory.exists():
            rows.append({"cell": cell, "status": "not_started"})
            continue
        require(not directory.is_symlink() and directory.is_dir(), "invalid review directory")
        inventory = load(directory / "manifest.json")
        require({p.name for p in directory.iterdir()} == set(inventory) | {"manifest.json"}, "unplanned review artifact")
        require(all(Path(name).name == name and not (directory / name).is_symlink()
                    and (directory / name).is_file() and sha(directory / name) == value
                    for name, value in inventory.items()), "review artifact changed")
        record = load(directory / "record.json")
        require(record["cell"] == cell and record["plan_sha256"] == frozen["sha256"], "review record identity changed")
        require(record["status"] in {"completed", "failed"}, "invalid review status")
        row = {"cell": cell, "status": record["status"], "failure": record["failure"],
               "wall_seconds": record["wall_seconds"],
               "sampled_cpu_seconds": sum(p["sampled_cpu_seconds"] for p in record["processes"]),
               "sampled_peak_rss_bytes": max((p["sampled_aggregate_rss_bytes"] for p in record["processes"]), default=0),
               "stop_reasons": [p["stop_reason"] for p in record["processes"] if p["stop_reason"]],
               "review": None,
               "complete_context_accounting": False, "actual_usd": None}
        log = directory / "tools.jsonl"
        stream = directory / "opencode.stdout"
        row["costs"] = native_costs.observed(stream.read_bytes() if stream.exists() else b"",
            log.read_bytes() if log.exists() else b"", {"files": files[cell["task"]]}, cell["arm"],
            frozen["plan"]["tools_schema_version"], read_only=True)
        row["costs"]["coverage"].update(collection_complete=record["status"] == "completed",
                                       reported_step_usage_complete=record["status"] == "completed")
        if record["status"] == "completed":
            require(all(p["exit_code"] == 0 and p["stop_reason"] is None for p in record["processes"]),
                    "completed review has a failed process")
            audited = audit(frozen["plan"], cell, directory, files)
            require(audited == record["audit"] and record["failure"] is None, "review replay differs")
            row.update(review=audited["review"])
            row["costs"]["coverage"]["export_and_model_identity_verified"] = True
        else:
            require("audit" not in record and isinstance(record["failure"], str) and record["failure"], "failed review has no failure")
        rows.append(row)
    return {"schema": "fr-native-candidate-review-report-1", "plan_sha256": frozen["sha256"],
            "planned": len(rows), "completed": sum(r["status"] == "completed" for r in rows),
            "failed": sum(r["status"] == "failed" for r in rows),
            "not_started": sum(r["status"] == "not_started" for r in rows), "attempts": rows,
            "claims_verified": False, "independent_task_selection": False, "efficiency_comparison": False,
            "scope": "Observed host output includes failed attempts; partial logs do not establish provider delivery, complete usage or billing."}


if __name__ == "__main__":
    result = report()
    destination = HERE / "report.json"
    if sys.argv[1:] == ["--check"]:
        require(load(destination) == result, "review report differs")
    else:
        require(not sys.argv[1:], "only --check is supported")
        destination.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({key: result[key] for key in ("planned", "completed", "failed", "not_started")}))
