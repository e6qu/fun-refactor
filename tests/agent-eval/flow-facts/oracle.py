"""Independent Python execution and UTF-8 AST coordinates for flow facts."""
import ast
import json
from pathlib import Path
import runpy


def oracle():
    path = Path(__file__).with_name("subject.py")
    source = path.read_bytes()
    namespace = runpy.run_path(str(path))
    seen = []
    namespace["relay"].__globals__.update(café=lambda: 17, sink=seen.append, clean=lambda value: 0)
    outcomes = []
    for count in range(6):
        seen.clear()
        namespace["checkout"](count)
        assert seen == [17, 17]
        outcomes.append(list(seen))
    for name in ["overwritten", "sanitized"]:
        seen.clear()
        namespace[name]()
        assert seen == [0]
    seen.clear()
    namespace["normalization_boundary"]()
    assert seen == [1]
    lines = source.splitlines(keepends=True)
    calls = []
    for node in ast.walk(ast.parse(source)):
        if isinstance(node, ast.Call):
            calls.append([sum(map(len, lines[:node.lineno - 1])) + node.col_offset,
                          sum(map(len, lines[:node.end_lineno - 1])) + node.end_col_offset])
    return {"runtime_cases": 9, "checkout": outcomes, "call_spans": sorted(calls)}


if __name__ == "__main__":
    print(json.dumps(oracle(), sort_keys=True))
