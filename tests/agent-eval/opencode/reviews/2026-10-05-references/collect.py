"""Review proposed reference repairs; never resume an earlier stopped collection."""
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
from agent_eval import change_controls, rehearsal_evidence, source_reviews as reviews
from agent_eval.study import digest, encode, load, require
from agent_eval.workspace_bundle import unpack

BINARY = ROOT / "target/agent-tools/fr-v0.46.0/fr"
MODELS = ["kimi-code-plan-global/k3", "zai-coding-plan/glm-5.3-flash"]
QUESTIONS = [
    ("dotenv-alternate", "dotenv-reference-precedence",
     "Review only override precedence for alternate words through dotenv_values and load_dotenv. Does the proposed reference violate the requirement, or does a grader assertion contradict it? Give at most one concrete input and expected result. Do not review nested grammar or other parsing.",
     [("src/dotenv/variables.py", "Variable", "resolve"),
      ("src/dotenv/main.py", None, "resolve_variables")]),
    ("packaging-prerelease", "packaging-reference-policy",
     "Review only explicit constructor/call True and False policies, call precedence, and prerelease bounds. Does the proposed reference violate the requirement, or does a grader assertion contradict it? Give at most one concrete input and expected result. Do not review iteration, object identity or duplicates.",
     [("src/packaging/specifiers.py", "SpecifierSet", "filter")]),
    ("platformdirs-xdg", "platformdirs-reference-paths",
     "Review only preservation of absolute XDG site paths, including spaces, order and duplicates, with relative entries mixed in. Does the proposed reference violate the requirement, or does a grader assertion contradict it? Give at most one input and expected result. Do not review directory-creation effects.",
     [("src/platformdirs/unix.py", "Unix", "_site_data_dirs"),
      ("src/platformdirs/unix.py", "Unix", "_site_config_dirs"),
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
    require(row["source"]["next_offset"] is None, "review declaration exceeds source budget")
    raw = content(files, path)
    span = row["location"]["definition"]["span"]
    require(row["source"]["text"].encode() == raw[span["start"]:span["end"]], "fr source differs from reference")
    function = next(node for node in ast.walk(ast.parse(raw)) if isinstance(node, ast.FunctionDef)
                    and node.name == name and node.lineno == row["location"]["name"]["range"]["start"]["line"])
    if ast.get_docstring(function) is None:
        return [extent(files, path, span["start"], span["end"])]
    lines, doc = raw.splitlines(keepends=True), function.body[0]
    return [extent(files, path, span["start"], sum(map(len, lines[:doc.lineno - 1]))),
            extent(files, path, sum(map(len, lines[:doc.end_lineno])), span["end"])]


def prepare():
    require(not (HERE / "plan.json").exists(), "review plan already exists")
    manifest, controls = load(PACK / "manifest.json"), load(PACK / "controls.json")
    questions, bindings = [], {}
    inputs = {"manifest.json", "sources.json", "public-checks.json", "controls.json"}
    change_controls.verify_sources(PACK, manifest["tasks"])
    for task_id, identity, question, declarations in QUESTIONS:
        task = next(t for t in manifest["tasks"] if t["id"] == task_id)
        baseline = rehearsal_evidence.source_bundle(task, PACK)
        reference = next(c for c in controls[task_id] if c["id"] == "reference")
        repaired = change_controls.apply(baseline, reference)
        files = {**repaired, **{"baseline/" + p: row for p, row in baseline.items()}}
        grader = load(PACK / task["grader"])
        extra = {"requirement.txt": task["requirement"], "grader.py": grader["command"][-1],
                 "reference.json": json.dumps(reference), "cases.json": json.dumps(grader["cases"])}
        for name, text in extra.items():
            files["review/" + name] = {"data": base64.b64encode(text.encode()).decode(), "executable": False}
        selections = [extent(files, "review/requirement.txt"), extent(files, "review/grader.py")]
        with tempfile.TemporaryDirectory(prefix="fr-reference-packet-") as temporary:
            workspace = Path(temporary) / "source"
            unpack(files, workspace, reviews.legacy.MAX_WORKSPACE)
            for path, scope, name in declarations:
                selections.extend(source_slices(files, path, scope, name, workspace))
        note = ("Source under src/ contains the task author's proposed reference repair, not a proven answer. "
                "Original source is available under baseline/; review/reference.json records its edits. ")
        questions.append({"id": identity, "question": note + question, "files": files, "selections": selections})
        bindings[identity] = {"task": task_id, "baseline_sha256": digest(baseline),
                              "reference_sha256": digest(repaired), "control_sha256": digest(reference)}
        inputs.update((task["source"], task["grader"]))
    provenance = {"baseline": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
                  "collector_sha256": reviews.identity(__file__), "references": bindings,
                  "public_inputs": {p: reviews.identity(PACK / p) for p in sorted(inputs)},
                  "source_selection": "fr declaration bytes; docstrings omitted; baseline and reference source remain available"}
    frozen, snapshots = reviews.freeze_reference(questions, MODELS, BINARY, Path(shutil.which("opencode")), provenance)
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
