"""Independent AST coordinates and finite behavior for the pinned declaration."""
import ast
import json
from pathlib import Path


def inspect(source):
    lines = source.encode().splitlines(keepends=True)
    rows = []
    for node in ast.walk(ast.parse(source)):
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
            start = sum(map(len, lines[:node.lineno - 1])) + node.col_offset
            end = sum(map(len, lines[:node.end_lineno - 1])) + node.end_col_offset
            rows.append({"name": node.name, "span": {"start": start, "end": end}})
    return rows


if __name__ == "__main__":
    source = Path(__file__).with_name("subject.py").read_text()
    namespace = {}
    exec(compile(source, "subject.py", "exec"), namespace)
    print(json.dumps({"declarations": inspect(source), "outcomes": [namespace["café"](n) for n in (-2, 0, 7)]}, ensure_ascii=False))
