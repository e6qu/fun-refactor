#!/usr/bin/env python3
"""Report every native change attempt, including observed costs of failed attempts."""
import argparse
from pathlib import Path
import sys

from agent_eval import native_outcomes
from agent_eval.study import encode, require


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("cohorts", nargs="+", type=Path)
    parser.add_argument("--grades-dir", type=Path, help="Use fresh CI grade reports named COHORT.json")
    parser.add_argument("--json", type=Path, required=True, help="Write the combined report")
    parser.add_argument("--markdown", type=Path, required=True, help="Write a concise comparison")
    parser.add_argument("--check", action="store_true", help="Verify existing reports without rewriting them")
    args = parser.parse_args()
    try:
        require(len({p.resolve() for p in args.cohorts}) == len(args.cohorts), "duplicate cohort")
        require(len({p.name for p in args.cohorts}) == len(args.cohorts), "ambiguous cohort name")
        destinations = [args.json.resolve(), args.markdown.resolve()]
        require(len(set(destinations)) == 2, "report destinations must differ")
        require(all(not target.is_relative_to(root.resolve()) for target in destinations for root in args.cohorts),
                "reports cannot overwrite retained cohort data")
        reports = []
        for root in args.cohorts:
            path = args.grades_dir / (root.name + ".json") if args.grades_dir else None
            require(path is None or path.is_file(), "missing fresh CI grade report")
            reports.append(native_outcomes.cohort(root, path))
        require(len({r["plan_sha256"] for r in reports}) == len(reports), "duplicate frozen plan")
        outputs = [(args.json, encode({"schema": "fr-native-change-comparison-1", "cohorts": reports})),
                   (args.markdown, native_outcomes.markdown(reports).encode())]
        for path, raw in outputs:
            require(not path.is_symlink(), "linked report destination")
            if args.check:
                require(path.read_bytes() == raw, "report differs: " + str(path))
            else:
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(raw)
        print(encode({"cohorts": [{"name": r["cohort"], "outcomes": r["outcomes"]} for r in reports], "checked": args.check}).decode())
    except (OSError, ValueError, KeyError, TypeError) as error:
        parser.exit(2, f"native change report: {error}\n")


if __name__ == "__main__":
    main()
