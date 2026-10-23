"""Independent task controls and patch replay; never launch a model or agent."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess

from .comparison_clients import require
from .investigation_rehearsal import replacement

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("comparison_base", ROOT / "tools/agent-eval.py")
BASE = importlib.util.module_from_spec(spec)
spec.loader.exec_module(BASE)
TASKS = ("unicode-dice", "regex-escape-len")


def bindings():
    names = ("tools/agent-comparison-grade.py", "tools/agent_eval/comparison_grade.py",
             "tools/agent_eval/investigation_rehearsal.py", "tools/agent-eval.py", "tools/agent_eval/oracle.py",
             "tools/agent_eval/regex_workspace.py", "tools/agent_eval/regex_escape_len.py",
             "tests/agent-eval/unknown-target/task.json")
    return {name: hashlib.sha256((ROOT / name).read_bytes()).hexdigest() for name in names}


def environment():
    return {**os.environ, "CARGO_HOME": str(ROOT / "target/cargo-home"), "CARGO_NET_OFFLINE": "true",
            "CARGO_BUILD_JOBS": "1", "CARGO_INCREMENTAL": "0", "CARGO_PROFILE_DEV_DEBUG": "0",
            "CARGO_PROFILE_TEST_DEBUG": "0", "RUST_TEST_THREADS": "1"}


def grade(candidate, task):
    require(task in TASKS, "unknown comparison task")
    checks = []
    for check in BASE.profile(task)["checks"]:
        result = subprocess.run(check["argv"], cwd=candidate / check["cwd"], env=environment(),
                                capture_output=True, timeout=check["timeout_seconds"])
        require(len(result.stdout) + len(result.stderr) <= 1024**2, "grader output exceeds 1 MiB")
        checks.append({"name": check["name"], "argv": check["argv"], "exit_code": result.returncode,
                       "stdout": result.stdout.decode(errors="replace"), "stderr": result.stderr.decode(errors="replace")})
    oracle = BASE.verify(candidate, task)
    return {"task": task, "passed": all(c["exit_code"] == 0 for c in checks) and oracle["passed"],
            "checks": checks, "oracle": oracle, "bindings": bindings(),
            "scope": "Pinned upstream checks and independent bounded behavior; no live-agent or general correctness claim."}


def edit(project, task, variant):
    require(variant in {"incomplete", "reference"}, "declare the control implementation")
    for path, _, old, new in replacement(task, project):
        if variant == "incomplete":
            if task == "unicode-dice":
                new = old.replace("a.len()", "a.chars().count()")
            elif old:
                continue  # APIs alone do not satisfy the allocation requirement.
        source = project / path
        text = source.read_text()
        require(not old or text.count(old) == 1, "reference edit no longer matches exactly once")
        source.write_text(text.replace(old, new, 1) if old else text + new)


def write(path, data):
    path.write_text(json.dumps(data, indent=2, ensure_ascii=False, allow_nan=False) + "\n")


def controls(task, output):
    require(os.environ.get("GITHUB_ACTIONS") == "true", "collect grader controls on GitHub")
    require(task in TASKS, "unknown comparison task")
    output.mkdir(parents=True, exist_ok=False)
    original = output / "original"
    BASE.unpack(original, task)
    results = {}
    for variant in ("unchanged", "incomplete", "reference"):
        project = output / variant
        # Copy immutable source, not a Git database that maintenance can change.
        shutil.copytree(original, project)
        BASE.initialize(project)
        if variant != "unchanged":
            edit(project, task, variant)
        results[variant] = grade(project, task)
        write(output / (variant + ".json"), results[variant])
        require(results[variant]["passed"] == (variant == "reference"), f"unexpected {variant} grading outcome")
        if variant != "reference":
            require(all(c["exit_code"] == 0 for c in results[variant]["checks"]), f"{variant} upstream checks failed")
            expected_stage = 1 if task == "regex-escape-len" and variant == "unchanged" else 2
            require(results[variant]["oracle"].get("stage") == expected_stage,
                    f"{variant} failed for an unexpected reason")
            if variant == "incomplete":
                expected = 'a="aa" b="aaé": 0.5 != 0.6666666666666666' if task == "unicode-dice" else "escaping allocated 2 times"
                require(expected in results[variant]["oracle"]["detail"], "incomplete repair missed its intended failure")
    patch = BASE.git(output / "reference", "diff", "--binary").stdout
    require(patch, "reference produced no patch")
    (output / "reference.patch").write_bytes(patch)
    receiver = output / "receiver"
    BASE.unpack(receiver, task)
    BASE.initialize(receiver)
    before = BASE.snapshot(receiver)
    BASE.git(receiver, "apply", "--check", "-", data=patch)
    BASE.git(receiver, "apply", "-", data=patch)
    results["receiver"] = grade(receiver, task)
    write(output / "receiver.json", results["receiver"])
    require(results["receiver"]["passed"], "reference patch failed independent receiver checks")
    changed = BASE.snapshot(receiver)
    BASE.git(receiver, "apply", "--reverse", "-", data=patch)
    require(BASE.snapshot(receiver) == before, "receiver reversal was not exact")
    BASE.git(receiver, "apply", "-", data=patch)
    require(BASE.snapshot(receiver) == changed, "receiver reapplication was not exact")
    result = {"task": task, "passed": True, "live_model_calls": 0, "controls": results,
              "patch_sha256": hashlib.sha256(patch).hexdigest(), "undo_exact": True, "redo_exact": True,
              "bindings": bindings(), "scope": "Prescribed controls on familiar tasks, not completed agent attempts."}
    write(output / "result.json", result)
    # Build trees and source copies are not the retained evidence artifact.
    for path in (original, receiver, *(output / name for name in ("unchanged", "incomplete", "reference"))):
        shutil.rmtree(path)
    return result
