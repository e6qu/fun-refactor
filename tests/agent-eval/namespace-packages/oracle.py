"""Independent runtime and byte-coordinate oracle for the pinned package task."""
import argparse
import ast
import importlib
import json
from pathlib import Path
import sys


def observe(root, expected):
    sys.dont_write_bytecode = True
    sys.path.insert(0, str(root.resolve()))
    app = importlib.import_module("app")
    api = importlib.import_module("portal.api")
    emitted = []

    def sink(value):
        emitted.append(value)
        return value

    app.source = lambda: 17
    app.sink = api.sink = sink
    results = {}
    for name in ("render", "safe", "preview"):
        emitted.clear()
        function = getattr(app, name, None)
        results[name] = None if function is None else {"returned": function(), "emitted": emitted.copy()}
    assert results["safe"] == {"returned": 0, "emitted": [0]}
    value = 17 if expected == "baseline" else 0
    assert results["render"] == {"returned": value, "emitted": [value]}
    if expected == "feature":
        assert results["preview"] == {"returned": 0, "emitted": []}
    else:
        assert results["preview"] is None
    calls = {}
    for path in sorted(root.rglob("*.py")):
        if path.name == "oracle.py":
            continue
        source = path.read_text()
        lines = source.splitlines(keepends=True)
        offsets = [0]
        for line in lines:
            offsets.append(offsets[-1] + len(line.encode()))
        calls[path.relative_to(root).as_posix()] = sorted(
            [offsets[node.lineno - 1] + node.col_offset,
             offsets[node.end_lineno - 1] + node.end_col_offset]
            for node in ast.walk(ast.parse(source)) if isinstance(node, ast.Call))
    return {"results": results, "call_spans": calls}


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("root", type=Path)
    parser.add_argument("--expect", choices=("baseline", "repair", "feature"), default="baseline")
    args = parser.parse_args()
    print(json.dumps(observe(args.root, args.expect), sort_keys=True))
