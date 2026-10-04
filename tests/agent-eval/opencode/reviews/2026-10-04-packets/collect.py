"""Three frozen review questions; earlier stopped collections remain untouched."""
import ast
import base64
import gzip
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[4]
PACK = ROOT / "tests/agent-eval/opencode/candidates"
sys.path.insert(0, str(ROOT / "tools"))
from agent_eval import rehearsal_evidence, source_reviews as reviews
from agent_eval.study import encode, load, require
from agent_eval.workspace_bundle import unpack

BINARY = ROOT / "target/agent-tools/fr-v0.46.0/fr"
MODELS = ["kimi-code-plan-global/k3", "zai-coding-plan/glm-5.3-flash"]
QUESTIONS = [
    ("dotenv-alternate", "dotenv-flat-grammar",
     "Review only flat, nonrecursive interpolation and literal alternate words. Can the grader accept a repair that violates this part of the requirement? Give at most one concrete counterexample; do not review other behavior.",
     [("src/dotenv/variables.py", None, "parse_variables")]),
    ("packaging-prerelease", "packaging-iterable-identity",
     "Review only preservation of one-shot iteration and original item identity, order and duplicates. Can the grader accept a repair that violates these requirements? Give at most one concrete counterexample; do not review release-selection policy.",
     [("src/packaging/specifiers.py", "SpecifierSet", "filter")]),
    ("platformdirs-xdg", "platformdirs-creation",
     "Review only ensure_exists side effects: rejected relative paths must never cause directory creation. Can the grader accept a repair violating that requirement? Give at most one concrete counterexample; do not review unrelated directory selection.",
     [("src/platformdirs/unix.py", "Unix", "user_config_dir"),
      ("src/platformdirs/api.py", "PlatformDirsABC", "_append_app_name_and_version")]),
]


def content(files, path):
    return base64.b64decode(files[path]["data"], validate=True)


def extent(files, path, start=0, end=None):
    raw = content(files, path)
    return {"path": path, "sha256": hashlib.sha256(raw).hexdigest(), "start": start,
            "end": len(raw) if end is None else end}


def source_slices(files, path, scope, name, workspace):
    base = [str(BINARY), "--json", "-C", str(workspace), "project", "find"]
    selected = path
    if scope:
        report = json.loads(subprocess.check_output(base + [scope, "--in", path]))
        require(len(report["rows"]) == 1, "review scope is ambiguous")
        selected = dict(zip(report["columns"], report["rows"][0]))["handle"]
    report = json.loads(subprocess.check_output(base + [name, "--in", selected, "--source", "--bytes", "8192"]))
    require(len(report["rows"]) == 1, "review declaration is ambiguous")
    row = dict(zip(report["columns"], report["rows"][0]))
    require(row["source"]["next_offset"] is None, "review declaration exceeds the source read budget")
    if scope is None:
        whole = json.loads(subprocess.check_output(base[:-1] + ["show", report["root"], "--source", "--bytes", "8192"]))
        require(whole["source"]["next_offset"] is None
                and whole["source"]["text"].encode() == content(files, path), "fr file source differs from snapshot")
        return [extent(files, path)]
    raw = content(files, path)
    lines = raw.splitlines(keepends=True)
    function = next(node for node in ast.walk(ast.parse(raw)) if isinstance(node, ast.FunctionDef)
                    and node.name == name and node.lineno == row["location"]["name"]["range"]["start"]["line"])
    span = row["location"]["definition"]["span"]
    require(row["source"]["text"].encode() == raw[span["start"]:span["end"]], "fr source differs from snapshot")
    if ast.get_docstring(function) is None:
        return [extent(files, path, span["start"], span["end"])]
    doc = function.body[0]
    return [extent(files, path, span["start"], sum(map(len, lines[:doc.lineno - 1]))),
            extent(files, path, sum(map(len, lines[:doc.end_lineno])), span["end"])]


def prepare():
    require(not (HERE / "plan.json").exists(), "review plan already exists")
    manifest = load(PACK / "manifest.json")
    tasks, inputs = [], {"manifest.json", "sources.json", "public-checks.json"}
    for task_id, identity, question, declarations in QUESTIONS:
        task = next(t for t in manifest["tasks"] if t["id"] == task_id)
        files = rehearsal_evidence.source_bundle(task, PACK)
        grader = load(PACK / task["grader"])
        extra = {"requirement.txt": task["requirement"], "grader.py": grader["command"][-1],
                 "cases.json": json.dumps(grader["cases"]),
                 "public-check.py": load(PACK / "public-checks.json")[task_id]["command"][-1]}
        for name, text in extra.items():
            files["review/" + name] = {"data": base64.b64encode(text.encode()).decode(), "executable": False}
        selections = [extent(files, "review/requirement.txt"), extent(files, "review/grader.py")]
        with tempfile.TemporaryDirectory(prefix="fr-review-packet-") as temporary:
            workspace = Path(temporary) / "source"
            unpack(files, workspace, reviews.legacy.MAX_WORKSPACE)
            for path, scope, name in declarations:
                selections.extend(source_slices(files, path, scope, name, workspace))
        tasks.append({"id": identity, "question": question, "files": files, "selections": selections})
        inputs.update((task["source"], task["grader"]))
    previous = load(HERE.parent / "2026-10-04-native/plan.json")["plan"]
    hashes = {name: reviews.identity(PACK / name) for name in sorted(inputs)}
    require(hashes == previous["public_inputs"], "candidate inputs differ from verified public inputs")
    provenance = {"baseline": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
                  "collector_sha256": reviews.identity(__file__), "public_inputs": hashes,
                  "public_input_revision": previous["baseline"],
                  "source_selection": "fr declarations; docstrings omitted for scoped methods; omitted files remain available",
                  "questions": [{"task": row[0], "question": row[1]} for row in QUESTIONS]}
    frozen, snapshots = reviews.freeze(tasks, MODELS, BINARY, Path(shutil.which("opencode")), provenance)
    reviews.checked(frozen, snapshots, execution=True)
    (HERE / "plan.json").write_bytes(encode(frozen))
    (HERE / "inputs.json.gz").write_bytes(gzip.compress(encode(snapshots), mtime=0))
    print(json.dumps({"cells": len(frozen["plan"]["cells"]), "packets": [
        {"id": t["id"], "bytes": len(encode(t["packet"])),
         "source_bytes": sum(s["end"] - s["start"] for s in t["packet"]["spans"])} for t in frozen["plan"]["tasks"]]}))


if __name__ == "__main__":
    if sys.argv[1:] == ["prepare"]:
        prepare()
    else:
        frozen = load(HERE / "plan.json")
        require(reviews.identity(__file__) == frozen["plan"]["provenance"]["collector_sha256"], "collector changed")
        snapshots = reviews.read_inputs(HERE)
        if sys.argv[1:] in (["report"], ["check"]):
            result = reviews.report(frozen, snapshots, HERE / "attempts")
            if sys.argv[1] == "check":
                require(load(HERE / "report.json") == result, "review report differs")
            else:
                (HERE / "report.json").write_text(json.dumps(result, indent=2) + "\n")
            print(json.dumps({k: result[k] for k in ("planned", "completed", "failed", "not_started")}))
        else:
            require(len(sys.argv) == 2, "supply one frozen cell ID")
            result = reviews.collect(frozen, snapshots, sys.argv[1], HERE / "attempts", BINARY, Path(shutil.which("opencode")))
            print(json.dumps({k: result[k] for k in ("cell", "status", "failure", "wall_seconds")}))
