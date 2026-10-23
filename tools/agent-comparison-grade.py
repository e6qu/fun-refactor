#!/usr/bin/env python3
"""Rehearse comparison grading or grade a candidate on a GitHub worker."""
import argparse
import os
from pathlib import Path

from agent_eval.comparison_clients import require
from agent_eval.comparison_grade import BASE, TASKS, controls, environment, grade, write


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("controls", "grade"))
    parser.add_argument("--task", choices=TASKS, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--candidate", type=Path)
    args = parser.parse_args()
    require(os.environ.get("GITHUB_ACTIONS") == "true", "run grading on GitHub, not a workstation")
    os.environ.update(environment())
    output = args.output.resolve()
    require(not output.exists(), "preserve previous grading attempts")
    if args.candidate is not None:
        require(not output.is_relative_to(args.candidate.resolve()), "keep private grading results outside the candidate")
    try:
        if args.command == "controls":
            require(args.candidate is None, "controls prepare their own pinned candidates")
            controls(args.task, output)
        else:
            require(args.candidate is not None, "grade requires an explicit candidate directory")
            candidate = args.candidate.resolve(strict=True)
            output.mkdir(parents=True, exist_ok=False)
            result = grade(candidate, args.task)
            result["candidate"] = BASE.snapshot(candidate)
            write(output / "result.json", result)
            require(result["passed"], "candidate failed the independent comparison grade")
    except Exception as error:
        output.mkdir(parents=True, exist_ok=True)
        write(output / "failure.json", {"task": args.task, "passed": False, "live_model_calls": 0,
              "error": type(error).__name__ + ": " + str(error)[:4096]})
        raise


if __name__ == "__main__":
    main()
