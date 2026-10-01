"""Unknown-target trials with retained observations and fresh-process resumption.

The tool boundary is cooperative. Scores audit the agent command stream separately.
No model service is used by preparation, tool steps, scoring or replay.
"""
from __future__ import annotations

from dataclasses import replace
import difflib
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time
from typing import Any
from .investigation_lock import session_lock
from .external_environment import environment as external_environment

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "sdk/python/src"))
from fr_ir.context import DirectoryObjectStore
from fr_ir.investigation import Dependency, DependencyKind, Evidence, EvidenceKind, TaskPlan, TaskStep
from fr_ir.ir import TaskChange, TaskDelivery, TaskTarget
from fr_ir.runtime import FrClient, FrRuntimeError

SPEC = importlib.util.spec_from_file_location("investigation_base", ROOT / "tools/agent-eval.py")
assert SPEC is not None and SPEC.loader is not None
BASE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BASE)
TASK = ROOT / "tests/agent-eval/unknown-target/task.json"
MAX_CALLS, MAX_BYTES = 80, 1_048_576
PERTURBATION = "\n// Independent source revision while the investigation was interrupted.\n"
SOURCE_TOOLS = {"project", "read", "search", "guide"}


def encode(value):
    return json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(",", ":"), allow_nan=False).encode()


def sha(data):
    return hashlib.sha256(data).hexdigest()


def load(path) -> Any:
    return json.loads(path.read_text())


def save(path, value):
    path.write_bytes(encode(value) + b"\n")


def bindings():
    paths = [ROOT / p for p in ("tools/investigation-agent.py", "tools/agent_eval/investigation.py",
        "tools/agent_eval/investigation_run.py", "tools/agent_eval/investigation_prompt.py", "tools/agent_eval/investigation_rehearsal.py",
        "tools/agent_eval/investigation_lock.py",
        "tools/agent-eval.py", "tools/agent_eval/oracle.py", "tools/agent_eval/regex_workspace.py",
        "tools/agent_eval/regex_escape_len.py", "tools/agent_eval/external_environment.py",
        "tests/agent-eval/unknown-target/task.json")]
    paths += sorted((ROOT / "sdk/python/src/fr_ir").glob("*.py"))
    return {str(p.relative_to(ROOT)): sha(p.read_bytes()) for p in paths}


def environment():
    return dict(external_environment(),
                CARGO_BUILD_JOBS="1", CARGO_INCREMENTAL="0", CARGO_PROFILE_DEV_DEBUG="0",
                CARGO_PROFILE_TEST_DEBUG="0", RAYON_NUM_THREADS="1", RUST_TEST_THREADS="1")


def command(argv, root, *, data=None, timeout=180):
    result = subprocess.run(argv, cwd=root, input=data, capture_output=True, timeout=timeout, env=environment())
    return {"exit_code": result.returncode, "stdout": result.stdout.decode(errors="replace")[:16000],
            "stderr": result.stderr.decode(errors="replace")[-4096:]}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def checked_path(root, name):
    require(isinstance(name, str) and name and not Path(name).is_absolute()
            and not any(p in {"..", ".git", ".fr", "target", "artifacts"} for p in Path(name).parts),
            "path must name project source or documentation")
    path = root / name
    require(path.resolve().is_relative_to(root.resolve()) and not path.is_symlink(), "path escaped project")
    return path


def snapshot(root):
    return BASE.snapshot(root)


def config(session):
    value = load(session / "session.json")
    require(value["bindings"] == bindings(), "evaluator changed after preparation")
    require(sha(Path(value["binary"]).read_bytes()) == value["binary_sha256"], "frozen binary changed")
    require(sha((session / "project/.fr/checks.json").read_bytes()) == value["checks_sha256"], "declared checks changed")
    return value


def events(session):
    path = session / "events.jsonl"
    return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []


def client(session, selected):
    return FrClient(session / "project", executable=selected["binary"], timeout=900, max_output_bytes=2_097_152)


