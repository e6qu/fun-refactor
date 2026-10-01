"""Freeze independent tasks and matched tool arms before collecting outcomes."""
from __future__ import annotations

import hashlib
import json
import math
import random
import re
from pathlib import Path

ARMS = ("files", "fr")
TOKEN_FIELDS = ("uncached_input", "cache_read", "cache_write", "output")


def require(condition, message):
    if not condition:
        raise ValueError(message)


def encode(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False).encode()


def digest(value):
    return hashlib.sha256(encode(value)).hexdigest()


def load(path):
    def unique(pairs):
        result = {}
        for key, value in pairs:
            require(key not in result, f"duplicate JSON key: {key}")
            result[key] = value
        return result
    return json.loads(Path(path).read_text(), object_pairs_hook=unique,
                      parse_constant=lambda value: require(False, f"invalid JSON number: {value}"))


def number(value, label, *, integer=False, positive=False):
    require(type(value) in ((int,) if integer else (int, float))
            and 0 <= value <= 2**63 - 1 and math.isfinite(value) and (value > 0 if positive else value >= 0),
            f"{label} must be a finite {'positive' if positive else 'nonnegative'} number")
    return value


def text(value, label):
    require(isinstance(value, str) and bool(value.strip()), f"{label} must be nonempty text")
    return value


def sha(value, label):
    require(isinstance(value, str) and re.fullmatch(r"[0-9a-f]{64}", value), f"invalid {label} SHA-256")
    return value


def named(rows, label):
    require(isinstance(rows, list) and rows, f"{label} must be a nonempty list")
    result = {}
    for row in rows:
        identity = row["id"]
        require(isinstance(identity, str) and re.fullmatch(r"[a-z0-9][a-z0-9-]*", identity),
                f"invalid {label} id")
        require(identity not in result, f"duplicate {label} id: {identity}")
        result[identity] = row
    return result


def validate(manifest):
    require(manifest["schema"] == "fr-agent-study-1", "unsupported study schema")
    number(manifest["seed"], "seed", integer=True)
    repetitions = number(manifest["repetitions"], "repetitions", integer=True, positive=True)
    require(repetitions <= 100, "at most 100 repetitions per study")
    number(manifest["spend_cap_usd"], "spend cap", positive=True)
    require(manifest["cache_state"] in {"cold", "warm", "uncontrolled"}, "invalid cache state")
    budgets = manifest["budgets"]
    for field in ("wall_seconds", "aggregate_agent_seconds", "aggregate_tokens", "rss_bytes", "disk_bytes"):
        number(budgets[field], field, integer=True, positive=True)
    number(budgets["attempt_cap_usd"], "attempt cap", positive=True)
    require(budgets["attempt_cap_usd"] <= manifest["spend_cap_usd"], "attempt cap exceeds study cap")
    children = number(manifest["max_children"], "max children", integer=True)
    for field in ("binary_sha256", "skill_sha256"):
        sha(manifest["fr"][field], field)
    text(manifest["fr"]["version"], "fr version")
    tasks, models = named(manifest["tasks"], "task"), named(manifest["models"], "model")
    require(len(tasks) * len(models) * repetitions * 4 <= 10000, "study matrix exceeds 10000 cells")
    for task in tasks.values():
        require(task["kind"] in {"explain", "fix", "feature", "proof"}, "invalid task kind")
        text(task["repository"], "repository")
        require(isinstance(task["revision"], str) and re.fullmatch(r"[0-9a-f]{40}|[0-9a-f]{64}", task["revision"]),
                "repository revision must be a full Git object id")
        text(task["requirement"], "requirement")
        sha(task["grader_sha256"], "grader")
        require(type(task["held_out"]) is bool and type(task["delegated"]) is bool, "task flags must be booleans")
        require(not task["delegated"] or children > 0, "delegated task needs a child budget")
    for model in models.values():
        for field in ("provider", "model", "harness"):
            text(model[field], field)
        require(isinstance(model["settings"], dict), "model settings must be an object")
        prices = model["pricing"]
        text(prices["source"], "pricing source")
        text(prices["as_of"], "pricing date")
        for field in TOKEN_FIELDS:
            number(prices["usd_per_million"][field], f"{field} price")
    # Hashing also rejects non-JSON values and nonfinite numbers in host settings.
    digest(manifest)
    return tasks, models


def plan(manifest):
    tasks, models = validate(manifest)
    pairs = []
    rng = random.Random(manifest["seed"])
    for task in sorted(tasks):
        modes = ("single", "delegated") if tasks[task]["delegated"] else ("single",)
        for model in sorted(models):
            for mode in modes:
                for repetition in range(1, manifest["repetitions"] + 1):
                    key = {"task": task, "model": model, "mode": mode, "repetition": repetition}
                    pair = digest(key)[:24]
                    arms = list(ARMS)
                    rng.shuffle(arms)
                    pairs.append([{**key, "pair": pair, "arm": arm, "id": f"{pair}-{arm}"} for arm in arms])
    rng.shuffle(pairs)
    cells = [cell for pair in pairs for cell in pair]
    return {"schema": "fr-agent-study-plan-1", "manifest": manifest,
            "manifest_sha256": digest(manifest), "cells": cells}


def checked_plan(value):
    require(encode(value) == encode(plan(value["manifest"])), "plan differs from its frozen manifest")
    return value


def artifact(root, reference):
    """Read only a pinned, bounded file below the attempt evidence directory."""
    relative = Path(text(reference["path"], "artifact path"))
    require(not relative.is_absolute() and ".." not in relative.parts, "unsafe artifact path")
    path = root / relative
    require(path.resolve().is_relative_to(root.resolve()), "artifact escapes evidence directory")
    require(path.is_file() and path.stat().st_size <= 16 * 1024 * 1024, "missing or oversized artifact")
    data = path.read_bytes()
    require(hashlib.sha256(data).hexdigest() == sha(reference["sha256"], "artifact"), "artifact digest differs")
    return data
