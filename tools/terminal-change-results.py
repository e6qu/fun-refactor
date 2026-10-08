#!/usr/bin/env python3
"""Grade terminal submissions on GitHub or report their verified outcomes offline."""
import argparse
from pathlib import Path

from agent_eval import source_reviews, terminal_change_results as results
from agent_eval.study import encode, load, require


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("grade", "report"))
    parser.add_argument("inputs", type=Path)
    parser.add_argument("attempts", type=Path)
    parser.add_argument("--grades", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--markdown", type=Path)
    args = parser.parse_args()
    targets = [p.resolve() for p in (args.output, args.markdown) if p is not None]
    require(len(set(targets)) == len(targets), "report destinations must differ")
    require(all(not p.is_relative_to(args.attempts.resolve()) for p in targets), "cannot overwrite attempt evidence")
    protected = [args.inputs / "plan.json", args.inputs / "inputs.json.gz"] + ([args.grades] if args.grades else [])
    require(not set(targets).intersection(p.resolve() for p in protected), "cannot overwrite report inputs")
    frozen, snapshots = load(args.inputs / "plan.json"), source_reviews.read_inputs(args.inputs)
    if args.command == "grade":
        require(args.grades is None and args.markdown is None, "grade writes only a new grade report")
        result = results.grade(frozen, snapshots, args.attempts.resolve())
    else:
        result = results.report(frozen, snapshots, args.attempts.resolve(), load(args.grades) if args.grades else None)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("xb") as destination:
        destination.write(encode(result))
    if args.markdown:
        args.markdown.parent.mkdir(parents=True, exist_ok=True)
        with args.markdown.open("xb") as destination:
            destination.write(results.markdown(result).encode())
    print(encode({"plan_sha256": frozen["sha256"], "output": str(args.output)}).decode())


if __name__ == "__main__":
    main()
