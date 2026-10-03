#!/usr/bin/env python3
"""Audit frozen read-only OpenCode attempts and report observed work, including failures."""
import argparse
from pathlib import Path

from agent_eval import native_read_outcomes
from agent_eval.study import encode, load, require


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("cohorts", type=Path, nargs="+")
    parser.add_argument("--json", type=Path, required=True)
    parser.add_argument("--markdown", type=Path, required=True)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    reports = [native_read_outcomes.cohort(root) for root in args.cohorts]
    result = {"schema": "fr-native-read-comparison-1", "cohorts": reports}
    prose = native_read_outcomes.markdown(reports)
    if args.check:
        require(load(args.json) == result and args.markdown.read_text() == prose, "native read report differs")
    else:
        args.json.write_bytes(encode(result))
        args.markdown.write_text(prose)
    print(encode({report["cohort"]: report["outcomes"] for report in reports}).decode())


if __name__ == "__main__":
    main()
