"""Admit and collect a small frozen review without spending calls on setup errors."""
import os
import re
from urllib.parse import urlsplit

from . import native_mcp as mcp, source_reviews, terminal_reviews as review
from .study import require

MAX_CELLS = 6


def credentials(models):
    """Select explicit provider keys in memory; never include them in evidence."""
    endpoints = {}
    for model in models:
        review.profile(model)
        provider, endpoint = model["providerID"], model["baseURL"]
        require(provider not in endpoints or endpoints[provider] == endpoint,
                "one provider identity must use one endpoint")
        endpoints[provider] = endpoint
    remote = {provider for provider, endpoint in endpoints.items()
              if urlsplit(endpoint).hostname != "127.0.0.1"}
    if not remote:
        return {}
    require(os.environ.get("GITHUB_ACTIONS") == "true", "live reviews require the remote bounded runner")
    raw, single = os.environ.get("FR_REVIEW_API_KEYS"), os.environ.get("FR_REVIEW_API_KEY")
    require(not (raw and single), "configure either provider keys or one legacy key")
    if raw:
        require(len(raw.encode()) <= 16384, "provider credential map exceeds budget")
        try:
            keys = mcp.decode(raw.encode())
        except (ValueError, UnicodeError):
            raise ValueError("invalid provider credential map") from None
        require(isinstance(keys, dict) and 1 <= len(keys) <= 4
                and all(isinstance(k, str) and re.fullmatch(r"[\w./-]{1,256}", k) for k in keys),
                "invalid provider credential identities")
    else:
        require(len(remote) == 1 and single, "configure a key for every remote provider")
        keys = {next(iter(remote)): single}
    require(all(isinstance(v, str) and 0 < len(v.encode()) <= 4096
                and not any(c in v for c in "\r\n\0") for v in keys.values()), "invalid provider credential")
    require(remote <= keys.keys(), "configure a key for every remote provider")
    return {provider: keys[provider] for provider in remote}


def state(result):
    failures = 0
    for attempt in result["attempts"]:
        if attempt["status"] == "not_started":
            break
        failures = failures + 1 if attempt["status"] == "failed" else 0
    if failures >= 2:
        return "stopped"
    return "finished" if result["not_started"] == 0 else "ready"


def preflight(frozen, snapshots, output, binary, opencode):
    """Validate every remaining provider before creating an attempt directory."""
    plan = review.checked(frozen, snapshots, execution=True)
    require(len(plan["cells"]) <= MAX_CELLS, "serial collection permits at most six frozen cells")
    require(source_reviews.identity(binary) == plan["binary_sha256"]
            and source_reviews.identity(opencode) == plan["opencode_sha256"], "executable changed")
    result = review.report(frozen, snapshots, output)
    status = state(result)
    if status == "ready":
        remaining = {row["cell"]["model"] for row in result["attempts"] if row["status"] == "not_started"}
        credentials([model for i, model in enumerate(plan["models"]) if i in remaining])
    return {"plan_sha256": frozen["sha256"], "status": status,
            "completed": result["completed"], "failed": result["failed"], "not_started": result["not_started"],
            "maximum_remaining_seconds": result["not_started"] * plan["limits"]["wall_seconds"],
            "models": [{k: model[k] for k in ("providerID", "modelID")} for model in plan["models"]],
            "live_compatibility_verified": False, "provider_usage_verified": False}


def collect_all(frozen, snapshots, output, binary, opencode, inputs, plan_sha):
    """Keep frozen order, retain failures and never retry or pass the stop rule."""
    from . import terminal_review_runner as runner
    require(plan_sha == frozen["sha256"], "collection plan differs from reviewed identity")
    admission = preflight(frozen, snapshots, output, binary, opencode)
    if admission["status"] == "ready":
        for cell in frozen["plan"]["cells"]:
            result = review.report(frozen, snapshots, output)
            if state(result) != "ready":
                break
            if next(row for row in result["attempts"] if row["cell"] == cell)["status"] != "not_started":
                continue
            runner.collect(frozen, snapshots, cell["id"], output, binary, opencode, inputs)
    result = review.report(frozen, snapshots, output)
    return {"status": state(result), "report": result}
