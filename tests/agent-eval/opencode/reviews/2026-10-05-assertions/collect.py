"""Freeze single-assertion reviews with explicit before/after file identities."""
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
from agent_eval import change_controls, rehearsal_evidence, source_packets as packets, source_reviews as reviews
from agent_eval.study import digest, encode, load, require
from agent_eval.workspace_bundle import unpack

BINARY = ROOT / "target/agent-tools/fr-v0.46.0/fr"
MODELS = ["kimi-code-plan-global/k3", "zai-coding-plan/glm-5.3-flash"]
QUESTIONS = [
    {"task": "packaging-prerelease", "id": "packaging-version-identity",
     "question": "Review only the assertion that the two Version objects in the supplied grader excerpt are returned as those exact original objects, in order. Does that assertion contradict the requirement, or does the proposed reference violate it? Assume the two input versions satisfy the range; do not review version ordering, other inputs or other policies. A no-finding answer is limited to this assertion.",
     "grader_start": "    values = [Version('1.5a1'), Version('1.6rc1')]",
     "grader_end": "    assert len(result) == 2 and all(a is b for a, b in zip(values, result))",
     "files": ["src/packaging/specifiers.py", "src/packaging/version.py"],
     "declarations": [("src/packaging/specifiers.py", "SpecifierSet", "filter", True)]},
    {"task": "platformdirs-xdg", "id": "platformdirs-rejected-directory",
     "question": "Review only the assertion that /tmp/rejected-relative does not exist after accessing user_config_dir with XDG_CONFIG_HOME='rejected-relative' and ensure_exists=True. Assume that directory was absent before the test and HOME=/tmp/fixture-home. Does the assertion contradict the requirement, or does the proposed reference create this rejected directory? Exclude other path APIs, environment values and directory existence claims.",
     "grader_start": "    os.chdir('/tmp')\n    os.environ['XDG_CONFIG_HOME'] = 'rejected-relative'",
     "grader_end": "    assert not Path('/tmp/rejected-relative').exists()",
     "files": ["src/platformdirs/unix.py", "src/platformdirs/api.py"],
     "declarations": [("src/platformdirs/unix.py", "Unix", "user_config_dir", False),
                      ("src/platformdirs/api.py", "PlatformDirsABC", "_append_app_name_and_version", False),
                      ("src/platformdirs/api.py", "PlatformDirsABC", "_optionally_create_directory", False)]},
    {"task": "dotenv-alternate", "id": "dotenv-absent-alternate",
     "question": "Review only the value=None iteration of the supplied alternate grader excerpt: BASE is absent and OUT=before${BASE:+chosen}after must become beforeafter. Does this assertion contradict the requirement, or does the proposed reference violate it? Exclude empty/present BASE, precedence, malformed expressions and all other interpolation syntax. A no-finding answer is limited to this one input.",
     "grader_start": "    for value, expected in [('present', 'chosen'), ('', ''), (None, '')]:",
     "grader_end": "        assert got['OUT'] == 'before' + expected + 'after', got",
     "files": ["src/dotenv/variables.py", "src/dotenv/main.py"],
     "declarations": [("src/dotenv/variables.py", None, "parse_variables", False),
                      ("src/dotenv/main.py", None, "resolve_variables", False)]},
]


def raw(files, path):
    return base64.b64decode(files[path]["data"], validate=True)


def extent(files, path, start=0, end=None):
    data = raw(files, path)
    return {"path": path, "sha256": hashlib.sha256(data).hexdigest(),
            "start": start, "end": len(data) if end is None else end}


def declaration(files, workspace, path, scope, name, omit_docstring):
    base = [str(BINARY), "--json", "-C", str(workspace), "project"]
    selected = path
    if scope:
        report = json.loads(subprocess.check_output(base + ["find", scope, "--in", path]))
        require(len(report["rows"]) == 1, "ambiguous scope")
        selected = dict(zip(report["columns"], report["rows"][0]))["handle"]
    report = json.loads(subprocess.check_output(base + ["find", name, "--in", selected, "--source", "--bytes", "8192"]))
    require(len(report["rows"]) == 1, "ambiguous declaration")
    row = dict(zip(report["columns"], report["rows"][0]))
    require(row["source"]["next_offset"] is None, "declaration exceeds source budget")
    data = raw(files, path)
    if path.endswith("/variables.py"):
        whole = json.loads(subprocess.check_output(base + ["show", report["root"], "--source", "--bytes", "8192"]))
        require(whole["source"]["next_offset"] is None and whole["source"]["text"].encode() == data,
                "fr module differs from snapshot")
        return [extent(files, path)]
    span = row["location"]["definition"]["span"]
    require(row["source"]["text"].encode() == data[span["start"]:span["end"]], "fr declaration differs")
    if not omit_docstring:
        return [extent(files, path, span["start"], span["end"])]
    function = next(n for n in ast.walk(ast.parse(data)) if isinstance(n, ast.FunctionDef)
                    and n.name == name and n.lineno == row["location"]["name"]["range"]["start"]["line"])
    require(ast.get_docstring(function) is not None, "expected docstring missing")
    doc, lines = function.body[0], data.splitlines(keepends=True)
    return [extent(files, path, span["start"], sum(map(len, lines[:doc.lineno - 1]))),
            extent(files, path, sum(map(len, lines[:doc.end_lineno])), span["end"])]


