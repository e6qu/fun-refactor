#!/usr/bin/env python3
"""Run an explicitly selected fresh agent-evaluation pair through Codex CLI."""

import argparse
import hashlib
import io
import json
from pathlib import Path
import tempfile
import time

from agent_eval.bounded_host import run as run_bounded


def load(path):
    return json.loads(path.read_text())


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def selected_pairs(sessions, names):
    experiment = load(sessions / "experiment.json")
    planned = experiment.get("trials")
    expected_arms = experiment.get("pair_arms", ["fr", "files"])
    if not isinstance(planned, list) or not planned or not all(isinstance(name, str) for name in planned) or len(set(planned)) != len(planned):
        raise ValueError("Experiment has no planned trials")
    if (
        not isinstance(expected_arms, list)
        or len(expected_arms) != 2
        or not all(isinstance(arm, str) and arm for arm in expected_arms)
        or len(set(expected_arms)) != 2
    ):
        raise ValueError("Experiment pair_arms must name two distinct non-empty arms")
    if not names:
        raise ValueError("Select both members of at least one pair with --trial")
    if len(names) != len(set(names)) or any(name not in planned for name in names):
        raise ValueError("Selected trials must be distinct members of this experiment")
    selected = []
    groups = {}
    for name in planned:
        if name not in names:
            continue
        if Path(name).name != name or name in {".", ".."}:
            raise ValueError("Unsafe trial path")
        session = sessions / name
        if not session.resolve().is_relative_to(sessions.resolve()):
            raise ValueError("Trial path escapes sessions directory")
        config = load(session / "session.json")
        key = (config["task"], config["repetition"], config.get("model"), config.get("mode"))
        if config["arm"] not in expected_arms or config["arm"] in groups.setdefault(key, {}):
            raise ValueError(f"Pair {key} has repeated or unknown arms")
        groups.setdefault(key, {})[config["arm"]] = (name, session, config)
    for key, arms in groups.items():
        if set(arms) != set(expected_arms):
            raise ValueError(f"Select both {expected_arms[0]} and {expected_arms[1]} trials for pair {key}")
        left, right = (arms[arm][2] for arm in expected_arms)
        for field in ("repository_revision", "requirement_sha256", "grader_sha256",
                      "budgets", "model_settings", "fr", "cache_state", "plan_sha256"):
            if (field in left) != (field in right) or left.get(field) != right.get(field):
                raise ValueError(f"Pair {key} has unmatched {field}")
        order = list(arms) if experiment.get("preserve_trial_order") is True else expected_arms
        selected.append(tuple(arms[arm] for arm in order))
    return selected


def codex_command(codex, session, model, effort, service_tier):
    return [
        str(codex),
        "exec",
        "--ephemeral",
        "--ignore-user-config",
        "--ignore-rules",
        "--skip-git-repo-check",
        "--json",
        "--color",
        "never",
        "--sandbox",
        "workspace-write",
        "--add-dir",
        str(session),
        "--model",
        model,
        "--config",
        f'model_reasoning_effort="{effort}"',
        "--config",
        f'service_tier="{service_tier}"',
        "--cd",
        str(session / "project"),
        "--output-last-message",
        str(session / "codex-final.txt"),
        "-",
    ]


def validate_settings(config, model, effort, service_tier):
    if not all(isinstance(value, str) and value.strip() for value in (model, effort, service_tier)):
        raise ValueError("Model, effort and service tier must be declared as nonempty text")
    if config.get("model") != model:
        raise ValueError("Requested model differs from the session's declared model")
    expected = {"reasoning_effort": effort, "service_tier": service_tier}
    if config.get("model_settings") != expected:
        raise ValueError("Requested settings differ from the session's declared settings")


def freeze_settings(pairs, model, effort, service_tier):
    """Declare settings before any attempt, without changing the task evaluator."""
    declared = {"model": model, "model_settings": {"reasoning_effort": effort, "service_tier": service_tier}}
    validate_settings(declared, model, effort, service_tier)
    entries = [entry for pair in pairs for entry in pair]
    # Check the whole selection before writing any session. Existing declarations
    # and attempt records cannot be overwritten, even with identical values.
    for name, session, config in entries:
        if any(key in config for key in declared):
            raise ValueError(f"Trial {name} already declares model settings; prepare a new session")
        if any((session / filename).exists() for filename in (
                "codex-run.json", "codex-events.jsonl", "codex-stderr.txt", "codex-final.txt", "events.jsonl")):
            raise ValueError(f"Trial {name} is not fresh; prepare a new session")
    for _, session, config in entries:
        (session / "session.json").write_text(json.dumps({**config, **declared}, indent=2) + "\n")
    return {"trials": [name for name, _, _ in entries], **declared}


