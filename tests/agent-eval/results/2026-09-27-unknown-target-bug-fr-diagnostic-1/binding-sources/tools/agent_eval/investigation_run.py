"""Bounded live execution, behavioral scoring and offline evidence retention."""
from __future__ import annotations

import json
import os
from pathlib import Path
import shutil
import shlex
import signal
import subprocess
import tempfile
import time
from typing import Any

from . import investigation as trial

STAGES = ["check-original", "apply", "check-applied", "undo", "check-restored", "redo", "check-applied", "deliver-patch"]


def stop(process):
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    process.wait()


def group_rss(pid):
    # Sample only the newly owned process group; unrelated compiler jobs are excluded.
    result = subprocess.run(["ps", "-axo", "pgid=,rss="], capture_output=True, text=True, timeout=5)
    return sum(int(row[1])*1024 for line in result.stdout.splitlines()
               if len(row := line.split()) == 2 and row[0] == str(pid))


def run(session, codex, model, effort, tier, timeout):
    trial.config(session)
    phase = trial.load(session / "state.json")["phase"]
    trial.require(phase in {"discover", "deliver"} and 60 <= timeout <= 1200, "invalid phase or time budget")
    prefix = session / f"codex-{phase}"
    paths = {key: Path(str(prefix)+suffix) for key, suffix in
             (("events", "-events.jsonl"), ("stderr", "-stderr.txt"), ("final", "-final.txt"), ("run", "-run.json"))}
    trial.require(not any(path.exists() for path in paths.values()), "phase already attempted; retain it and prepare a fresh session")
    version = subprocess.check_output([codex, "--version"], text=True).strip()
    argv = [codex, "exec", "--ephemeral", "--ignore-user-config", "--ignore-rules", "--skip-git-repo-check",
            "--json", "--color", "never", "--sandbox", "workspace-write", "--add-dir", str(session),
            "--model", model, "--config", f'model_reasoning_effort="{effort}"', "--config", f'service_tier="{tier}"',
            "--cd", str(session / "project"), "--output-last-message", str(paths["final"]), "-"]
    started = time.monotonic()
    process, reason, peak = None, None, 0
    try:
        with (session / f"prompt-{phase}.txt").open("rb") as prompt, paths["events"].open("wb") as out, paths["stderr"].open("wb") as err:
            process = subprocess.Popen(argv, stdin=prompt, stdout=out, stderr=err, env=trial.environment(), start_new_session=True)
            while process.poll() is None:
                peak = max(peak, group_rss(process.pid))
                if time.monotonic()-started > timeout:
                    reason = "time budget"
                elif sum(path.stat().st_size for path in (paths["events"], paths["stderr"])) > 16_777_216:
                    reason = "transcript budget"
                elif peak > 3_221_225_472:
                    reason = "3 GiB sampled process-group RSS budget"
                if reason:
                    stop(process)
                    break
                time.sleep(0.5)
    except BaseException as error:
        reason = f"{type(error).__name__}: {error}"
        if process is not None:
            stop(process)
        raise
    finally:
        record = {"schema": "fr-unknown-target-run-1", "phase": phase, "model": model, "effort": effort,
                  "service_tier": tier, "codex_version": version, "command": argv,
                  "exit_code": process.returncode if process else None, "stopped_reason": reason,
                  "seconds": time.monotonic()-started, "sampled_group_peak_rss_bytes": peak,
                  "prompt_sha256": trial.sha((session / f"prompt-{phase}.txt").read_bytes()),
                  "files": {p.name: trial.sha(p.read_bytes()) for p in paths.values() if p.exists()}}
        trial.save(paths["run"], record)
    return record


