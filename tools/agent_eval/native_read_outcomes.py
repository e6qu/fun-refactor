"""Observed costs for every frozen native source-reading attempt, including failures."""
from collections import Counter
import hashlib

from . import native_costs, native_discovery, native_outcomes, opencode_native
from .study import encode, load, require


def cohort(root):
    frozen = load(root / "plan.json")
    original = opencode_native.report(frozen, root / "attempts")
    source = opencode_native.checked(frozen)
    plan = frozen["plan"]
    version = plan.get("tools_schema_version", 1)
    require(version in (2, 3, 4, 5), "native cost replay needs an explicit supported tool schema")
    rows = []
    for record in original["attempts"]:
        cell, status = record["cell"], record["status"]
        outcome = ("passed" if record["passed"] else "answer_failed") if status == "completed" else status
        row = {"cell": cell, "outcome": outcome, "collection_status": status,
               "failure": record.get("failure"), "costs": None}
        if status in {"completed", "failed"}:
            task = next(t for t in source["tasks"] if t["id"] == cell["task"])
            folder = root / "attempts" / cell["id"]
            def retained(name):
                path = folder / name
                return path.read_bytes() if path.exists() else b""
            cost = native_costs.observed(retained("opencode.stdout"), retained("tools.jsonl"),
                                         task, cell["arm"], version, read_only=True)
            cost["coverage"].update(collection_complete=status == "completed",
                                    reported_step_usage_complete=status == "completed",
                                    export_and_model_identity_verified=status == "completed")
            cost["processes"] = native_outcomes.process_cost({"wall_seconds": record["seconds"], "processes": record["processes"]})
            cost["configured_context"] = {
                "prompt_bytes": len((native_discovery.prompt(plan, cell["arm"]) + "\nTask:\n" + task["requirement"]).encode()),
                "tool_schema_bytes": len(encode(plan["tools"][cell["arm"]]))}
            cost["attempt_manifest_sha256"] = hashlib.sha256((folder / "manifest.json").read_bytes()).hexdigest()
            row["costs"] = cost
        rows.append(row)
    groups = []
    for model in source["manifest"]["models"]:
        for arm in plan["tools"]:
            selected = [r for r in rows if r["cell"]["model"] == model and r["cell"]["arm"] == arm]
            costs = [r["costs"] for r in selected if r["costs"] is not None]
            counts = dict(Counter(r["outcome"] for r in selected))
            totals = {key: sum(c["observed"][key] for c in costs) for key in
                      ("host_calls", "fr_calls", "produced_result_bytes", "source_page_bytes", "repeated_source_page_bytes")}
            wall = sum(c["processes"]["collection_wall_seconds"] for c in costs)
            resolved = all(r["outcome"] in {"passed", "answer_failed", "failed"} for r in selected)
            passed = counts.get("passed", 0)
            groups.append({"model": model, "arm": arm, "planned": len(selected), "outcomes": counts,
                           "attempts_with_retained_costs": len(costs), "observed_work_all_attempts": totals,
                           "collection_wall_seconds_all_attempts": wall,
                           "collection_wall_seconds_per_pass": wall / passed if resolved and passed else None,
                           "all_outcomes_resolved": resolved, "actual_usd": None})
    pairs = []
    for row in (r for r in rows if r["cell"]["arm"] != "files"):
        key = {k: row["cell"][k] for k in ("task", "model", "repetition")}
        baseline = next(r for r in rows if r["cell"]["arm"] == "files" and all(r["cell"][k] == v for k, v in key.items()))
        passed = baseline["outcome"] == row["outcome"] == "passed"
        pairs.append({**key, "arm": row["cell"]["arm"], "files_outcome": baseline["outcome"], "arm_outcome": row["outcome"],
                      "both_passed": passed, "fr_calls": row["costs"]["observed"]["fr_calls"] if row["costs"] else None,
                      "result_bytes_difference": row["costs"]["observed"]["produced_result_bytes"] - baseline["costs"]["observed"]["produced_result_bytes"] if passed else None})
    return {"schema": "fr-native-read-outcomes-1", "cohort": root.name, "plan_sha256": frozen["sha256"],
            "outcomes": dict(Counter(r["outcome"] for r in rows)), "attempts": rows, "groups": groups, "pairs": pairs,
            "complete_context_accounting": False, "independent_efficiency_evidence": False,
            "scope": "Original outcomes and all observed work; later citation reviews remain separate. Partial usage is not a complete provider bill."}


def markdown(reports):
    lines = ["# Native source-reading outcomes and observed costs", "",
             "Original outcomes are preserved. Tool output includes failed attempts. Collection time omits",
             "grader, provider, host and cache costs. Source-page counts omit code in metadata; total context",
             "and provider billing remain unknown. Tool availability does not establish fr use or benefit.", ""]
    for report in reports:
        lines.extend(["## " + report["cohort"], "",
                      "| Model | Tools | Outcome | Calls | fr calls | Result bytes | Source bytes | Seconds |",
                      "| --- | --- | --- | --- | --- | --- | --- | --- |"])
        for row in report["attempts"]:
            cost = row["costs"]
            values = [str(cost["observed"][key]) for key in ("host_calls", "fr_calls", "produced_result_bytes", "source_page_bytes")] if cost else ["unknown"] * 4
            seconds = f'{cost["processes"]["collection_wall_seconds"]:.1f}' if cost else "unknown"
            lines.append("| " + " | ".join([row["cell"]["model"], row["cell"]["arm"], row["outcome"], *values, seconds]) + " |")
        lines.extend(["", f'Comparable successful ordinary/fr pairs: {sum(p["both_passed"] for p in report["pairs"])}.', ""])
    lines.extend(["The JSON report retains partial step usage, produced versus native-confirmed results,",
                  "repeated source, configuration bytes and per-model/arm totals including failures.",
                  "Zero successful outcomes leave time per pass undefined. Missing attempts stay missing.", ""])
    return "\n".join(lines)
