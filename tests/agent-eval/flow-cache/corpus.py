"""Scaled scalar workloads with executable outcomes independent of fr's reports."""
from __future__ import annotations

import ast
import json
from pathlib import Path
import types

CASES = ("pipeline-8", "pipeline-16", "fanout-8", "fanout-24", "recursive-8", "recursive-12")
SCENARIOS = ("warm", "unrelated", "helper", "coordinates", "configuration", "rules", "missing")


def sources(case: str) -> dict[str, str]:
    shape, count = case.split("-")
    size = int(count)
    files = {}
    if shape == "pipeline":
        parts = ["def helper_0(value):\n    return value\n"]
        for index in range(1, size):
            parts.append(f"def helper_{index}(value):\n    return helper_{index - 1}(value)\n")
        parts.append(f"def entry():\n    return sink(helper_{size - 1}(source()))\n")
        files["app.py"] = "\n".join(parts)
    elif shape == "fanout":
        modules = size // 4
        for index in range(modules):
            files[f"part{index}.py"] = "\n".join(
                f"def helper_{item}(value):\n    return value\n" for item in range(4))
        imports = "".join(f"import part{index}\n" for index in range(modules))
        calls = " + ".join(f"part{index}.helper_0(source())" for index in range(modules))
        files["app.py"] = imports + f"\ndef entry():\n    return sink({calls})\n"
    else:
        parts = []
        for index in range(size // 2):
            parts.append(f"def helper_{index}(value, count):\n    if count:\n        return recur_{index}(value, count - 1)\n    return value\n"
                         f"def recur_{index}(value, count):\n    return helper_{index}(value, count)\n")
        calls = " + ".join(f"helper_{index}(source(), 3)" for index in range(size // 2))
        parts.append(f"def entry():\n    return sink({calls})\n")
        files["app.py"] = "\n".join(parts)
    for index in range(size * 2):
        files[f"noise{index}.py"] = f"def unrelated_{index}(value):\n    return value + {index}\n"
    files["rules.json"] = json.dumps({"version": "cache-corpus-1", "sources": ["source"], "sinks": ["sink"]})
    return files


def mutate(files: dict[str, str], case: str, scenario: str) -> dict[str, str]:
    changed = dict(files)
    helper = "part0.py" if case.startswith("fanout") else "app.py"
    if scenario == "unrelated":
        changed["noise0.py"] += "\ndef extra():\n    return 17\n"
    elif scenario == "helper":
        changed[helper] = changed[helper].replace("return value", "return 0", 1)
    elif scenario == "coordinates":
        changed[helper] = "# Coordinates move without changing behavior.\n" + changed[helper]
    elif scenario == "configuration":
        changed["pyproject.toml"] = '[project]\nname = "cache-measurement"\nversion = "1.0"\n'
    elif scenario == "rules":
        changed["rules.json"] = changed["rules.json"].replace("cache-corpus-1", "cache-corpus-2")
    elif scenario == "missing":
        changed[helper] = changed[helper].replace("def helper_0(", "def absent_0(", 1)
    elif scenario != "warm":
        raise ValueError(scenario)
    return changed


def install(root: Path, files: dict[str, str]) -> None:
    root.mkdir(exist_ok=True)
    for path in root.iterdir():
        if path.is_file():
            path.unlink()
    for name, contents in files.items():
        (root / name).write_text(contents, encoding="utf-8")


def oracle(files: dict[str, str]) -> dict:
    modules = {}
    sinks = []
    source_value = 37

    def imported(name, *args, **kwargs):
        return modules[name]

    def sink(value):
        sinks.append(value)
        return value

    for name, contents in sorted(files.items(), key=lambda pair: pair[0] == "app.py"):
        if not name.endswith(".py"):
            continue
        module = types.ModuleType(name[:-3])
        module.__dict__.update(source=lambda: source_value, sink=sink,
                               __builtins__={"__import__": imported})
        exec(compile(contents, name, "exec"), module.__dict__)
        modules[name[:-3]] = module
    error = None
    try:
        modules["app"].entry()
    except (NameError, AttributeError) as failure:
        error = type(failure).__name__
    calls = {}
    for name, contents in files.items():
        if name.endswith(".py"):
            lines = contents.encode().splitlines(keepends=True)
            calls[name] = sorted([sum(map(len, lines[:node.lineno - 1])) + node.col_offset,
                                  sum(map(len, lines[:node.end_lineno - 1])) + node.end_col_offset]
                                 for node in ast.walk(ast.parse(contents)) if isinstance(node, ast.Call))
    return {"sinks": sinks, "error": error, "call_spans": calls}
