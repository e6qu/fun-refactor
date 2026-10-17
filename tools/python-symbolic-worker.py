#!/usr/bin/env python3
"""Execute one measured operation; each invocation starts with fresh imports."""
import argparse
import importlib
import json
from pathlib import Path
import sys

from python_symbolic_cases import admitted, inputs, satisfies


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("method", choices=("examples", "generated", "replay"))
    parser.add_argument("root", type=Path)
    parser.add_argument("case")
    parser.add_argument("--arguments")
    args = parser.parse_args()
    sys.path.insert(0, str(args.root))
    function = getattr(importlib.import_module("checks"), args.case)
    candidates = ([json.loads(args.arguments)] if args.method == "replay"
                  else inputs(args.case, args.method))
    checked, excluded, failures = 0, 0, []
    for arguments in candidates:
        if not admitted(args.case, arguments):
            excluded += 1
            continue
        result = function(**arguments)
        checked += 1
        if not satisfies(args.case, arguments, result):
            failures.append({"arguments": arguments, "returned": result})
    print(json.dumps({"checked": checked, "excluded": excluded, "failures": failures}))


if __name__ == "__main__":
    main()
