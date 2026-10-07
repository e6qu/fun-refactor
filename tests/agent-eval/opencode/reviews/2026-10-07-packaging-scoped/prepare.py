"""Prepare scoped questions from unchanged source; never execute candidate code."""
import ast
import base64
import hashlib
import json
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[4]
sys.path.insert(0, str(REPO / "tools"))
from agent_eval import source_packets, source_reviews
from agent_eval.study import encode, require


def prepare():
    previous = json.loads((HERE.with_name("2026-10-07-packaging-low") / "design.json").read_bytes())
    files = source_reviews.read_inputs((REPO / previous["source_archive"]["path"]).parent)["packaging-prerelease"]
    source_path = "src/packaging/specifiers.py"
    raw = base64.b64decode(files[source_path]["data"], validate=True)
    lines = raw.splitlines(keepends=True)
    offsets = [0]
    for line in lines:
        offsets.append(offsets[-1] + len(line))
    classes = {n.name: n for n in ast.parse(raw).body if isinstance(n, ast.ClassDef)}

    def selection(path, start=0, end=None):
        data = base64.b64decode(files[path]["data"], validate=True)
        return {"path": path, "sha256": hashlib.sha256(data).hexdigest(), "start": start,
                "end": len(data) if end is None else end}

    def methods(owner, name):
        result = []
        nodes = [n for n in classes[owner].body if isinstance(n, ast.FunctionDef) and n.name == name]
        require(bool(nodes), "missing selected declaration")
        for node in nodes:
            first = node.body[0]
            if isinstance(first, ast.Expr) and isinstance(first.value, ast.Constant) and isinstance(first.value.value, str):
                result.append(selection(source_path, offsets[node.lineno - 1], offsets[first.lineno - 1]))
                result.append(selection(source_path, offsets[node.body[1].lineno - 1], offsets[node.end_lineno]))
            else:
                result.append(selection(source_path, offsets[node.lineno - 1], offsets[node.end_lineno]))
        return result

    groups = [
        ("fallback-bounds", "intersection fallback, matching final/postrelease suppression, and bounds/exclusions",
         [("Specifier", "_compare_less_than"), ("Specifier", "_compare_greater_than")],
         [("intersection-fallback", "Fallback applies across the complete intersection, including out-of-range finals."),
          ("matching-finals", "Matching final and post releases suppress matching prereleases."),
          ("bounds-exclusions", "Prerelease bounds, exclusive comparisons and exclusions retain their matching semantics.")]),
        ("explicit-policies", "constructor policy, call overrides, inferred prerelease policy, and empty sets",
         [("SpecifierSet", "__init__"), ("SpecifierSet", "prereleases")],
         [("constructor-policy", "Explicit constructor prerelease policy is respected."),
          ("call-overrides", "Explicit call policy overrides constructor policy."),
          ("inferred-policy", "Prerelease bounds infer policy consistently when no explicit policy is supplied."),
          ("empty-sets", "Empty specifier sets and empty iterables obey fallback and explicit policies.")]),
        ("objects-and-apis", "Version objects, per-occurrence identity, order, duplicates, one-shot inputs, and unchanged comparison APIs",
         [("SpecifierSet", "contains"), ("Specifier", "filter")],
         [("version-identity", "Original Version objects are returned with per-occurrence identity."),
          ("order-duplicates", "Output preserves matching occurrence order and duplicates."),
          ("one-shot-input", "One-shot inputs are consumed once without losing matching occurrences."),
          ("unchanged-apis", "Contains and individual Specifier filtering retain their behavior.")])]
    questions, coverage = [], []
    for name, scope, declarations, areas in groups:
        identifier = "packaging-" + name
        selections = [selection("review/requirement.txt"), selection("review/grader.py"), *methods("SpecifierSet", "filter")]
        for owner, method in declarations:
            selections.extend(methods(owner, method))
        source_packets.build(files, selections)
        question = (
            "Review only " + scope + ". The complete requirement and grader are supplied; other requirement families are outside this question. "
            "Source under src/ is the proposed reference; baseline/src/ is original. Selected function signatures and executable bodies omit docstrings; complete source remains available through tools. "
            "Before alleging a matching or policy error, inspect the implementation that decides it; version ordering alone is not specifier membership. "
            "Find at most one concrete reference violation, contradictory grader assertion, or wrong repair accepted by the grader within this scope. "
            "Return only the StructuredOutput tool call when ready: no prose preface and no repeated explanation. Keep each finding field to one short sentence, with a distinguishing input, expected result and source citation. "
            "If unsupported, submit findings=[]; in limitations, state which assigned areas you inspected and which remain unchecked. "
            "Do not infer whole-task acceptance, execute code or edit code."
        )
        questions.append({"id": identifier, "source_task": "packaging-prerelease", "question": question,
                          "selections": selections, "comparison": "review/file-comparison.json"})
        coverage.extend({"id": area, "description": description, "question": identifier} for area, description in areas)
    return {**previous, "questions": questions, "coverage": coverage}


if __name__ == "__main__":
    require(sys.argv[1:] in (["write"], ["check"]), "choose write or check")
    raw = encode(prepare())
    path = HERE / "design.json"
    if sys.argv[1] == "write":
        with path.open("xb") as stream:
            stream.write(raw)
    else:
        require(path.read_bytes() == raw, "scoped question design changed")
    print("Three scoped questions prepared from unchanged task inputs; no model calls.")
