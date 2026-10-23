"""Prescribed source operations executed through a native client's real shell tool.

This trusted fixture is never an autonomous agent or a live-trial image.
"""
import argparse
import difflib
import hashlib
import json
from pathlib import Path
import subprocess

from agent_eval.comparison_clients import require

ARTIFACTS = Path("/tmp/handoff")


def run(argv, expected):
    result = subprocess.run(argv, capture_output=True, timeout=30)
    require(len(result.stdout) + len(result.stderr) <= 1024**2, "handoff command output exceeds 1 MiB")
    require(result.returncode == expected, f"{argv[:5]!r}: {result.returncode}: " +
            (result.stdout + result.stderr).decode(errors="replace")[-4096:])
    return result


def git(*args):
    return run(["git", "-c", "user.name=fr acceptance", "-c", "user.email=fr@example.invalid",
                "-c", "core.hooksPath=/dev/null", *args], 0).stdout


def snapshot():
    return {name: {"sha256": hashlib.sha256(Path(name).read_bytes()).hexdigest(),
                   "mode": Path(name).stat().st_mode & 0o777}
            for name in git("ls-files", "-z").decode().split("\0") if name}


def fingerprint(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True).encode()).hexdigest()


def write(name, value):
    (ARTIFACTS / name).write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n")


def patched(edits):
    sources = {edit["path"]: Path(edit["path"]).read_text() for edit in edits}
    for edit in edits:
        old, new, path = edit["old"], edit["new"], edit["path"]
        require(not old or sources[path].count(old) == 1, "prescribed edit must match current source exactly once")
        sources[path] = sources[path].replace(old, new, 1) if old else sources[path] + new
    return sources


def patch(edits):
    return "".join("".join(difflib.unified_diff(Path(path).read_text().splitlines(True), text.splitlines(True),
                    fromfile="a/" + path, tofile="b/" + path)) for path, text in patched(edits).items())


def fr(*args):
    result = json.loads(run(["fr", "--json", *args], 0).stdout)
    require("error" not in result, "fr refused the prescribed current operation")
    return result


def review(edits, prefix):
    operations = []
    for index, edit in enumerate(edits):
        path, name = edit["path"], edit["name"]
        if name:
            found = fr("project", "find", name, "--in", path, "--source", "--bytes", "8192")
            rows = [dict(zip(found["columns"], row)) for row in found["rows"]]
            rows = [row for row in rows if row["name"] == name]
            require(len(rows) == 1, "fr must resolve exactly one current declaration")
            handle = rows[0]["handle"]
            fragment = edit["new"][edit["new"].index("{"):]
            operation = "replace-body"
        else:
            found = fr("project", "map", path, "--depth", "0", "--fields", "handle,kind")
            require(len(found["rows"]) == 1, "fr must resolve one file insertion target")
            handle, fragment, operation = found["rows"][0][0], edit["new"], "insert-declaration"
        body = ARTIFACTS / f"{prefix}-{index}.txt"
        body.write_text(fragment)
        operations.append({"op": operation, "handle": handle, "from": str(body)})
    manifest = ARTIFACTS / (prefix + "-manifest")
    manifest.write_text(json.dumps({"operations": operations, "postconditions": {
        "files-changed": len({e["path"] for e in edits}), "paths-changed": sorted({e["path"] for e in edits})}}))
    preview = fr("author", "batch", "--from", str(manifest))
    saved = fr("author", "batch", "--from", str(manifest), "--save-plan", "--plan-basis", preview["plan_context_basis"])
    require(saved["transaction"] is not None, "prescribed repair must save a transaction")
    write(prefix + "-review.json", {"preview": preview, "saved": saved})
    return saved


def discover(scenario, setup):
    ARTIFACTS.mkdir(exist_ok=False)
    before = snapshot()
    for path in scenario["paths"]:
        require(Path(path).is_file(), "task source is missing")
    handoff = ("# Prescribed task handoff\n\n" + scenario["diagnosis"] + "\n\n" +
               "Sources inspected: " + ", ".join(scenario["paths"]) + "\n\n" +
               "A repair was reviewed but has not been applied. Read current source before resuming.\n\n" +
               "Upstream tests have not run in this container. Independent grading is still pending.\n")
    Path("HANDOFF.md").write_text(handoff)
    if setup == "fr":
        saved = review(scenario["original_edits"], "old")
        fr("history", "apply", str(saved["transaction"]))
    else:
        (ARTIFACTS / "old.patch").write_text(patch(scenario["original_edits"]))
        git("apply", "--check", str(ARTIFACTS / "old.patch"))
        saved = None
    require(snapshot() == before, "discovery changed tracked source")
    state = {"before": before, "handoff_sha256": hashlib.sha256(handoff.encode()).hexdigest(), "saved": saved,
             "old_review_accepted": True}
    write("discovery.json", state)
    print(json.dumps({"status": "DISCOVER_OK", "source": fingerprint(before), "paths": scenario["paths"],
                      "reviewed_not_applied": True, "upstream_checks": "pending"}))