def prepare(session, binary, task, arm):
    from .investigation_prompt import prompt
    require(task in load(TASK)["tasks"] and arm in {"fr", "files"}, "unknown task or arm")
    session.mkdir(parents=True, exist_ok=False)
    project = session / "project"
    BASE.unpack(project, task)
    (project / ".fr").mkdir()
    checks = BASE.profile(task)["checks"]
    save(project / ".fr/checks.json", {"schema": 1, "checks": checks})
    BASE.initialize(project)
    (project / "artifacts").mkdir()
    preflight = run_checks(project, checks)
    require(preflight["passed"], f"upstream preflight failed: {preflight}")
    original_oracle = BASE.verify(project, task)
    require(not original_oracle["passed"] and original_oracle.get("stage") == (2 if task == "unicode-dice" else 1),
            f"unexpected original oracle: {original_oracle}")
    frozen = session / "fr-bin"
    shutil.copyfile(binary, frozen)
    frozen.chmod(0o555)
    selected = {"schema": "fr-unknown-target-session-1", "task": task, "arm": arm,
                "runner": str(ROOT / "tools/investigation-agent.py"),
                "profile": BASE.profile(task), "binary": str(frozen), "binary_sha256": sha(frozen.read_bytes()),
                "bindings": bindings(), "original": snapshot(project), "preflight": preflight,
                "original_oracle": original_oracle,
                "checks_sha256": sha((project / ".fr/checks.json").read_bytes())}
    save(session / "session.json", selected)
    save(session / "state.json", {"phase": "discover", "finished": False})
    (session / "prompt-discover.txt").write_text(prompt(session, selected, "discover"))
    return {"task": task, "arm": arm, "preflight": preflight, "original_oracle": original_oracle}


def run_checks(project, checks):
    results = [{"name": check["name"], **command(check["argv"], project, timeout=check["timeout_seconds"])} for check in checks]
    return {"passed": all(r["exit_code"] == 0 for r in results), "checks": results}


def make_change(request, checks):
    require(set(request) <= {"tool", "targets", "postconditions"}, "unknown review field")
    targets = []
    for target in request["targets"]:
        require(set(target) == {"id", "handle", "op", "fragment"}, "review targets need inline fragments")
        targets.append(TaskTarget(**target))
    return TaskChange([], targets, request["postconditions"], [c["name"] for c in checks],
                      TaskDelivery(patch="artifacts/change.patch", check_output_bytes=1024))


def baseline_patch(project, edits):
    require(isinstance(edits, list) and 1 <= len(edits) <= 16, "review needs 1..16 edits")
    originals: dict[Path, bytes] = {}
    try:
        for edit in edits:
            require(set(edit) == {"path", "old", "new"}, "edit requires path, old and new")
            path = checked_path(project, edit["path"])
            require(path.suffix == ".rs" and path.is_file(), "edit must select an existing Rust source")
            originals.setdefault(path, path.read_bytes())
            text = path.read_text()
            old, new = edit["old"], edit["new"]
            require(isinstance(old, str) and isinstance(new, str), "edit text must be strings")
            require(not old or text.count(old) == 1, "replacement needs one exact match; empty old appends")
            path.write_text(text.replace(old, new, 1) if old else text + new)
        return "".join("".join(difflib.unified_diff(data.decode().splitlines(keepends=True),
            path.read_text().splitlines(keepends=True), fromfile="a/"+str(path.relative_to(project)),
            tofile="b/"+str(path.relative_to(project)))) for path, data in originals.items())
    finally:
        for path, data in originals.items():
            path.write_bytes(data)


