"""Check remaining policy combinations in isolation on GitHub, never locally."""
import argparse
import ast
import base64
import hashlib
import json
import os
from pathlib import Path
import sys
import tempfile

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[4]
sys.path.insert(0, str(REPO / "tools"))
from agent_eval import isolated_grade, source_reviews
from agent_eval.study import digest, encode, require
from agent_eval.workspace_bundle import unpack

PROGRAM = """import itertools, json, sys
sys.path.insert(0, '/workspace/src')
from packaging.specifiers import SpecifierSet
case = json.load(sys.stdin)
spec, inferred = {'ordinary': ('>=1,<2', False), 'inferred': ('>=1.5a1,<2', True), 'empty': ('', False)}[case]
inputs = [['1.6a1', '1.6', '1.6a1'], ['1.6a1'], []]
outputs = {True: inputs, False: [['1.6'], [], []], None: [['1.6'], ['1.6a1'], []]}
count = 0
for constructor, setter, call in itertools.product((None, False, True), ('unchanged', None, False, True), (None, False, True)):
    selector = SpecifierSet(spec, prereleases=constructor)
    if setter != 'unchanged':
        selector.prereleases = setter
    policy = call if call is not None else constructor if setter == 'unchanged' else setter
    if policy is None and inferred:
        policy = True
    for values, expected in zip(inputs, outputs[policy]):
        actual = list(selector.filter(iter(values), prereleases=call))
        assert actual == expected, (case, constructor, setter, call, values, actual, expected)
        count += 1
assert count == 108
print('ok')
"""


def inputs():
    frozen = json.loads((HERE / "plan.json").read_bytes())
    snapshots = source_reviews.read_inputs(HERE)
    from agent_eval import terminal_reviews
    terminal_reviews.checked(frozen, snapshots)
    files = snapshots["packaging-explicit-policies"]
    baseline = {p.removeprefix("baseline/"): row for p, row in files.items() if p.startswith("baseline/")}
    candidate = {p: files[p] for p in baseline}
    changed = [p for p in baseline if baseline[p] != candidate[p]]
    require(changed == ["src/packaging/specifiers.py"], "unexpected changed files")
    path = changed[0]
    outside = []
    for tree in (baseline, candidate):
        raw = base64.b64decode(tree[path]["data"], validate=True)
        owner = next(n for n in ast.parse(raw).body if isinstance(n, ast.ClassDef) and n.name == "SpecifierSet")
        method = next(n for n in owner.body if isinstance(n, ast.FunctionDef) and n.name == "filter")
        lines = raw.splitlines(keepends=True)
        outside.append(b"".join(lines[:method.lineno - 1] + lines[method.end_lineno:]))
    require(outside[0] == outside[1], "code outside the selected filter changed")
    return frozen, candidate, {"unchanged_files": len(baseline) - 1,
        "changed_file": path, "outside_filter_sha256": hashlib.sha256(outside[0]).hexdigest()}


def check(output):
    require(os.environ.get("GITHUB_ACTIONS") == "true", "policy execution belongs on GitHub")
    frozen, candidate, unchanged = inputs()
    profile = json.loads((REPO / "tests/agent-eval/opencode/candidates/packaging-prerelease-grader.json").read_bytes())
    profile.update(command=["python3", "-I", "-c", PROGRAM],
        cases=[{"id": name, "stdin": json.dumps(name), "stdout": "ok\n", "exit_code": 0}
               for name in ("ordinary", "inferred", "empty")])
    isolated_grade.validate(profile)
    output.mkdir(parents=True, exist_ok=False)
    grader = output / "grader.json"
    grader.write_bytes(encode(profile))
    process, _, _ = isolated_grade.invoke(isolated_grade.DOCKER + ["pull", profile["image"]], b"", output, 90, 131072)
    require(process["exit_code"] == 0 and not process["stop_reason"], "pinned grading image unavailable")
    with tempfile.TemporaryDirectory(prefix="fr-policy-review-") as temporary:
        folder = Path(temporary) / "candidate"
        unpack(candidate, folder)
        grade = isolated_grade.grade(folder, grader, source_reviews.identity(grader))
    result = {"schema": "fr-scoped-policy-check-1", "plan_sha256": frozen["sha256"],
        "checker_sha256": source_reviews.identity(Path(__file__)), "candidate_sha256": digest(candidate),
        "unchanged": unchanged, "policy_cases": 324, "grade": grade, "task_accepted": False,
        "scope": "Policy tables and byte identity outside the filter; no claim of exhaustive semantic correctness."}
    (output / "result.json").write_bytes(encode(result))
    require(grade["outcome"] == "passed", "policy matrix failed")
    print("324 policy combinations passed; code outside the filter is byte-identical.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    check(parser.parse_args().output)
