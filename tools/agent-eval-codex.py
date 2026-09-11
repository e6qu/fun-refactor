#!/usr/bin/env python3
"""Run an explicitly selected fresh agent-evaluation pair through Codex CLI."""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import time


DEFAULT_MODEL = "gpt-5.6-luna"
DEFAULT_EFFORT = "low"
DEFAULT_SERVICE_TIER = "default"


def load(path):
    return json.loads(path.read_text())


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def selected_pairs(sessions, names):
    experiment = load(sessions / "experiment.json")
    planned = experiment.get("trials")
    if not isinstance(planned, list) or not planned:
        raise ValueError("Experiment has no planned trials")
    if not names:
        raise ValueError("Select both members of at least one pair with --trial")
    if len(names) != len(set(names)) or any(name not in planned for name in names):
        raise ValueError("Selected trials must be distinct members of this experiment")
    selected = []
    groups = {}
    for name in names:
        session = sessions / name
        config = load(session / "session.json")
        key = (config["task"], config["repetition"])
        if config["arm"] not in ("fr", "files") or config["arm"] in groups.setdefault(key, {}):
            raise ValueError(f"Pair {key} has repeated or unknown arms")
        groups.setdefault(key, {})[config["arm"]] = (name, session, config)
    for key, arms in groups.items():
        if set(arms) != {"fr", "files"}:
            raise ValueError(f"Select both fr and files trials for pair {key}")
        selected.append((arms["fr"], arms["files"]))
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


def run_trial(codex, entry, model, effort, service_tier, timeout):
    name, session, config = entry
    record_path = session / "codex-run.json"
    events_path = session / "codex-events.jsonl"
    stderr_path = session / "codex-stderr.txt"
    if any(path.exists() for path in (record_path, events_path, stderr_path, session / "events.jsonl")):
        raise ValueError(f"Trial {name} is not fresh; prepare a new session")
    prompt_path = session / "prompt.txt"
    command = codex_command(codex, session, model, effort, service_tier)
    started = time.time()
    timed_out = False
    launch_error = None
    with events_path.open("wb") as stdout, stderr_path.open("wb") as stderr:
        try:
            result = subprocess.run(
                command,
                input=prompt_path.read_bytes(),
                stdout=stdout,
                stderr=stderr,
                timeout=timeout,
                check=False,
            )
            exit_code = result.returncode
        except subprocess.TimeoutExpired:
            timed_out = True
            exit_code = 124
        except OSError as error:
            launch_error = str(error)
            stderr.write((launch_error + "\n").encode())
            exit_code = None
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
        "exit_code": exit_code,
        "timed_out": timed_out,
        "launch_error": launch_error,
        "command": command,
    }
    record_path.write_text(json.dumps(record, indent=2) + "\n")
    return record


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("sessions", type=Path)
    parser.add_argument("--trial", action="append", default=[])
    parser.add_argument("--codex", type=Path, default=Path("codex"))
    parser.add_argument("--model", default=DEFAULT_MODEL)
    parser.add_argument("--effort", default=DEFAULT_EFFORT)
    parser.add_argument("--service-tier", default=DEFAULT_SERVICE_TIER)
    parser.add_argument("--timeout", type=int, default=3600)
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument(
        "--confirm-agent-spend",
        action="store_true",
        help="Required acknowledgement that the selected Codex runs consume quota.",
    )
    args = parser.parse_args()
    if args.timeout < 60:
        parser.error("--timeout must be at least 60 seconds")
    try:
        pairs = selected_pairs(args.sessions, args.trial)
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
    if not args.confirm_agent_spend:
        parser.error("--confirm-agent-spend is required for a real Codex run")
    version = subprocess.check_output([args.codex, "--version"], text=True).strip()
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
