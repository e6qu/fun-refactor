"""Join immutable native change attempts, observed costs and behavior grades."""
import base64
from collections import Counter
import hashlib
import json
from pathlib import PurePosixPath

from . import native_changes as changes, native_costs, opencode_changes as runner
from .study import digest, encode, load, number, require

OUTCOMES = {"passed": "behavior pass", "behavior_failed": "behavior failed", "failed": "collection failed",
            "pending_grading": "awaiting grading", "pending": "not run", "interrupted": "incomplete artifacts"}


def candidate_identity(files):
    entries, directories = [], set()
    for path, item in files.items():
        raw = base64.b64decode(item["data"], validate=True)
        entries.append({"path": path, "executable": item["executable"],
                        "sha256": hashlib.sha256(raw).hexdigest(), "bytes": len(raw)})
        directories.update(str(p) for p in PurePosixPath(path).parents if str(p) != ".")
    entries.extend({"path": p, "directory": True, "bytes": 0} for p in directories)
    return {"sha256": digest(sorted(entries, key=lambda e: e["path"])), "files": len(entries),
            "bytes": sum(e["bytes"] for e in entries)}


def grades(frozen, report, attempts, root):
    source = runner.checked(frozen)
    require(report["schema"] == "fr-opencode-change-grades-1"
            and report["plan_sha256"] == frozen["sha256"], "grade report plan differs")
    require([r["cell"] for r in report["outcomes"]] == source["cells"], "grade cells differ")
    result = {}
    for row, record in zip(report["outcomes"], attempts):
        cell = row["cell"]
        if record["status"] != "submitted":
            require(row == {"cell": cell, "outcome": record["status"]}, "unsubmitted attempt has a grade")
            continue
        task = next(t for t in source["tasks"] if t["id"] == cell["task"])
        spec = json.loads(frozen["plan"]["graders"][task["id"]])
        grade = row["grade"]
        files = load(root / "attempts" / cell["id"] / "submission.json")
        require(row["submission_sha256"] == digest(files) and grade["candidate"] == candidate_identity(files), "graded candidate differs")
        require(grade["schema"] == "fr-isolated-grade-1" and grade["image"] == spec["image"]
                and grade["grader_sha256"] == task["grader_sha256"], "grader identity differs")
        require([c["id"] for c in grade["cases"]] == [c["id"] for c in spec["cases"]], "graded case set differs")
        for actual, expected in zip(grade["cases"], spec["cases"]):
            require(type(actual["passed"]) is bool, "invalid case verdict")
            if "execution" not in actual:
                require(actual["passed"] is False and isinstance(actual["failure"], str) and actual["failure"], "missing failed case evidence")
                continue
            for channel in ("stdout", "stderr"):
                raw = base64.b64decode(actual[channel + "_base64"], validate=True)
                require(len(raw) == actual[channel + "_bytes"]
                        and hashlib.sha256(raw).hexdigest() == actual[channel + "_sha256"], "case stream identity differs")
            state = actual["container_state"]
            passed = (not actual["execution"]["stop_reason"] and not state["Running"]
                      and not state["OOMKilled"] and not state.get("Error")
                      and state["ExitCode"] == expected["exit_code"]
                      and base64.b64decode(actual["stdout_base64"]) == expected["stdout"].encode())
            require(actual["passed"] == passed, "case verdict differs from retained execution")
        passed = all(c["passed"] for c in grade["cases"])
        require(row["outcome"] == grade["outcome"] == ("passed" if passed else "failed"), "grade outcome differs")
        result[cell["id"]] = "passed" if passed else "behavior_failed"
    require(type(report["passed"]) is int and report["passed"] == sum(v == "passed" for v in result.values()), "grade pass count differs")
    return result


def process_cost(record):
    processes = record["processes"]
    require(isinstance(processes, list), "invalid process records")
    names = [p["name"] for p in processes]
    require(all(isinstance(name, str) and name for name in names) and len(set(names)) == len(names), "duplicate process accounting")
    return {"collection_wall_seconds": number(record["wall_seconds"], "collection wall time"),
            "sampled_cpu_seconds": sum(number(p["sampled_cpu_seconds"], "sampled CPU") for p in processes) if processes else None,
            "sampled_peak_rss_bytes": max((number(p["sampled_aggregate_rss_bytes"], "sampled RSS", integer=True) for p in processes), default=None),
            "sampled_peak_disk_growth_bytes": max((number(p["sampled_disk_growth_bytes"], "sampled disk", integer=True) for p in processes), default=None),
            "stop_reasons": [p["stop_reason"] for p in processes if p["stop_reason"] is not None],
            "scope": "Monitored collection processes only; excludes grader, provider, host and global cache totals."}


