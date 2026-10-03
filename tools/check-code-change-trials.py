#!/usr/bin/env python3
"""CI-only controls and grading for retained native code-change trials."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import sys

from agent_eval import change_controls, isolated_grade
from agent_eval.study import encode, load, require

ROOT = Path(__file__).resolve().parents[1]


def main():
    require(os.environ.get("GITHUB_ACTIONS") == "true", "run candidate controls on GitHub")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--task-pack", type=Path, default=ROOT / "tests/agent-eval/opencode/changes")
    parser.add_argument("--task", help="Run one declared task's controls")
    parser.add_argument("--controls-only", action="store_true")
    parser.add_argument("--verify-upstream", action="store_true", help="Compare selected source blobs with the pinned GitHub tree")
    parser.add_argument("--output", type=Path, default=ROOT / "target/agent-change-grades")
    args = parser.parse_args()
    require(args.controls_only or args.task_pack.resolve() == (ROOT / "tests/agent-eval/opencode/changes").resolve(),
            "alternate task packs require --controls-only")
    task_root = args.task_pack
    manifest = load(task_root / "manifest.json")
    image = load(task_root / manifest["tasks"][0]["grader"])["image"]
    result, _, _ = isolated_grade.invoke(isolated_grade.DOCKER + ["pull", image], b"", task_root, 90, 131072)
    require(result["exit_code"] == 0 and not result["stop_reason"], "pinned grading image unavailable")
    destination = args.output
    destination.mkdir(parents=True, exist_ok=True)
    tasks = [task for task in manifest["tasks"] if args.task is None or task["id"] == args.task]
    require(bool(tasks), "unknown control task")
    change_controls.verify_sources(task_root, manifest["tasks"])
    results, upstream = [], []
    for task in tasks:
        require(load(task_root / task["grader"])["image"] == image, "task image differs")
        if args.verify_upstream:
            upstream.append(change_controls.verify_upstream(task_root, task))
        results.extend(change_controls.grade_controls(task_root, task))
    (destination / "controls.json").write_bytes(encode({"results": results, "upstream": upstream}))
    change_controls.verify(results)
    print(json.dumps({"controls": {r["id"]: {"private": r["grade"]["outcome"],
                       "public": r.get("public_grade", {}).get("outcome")} for r in results}}))
    if args.controls_only:
        return
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
