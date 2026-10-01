"""Private finite-set oracle; no expected patch is given to the agent."""
import ast
import json
from pathlib import Path
import sys

passed, checks = True, 0
scope = "Finite integer interval behavior and unchanged span; no proof."
try:
    tree = ast.parse((Path(sys.argv[1]) / "intervals.py").read_text())
    # This local fixture admits only simple pure expressions and declarations.
    # Never import or execute arbitrary module-level statements from a submission.
    allowed = (ast.Module, ast.FunctionDef, ast.arguments, ast.arg, ast.Return, ast.Expr,
               ast.Constant, ast.Name, ast.Load, ast.Subscript, ast.Call, ast.Compare,
               ast.Lt, ast.LtE, ast.Gt, ast.GtE, ast.Eq, ast.NotEq, ast.BinOp, ast.Sub,
               ast.Add, ast.BoolOp, ast.And, ast.Or, ast.If, ast.IfExp, ast.UnaryOp, ast.Not)
    assert all(isinstance(node, allowed) for node in ast.walk(tree))
    assert all(not isinstance(node, ast.Name) or not node.id.startswith("_") for node in ast.walk(tree))
    assert all(not node.decorator_list for node in ast.walk(tree) if isinstance(node, ast.FunctionDef))
    namespace = {"__builtins__": {"max": max, "min": min, "bool": bool}}
    exec(compile(tree, "submission", "exec"), namespace)
    intervals = [(start, end) for start in range(-3, 4) for end in range(start, 4)]
    for left in intervals:
        for right in intervals:
            expected = bool(set(range(*left)) & set(range(*right)))
            passed &= namespace["overlaps"](left, right) is expected
            checks += 1
    for interval in intervals:
        passed &= namespace["span"](interval) == len(range(*interval))
        checks += 1
except AssertionError:
    passed, scope = False, "Unsupported submission syntax; behavior was not graded."
except Exception:
    passed = False
    scope = "Submission execution failed during finite behavior checks; no proof."
print(json.dumps({"passed": bool(passed), "checks": checks, "scope": scope}))