def cohort(root, grade_path=None):
    frozen = load(root / "plan.json")
    replay = runner.replay(frozen, root / "attempts")
    source, version = runner.checked(frozen), frozen["plan"].get("tools_schema_version", 1)
    grade_path = grade_path or root / "github-grades.json"
    outcomes = grades(frozen, load(grade_path), replay["attempts"], root) if grade_path.exists() else {}
    rows = []
    for record in replay["attempts"]:
        cell, status = record["cell"], record["status"]
        row = {"trial_id": frozen["sha256"] + ":" + cell["id"], "cell": cell, "collection_status": status,
               "outcome": outcomes.get(cell["id"], "pending_grading" if status == "submitted" else status),
               "failure": record.get("failure"), "costs": None}
        if status in {"submitted", "failed"}:
            task = next(t for t in source["tasks"] if t["id"] == cell["task"])
            folder = root / "attempts" / cell["id"]
            def retained(name):
                path = folder / name
                return path.read_bytes() if path.exists() else b""
            cost = native_costs.observed(retained("opencode.stdout"), retained("tools.jsonl"), task, cell["arm"], version)
            cost["coverage"].update(collection_complete=status == "submitted", reported_step_usage_complete=status == "submitted",
                                    export_and_model_identity_verified=status == "submitted")
            cost["processes"] = process_cost(record)
            cost["configured_context"] = {"prompt_bytes": len((changes.prompt(cell["arm"], version) + "\nTask:\n" + task["requirement"]).encode()),
                                          "tool_schema_bytes": len(encode(changes.schemas(cell["arm"], version)))}
            cost["attempt_manifest_sha256"] = hashlib.sha256((folder / "manifest.json").read_bytes()).hexdigest()
            row["costs"] = cost
        rows.append(row)
    groups = []
    for model in source["manifest"]["models"]:
        for arm in ("files", "fr"):
            selected = [r for r in rows if r["cell"]["model"] == model and r["cell"]["arm"] == arm]
            costs = [r["costs"] for r in selected if r["costs"] is not None]
            counts = dict(Counter(r["outcome"] for r in selected))
            totals = {key: sum(c["observed"][key] for c in costs) for key in ("host_calls", "fr_calls", "author_applies", "produced_result_bytes", "edit_argument_bytes")}
            wall = sum(c["processes"]["collection_wall_seconds"] for c in costs)
            resolved = all(r["outcome"] in {"passed", "behavior_failed", "failed"} for r in selected)
            passed = counts.get("passed", 0)
            groups.append({"model": model, "arm": arm, "planned": len(selected), "outcomes": counts,
                           "attempts_with_retained_costs": len(costs),
                           "observed_work_all_attempts": totals, "collection_wall_seconds_all_attempts": wall,
                           "collection_wall_seconds_per_behavior_pass": wall / passed if resolved and passed else None,
                           "all_outcomes_resolved": resolved,
                           "tool_and_usage_totals_complete": len(costs) == len(selected) and all(c["coverage"]["collection_complete"] for c in costs),
                           "actual_usd": None})
    pairs = []
    for files in (r for r in rows if r["cell"]["arm"] == "files"):
        key = {k: files["cell"][k] for k in ("task", "model", "repetition")}
        fr = next(r for r in rows if r["cell"]["arm"] == "fr" and all(r["cell"][k] == v for k, v in key.items()))
        both_passed = files["outcome"] == fr["outcome"] == "passed"
        pairs.append({**key, "files_outcome": files["outcome"], "fr_outcome": fr["outcome"], "both_passed": both_passed,
                      "fr_calls": fr["costs"]["observed"]["fr_calls"] if fr["costs"] else None,
                      "fr_minus_files_result_bytes": fr["costs"]["observed"]["produced_result_bytes"] - files["costs"]["observed"]["produced_result_bytes"] if both_passed else None})
    return {"schema": "fr-native-change-outcomes-1", "cohort": root.name, "plan_sha256": frozen["sha256"],
            "grade_report_sha256": hashlib.sha256(grade_path.read_bytes()).hexdigest() if grade_path.exists() else None,
            "outcomes": dict(Counter(r["outcome"] for r in rows)), "attempts": rows, "groups": groups, "pairs": pairs,
            "complete_context_accounting": False, "independent_efficiency_evidence": False,
            "scope": "All planned cells, observed work including failures, and identity-checked behavior reports. A report hash does not authenticate a remote publisher."}


def markdown(reports):
    lines = ["# Native code-change outcomes and observed costs", "",
             "These reviewed integration tasks establish no general efficiency advantage.",
             "Failed attempts remain in cost totals. Missing totals are unknown, not zero.", ""]
    for report in reports:
        lines += ["## " + report["cohort"], "", "Plan: `" + report["plan_sha256"] + "`.", "",
                  "| Configured model | Tools available | Outcome | Host calls | fr calls | Result bytes | Collection seconds | CLI steps complete |",
                  "|---|---|---|---:|---:|---:|---:|---|"]
        for row in report["attempts"]:
            cost = row["costs"]
            values = [row["cell"]["model"], "ordinary files + fr" if row["cell"]["arm"] == "fr" else "ordinary files", OUTCOMES[row["outcome"]],
                      cost["observed"]["host_calls"] if cost else "unknown", cost["observed"]["fr_calls"] if cost else "unknown",
                      cost["observed"]["produced_result_bytes"] if cost else "unknown",
                      f"{cost['processes']['collection_wall_seconds']:.1f}" if cost else "unknown",
                      "yes" if cost and cost["coverage"]["reported_step_usage_complete"] else "no"]
            lines.append("| " + " | ".join(map(str, values)) + " |")
        lines += ["", "| Configured model | Tools available | Outcomes | Collection seconds per behavior pass |",
                  "|---|---|---|---:|"]
        for group in report["groups"]:
            rate = group["collection_wall_seconds_per_behavior_pass"]
            counts = ", ".join(f"{v} {OUTCOMES[k]}" for k, v in sorted(group["outcomes"].items()))
            tools = "ordinary files + fr" if group["arm"] == "fr" else "ordinary files"
            lines.append("| " + " | ".join([group["model"], tools, counts, f"{rate:.1f}" if rate is not None else "undefined"]) + " |")
        lines += ["", "Collection time includes failed attempts; it excludes remote grading time.",
                  "Host bytes are produced results, not complete model context. See JSON for native stream confirmations,",
                  "partial usage, resource samples, tool schemas and paired outcomes. Provider billing remains unverified.", ""]
    return "\n".join(lines)