def transcript(session, phase, tool_events, *, command_session=None, command_runner=None):
    run_path = session / f"codex-{phase}-run.json"
    if not run_path.exists():
        return {"passed": False, "reason": "no live run", "usage": None}
    run_record = trial.load(run_path)
    trial.require(all(trial.sha((session/name).read_bytes()) == digest for name, digest in run_record["files"].items()), "changed run stream")
    trial.require(run_record["prompt_sha256"] == trial.sha((session/f"prompt-{phase}.txt").read_bytes()), "changed prompt")
    rows = [json.loads(line) for line in (session/f"codex-{phase}-events.jsonl").read_text().splitlines() if line]
    commands, violations, usage = [], [], []
    for row in rows:
        if row.get("type") == "turn.completed":
            usage.append(row.get("usage"))
        if row.get("type") != "item.completed":
            continue
        item = row.get("item", {})
        if item.get("type") == "command_execution":
            commands.append(item)
        elif item.get("type") not in {"agent_message", "reasoning", "todo_list"}:
            violations.append(item.get("type"))
    requests = []
    prefix = f"python3 {command_runner or trial.ROOT / 'tools/investigation-agent.py'} step {command_session or session} --request-stdin <<'FRJSON'\n"
    for item in commands:
        command = item.get("command", "").strip()
        # Codex records the shell argv, including its one command-string argument.
        if command.startswith(("/bin/zsh ", "/bin/bash ", "/bin/sh ")):
            try:
                argv = shlex.split(command)
            except ValueError:
                argv = []
            if len(argv) != 3 or argv[1] not in {"-c", "-lc"}:
                violations.append(command)
                continue
            command = argv[2].strip()
        if not command.startswith(prefix) or not command.endswith("\nFRJSON"):
            violations.append(command)
            continue
        try:
            requests.append(json.loads(command[len(prefix):-len("\nFRJSON")]))
        except ValueError:
            violations.append(command)
    expected = [e["request"] for e in tool_events if e["phase"] == phase]
    passed = (not violations and requests == expected and bool(requests) and bool(usage)
              and run_record["exit_code"] == 0 and run_record["stopped_reason"] is None
              and all(c.get("exit_code") == 0 for c in commands))
    return {"passed": passed, "violations": violations, "commands": len(commands), "usage": usage,
            "requests_match": requests == expected, "seconds": run_record["seconds"],
            "sampled_group_peak_rss_bytes": run_record["sampled_group_peak_rss_bytes"]}


def event_chain(rows, selected, state, final):
    if not rows or [e["id"] for e in rows] != list(range(len(rows))):
        return False
    for phase, initial in (("discover", selected["original"]), ("deliver", state.get("resume_snapshot"))):
        subset = [e for e in rows if e["phase"] == phase]
        if not subset or initial is None:
            return False
        previous = trial.sha(trial.encode(initial))
        for row in subset:
            if row["before"] != previous or row["request"]["tool"] != "execute" and row["before"] != row["after"]:
                return False
            previous = row["after"]
        if phase == "deliver" and previous != trial.sha(trial.encode(final)):
            return False
    phases = [e["phase"] for e in rows]
    return (phases == sorted(phases, key=lambda p: {"discover":0,"deliver":1}.get(p,2))
            and phases[-1] == "deliver" and all(p in {"discover","deliver"} for p in phases))


def delivery_passed(value):
    if value.get("kind") == "fr":
        report = value.get("report", {})
        stages = report.get("workflow", {}).get("stages", [])
        return (report.get("passed") is True and [s.get("stage") for s in stages] == STAGES
                and all(s.get("status") == "passed" for s in stages))
    stages = value.get("stages", [])
    return (value.get("kind") == "files" and value.get("passed") is True
            and [s.get("name") for s in stages] == ["original", "changed", "restored", "reapplied"]
            and all(s.get("passed") is True for s in stages)
            and stages[0]["snapshot"] == stages[2]["snapshot"] and stages[1]["snapshot"] == stages[3]["snapshot"])


