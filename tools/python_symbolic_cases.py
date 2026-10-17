"""Small integer contracts for the pinned Pydash clamp experiment."""
from __future__ import annotations

import ast
import itertools

CASES = ("ordered", "unchecked_bounds", "upper_only")
PARAMETERS = {"ordered": ("x", "lower", "upper"),
              "unchecked_bounds": ("x", "lower", "upper"),
              "upper_only": ("x", "upper")}
DOMAIN = (-100, -10, -1, 0, 1, 10, 100)
EXAMPLES = ((-10, -5, 5), (10, -5, 5), (0, -5, 5),
            (-5, -5, 5), (5, -5, 5), (0, 0, 0), (0, 5, -5))


def require(condition, message):
    if not condition:
        raise ValueError(message)


def admitted(case, arguments):
    return case != "ordered" or arguments["lower"] <= arguments["upper"]


def satisfies(case, arguments, result):
    require(type(result) is int, "clamp returned a non-integer")
    if case == "upper_only":
        # Describe the two-argument requirement without calling min or clamp.
        expected = arguments["x"] if arguments["x"] <= arguments["upper"] else arguments["upper"]
        return result == expected
    return arguments["lower"] <= result <= arguments["upper"]


def inputs(case, method):
    names = PARAMETERS[case]
    if method == "generated":
        values = itertools.product(DOMAIN, repeat=len(names))
    else:
        require(method == "examples", "unknown concrete method")
        values = EXAMPLES if len(names) == 3 else ((-10, 5), (10, 5), (5, 5), (0, 0), (-1, -5))
    return [dict(zip(names, row)) for row in values]


def wrappers(import_statement):
    return import_statement + '''

def ordered(x: int, lower: int, upper: int) -> int:
    """
    pre: lower <= upper
    post: lower <= _ <= upper
    """
    return clamp(x, lower, upper)

def unchecked_bounds(x: int, lower: int, upper: int) -> int:
    """post: lower <= _ <= upper"""
    return clamp(x, lower, upper)

def upper_only(x: int, upper: int) -> int:
    """post: _ == (x if x <= upper else upper)"""
    return clamp(x, upper)
'''


def counterexample(output, case):
    """Read only a call with literal integer arguments; never execute tool text."""
    marker = "when calling " + case + "("
    require(output.count(marker) == 1, "expected one explicit counterexample call")
    calls = [line.split(marker, 1)[1] for line in output.splitlines() if marker in line]
    require(len(calls) == 1, "expected one explicit counterexample call")
    require(len(calls[0]) <= 4096 and ")" in calls[0], "counterexample call exceeds its limit")
    # CrossHair can append '(which returns ...)' after the call. Only flat
    # integer arguments are admitted, so the first closing parenthesis ends it.
    call = ast.parse(case + "(" + calls[0].split(")", 1)[0] + ")", mode="eval").body
    require(isinstance(call, ast.Call) and isinstance(call.func, ast.Name)
            and call.func.id == case, "unexpected counterexample expression")
    names = PARAMETERS[case]
    require(len(call.args) <= len(names), "too many counterexample arguments")
    arguments = dict(zip(names, call.args))
    for keyword in call.keywords:
        require(keyword.arg in names and keyword.arg not in arguments, "ambiguous counterexample arguments")
        arguments[keyword.arg] = keyword.value
    require(set(arguments) == set(names), "counterexample arguments are incomplete")
    result = {name: ast.literal_eval(value) for name, value in arguments.items()}
    require(all(type(value) is int for value in result.values()), "counterexample needs integer literals")
    require(admitted(case, result), "counterexample violates the input requirement")
    return result


def symbolic_observation(observation, case):
    """Keep process stops, missing checks and impossible preconditions unfinished."""
    require(not observation["stop_reason"], "symbolic check stopped")
    require(observation["exit_code"] in (0, 1), "CrossHair failed to run")
    if observation["exit_code"] == 1:
        return counterexample(observation["stdout"], case)
    lines = [line for line in observation["stdout"].splitlines() if line.strip()]
    require(len(lines) == 1 and ": info:" in lines[0], "CrossHair returned no single check result")
    require("Unable to meet precondition" not in lines[0], "no input met the requirement")
    return None
