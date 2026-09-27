#!/usr/bin/env python3
"""Audit matched investigation arms, including failed behavioral outcomes."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path

from agent_eval import investigation as trial
from agent_eval import investigation_run as live

DEFAULT = trial.ROOT / "tests/agent-eval/unknown-target/cohort.json"


def validate_pairs(entries):
    pairs = {}
    threads = set()
    for entry in entries:
        key, arm = entry["task"], entry["arm"]
        trial.require(key in {"unicode-dice", "regex-escape-len"} and arm in {"fr", "files"}, "unexpected cohort member")
        pair = pairs.setdefault(key, {})
        trial.require(arm not in pair, "duplicate cohort arm")
        pair[arm] = entry
        for identity in entry["thread_ids"]:
            trial.require(isinstance(identity, str) and identity and identity not in threads, "agent process was reused")
            threads.add(identity)
        trial.require(len(entry["thread_ids"]) == 2, "trial needs two fresh agent processes")
    trial.require(set(pairs) == {"unicode-dice", "regex-escape-len"}, "cohort needs both task shapes")
    for pair in pairs.values():
        trial.require(set(pair) == {"fr", "files"}, "cohort needs both arms for every task")
        for field in ("profile", "binary_sha256", "bindings", "requirement", "settings", "perturbation"):
            trial.require(pair["fr"][field] == pair["files"][field], f"unmatched {field}")
    return pairs


def entry(path):
    selected = trial.load(path/"session.json")
    result = trial.load(path/"result.json")
    contract = trial.load(path/"binding-sources/tests/agent-eval/unknown-target/task.json")
    identities, settings = [], []
    for phase in ("discover", "deliver"):
        rows = [json.loads(line) for line in (path/f"codex-{phase}-events.jsonl").read_text().splitlines() if line]
        starts = [r["thread_id"] for r in rows if r.get("type") == "thread.started"]
        trial.require(len(starts) == 1, "phase must have one fresh thread")
        identities.extend(starts)
        run = trial.load(path/f"codex-{phase}-run.json")
        settings.append({k:run[k] for k in ("model", "effort", "service_tier", "codex_version")})
    trial.require(settings[0] == settings[1], "model settings changed between phases")
    resumption = trial.load(path/"resumption.json")
    usage = [u for phase in result["phases"].values() for u in phase["usage"]]
    metrics = {k:result[k] for k in ("tool_calls", "visible_bytes", "request_bytes", "source_reveals", "refusals")}
    metrics["task_passed"] = result["passed"]
    metrics["unsupported_proof_claim_attempts"] = sum(e["request"].get("source_equivalence_proven") is True
                                                    for e in trial.events(path))
    metrics["agent_seconds"] = sum(p["seconds"] for p in result["phases"].values())
    metrics["sampled_group_peak_rss_bytes"] = max(p["sampled_group_peak_rss_bytes"] for p in result["phases"].values())
    metrics["usage"] = {key:sum(u[key] for u in usage) if usage and all(key in u for u in usage) else None
                        for key in ("input_tokens", "cached_input_tokens", "output_tokens", "reasoning_output_tokens")}
    return {"task":selected["task"], "arm":selected["arm"], "thread_ids":identities,
            "profile":selected["profile"], "binary_sha256":selected["binary_sha256"],
            "bindings":selected["bindings"], "requirement":contract["tasks"][selected["task"]]["requirement"],
            "settings":settings[0], "perturbation":{k:resumption[k] for k in ("changed_path", "appended")}, "metrics":metrics}


def audit_comparison(path, replay):
    manifest = trial.load(path/"manifest.json")
    selected, result = trial.load(path/"session.json"), trial.load(path/"result.json")
    rows, state = trial.events(path), trial.load(path/"state.json")
    metrics = {"tool_calls":len(rows),
               "visible_bytes":sum(len(trial.encode(e["visible"])) for e in rows),
               "request_bytes":sum(len(trial.encode(e["request"])) for e in rows),
               "refusals":sum("error" in e["visible"] for e in rows)}
    trial.require(all(result[key] == value for key,value in metrics.items()), "comparison metrics differ")
    for phase in ("discover", "deliver"):
        actual = live.transcript(path, phase, rows, command_session=manifest["source_session"], command_runner=selected["runner"])
        trial.require(actual == result["phases"][phase] and actual["passed"], "comparison has an invalid agent transcript")
    trial.require(live.delivery_passed(trial.load(path/"delivery.json")) and state["finished"]
                  and result["resumption_passed"] and result["event_chain_passed"], "comparison did not complete its protocol")
    trial.require(live.event_chain(rows, selected, state, result["receiver"]["final_snapshot"]), "comparison source chain differs")
    receiver = result["receiver"]
    trial.require(all(receiver[k] for k in ("restored", "reapplied", "matches"))
                  and receiver["checks"]["passed"], "receiver delivery failed")
    trial.require(result["oracle"]["passed"] == receiver["oracle"]["passed"]
                  and result["passed"] == (result["oracle"]["passed"] and receiver["passed"]), "task outcome differs from its oracles")
    if replay:
        actual = live.behavior(selected["task"], (path/"change.patch").read_bytes(), trial.load(path/"resumption.json"), receiver["final_snapshot"])
        trial.require(actual["oracle"]["passed"] == receiver["oracle"]["passed"]
                      and actual["oracle"].get("stage") == receiver["oracle"].get("stage")
                      and actual["checks"]["passed"] and all(actual[k] for k in ("restored", "reapplied", "matches")),
                      "behavior replay changed the recorded outcome")


def audit(manifest_path=DEFAULT, replay=False):
    manifest = trial.load(manifest_path)
    trial.require(manifest["schema"] == "fr-unknown-target-cohort-1", "unsupported cohort")
    comparisons, diagnostics = [], []
    paths = set()
    for member in manifest["trials"]:
        path = trial.ROOT/member["path"]
        trial.require(path.resolve().is_relative_to(trial.ROOT) and path not in paths, "unsafe or repeated cohort path")
        paths.add(path)
        trial.require(trial.sha((path/"manifest.json").read_bytes()) == member["manifest_sha256"], "cohort member changed")
        result = live.audit(path)
        if member["role"] == "comparison":
            audit_comparison(path, replay)
            comparisons.append(entry(path))
        else:
            trial.require(member["role"] == "diagnostic" and not result["acceptance"], "diagnostic classification differs")
            diagnostics.append(member["path"])
    pairs = validate_pairs(comparisons)
    return {"verified":True, "replayed":replay, "comparison_trials":len(comparisons), "diagnostic_trials":len(diagnostics),
            "tasks":{task:{arm:row["metrics"] for arm,row in arms.items()} for task,arms in pairs.items()},
            "scope":"One completed-protocol pair per task, including failed behavioral outcomes, plus retained diagnostics. Automated delivery in both arms; no population efficiency claim."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("manifest", nargs="?", type=Path, default=DEFAULT)
    parser.add_argument("--replay", action="store_true")
    args = parser.parse_args()
    os.environ.update(trial.environment())
    print(json.dumps(audit(args.manifest, args.replay), indent=2))


if __name__ == "__main__":
    main()