def behavior(task, patch, perturbation, expected=None):
    with tempfile.TemporaryDirectory(prefix="fr-investigation-replay-") as directory:
        project = Path(directory) / "project"
        trial.BASE.unpack(project, task)
        (project / ".fr").mkdir()
        trial.save(project / ".fr/checks.json", {"schema": 1, "checks": trial.BASE.profile(task)["checks"]})
        trial.BASE.initialize(project)
        target = trial.checked_path(project, perturbation["changed_path"])
        target.write_bytes(target.read_bytes() + perturbation["appended"].encode())
        original = trial.snapshot(project)
        trial.BASE.git(project, "apply", "--check", data=patch)
        trial.BASE.git(project, "apply", data=patch)
        final = trial.snapshot(project)
        checks = trial.run_checks(project, trial.BASE.profile(task)["checks"])
        oracle = trial.BASE.verify(project, task)
        trial.BASE.git(project, "apply", "--reverse", data=patch)
        restored = trial.snapshot(project) == original
        trial.BASE.git(project, "apply", data=patch)
        reapplied = trial.snapshot(project) == final
        return {"passed": checks["passed"] and oracle["passed"] and restored and reapplied
                and (expected is None or expected == final), "checks": checks, "oracle": oracle,
                "restored": restored, "reapplied": reapplied, "matches": expected is None or expected == final,
                "final_snapshot": final}


def score(session):
    selected = trial.config(session)
    rows = trial.events(session)
    state = trial.load(session / "state.json")
    phase_results = {phase: transcript(session, phase, rows) for phase in ("discover", "deliver")}
    patch = session / "project/artifacts/change.patch"
    resumption = trial.load(session / "resumption.json") if (session / "resumption.json").exists() else {}
    delivery = trial.load(session / "delivery.json") if (session / "delivery.json").exists() else {}
    final = trial.snapshot(session / "project")
    receiver = behavior(selected["task"], patch.read_bytes(), resumption, final) if patch.exists() and resumption else {"passed": False}
    oracle = trial.BASE.verify(session / "project", selected["task"])
    changed = sorted(name for name in final if final[name] != state.get("resume_snapshot", selected["original"]).get(name))
    resumption_ok = bool(resumption.get("stale_review_refusal")) and resumption.get("stale_review_unchanged_source") is True
    if selected["arm"] == "fr":
        native = resumption.get("native_plan", {})
        states = {s["id"]: s["state"] for s in native.get("plan", {}).get("steps", [])}
        resumption_ok = resumption_ok and "diagnosis" in native.get("invalidated", []) and states.get("independent") == "satisfied"
    result = {"schema": "fr-unknown-target-result-1", "task": selected["task"], "arm": selected["arm"],
              "passed": False, "phases": phase_results, "resumption_passed": resumption_ok,
              "delivery_passed": delivery_passed(delivery), "oracle": oracle, "receiver": receiver,
              "changed_paths": changed, "finished": state["finished"], "tool_calls": len(rows),
              "event_chain_passed": event_chain(rows, selected, state, final),
              "tool_seconds": sum(e["seconds"] for e in rows),
              "visible_bytes": sum(len(trial.encode(e["visible"])) for e in rows),
              "request_bytes": sum(len(trial.encode(e["request"])) for e in rows),
              "source_reveals": sum(e["request"]["tool"] == "read" and e["request"].get("path", "").endswith(".rs") or e["request"]["tool"] == "project"
                                    and "--source" in e["request"].get("args", []) for e in rows),
              "refusals": sum("error" in e["visible"] for e in rows),
              "measurement_scope": "One two-phase live trial; cooperative tool isolation. Usage comes from Codex turn events. No billed quota, population, security or semantic-equivalence claim."}
    result["passed"] = bool(all(p["passed"] for p in phase_results.values()) and resumption_ok
        and result["event_chain_passed"] and result["delivery_passed"] and oracle["passed"] and receiver["passed"] and state["finished"]
        and bool(changed) and all(p.endswith(".rs") for p in changed)
        and set(changed) <= set(trial.load(session / "handoff.json")["dependencies"])
        and (selected["arm"] != "fr" or any(e["request"]["tool"] == "guide" and "error" not in e["visible"] for e in rows))
        and rows and rows[-1]["request"]["tool"] == "finish")
    trial.save(session / "result.json", result)
    return result