def intervene(scenario):
    state = json.loads((ARTIFACTS / "discovery.json").read_text())
    require(snapshot() == state["before"], "source changed before the declared intervention")
    edit = scenario["intervention"]
    source = Path(edit["path"])
    text = source.read_text()
    require(text.count(edit["old"]) == 1 and edit["old"] != edit["new"], "intervention no longer matches once")
    source.write_text(text.replace(edit["old"], edit["new"], 1))
    git("add", edit["path"])
    git("commit", "-qm", "Rename a local variable while the task is paused")
    resumed = snapshot()
    require(resumed != state["before"], "the pause did not change source")
    state["resumed"] = resumed
    write("discovery.json", state)
    return {"before": fingerprint(state["before"]), "resumed": fingerprint(resumed), "path": edit["path"]}


def deliver(scenario, setup):
    state = json.loads((ARTIFACTS / "discovery.json").read_text())
    require(hashlib.sha256(Path("HANDOFF.md").read_bytes()).hexdigest() == state["handoff_sha256"], "handoff changed during the pause")
    before = snapshot()
    require(before == state["resumed"] and before != state["before"], "delivery needs the changed current source")
    if setup == "fr":
        saved = state["saved"]
        refused = run(["fr", "--json", "history", "apply", str(saved["transaction"]), "--write", "--no-diff",
                       "--context-basis", saved["transaction_context_basis"]], 1)
        require(b"project source changed after planning" in refused.stdout + refused.stderr, "old plan failed for an unexpected reason")
    else:
        refused = run(["git", "apply", "--check", str(ARTIFACTS / "old.patch")], 1)
        require(b"patch failed:" in refused.stderr, "old patch failed for an unexpected reason")
    require(snapshot() == before, "stale-plan refusal changed source")
    write("stale-refusal.json", {"exit_code": refused.returncode, "stdout": refused.stdout.decode(errors="replace"),
                                "stderr": refused.stderr.decode(errors="replace"), "source_unchanged": True})
    for edit in scenario["resumed_edits"]:
        if edit["old"]:
            require(Path(edit["path"]).read_text().count(edit["old"]) == 1, "read current source before the new review")
    if setup == "fr":
        saved = review(scenario["resumed_edits"], "fresh")
        transaction = str(saved["transaction"])
        fr("history", "apply", transaction, "--write", "--no-diff", "--context-basis", saved["transaction_context_basis"])
        changed = snapshot()
        fr("history", "undo", transaction)
        fr("history", "undo", transaction, "--write", "--no-diff")
        require(snapshot() == before, "fr undo did not restore the intervening source exactly")
        fr("history", "redo", transaction)
        fr("history", "redo", transaction, "--write", "--no-diff")
    else:
        current = ARTIFACTS / "fresh.patch"
        current.write_text(patch(scenario["resumed_edits"]))
        git("apply", "--check", str(current))
        git("apply", str(current))
        changed = snapshot()
        git("apply", "--reverse", str(current))
        require(snapshot() == before, "Git undo did not restore the intervening source exactly")
        git("apply", str(current))
    require(snapshot() == changed and changed != before, "redo did not restore the changed source exactly")
    require(sorted(path for path in before if before[path] != changed[path]) == scenario["paths"], "repair changed an unexpected path")
    require(scenario["preserved_name"] in Path(scenario["intervention"]["path"]).read_text(), "repair overwrote the intervening change")
    require(scenario["resumed_edits"][0]["new"] in Path(scenario["intervention"]["path"]).read_text(),
            "the delivered function differs from the prescribed current-source repair")
    git("diff", "--check")
    data = git("diff", "--binary")
    require(0 < len(data) <= 256 * 1024, "invalid delivered patch size")
    Path("change.patch").write_bytes(data)
    result = {"status": "DELIVER_OK", "before": fingerprint(before), "after": fingerprint(changed),
              "stale_refused": True, "undo_exact": True, "redo_exact": True, "paths": scenario["paths"],
              "handoff_sha256": state["handoff_sha256"], "patch_sha256": hashlib.sha256(data).hexdigest(),
              "upstream_checks": "pending independent grader", "light_check": "git diff --check passed"}
    write("delivery.json", result)
    print(json.dumps(result))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("phase", choices=("discover", "deliver"))
    parser.add_argument("--setup", choices=("files", "fr", "codegraph"), required=True)
    args = parser.parse_args()
    scenario = json.loads(Path("/opt/handoff.json").read_text())
    (discover if args.phase == "discover" else deliver)(scenario, args.setup)
