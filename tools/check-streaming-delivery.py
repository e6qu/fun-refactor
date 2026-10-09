#!/usr/bin/env python3
"""Hosted factorial delivery diagnostic; never admits workstation collection."""
import argparse
import importlib.util
import json
import os
from pathlib import Path

from agent_eval import scripted_stream
from agent_eval.study import encode, number, require

spec = importlib.util.spec_from_file_location("streaming_base", Path(__file__).with_name("check-change-streaming.py"))
base = importlib.util.module_from_spec(spec)
spec.loader.exec_module(base)
MODES = scripted_stream.DELIVERY_MODES
REPETITIONS = 2


def attempt_report(folder):
    value = base.report(folder)
    require(value["case"] in [base.control.CASES[i] for i in (0, 2)] and value["mode"] in MODES,
            "delivery diagnostic requires ordinary tools and a declared mode")
    samples = [json.loads(line) for line in (folder / "profile.jsonl").read_bytes().splitlines()]
    counter = samples[0].get("cpu_counter")
    require(counter is not None, "delivery diagnostic requires explicit CPU precision")
    deliveries = json.loads((folder / "delivery.json").read_bytes()) if (folder / "delivery.json").exists() else []
    replies = json.loads((folder / "replies.json").read_bytes()) if (folder / "replies.json").exists() else []
    require(len(deliveries) <= len(replies), "extra delivery measurements")
    for delivery, reply in zip(deliveries, replies):
        payload, deadlines = scripted_stream.schedule(reply, value["mode"])
        require(set(delivery) == {"mode", "frames", "scheduled_seconds", "elapsed_seconds", "max_lateness_seconds"},
                "invalid delivery measurement")
        require(delivery["mode"] == value["mode"] and delivery["frames"] == len(payload)
                and delivery["scheduled_seconds"] == deadlines[-1], "delivery schedule differs")
        for key in ("elapsed_seconds", "max_lateness_seconds"):
            number(delivery[key], key)
        require(delivery["elapsed_seconds"] + 1e-6 >= deadlines[-1], "delivery finished before schedule")
    if value["status"] == "completed":
        require(len(deliveries) == len(replies) == 3, "incomplete delivery measurements")
    return {**value, "cpu_counter": counter, "deliveries": deliveries}


def summarize(case, mode, attempts):
    require(case in (0, 2) and mode in MODES and 1 <= len(attempts) <= REPETITIONS, "invalid diagnostic cell")
    completed = [row for row in attempts if row["status"] == "completed"]
    stopped = False
    for row in attempts:
        require(row["mode"] == mode, "cell mode changed")
        require((row["status"] == "not_started") == stopped, "stopped cell was resumed")
        if row["status"] != "not_started":
            require(row["case"] == base.control.CASES[case], "cell provider changed")
            stopped = row["status"] != "completed" or not row["admitted"]
    for key in ("submission_sha256", "reply_identities", "runtime", "binary_sha256", "opencode_sha256", "cpu_counter"):
        require(all(row[key] == completed[0][key] for row in completed), f"repeated {key} differs")
    complete = len(completed) == REPETITIONS
    return {"schema": "fr-streaming-delivery-1", "case": case, "mode": mode, "attempts": attempts,
            "complete": complete, "resource_qualified": complete and all(row["admitted"] for row in completed),
            "mean_cpu_seconds": sum(row["process"]["sampled_cpu_seconds"] for row in completed) / REPETITIONS if complete else None,
            "admitted": False,
            "scope": "Ordinary-tool transport diagnostic, not fr benefit or workstation admission. "
                     "Independent workers; two repetitions, sampled CPU/RSS, fixed client configuration."}


def report(root):
    retained = json.loads((root / "result.json").read_bytes())
    attempts = [attempt_report(root / str(index)) if row["status"] != "not_started" else row
                for index, row in enumerate(retained["attempts"])]
    actual = summarize(retained["case"], retained["mode"], attempts)
    require(actual == retained, "retained delivery report differs")
    return actual


def check(root, case, mode, binary, client):
    require(os.environ.get("GITHUB_ACTIONS") == "true", "run delivery captures only on GitHub")
    require(case in (0, 2) and mode in MODES, "invalid diagnostic cell")
    root.mkdir(parents=True, exist_ok=False)
    attempts, stopped = [], False
    for index in range(REPETITIONS):
        if stopped:
            attempts.append({"status": "not_started", "mode": mode, "reason": "earlier capture failed admission"})
        else:
            folder = root / str(index)
            base.single(folder, case, mode, binary, client, entrypoint=Path(__file__).resolve())
            value = attempt_report(folder)
            attempts.append(value)
            stopped = value["status"] != "completed" or not value["admitted"]
        (root / "result.json").write_bytes(encode(summarize(case, mode, attempts)))
        print(f"{index}/{mode}: {attempts[-1]['status']}", flush=True)


def matrix(root):
    groups = []
    for platform in ("ubuntu-latest", "macos-15"):
        for case in (0, 2):
            cells = {mode: report(root / f"delivery-{platform}-{case}-{mode}") for mode in MODES}
            for mode, cell in cells.items():
                require(cell["case"] == case and cell["mode"] == mode, "matrix cell identity differs")
                expected_counter = "linux-proc-stat" if platform == "ubuntu-latest" else "darwin-ps-time"
                require(all(row["cpu_counter"]["source"] == expected_counter
                            for row in cell["attempts"] if row["status"] != "not_started"), "matrix platform differs")
            complete = [row for cell in cells.values() for row in cell["attempts"] if row["status"] == "completed"]
            for key in ("submission_sha256", "reply_identities", "runtime", "binary_sha256", "opencode_sha256", "cpu_counter"):
                require(all(row[key] == complete[0][key] for row in complete), f"factorial {key} differs")
            differences = {}
            for label, left, right in (("fragmentation_burst", "chunked-burst", "whole-burst"),
                                       ("fragmentation_paced", "chunked-paced", "whole-paced"),
                                       ("pacing_whole", "whole-paced", "whole-burst"),
                                       ("pacing_chunked", "chunked-paced", "chunked-burst")):
                differences[label] = (cells[left]["mean_cpu_seconds"] - cells[right]["mean_cpu_seconds"]
                                      if cells[left]["complete"] and cells[right]["complete"] else None)
                if label.startswith("fragmentation") and differences[label] is not None:
                    require(min(r["text_delta_events"] for r in cells[left]["attempts"]) >
                            max(r["text_delta_events"] for r in cells[right]["attempts"]), "fragmentation not observed")
            groups.append({"platform": platform, "case": case, "cells": cells, "mean_cpu_differences": differences})
    return {"schema": "fr-streaming-delivery-matrix-1", "groups": groups, "admitted": False}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("check", "capture", "report", "matrix"))
    parser.add_argument("output", type=Path)
    parser.add_argument("--case", type=int, choices=(0, 2), default=0)
    parser.add_argument("--mode", choices=MODES)
    parser.add_argument("--fr", type=Path)
    parser.add_argument("--opencode", type=Path)
    args = parser.parse_args()
    if args.command in {"check", "capture"}:
        require(os.environ.get("GITHUB_ACTIONS") == "true", "run delivery captures only on GitHub")
        require(args.mode and args.fr and args.opencode, "capture requires mode and pinned tools")
        if args.command == "capture":
            base.capture(args.output.resolve(), base.control.CASES[args.case], args.mode, args.fr.resolve(), args.opencode.resolve())
        else:
            check(args.output.resolve(), args.case, args.mode, args.fr.resolve(), args.opencode.resolve())
    else:
        print(encode(report(args.output) if args.command == "report" else matrix(args.output)).decode())
