"""Replay declared task-control edits and check public and private outcomes."""
import base64
import hashlib
from pathlib import Path
import re
import tempfile

from . import isolated_grade, native_changes, native_checks, rehearsal_evidence
from .study import digest, encode, load, named, require
from .workspace_bundle import unpack


def verify_sources(root, tasks):
    path = root / "sources.json"
    if not path.exists():
        return
    records = load(path)
    require(set(records) == {task["id"] for task in tasks}, "source task set differs")
    for task in tasks:
        record = records[task["id"]]
        require(all(record[key] == task[key] for key in ("repository", "revision")), "source origin differs")
        files = rehearsal_evidence.source_bundle(task, root)
        observed = {p: hashlib.sha256(base64.b64decode(row["data"])).hexdigest() for p, row in files.items()}
        require(observed == record["files"], "source inventory differs")
        selection = record["selection"]
        require(all(p.startswith(selection["prefix"]) or p in selection["root_files"] for p in files),
                "source selection differs")


def compare_upstream(files, record, tree):
    require(tree.get("truncated") is False, "upstream tree is incomplete")
    selection = record["selection"]
    selected = [row for row in tree["tree"] if row["type"] == "blob"
                and (row["path"].startswith(selection["prefix"]) or row["path"] in selection["root_files"])]
    require(len(selected) == len(files) and {row["path"] for row in selected} == set(files), "upstream selection differs")
    for row in selected:
        raw = base64.b64decode(files[row["path"]]["data"])
        sha = hashlib.sha1(b"blob " + str(len(raw)).encode() + b"\0" + raw).hexdigest()
        require(row["mode"] == ("100755" if files[row["path"]]["executable"] else "100644")
                and row["sha"] == sha, "upstream source differs: " + row["path"])


def verify_upstream(root, task):
    match = re.fullmatch(r"https://github.com/([\w.-]+/[\w.-]+)", task["repository"])
    require(match is not None and re.fullmatch(r"[0-9a-f]{40}", task["revision"]), "unsupported upstream identity")
    endpoint = "repos/" + match[1] + "/git/trees/" + task["revision"] + "?recursive=1"
    state, raw, _ = isolated_grade.invoke(["gh", "api", endpoint], b"", root, 30, 2 * 1024**2)
    require(state["exit_code"] == 0 and not state["stop_reason"], "upstream tree unavailable")
    tree = isolated_grade.decode(raw)
    compare_upstream(rehearsal_evidence.source_bundle(task, root), load(root / "sources.json")[task["id"]], tree)
    return {"repository": task["repository"], "revision": task["revision"], "tree_sha": tree["sha"]}


def apply(files, control):
    machine = native_changes.Machine(files, "files")
    edits = control.get("edits", [{k: control[k] for k in ("path", "old", "new")}] if "old" in control else [])
    require(isinstance(edits, list) and len(edits) <= 24, "control edit budget exceeded")
    require(not ("edits" in control and "old" in control), "ambiguous control edits")
    for edit in edits:
        require(set(edit) == {"path", "old", "new"} and edit["path"] in machine.files, "invalid control edit")
        sha = hashlib.sha256(base64.b64decode(machine.files[edit["path"]]["data"])).hexdigest()
        result = machine.call({"name": "replace_source", "arguments": {**edit, "sha256": sha}})
        require("error" not in result, "control edit failed: " + str(result))
    return machine.files


def definitions(root, task):
    files = rehearsal_evidence.source_bundle(task, root)
    controls = load(root / "controls.json")[task["id"]]
    names = named(controls, "control")
    require("unchanged" not in names and "reference" in names, "control set needs a reference and reserved baseline")
    require(names["reference"]["expected"] == "passed", "reference must pass")
    require(all(row["expected"] == ("passed" if row["id"] == "reference" else "failed") for row in controls),
            "mutation controls must fail")
    profile = load(root / task["grader"])
    isolated_grade.validate(profile)
    baseline = {"id": "unchanged", "expected": "failed"}
    public_path = root / "public-checks.json"
    public = load(public_path)[task["id"]] if public_path.exists() else None
    if public is not None:
        native_checks.validate(public)
        require(public["image"] == profile["image"], "public and private images differ")
        baseline["expected_public"] = "failed"
        require(all(row["expected_public"] in {"passed", "failed"} for row in controls), "missing public outcome")
    rows = [baseline, *controls]
    expectations = root / "control-failures.json"
    if expectations.exists():
        expected = load(expectations)[task["id"]]
        require(set(expected) == {row["id"] for row in rows}, "failure control set differs")
        cases = {case["id"] for case in profile["cases"]}
        for row in rows:
            failures = expected[row["id"]]
            require(isinstance(failures, list) and len(set(failures)) == len(failures) and set(failures) <= cases,
                    "unknown or repeated expected failed case")
            require(bool(failures) == (row["expected"] == "failed"), "failure expectations contradict outcome")
            row["failed_cases"] = sorted(failures)
    return files, rows, public


def grade_controls(root, task, *, grader=isolated_grade.grade):
    files, controls, public = definitions(root, task)
    grader_path = root / task["grader"]
    sha = hashlib.sha256(grader_path.read_bytes()).hexdigest()
    results = []
    for control in controls:
        candidate_files = apply(files, control)
        with tempfile.TemporaryDirectory(prefix="fr-control-") as temporary:
            candidate = Path(temporary) / "candidate"
            unpack(candidate_files, candidate)
            grade = grader(candidate, grader_path, sha)
            row = {"id": task["id"] + "/" + control["id"], "repository_revision": task["revision"],
                   "expected": control["expected"], "submission_sha256": digest(candidate_files), "grade": grade}
            if "failed_cases" in control:
                row["expected_failed_cases"] = control["failed_cases"]
            if public is not None:
                path = Path(temporary) / "public.json"
                path.write_bytes(encode(public))
                row.update(expected_public=control["expected_public"],
                           public_grade=grader(candidate, path, hashlib.sha256(path.read_bytes()).hexdigest()))
            results.append(row)
    return results


def verify(results):
    require(bool(results), "missing control results")
    for row in results:
        require(row["grade"]["outcome"] == row["expected"], "private control outcome differs: " + row["id"])
        if "expected_failed_cases" in row:
            observed = sorted(case["id"] for case in row["grade"]["cases"] if not case["passed"])
            require(observed == row["expected_failed_cases"], "failed case set differs: " + row["id"] + " " + str(observed))
        if "public_grade" in row:
            require(row["public_grade"]["outcome"] == row["expected_public"], "public control outcome differs: " + row["id"])
