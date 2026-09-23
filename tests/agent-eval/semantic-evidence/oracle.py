"""Independent AST coordinates and runtime outcomes for the pinned source."""
import ast
import json
from pathlib import Path

source = Path(__file__).with_name("subject.py").read_text()
module = ast.parse(source)
lines = source.encode().splitlines(keepends=True)
starts = [sum(map(len, lines[:i])) for i in range(len(lines))]
calls = sorted((starts[n.lineno - 1] + n.col_offset,
                starts[n.end_lineno - 1] + n.end_col_offset)
               for n in ast.walk(module) if isinstance(n, ast.Call))
assert len(calls) == 2 and calls[0] != calls[1]
assert all(source.encode()[start:end].decode() == "café(amount)" for start, end in calls)
scope = {}
exec(compile(module, "subject.py", "exec"), scope)
assert [scope["total"](x) for x in (0, 1, -2)] == [1, 5, -1]
print(json.dumps({"calls": calls, "outcomes": [1, 5, -1]}))