def prepare():
    require(not (HERE / "plan.json").exists(), "review plan already exists")
    manifest, controls = load(PACK / "manifest.json"), load(PACK / "controls.json")
    change_controls.verify_sources(PACK, manifest["tasks"])
    questions, bindings, inputs = [], {}, {"manifest.json", "sources.json", "controls.json"}
    for spec in QUESTIONS:
        task = next(t for t in manifest["tasks"] if t["id"] == spec["task"])
        baseline = rehearsal_evidence.source_bundle(task, PACK)
        reference = next(c for c in controls[task["id"]] if c["id"] == "reference")
        repaired = change_controls.apply(baseline, reference)
        files = {**repaired, **{"baseline/" + p: row for p, row in baseline.items()}}
        comparison = packets.compare(files, [{"before": "baseline/" + p, "after": p} for p in spec["files"]])
        grader = load(PACK / task["grader"])["command"][-1]
        extra = {"requirement.txt": task["requirement"], "grader.py": grader,
                 "reference.json": json.dumps(reference), "file-comparison.json": encode(comparison).decode()}
        for name, text in extra.items():
            files["review/" + name] = {"data": base64.b64encode(text.encode()).decode(), "executable": False}
        require(grader.count(spec["grader_start"]) == grader.count(spec["grader_end"]) == 1,
                "grader excerpt is ambiguous")
        start = grader.index(spec["grader_start"])
        end = grader.index(spec["grader_end"], start) + len(spec["grader_end"])
        selections = [extent(files, "review/requirement.txt"), extent(files, "review/file-comparison.json"),
                      extent(files, "review/grader.py", len(grader[:start].encode()), len(grader[:end].encode()))]
        with tempfile.TemporaryDirectory(prefix="fr-assertion-packet-") as temporary:
            workspace = Path(temporary) / "source"
            unpack(files, workspace, reviews.legacy.MAX_WORKSPACE)
            for path, scope, name, omit in spec["declarations"]:
                selections.extend(declaration(files, workspace, path, scope, name, omit))
        note = ("Source under src/ is the author's proposed reference, not a proven answer. "
                "Original files are under baseline/src/. The packet's file comparison identifies identical "
                "content and changed files; it does not prove behavioral equivalence. review/reference.json "
                "contains the patch. Only the selected grader assertion is in scope. ")
        questions.append({"id": spec["id"], "question": note + spec["question"],
                          "files": files, "selections": selections})
        bindings[spec["id"]] = {"task": task["id"], "baseline_sha256": digest(baseline),
                               "reference_sha256": digest(repaired), "control_sha256": digest(reference)}
        inputs.update((task["source"], task["grader"]))
    provenance = {"baseline": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
                  "collector_sha256": reviews.identity(__file__), "references": bindings,
                  "public_inputs": {p: reviews.identity(PACK / p) for p in sorted(inputs)},
                  "scope": "One assertion per question, exact grader excerpt, selected before/after file identities.",
                  "source_selection": "fr exact declarations; complete small variables module; long filter docstring omitted",
                  "prior_collections": "All prior stopped cells remain stopped; no retry or budget increase."}
    frozen, snapshots = reviews.freeze_recovery_reference(questions, MODELS, BINARY, Path(shutil.which("opencode")), provenance)
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
        for files in snapshots.values():
            packets.check_comparison(files, json.loads(raw(files, "review/file-comparison.json")))
        if sys.argv[1:] in (["report"], ["check"]):
            result = reviews.report(frozen, snapshots, HERE / "attempts")
            reuse = reviews.source_reuse(frozen, snapshots, HERE / "attempts")
            if sys.argv[1] == "check":
                require(load(HERE / "report.json") == result, "review report differs")
                require(load(HERE / "source-reuse.json") == reuse, "source reuse differs")
            else:
                (HERE / "report.json").write_bytes(encode(result))
                (HERE / "source-reuse.json").write_bytes(encode(reuse))
            print(json.dumps({k: result[k] for k in ("planned", "completed", "failed", "not_started")}))
        else:
            require(len(sys.argv) == 2, "supply one frozen cell ID")
            result = reviews.collect(frozen, snapshots, sys.argv[1], HERE / "attempts", BINARY, Path(shutil.which("opencode")))
            print(json.dumps({k: result[k] for k in ("cell", "status", "failure", "wall_seconds")}))