def record(session, destination, diagnostic=False):
    selected = trial.config(session)
    result: dict[str, Any] = trial.load(session / "result.json") if (session / "result.json").exists() else {"passed": False}
    trial.require(diagnostic or result["passed"], "failed/incomplete trial must be retained as diagnostic")
    destination.mkdir(parents=True, exist_ok=False)
    names = [p.name for p in session.iterdir() if p.is_file() and p.name != "fr-bin"]
    for name in names:
        shutil.copyfile(session/name, destination/name)
    for name in selected["bindings"]:
        target = destination / "binding-sources" / name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(trial.ROOT/name, target)
    patch = session / "project/artifacts/change.patch"
    if patch.exists():
        shutil.copyfile(patch, destination/"change.patch")
    if (session/"objects").exists():
        shutil.copytree(session/"objects", destination/"objects")
    manifest = {"schema": "fr-unknown-target-manifest-1", "acceptance": result["passed"] and not diagnostic,
                "source_session": str(session), "task": selected["task"], "arm": selected["arm"],
                "files": {str(p.relative_to(destination)): trial.sha(p.read_bytes())
                          for p in sorted(destination.rglob("*")) if p.is_file()}}
    trial.save(destination/"manifest.json", manifest)
    return audit(destination)


def audit(destination, replay=False):
    manifest = trial.load(destination/"manifest.json")
    trial.require(manifest["schema"] == "fr-unknown-target-manifest-1", "unsupported manifest")
    for name, digest in manifest["files"].items():
        path = destination/name
        trial.require(path.resolve().is_relative_to(destination) and not path.is_symlink(), "unsafe retained path")
        trial.require(trial.sha(path.read_bytes()) == digest, f"changed retained file: {name}")
    selected = trial.load(destination/"session.json")
    for name, digest in selected["bindings"].items():
        trial.require(trial.sha((destination/"binding-sources"/name).read_bytes()) == digest, "changed evaluator snapshot")
    result: dict[str, Any] = trial.load(destination/"result.json") if (destination/"result.json").exists() else {"passed": False}
    if manifest["acceptance"]:
        rows = trial.events(destination)
        for phase in ("discover", "deliver"):
            actual = transcript(destination, phase, rows, command_session=manifest["source_session"], command_runner=selected["runner"])
            trial.require(actual == result["phases"][phase] and actual["passed"], "retained command accounting differs")
        trial.require(delivery_passed(trial.load(destination/"delivery.json")), "retained delivery stages failed")
        trial.require(event_chain(rows, selected, trial.load(destination/"state.json"), result["receiver"]["final_snapshot"]),
                      "retained source events do not connect")
        trial.require(result["tool_calls"] == len(rows)
                      and result["visible_bytes"] == sum(len(trial.encode(e["visible"])) for e in rows)
                      and result["refusals"] == sum("error" in e["visible"] for e in rows), "retained metrics differ")
        trial.require(result["passed"] and all(p["passed"] for p in result["phases"].values())
                      and result["resumption_passed"] and result["delivery_passed"] and result["oracle"]["passed"]
                      and result["receiver"]["passed"], "acceptance lacks passing evidence")
    if replay:
        trial.require(manifest["acceptance"], "replay needs accepted delivery")
        actual = behavior(selected["task"], (destination/"change.patch").read_bytes(),
                          trial.load(destination/"resumption.json"), result["receiver"]["final_snapshot"])
        trial.require(actual["passed"], f"behavior replay failed: {actual}")
    return {"verified": True, "acceptance": manifest["acceptance"], "replayed": replay}