def review(session, selected, request):
    state = load(session / "state.json")
    require(state["phase"] in {"discover", "deliver"} and not state["finished"], "review outside active phase")
    prior = events(session)
    require(any(e["phase"] == state["phase"] and e["request"]["tool"] in SOURCE_TOOLS
                and "error" not in e["visible"] for e in prior), "inspect current evidence before review")
    project = session / "project"
    before = snapshot(project)
    require(before == state.get("resume_snapshot", selected["original"]), "source changed outside reviewed delivery")
    if selected["arm"] == "fr":
        change = make_change(request, selected["profile"]["checks"])
        native = client(session, selected).review(change)
        require(native.at("/ready") is True, "native review is not ready")
        paths = request["postconditions"].get("paths-changed")
        require(isinstance(paths, list) and paths, "review must declare its changed paths")
        value = {"kind": "fr", "manifest_json": native.manifest.decode(), "report": native.to_data(), "before": before, "paths": paths}
        visible = native.to_data()
    else:
        require(set(request) == {"tool", "edits"}, "file review requires edits")
        patch = baseline_patch(project, request["edits"])
        require(patch, "review has no change")
        value = {"kind": "files", "patch": patch, "before": before, "paths": sorted({e["path"] for e in request["edits"]})}
        visible = {"ready": True, "diff": patch}
    require(snapshot(project) == before, "review changed source")
    digest = sha(encode(value))
    save(session / "review.json", value)
    return {"review": digest, "report": visible}


def execute_review(session, selected, expected):
    project = session / "project"
    value = load(session / "review.json")
    require(expected == sha(encode(value)), "review identity changed")
    if selected["arm"] == "fr":
        report = client(session, selected).call("task-change", "--from", "-", "--write", "--basis",
                    value["report"]["task_change_basis"], input_bytes=value["manifest_json"].encode()).to_data()
        return {"kind": "fr", "report": report}
    require(snapshot(project) == value["before"], "stale source review")
    checks = selected["profile"]["checks"]
    stages = []
    patch = value["patch"].encode()
    original = snapshot(project)
    changed = None
    for name in ("original", "changed", "restored", "reapplied"):
        if name != "original":
            args = ["apply", "--reverse"] if name == "restored" else ["apply"]
            BASE.git(project, *args, "--check", data=patch)
            BASE.git(project, *args, data=patch)
        state = snapshot(project)
        if name == "changed":
            changed = state
        if name == "restored":
            require(state == original, "reversal changed source")
        if name == "reapplied":
            require(state == changed, "reapply differs")
        result = run_checks(project, checks)
        stages.append({"name": name, "snapshot": state, **result})
        require(result["passed"], f"check failed at {name}: {result}")
    (project / "artifacts").mkdir(exist_ok=True)
    (project / "artifacts/change.patch").write_bytes(patch)
    return {"kind": "files", "passed": True, "stages": stages}


def checkpoint(session, selected, request):
    require(set(request) == {"tool", "diagnosis", "dependencies", "evidence", "pending"}, "checkpoint fields differ")
    prior = events(session)
    require((session / "review.json").exists(), "review a proposed change before checkpoint")
    dependencies = request["dependencies"]
    require(isinstance(dependencies, list) and 1 <= len(dependencies) <= 8
            and len(set(dependencies)) == len(dependencies), "select 1..8 distinct source dependencies")
    for path in dependencies:
        require(checked_path(session / "project", path).is_file() and path.endswith(".rs"),
                f"dependencies: {path!r} must be a relative Rust source path, not a declaration handle")
    require(set(load(session / "review.json")["paths"]) <= set(dependencies), "diagnosis dependencies must cover every proposed changed path")
    cited = request["evidence"]
    require(isinstance(cited, list) and cited, "evidence: cite at least one successful discovery event ID")
    eligible = [i for i,e in enumerate(prior) if e["request"]["tool"] in SOURCE_TOOLS and "error" not in e["visible"]]
    for index, event in enumerate(cited):
        require(type(event) is int and event in eligible,
                f"evidence[{index}]: {event!r} is not successful discovery; cite IDs from {eligible}. Reviews are not discovery events.")
    require(all(isinstance(request[k], str) and 1 <= len(request[k]) <= 4096 for k in ("diagnosis", "pending")),
            "retain bounded diagnosis and pending work")
    save(session / "handoff.json", request)
    if selected["arm"] == "fr":
        plan = TaskPlan(load(TASK)["tasks"][selected["task"]]["requirement"], ("diagnosed",), (
            TaskStep("diagnosis", request["diagnosis"], tuple(Dependency(DependencyKind.SOURCE, p) for p in dependencies), satisfies=("diagnosed",)),
            TaskStep("delivery", request["pending"], (Dependency(DependencyKind.WORKSPACE, "selected-project"),), depends_on=("diagnosis",)),
            TaskStep("independent", "Pinned project documentation is retained", (Dependency(DependencyKind.SOURCE, "README.md"),)),
        ), hypotheses=(request["diagnosis"],))
        native = client(session, selected)
        for name in ("diagnosis", "independent"):
            started = plan.resume(native, transition=f"{name}:start")
            evidence = Evidence("retained-observation", EvidenceKind.OBSERVATION, started.input_digests[name], True,
                                "agent hypothesis and cited observations; not a behavioral proof")
            plan = replace(started.plan, steps=tuple(replace(s, evidence=(evidence,)) if s.id == name else s for s in started.plan.steps))
            plan = plan.resume(native, transition=f"{name}:satisfy").plan
        digest = plan.store(DirectoryObjectStore(session / "objects"))
        save(session / "plan-root.json", {"root": digest})
    state = load(session / "state.json")
    state["phase"] = "paused"
    save(session / "state.json", state)
    return {"checkpointed": True, "diagnosis": request["diagnosis"], "pending": request["pending"]}