def run_trial(codex, entry, model, effort, service_tier, timeout):
    name, session, config = entry
    validate_settings(config, model, effort, service_tier)
    record_path = session / "codex-run.json"
    events_path = session / "codex-events.jsonl"
    stderr_path = session / "codex-stderr.txt"
    if any(path.exists() for path in (record_path, events_path, stderr_path, session / "events.jsonl")):
        raise ValueError(f"Trial {name} is not fresh; prepare a new session")
    prompt_path = session / "prompt.txt"
    command = codex_command(codex, session, model, effort, service_tier)
    started = time.time()
    with events_path.open("wb") as stdout, stderr_path.open("wb") as stderr:
        execution = run_bounded(command, prompt_path.read_bytes(), stdout, stderr, session,
                                wall_seconds=timeout)
    record = {
        "schema": "fr-agent-eval-codex-run-1",
        "trial": name,
        "task": config["task"],
        "arm": config["arm"],
        "repetition": config["repetition"],
        "model": model,
        "reasoning_effort": effort,
        "service_tier": service_tier,
        "ephemeral": True,
        "ignored_user_config": True,
        "ignored_rules": True,
        "sandbox": "workspace-write",
        "prompt_sha256": digest(prompt_path),
        "events_sha256": digest(events_path),
        "stderr_sha256": digest(stderr_path),
        "started_at": started,
        "elapsed_seconds": time.time() - started,
        **execution,
        "command": command,
    }
    record_path.write_text(json.dumps(record, indent=2) + "\n")
    return record


def codex_version(codex):
    with tempfile.TemporaryDirectory() as temporary:
        stdout, stderr = io.BytesIO(), io.BytesIO()
        result = run_bounded([str(codex), "--version"], b"", stdout, stderr, Path(temporary),
                             wall_seconds=10, transcript_bytes=65536)
    if result["stop_reason"] or result["exit_code"] != 0:
        raise ValueError(f"Codex version probe failed: {result}")
    return stdout.getvalue().decode().strip()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    freeze = commands.add_parser("freeze-settings", help="Declare settings in fresh sessions without calling Codex")
    execute = commands.add_parser("run", help="Run or inspect commands for sessions with frozen settings")
    for command in (freeze, execute):
        command.add_argument("sessions", type=Path)
        command.add_argument("--trial", action="append", required=True)
        command.add_argument("--model", required=True)
        command.add_argument("--effort", required=True)
        command.add_argument("--service-tier", required=True)
    execute.add_argument("--codex", type=Path, required=True)
    execute.add_argument("--timeout", type=int, required=True)
    execution = execute.add_mutually_exclusive_group(required=True)
    execution.add_argument("--dry-run", action="store_true")
    execution.add_argument(
        "--confirm-agent-spend",
        action="store_true",
        help="Required acknowledgement that the selected Codex runs consume quota.",
    )
    args = parser.parse_args()
    if args.command == "run" and not 60 <= args.timeout <= 900:
        parser.error("--timeout must be between 60 and 900 seconds")
    try:
        pairs = selected_pairs(args.sessions, args.trial)
        if args.command == "freeze-settings":
            print(json.dumps(freeze_settings(pairs, args.model, args.effort, args.service_tier), indent=2))
            return
        for pair in pairs:
            for entry in pair:
                validate_settings(entry[2], args.model, args.effort, args.service_tier)
    except (OSError, KeyError, ValueError, json.JSONDecodeError) as error:
        parser.error(str(error))
    if args.dry_run:
        print(
            json.dumps(
                {
                    "pairs": [[fr[0], files[0]] for fr, files in pairs],
                    "commands": [
                        codex_command(args.codex, entry[1], args.model, args.effort, args.service_tier)
                        for pair in pairs
                        for entry in pair
                    ],
                },
                indent=2,
            )
        )
        return
    version = codex_version(args.codex)
    records = []
    for pair in pairs:
        for entry in pair:
            record = run_trial(
                args.codex,
                entry,
                args.model,
                args.effort,
                args.service_tier,
                args.timeout,
            )
            record["codex_version"] = version
            (entry[1] / "codex-run.json").write_text(json.dumps(record, indent=2) + "\n")
            records.append(record)
    print(json.dumps({"runs": records}, indent=2))


if __name__ == "__main__":
    main()
