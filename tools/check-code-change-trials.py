#!/usr/bin/env python3
"""CI-only controls and grading for retained native code-change trials."""
import base64
import hashlib
import json
import os
from pathlib import Path
import tempfile
import sys

from agent_eval import isolated_grade, native_changes, opencode_changes, rehearsal_evidence
from agent_eval.study import digest, encode, load, require
from agent_eval.workspace_bundle import unpack

ROOT = Path(__file__).resolve().parents[1]


def main():
    require(os.environ.get("GITHUB_ACTIONS") == "true", "run candidate controls on GitHub")
    task_root = ROOT / "tests/agent-eval/opencode/changes"
    manifest = load(task_root / "manifest.json")
    image = load(task_root / manifest["tasks"][0]["grader"])["image"]
    result, _, _ = isolated_grade.invoke(isolated_grade.DOCKER + ["pull", image], b"", task_root, 90, 131072)
    require(result["exit_code"] == 0 and not result["stop_reason"], "pinned grading image unavailable")
    destination = ROOT / "target/agent-change-grades"
    destination.mkdir(parents=True, exist_ok=True)
    results = []
    for task in manifest["tasks"]:
        grader_path = task_root / task["grader"]
        require(load(grader_path)["image"] == image, "task image differs")
        files = rehearsal_evidence.source_bundle(task, task_root)
        sha = hashlib.sha256(grader_path.read_bytes()).hexdigest()
        for control in [{"id": "unchanged", "expected": "failed"}, *load(task_root / "controls.json")[task["id"]]]:
            machine = native_changes.Machine(files, "files")
            if "old" in control:
                arguments = {k: control[k] for k in ("path", "old", "new")}
                arguments["sha256"] = hashlib.sha256(base64.b64decode(files[control["path"]]["data"])).hexdigest()
                require("error" not in machine.call({"name": "replace_source", "arguments": arguments}), "control edit failed")
            with tempfile.TemporaryDirectory() as tmp:
                candidate = Path(tmp) / "candidate"
                unpack(machine.files, candidate)
                result = isolated_grade.grade(candidate, grader_path, sha)
            results.append({"id": task["id"] + "/" + control["id"], "repository_revision": task["revision"],
                            "expected": control["expected"], "submission_sha256": digest(machine.files), "grade": result})
    (destination / "controls.json").write_bytes(encode({"results": results}))
    require(all(row["grade"]["outcome"] == row["expected"] for row in results), "baseline/reference/mutation control failed")
    print(json.dumps({"controls": {r["id"]: r["grade"]["outcome"] for r in results}}))
    for cohort in sorted((ROOT / "tests/agent-eval/opencode/results").glob("*-code-changes")):
        frozen = load(cohort / "plan.json")
        # Every frozen grader must use the image pulled above; refuse silent
        # image substitution or an unplanned download for retained cohorts.
        require(all(load_spec["image"] == image for load_spec in
                    (json.loads(raw) for raw in frozen["plan"]["graders"].values())), "cohort image differs")
        runner = cohort / "runner"
        for name, sha in frozen["plan"]["implementation"].items():
            path = runner / name if name == "native-changes.py" else runner / "agent_eval" / name
            require(Path(name).name == name and hashlib.sha256(path.read_bytes()).hexdigest() == sha,
                    "frozen runner differs")
        process, raw, _ = isolated_grade.invoke([sys.executable, "-B", str(runner / "native-changes.py"),
            "grade", str(cohort / "plan.json"), str(cohort / "attempts")], b"", destination, 120, 4 * 1024**2)
        require(process["exit_code"] == 0 and not process["stop_reason"], "frozen cohort grading failed")
        report = json.loads(raw)
        retained = cohort / "github-grades.json"
        if retained.exists():
            def verdicts(value):
                return [(r["cell"], r["outcome"], r.get("submission_sha256"),
                         [(c["id"], c["passed"]) for c in r.get("grade", {}).get("cases", [])])
                        for r in value["outcomes"]]
            require(verdicts(report) == verdicts(load(retained)), "retained behavior outcomes differ from fresh grading")
        (destination / (cohort.name + ".json")).write_bytes(encode(report))
        print(json.dumps({"cohort": cohort.name, "passed": report["passed"],
                          "outcomes": [r["outcome"] for r in report["outcomes"]]}))
        # Agent failure is evidence, not a CI failure. Invalid evidence or
        # broken grader controls raise above and do fail this job.


if __name__ == "__main__":
    main()
