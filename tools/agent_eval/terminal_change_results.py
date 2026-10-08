"""Grade exact terminal submissions separately and retain every planned outcome."""
import hashlib
import tempfile
from pathlib import Path

from . import isolated_grade, native_checks, source_reviews, terminal_changes as changes
from .study import digest, encode, load, require
from .workspace_bundle import unpack

SCHEMA = "fr-terminal-change-grades-1"


def grade(frozen, snapshots, attempts, *, execute=isolated_grade.grade):
    native_checks.require_runner()
    plan = changes.checked(frozen, snapshots, execution=True)
    replay = changes.report(frozen, snapshots, attempts)
    outcomes = []
    for record in replay["attempts"]:
        cell = record["cell"]
        if record["status"] != "completed":
            outcomes.append({"cell": cell, "outcome": record["status"]})
            continue
        task = next(t for t in plan["tasks"] if t["id"] == cell["task"])
        files = load(attempts / cell["id"] / "submission.json")
        raw = encode(task["grader"])
        with tempfile.TemporaryDirectory(prefix="fr-terminal-grade-") as temporary:
            root = Path(temporary)
            spec = root / "grader.json"
            spec.write_bytes(raw)
            unpack(files, root / "candidate", changes.edits.MAX_WORKSPACE)
            result = execute(root / "candidate", spec, hashlib.sha256(raw).hexdigest())
        native_checks.verify_grade(task["grader"], hashlib.sha256(raw).hexdigest(),
                                   native_checks.candidate_identity(files), result)
        outcomes.append({"cell": cell, "submission_sha256": digest(files),
                         "outcome": result["outcome"], "grade": result})
    return {"schema": SCHEMA, "plan_sha256": frozen["sha256"], "outcomes": outcomes,
            "passed": sum(r["outcome"] == "passed" for r in outcomes),
            "scope": "Exact frozen black-box cases on submitted files; collection failure is not a behavior grade."}


def verify_grades(frozen, replay, attempts, grades):
    plan = frozen["plan"]
    require(grades["schema"] == SCHEMA and grades["plan_sha256"] == frozen["sha256"], "grade plan differs")
    require([r["cell"] for r in grades["outcomes"]] == plan["cells"], "grade cells differ")
    outcomes = {}
    for row, record in zip(grades["outcomes"], replay["attempts"]):
        cell = record["cell"]
        if record["status"] != "completed":
            require(row == {"cell": cell, "outcome": record["status"]}, "unsubmitted attempt has a grade")
            continue
        files = load(attempts / cell["id"] / "submission.json")
        require(row["submission_sha256"] == digest(files), "graded submission differs")
        task = next(t for t in plan["tasks"] if t["id"] == cell["task"])
        passed = native_checks.verify_grade(task["grader"], hashlib.sha256(encode(task["grader"])).hexdigest(),
                                           native_checks.candidate_identity(files), row["grade"])
        require(row["outcome"] == row["grade"]["outcome"], "grade outcome differs")
        outcomes[cell["id"]] = "passed" if passed else "behavior_failed"
    require(type(grades["passed"]) is int and grades["passed"] == sum(v == "passed" for v in outcomes.values()),
            "grade pass count differs")
    return outcomes


def report(frozen, snapshots, attempts, grades=None):
    replay = changes.report(frozen, snapshots, attempts)
    outcomes = verify_grades(frozen, replay, attempts, grades) if grades is not None else {}
    rows = []
    for record in replay["attempts"]:
        cell, status = record["cell"], record["status"]
        row = {"cell": cell, "model": frozen["plan"]["models"][cell["model"]], "collection_status": status,
               "outcome": outcomes.get(cell["id"], "pending_grading" if status == "completed" else status),
               "failure": record.get("failure"), "observed": record.get("observed"), "process": record.get("process"),
               "submission_sha256": record.get("audit", {}).get("submission_sha256"),
               "changed_paths": record.get("audit", {}).get("changed_paths"),
               "manifest_sha256": source_reviews.identity(attempts / cell["id"] / "manifest.json") if status != "not_started" else None}
        rows.append(row)
    pairs = []
    for ordinary in (r for r in rows if r["cell"]["arm"] == "files"):
        cell = ordinary["cell"]
        assisted = next(r for r in rows if r["cell"]["arm"] == "fr"
                        and all(r["cell"][k] == cell[k] for k in ("task", "model")))
        both = ordinary["outcome"] == assisted["outcome"] == "passed"
        pairs.append({"task": cell["task"], "model": cell["model"], "files_outcome": ordinary["outcome"],
                      "fr_outcome": assisted["outcome"], "both_passed": both,
                      "fr_calls": assisted["observed"]["fr_calls"] if assisted["observed"] else None,
                      "fr_minus_files_result_bytes": assisted["observed"]["tool_result_bytes"] - ordinary["observed"]["tool_result_bytes"] if both else None})
    observed = [r for r in rows if r["observed"] is not None]
    return {"schema": "fr-terminal-change-results-1", "plan_sha256": frozen["sha256"],
            "grade_sha256": digest(grades) if grades is not None else None, "attempts": rows, "pairs": pairs,
            "totals_including_failures": {
                "attempts_with_observed_costs": len(observed), "planned": len(rows),
                "host_calls": sum(r["observed"]["host_calls"] for r in observed),
                "fr_calls": sum(r["observed"]["fr_calls"] for r in observed),
                "tool_result_bytes": sum(r["observed"]["tool_result_bytes"] for r in observed),
                "collection_seconds": sum(r["process"]["elapsed_seconds"] for r in observed)},
            "actual_usd": None, "provider_usage_verified": False, "complete_context_accounting": False,
            "efficiency_advantage": False,
            "scope": "All frozen attempts, exact submissions and observed client counters. Missing cost is unknown. "
                     "A report hash does not authenticate a remote publisher; no general efficiency claim."}


def markdown(result):
    lines = ["# Configured code-change results", "", "Plan: `" + result["plan_sha256"] + "`.", "",
             "| Model | Available tools | Outcome | Calls | fr calls | Result bytes | Seconds | Peak MiB |",
             "| --- | --- | --- | ---: | ---: | ---: | ---: | ---: |"]
    for row in result["attempts"]:
        model, cost, process = row["model"], row["observed"], row["process"]
        values = [model["providerID"] + "/" + model["modelID"],
                  "ordinary + fr" if row["cell"]["arm"] == "fr" else "ordinary", row["outcome"],
                  *[cost[k] if cost else "unknown" for k in ("host_calls", "fr_calls", "tool_result_bytes")],
                  f"{process['elapsed_seconds']:.2f}" if process else "unknown",
                  f"{process['sampled_aggregate_rss_bytes'] / 1024**2:.2f}" if process else "unknown"]
        lines.append("| " + " | ".join(str(v).replace("|", "\\|").replace("\n", " ") for v in values) + " |")
    lines += ["", "Failed attempts remain in observed totals. Unstarted attempts have unknown cost.",
              "Result bytes are tool output, not complete model context; reported tokens retain provider semantics.",
              "Behavior passes cover only the frozen cases. Dollar cost and general efficiency remain unverified.", ""]
    return "\n".join(lines)
