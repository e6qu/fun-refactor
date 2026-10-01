"""Private equality and mutation checks; independent of the agent's implementation."""
import copy
import ast
import json
from pathlib import Path
import sys

passed, checks = True, 0
try:
    tree = ast.parse((Path(sys.argv[1]) / "sequences.py").read_text())
    allowed = (ast.Module, ast.FunctionDef, ast.arguments, ast.arg, ast.Return, ast.Expr,
               ast.Constant, ast.Name, ast.Load, ast.Store, ast.Call, ast.Attribute, ast.Assign,
               ast.List, ast.Tuple, ast.For, ast.If, ast.Compare, ast.In, ast.NotIn, ast.Eq,
               ast.NotEq, ast.UnaryOp, ast.Not, ast.ListComp, ast.GeneratorExp, ast.comprehension,
               ast.Raise, ast.IfExp, ast.BoolOp, ast.And, ast.Or)
    assert all(isinstance(node, allowed) for node in ast.walk(tree))
    assert all(not isinstance(node, ast.Name) or not node.id.startswith("_") for node in ast.walk(tree))
    assert all(not isinstance(node, ast.Attribute) or node.attr == "append" for node in ast.walk(tree))
    assert all(not node.decorator_list for node in ast.walk(tree) if isinstance(node, ast.FunctionDef))
    namespace = {"__builtins__": {"list": list, "reversed": reversed, "any": any, "all": all,
                                   "NotImplementedError": NotImplementedError}}
    exec(compile(tree, "submission", "exec"), namespace)
    cases = [([], []), ([1, 2, 1, 3, 2], [1, 2, 3]),
             ([[1], [2], [1]], [[1], [2]]), ([{"a": 1}, {"a": 1}, {"a": 2}], [{"a": 1}, {"a": 2}]),
             ([0, False, 1, True], [0, 1]), ([None, None, "a", "a"], [None, "a"])]
    for values, expected in cases:
        before = copy.deepcopy(values)
        for argument in (values, (value for value in values)):
            result = namespace["stable_unique"](argument)
            passed &= isinstance(result, list) and result == expected and values == before
            checks += 1
        passed &= namespace["reversed_copy"](values) == list(reversed(values)) and values == before
        checks += 1
except Exception:
    passed = False
print(json.dumps({"passed": bool(passed), "checks": checks, "scope": "Finite equality, generator and mutation checks; no proof."}))
