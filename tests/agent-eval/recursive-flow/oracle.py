"""Independent finite runtime and syntax checks for recursive flow fixtures."""
import ast
import json
from pathlib import Path
import runpy


def oracle():
    path = Path(__file__).with_name("subject.py")
    namespace = runpy.run_path(str(path))
    seen = []
    namespace["relay"].__globals__.update(source=lambda: 731, sink=seen.append, clean=lambda value: 0)
    results = {}
    for name in ["positive", "negative", "mutual", "effects", "sanitized", "separate"]:
        outcomes = []
        for count in range(7):
            seen.clear()
            namespace[name](count)
            outcomes.append(731 in seen)
        assert all(outcomes) == (name in {"positive", "mutual", "effects"}), (name, outcomes)
        results[name] = outcomes
    seen.clear()
    try:
        namespace["exceptional"](ValueError("sentinel"))
    except ValueError:
        pass
    else:
        raise AssertionError("expected explicit exception")
    assert not seen
    try:
        namespace["unreachable"]()
    except RecursionError:
        pass
    else:
        raise AssertionError("expected Python recursion limit")
    assert not seen
    source = path.read_bytes()
    lines = source.splitlines(keepends=True)
    calls = []
    for node in ast.walk(ast.parse(source)):
        if isinstance(node, ast.Call):
            start = sum(map(len, lines[:node.lineno - 1])) + node.col_offset
            end = sum(map(len, lines[:node.end_lineno - 1])) + node.end_col_offset
            calls.append([start, end])
    return {"runtime": results, "sink_after_raise": False, "sink_after_recursion_limit": False,
            "call_spans": sorted(calls), "runtime_cases": 44}


if __name__ == "__main__":
    print(json.dumps(oracle(), sort_keys=True))