def resume(session):
    with session_lock(session):
        return _resume(session)


def _resume(session):
    from .investigation_prompt import prompt
    selected = config(session)
    state = load(session / "state.json")
    require(state["phase"] == "paused", "only a checkpointed session can resume")
    handoff = load(session / "handoff.json")
    path = sorted(handoff["dependencies"])[0]
    target = checked_path(session / "project", path)
    target.write_bytes(target.read_bytes() + PERTURBATION.encode())
    before = snapshot(session / "project")
    try:
        execute_review(session, selected, sha(encode(load(session / "review.json"))))
    except (ValueError, FrRuntimeError) as error:
        refusal = str(error)
    else:
        raise ValueError("stale review was admitted")
    require(snapshot(session / "project") == before, "stale review wrote source")
    result = {"changed_path": path, "appended": PERTURBATION, "stale_review_refusal": refusal,
              "stale_review_unchanged_source": True}
    if selected["arm"] == "fr":
        plan = TaskPlan.restore(DirectoryObjectStore(session / "objects"), load(session / "plan-root.json")["root"])
        restored = plan.resume(client(session, selected))
        states = {s.id: s.state.value for s in restored.plan.steps}
        require("diagnosis" in restored.invalidated and states["independent"] == "satisfied"
                and states["delivery"] != "satisfied", "plan reused dependent evidence")
        result["native_plan"] = restored.report.to_data()
    save(session / "resumption.json", result)
    state.update(phase="deliver", resume_snapshot=before)
    save(session / "state.json", state)
    (session / "prompt-deliver.txt").write_text(prompt(session, selected, "deliver"))
    return result


