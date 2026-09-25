"""Independent runtime and UTF-8 coordinate oracle for the pinned import corpus."""
import ast
import importlib
import json
from pathlib import Path
import sys


def observe(root, context):
    sys.path.insert(0, str(root))
    names = ("app", "relay", "leaf", "erase", "effects")
    for name in names:
        sys.modules.pop(name, None)
    app = importlib.import_module("app")
    reached = []
    for name in names:
        module = sys.modules[name]
        module.source = lambda: "tainted"
        module.sink = lambda value: reached.append(value) or value
        module.clean = lambda value: 0 if context == "html" else value
    outcomes = {}
    for name in ("positive", "negative", "effects", "contextual", "separate"):
        reached.clear()
        getattr(app, name)()
        outcomes[name] = "tainted" in reached
    sys.path.pop(0)
    coordinates = {}
    for name in names:
        path = root / f"{name}.py"
        source = path.read_text()
        lines = source.splitlines(keepends=True)
        offsets = [0]
        for line in lines:
            offsets.append(offsets[-1] + len(line.encode()))
        coordinates[path.name] = sorted([
            [offsets[node.lineno - 1] + node.col_offset,
             offsets[node.end_lineno - 1] + node.end_col_offset]
            for node in ast.walk(ast.parse(source)) if isinstance(node, ast.Call)
        ])
    return {"outcomes": outcomes, "call_spans": coordinates}


if __name__ == "__main__":
    root = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).parent
    print(json.dumps({context: observe(root, context) for context in ("html", "sql")}, sort_keys=True))
