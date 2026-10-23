#!/usr/bin/env python3
"""Rehearse one complete native-client task and independently grade its patch."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil

from agent_eval.comparison_clients import HOSTS, SETUPS, prompts, require
from agent_eval.comparison_grade import BASE, TASKS, environment, grade, replacement, write
from agent_eval.comparison_plan import bindings

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("native_comparison_controller", ROOT / "tools/agent-comparison-native.py")
native = importlib.util.module_from_spec(spec)
spec.loader.exec_module(native)


def edits(task, project):
    return [{"path": path, "name": name, "old": old, "new": new} for path, name, old, new in replacement(task, project)]


def scenario(task, project):
    original = edits(task, project)
    first = original[0]
    old_name, new_name = ("intersection_size", "shared_bigrams") if task == "unicode-dice" else ("quoted", "escaped")
    require(old_name in first["old"] and new_name not in first["old"], "intervention variable differs from the pinned source")
    intervention = {"path": first["path"], "old": first["old"], "new": first["old"].replace(old_name, new_name)}
    target = project / first["path"]
    before = target.read_bytes()
    text = before.decode()
    require(text.count(first["old"]) == 1, "the selected function must occur exactly once")
    start = text[:text.index(first["old"])].count("\n") + 1
    end = start + first["old"].count("\n")
    try:
        target.write_text(before.decode().replace(intervention["old"], intervention["new"], 1))
        current = edits(task, project)
    finally:
        target.write_bytes(before)
    requirement = json.loads((ROOT / "tests/agent-eval/unknown-target/task.json").read_text())["tasks"][task]["requirement"]
    return {"task": task, "symbol": "sorensen_dice" if task == "unicode-dice" else "escape",
            "codegraph_query": f"{first['path']}:{start}-{end}", "definition_path": first["path"],
            "query_variable": {"discover": old_name + " =", "deliver": new_name + " ="},
            "paths": sorted(BASE.edit_paths(task)), "prompts": prompts(requirement), "original_edits": original,
            "resumed_edits": current, "intervention": intervention, "preserved_name": new_name,
            "diagnosis": "Count Unicode scalars consistently for both inputs." if task == "unicode-dice" else
                         "Add the public length APIs without allocating and reserve the escaped output capacity."}


def receiver(task, recipe, patch, output):
    project = output / "receiver"
    BASE.unpack(project, task)
    BASE.initialize(project)
    edit = recipe["intervention"]
    source = project / edit["path"]
    text = source.read_text()
    require(text.count(edit["old"]) == 1, "receiver intervention differs")
    source.write_text(text.replace(edit["old"], edit["new"], 1))
    BASE.git(project, "add", edit["path"])
    BASE.git(project, "commit", "-qm", "Rename a local variable while the task is paused")
    before = BASE.snapshot(project)
    BASE.git(project, "apply", "--check", "-", data=patch)
    BASE.git(project, "apply", "-", data=patch)
    after = BASE.snapshot(project)
    require(sorted(path for path in before if before[path] != after[path]) == recipe["paths"], "receiver patch changed unexpected paths")
    require(recipe["preserved_name"] in source.read_text(), "receiver patch lost the intervening edit")
    require(recipe["resumed_edits"][0]["new"] in source.read_text(), "receiver function lost the prescribed current-source repair")
    result = grade(project, task)
    write(output / "grade.json", result)
    require(result["passed"], "delivered repair failed independent upstream or behavior checks")
    BASE.git(project, "apply", "--reverse", "-", data=patch)
    require(BASE.snapshot(project) == before, "receiver undo was not exact")
    BASE.git(project, "apply", "-", data=patch)
    require(BASE.snapshot(project) == after, "receiver redo was not exact")
    shutil.rmtree(project)
    return {"passed": True, "undo_exact": True, "redo_exact": True, "intervening_edit_preserved": True,
            "patch_sha256": hashlib.sha256(patch).hexdigest(), "grade": result}


def run(task, host, setup, output):
    require(os.environ.get("GITHUB_ACTIONS") == "true", "run handoff rehearsals on GitHub")
    output.mkdir(parents=True, exist_ok=False)
    os.environ.update(environment())
    result = {"task": task, "host": host, "setup": setup, "passed": False, "live_model_calls": 0, "bindings": bindings(ROOT),
              "scope": "Prescribed native task rehearsal on familiar projects. Not an autonomous attempt, cost result or complete live resource admission."}
    try:
        original = output / "original"
        BASE.unpack(original, task)
        BASE.initialize(original)
        recipe = scenario(task, original)
        write(output / "scenario.json", recipe)
        entry = json.loads((ROOT / "tests/agent-eval/agent-first-use/hosts.json").read_text())[host]
        binary = native.download(entry, output / "client-package") / entry["executable"]
        require(binary.is_file(), "pinned client executable is missing")
        binary.chmod(0o755)
        image = native.build(host, setup, output, binary, original, recipe)
        control = native.boundary.rehearsal(image, setup, output / "native", False, host, "handoff")
        require(control["passed"] and control["probe"]["passed"], "native handoff did not complete")
        delivered = control["probe"]
        patch = delivered["patch"].encode()
        (output / "change.patch").write_bytes(patch)
        (output / "HANDOFF.md").write_text(delivered["handoff"])
        result["native"] = {"passed": True, "image": image, "result": "native/result.json",
                            "intervention": delivered["intervention"], "checks": delivered["checks"]}
        result["receiver"] = receiver(task, recipe, patch, output)
        result["passed"] = True
        shutil.rmtree(original)
    except Exception as error:
        result["error"] = type(error).__name__ + ": " + str(error)[:4096]
        raise
    finally:
        write(output / "result.json", result)
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--task", choices=TASKS, required=True)
    parser.add_argument("--host", choices=HOSTS, required=True)
    parser.add_argument("--setup", choices=SETUPS, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    run(args.task, args.host, args.setup, args.output.resolve())