def act(session, selected, request):
    project = session / "project"
    state = load(session / "state.json")
    kind = request["tool"]
    require(state["phase"] in {"discover", "deliver"} and not state["finished"], "session is paused or finished")
    if kind == "project":
        require(selected["arm"] == "fr", "project belongs to fr arm")
        args = request["args"]
        require(isinstance(args, list) and args and args[0] in {"map", "find", "show", "select", "trace", "impact", "evidence", "architecture"}
                and all(isinstance(a, str) and not a.startswith(("--write", "--save", "--output")) for a in args), "unsupported read-only project query")
        return client(session, selected).project(*args).to_data()
    if kind == "guide":
        require(selected["arm"] == "fr", "guide belongs to fr arm")
        return client(session, selected).call("guide", "--from", "-", input_bytes=encode(request["goal"])).to_data()
    if kind == "files":
        require(selected["arm"] == "files", "files belongs to ordinary arm")
        paths = list(selected["original"])
        return {"paths": paths[:200], "omitted": max(0, len(paths)-200)}
    if kind == "read":
        path = request["path"]
        require(selected["arm"] == "files" or path in {"README.md", "Cargo.toml"}, "use project show for source")
        lines = checked_path(project, path).read_text().splitlines(keepends=True)
        start, count = request.get("start", 1), request.get("lines", 80)
        require(type(start) is int and start > 0 and type(count) is int and 1 <= count <= 200, "read needs positive start and 1..200 lines")
        text = "".join(f"{i+1}: {line}" for i, line in enumerate(lines) if start-1 <= i < start-1+count)
        return {"text": text[:20000], "total_lines": len(lines), "omitted_bytes": max(0, len(text.encode())-20000)}
    if kind == "search":
        require(selected["arm"] == "files", "search belongs to ordinary arm")
        path = checked_path(project, request.get("path", "."))
        return command(["rg", "-n", "-F", "--glob", "!Cargo.lock", "--", request["pattern"], str(path)], project)
    if kind == "review":
        return review(session, selected, request)
    if kind == "checkpoint":
        require(state["phase"] == "discover", "checkpoint belongs to discovery phase")
        return checkpoint(session, selected, request)
    if kind == "execute":
        require(state["phase"] == "deliver", "execute only after fresh-process resumption")
        require(any(e["phase"] == "deliver" and e["request"]["tool"] == "review" and "error" not in e["visible"] for e in events(session)),
                "review fresh evidence before execution")
        value = execute_review(session, selected, request["review"])
        save(session / "delivery.json", value)
        if value["kind"] == "fr":
            report = value["report"]
            stages = [{"stage":s["stage"], "status":s["status"]} for s in report["workflow"]["stages"]]
            passed = report["passed"]
        else:
            stages = [{"stage":s["name"], "status":"passed" if s["passed"] else "failed"} for s in value["stages"]]
            passed = value["passed"]
        patch = (project / "artifacts/change.patch").read_bytes()
        return {"kind":value["kind"], "passed":passed, "stages":stages,
                "patch":{"path":"artifacts/change.patch", "bytes":len(patch), "sha256":sha(patch)},
                "retained_detail":"Full checks, source identities and stages are retained in delivery.json."}
    if kind == "finish":
        require(state["phase"] == "deliver" and (session / "delivery.json").exists(), "deliver before finishing")
        require(request.get("source_equivalence_proven") is False, "tests do not prove source equivalence")
        require(isinstance(request.get("summary"), str), "finish needs a summary")
        state["finished"] = True
        save(session / "state.json", state)
        return {"finished": True, "summary": request["summary"], "source_equivalence_proven": False}
    raise ValueError("unknown tool")


def step(session, request):
    with session_lock(session):
        return _step(session, request)


def _step(session, request):
    selected = config(session)
    prior = events(session)
    require(len(prior) < MAX_CALLS and sum(len(encode(e["visible"])) for e in prior) < MAX_BYTES, "trial budget exhausted")
    require(len(encode(request)) <= 65536, "request exceeds 64 KiB")
    phase = load(session / "state.json")["phase"]
    started = time.monotonic()
    before = snapshot(session / "project")
    try:
        result = act(session, selected, request)
    except (ValueError, KeyError, TypeError, OSError, subprocess.SubprocessError, FrRuntimeError) as error:
        result = {"error": str(error)}
    # Oversized reports stay retained, but are never silently presented as complete.
    full = result
    if len(encode(result)) > 65536:
        result = {"error": "report exceeds visible 64 KiB budget; request a smaller page", "report_bytes": len(encode(full))}
    record = {"id": len(prior), "phase": phase, "request": request, "visible": result,
              "full": full if full != result else None, "before": sha(encode(before)),
              "after": sha(encode(snapshot(session / "project"))),
              "seconds": time.monotonic()-started}
    with (session / "events.jsonl").open("ab") as stream:
        stream.write(encode(record) + b"\n")
    return {"event": record["id"], "result": result}
