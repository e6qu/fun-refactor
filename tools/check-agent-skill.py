#!/usr/bin/env python3
"""Execute the packaged skill's shell examples in disposable projects."""

import argparse
import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent
SKILL = ROOT / "skills/fr"
ROUTES = {
    "targeted-author": ["SKILL.md", "references/author.md", "references/checks.md",
                        "references/history.md", "references/git.md"],
    "built-in-change": ["SKILL.md", "references/change.md", "references/checks.md",
                        "references/history.md", "references/git.md"],
    "recipe-change": ["SKILL.md", "references/change.md", "references/recipes.md",
                      "references/checks.md", "references/history.md", "references/git.md"],
    "author-recovery": ["SKILL.md", "references/author.md", "references/checks.md",
                        "references/history.md", "references/recovery.md", "references/git.md"],
    "exploration": ["SKILL.md", "references/explore.md", "references/batch.md"],
    "task": ["SKILL.md", "references/task.md", "references/batch.md"],
    "lean": ["SKILL.md", "references/lean.md"],
    "git-admin": ["SKILL.md", "references/git.md", "references/git-admin.md"],
    "verified-workflow": ["SKILL.md", "references/workflow.md"],
}


def blocks(path, language):
    return re.findall(r"```" + language + r"\n(.*?)```", path.read_text(), re.S)


def commands(path):
    return [shlex.split(line) for block in blocks(path, "sh")
            for line in block.splitlines() if line.strip()]


def check_bundle():
    files = sorted(SKILL.rglob("*.md"))
    entry_bytes = (SKILL / "SKILL.md").stat().st_size
    assert entry_bytes <= 1536, "Keep the introductory context within 1.5 KiB."
    for path in files:
        assert path.stat().st_size <= 4096, f"Split task-specific detail: {path}"
        for link in re.findall(r"\]\(([^)]+)\)", path.read_text()):
            target = (path.parent / link).resolve()
            assert target.is_relative_to(SKILL.resolve()) and target.is_file(), link
        for command in commands(path):
            assert command[0] == "fr", command
    route_bytes = {
        name: sum((SKILL / relative).stat().st_size for relative in route)
        for name, route in ROUTES.items()
    }
    assert max(route_bytes.values()) <= 7168, route_bytes
    return files, entry_bytes, route_bytes


class Exercise:
    def __init__(self, binary):
        self.binary = binary
        self.examples = 0
        self.exploration_bytes = 0
        self.values = {}
        self.executed = []

    def run(self, root, args, success=True):
        args = [self.values.get(arg, arg) for arg in args]
        assert not any(re.fullmatch(r"<[^>]+>", arg) for arg in args), args
        options = [] if "--json" in args else ["--json"]
        result = subprocess.run([self.binary, "--no-cache", "-C", str(root), *options, *args],
                                capture_output=True, text=True, timeout=120)
        assert (result.returncode == 0) == success, (args, result.stdout, result.stderr)
        value = json.loads(result.stdout)
        return value, len(result.stdout.encode())

    def example(self, root, path, command, success=True):
        value, size = self.run(root, command[1:], success)
        self.examples += 1
        self.executed.append((path, tuple(command)))
        if path.name in ("SKILL.md", "explore.md", "batch.md"):
            self.exploration_bytes += size
        return value


def git(root, *args, input_bytes=None):
    environment = {key: value for key, value in os.environ.items() if not key.startswith("GIT_")}
    environment.update(GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=os.devnull)
    result = subprocess.run(["git", "-c", "user.name=fixture", "-c", "user.email=fr@example.invalid",
                             *args], cwd=root, env=environment, input=input_bytes,
                            capture_output=True, timeout=30)
    assert result.returncode == 0, result.stderr.decode(errors="replace")
    return result.stdout


def behavior(root):
    result = subprocess.run([sys.executable, "-B", "-c", "import test_app; test_app.test_greeting()"],
                            cwd=root, capture_output=True, timeout=30)
    assert result.returncode == 0, result.stderr.decode(errors="replace")


def source_workflow(exercise, root):
    reference = SKILL / "references"
    original = {
        "app.py": 'def greet(name: str) -> str:\n    return "hello " + name\n\ndef run():\n    return greet("Ada")\n',
        "test_app.py": 'from app import greet\n\ndef test_greeting():\n    assert greet("Ada") == "hello Ada"\n',
        "background.py": "# Unrelated implementation context that this task does not need.\n" * 1024,
        "rename.recipe": blocks(reference / "recipes.md", "recipe")[0],
    }
    for name, source in original.items():
        (root / name).write_text(source)
    git(root, "init", "-q", "-b", "main")
    git(root, "add", ".")
    git(root, "commit", "-qm", "fixture")
    index = (root / ".git/index").read_bytes()
    behavior(root)
    query_manifest = root.parent / "project-queries.json"
    query_manifest.write_text(blocks(reference / "batch.md", "json")[0])
    exercise.values["<PROJECT_QUERIES>"] = str(query_manifest)

    for path in [SKILL / "SKILL.md", reference / "explore.md", reference / "batch.md"]:
        for command in commands(path):
            value = exercise.example(root, path, command)
            if value.get("query") in ("map", "find"):
                rows = [dict(zip(value["columns"], row)) for row in value["rows"]]
                selected = [row for row in rows if row["name"] == "greet" and "handle" in row]
                if selected:
                    exercise.values["<HANDLE>"] = selected[0]["handle"]
                assert value["page"]["returned"] <= 12
                assert "coverage" in value and "omitted" in value
            if value.get("query") == "show":
                node = value["node"]
                exercise.values["<TARGET>"] = f'{node["path"]}:{node["position"]["line"]}:{node["position"]["col"]}'
                if "--source" not in command:
                    assert "source" not in value
                else:
                    assert size_of_source(value) <= 256
            if value.get("query") == "batch":
                assert value["report_budget"]["omitted_requests"] == 0
                assert [request["id"] for request in value["requests"]] == [
                    "structure", "lookup", "source", "callers", "tests", "gaps"]
                assert all(request["status"] == "returned" for request in value["requests"])
                assert all("coverage" not in request["report"] for request in value["requests"])
                assert value["requests"][2]["report"]["query"] == "show"
                assert size_of_source(value["requests"][2]["report"]) <= 256
    assert not (root / ".fr-history").exists()

    path = reference / "change.md"
    examples = commands(path)
    recipe_path = reference / "recipes.md"
    recipe_examples = commands(recipe_path)
    rename_examples = [command for command in examples if "recipe" not in command]
    for command in recipe_examples:
        exercise.example(root, recipe_path, command)
        assert (root / "app.py").read_text() == original["app.py"]
    for command in rename_examples:
        if "--write" in command:
            (root / "background.py").write_text(original["background.py"] + "# A concurrent change.\n")
            exercise.run(root, ["history", "apply", exercise.values["<TX>"], "--write"], success=False)
            assert (root / "app.py").read_text() == original["app.py"]
            (root / "background.py").write_text(original["background.py"])
        value = exercise.example(root, path, command)
        if "--save-plan" in command:
            exercise.values["<TX>"] = str(value["transaction"])
            assert value["applied"] is False
        if "--write" not in command:
            assert (root / "app.py").read_text() == original["app.py"]
    changed = {name: (root / name).read_text() for name in ("app.py", "test_app.py")}
    assert all("welcome" in source and "greet(" not in source for source in changed.values())
    behavior(root)
    exercise.run(root, ["project", "show", exercise.values["<HANDLE>"]], success=False)
    (root / "unrelated.txt").write_text("Preserve this later user edit.\n")

    path = reference / "history.md"
    for command in commands(path):
        value = exercise.example(root, path, command)
        if "--no-diff" in command:
            assert value["applied"] is True and value["diffs_omitted"] is True
            assert all("diff" not in change for change in value["changes"])
        if "--write" in command:
            expected = original if "undo" in command else changed
            for name in changed:
                assert (root / name).read_text() == expected[name]
            behavior(root)
        assert (root / "unrelated.txt").read_text() == "Preserve this later user edit.\n"
    receiver = root.parent / "receiver"
    receiver.mkdir()
    for name, source in original.items():
        (receiver / name).write_text(source)
    git(receiver, "init", "-q", "-b", "main")
    git(receiver, "add", ".")
    git(receiver, "commit", "-qm", "receiver")
    receiving_index = (receiver / ".git/index").read_bytes()
    exercise.values["<RECEIVER>"] = str(receiver)
    exercise.values["<PATCH>"] = str(root.parent / "change.patch")

    path = reference / "git.md"
    patch = None
    for command in commands(path):
        value = exercise.example(root, path, command)
        if "patch" in command and "--git-check" not in command:
            patch = (value.get("patch") or Path(value["output"]).read_text()).encode()
    assert patch and b"welcome" in patch
    git(receiver, "apply", "--check", "-", input_bytes=patch)
    git(receiver, "apply", "-", input_bytes=patch)
    for name in changed:
        assert (receiver / name).read_text() == changed[name]
    assert (receiver / ".git/index").read_bytes() == receiving_index
    behavior(receiver)
    assert (root / ".git/index").read_bytes() == index
    assert (root / "unrelated.txt").read_text() == "Preserve this later user edit.\n"
    behavior(root)
    return sum(len(source.encode()) for name, source in original.items() if name.endswith(".py"))


def size_of_source(value):
    source = value["source"]
    assert source["next_offset"] is None or source["next_offset"] > 0
    return len(source["text"].encode())


def checks_workflow(exercise, root):
    (root / ".fr").mkdir()
    (root / ".fr/checks.json").write_text(json.dumps({"schema": 1, "checks": [{
        "name": "unit", "argv": [sys.executable, "-B", "-c", "import test_app; test_app.test_greeting()"],
        "cwd": ".", "timeout_seconds": 30, "covers": ["greeting behavior"]}]}))
    path = SKILL / "references/checks.md"
    for command in commands(path):
        value = exercise.example(root, path, command)
        if "--run" not in command:
            assert value["executed"] is False and value["passed"] is None
            exercise.values["<CHECK_BASIS>"] = value["basis"]
        else:
            assert value["passed"] is True and value["not_run"] == []


def author_workflow(exercise, root):
    path = SKILL / "references/author.md"
    source = root / "src/lib.rs"
    source.parent.mkdir()
    original = ('//! Skill fixture.\n#![deny(missing_docs)]\n'
                '/// Increments a value.\npub fn increment(value: u32) -> u32 { value + 1 }\n')
    source.write_text(original)
    fragment = root.parent / "fragment.rs"
    fragment.write_text(blocks(path, "rust")[0])
    exercise.values["<FRAGMENT>"] = str(fragment)
    body = root.parent / "body.rs"
    body.write_text("{\n    value + 1\n}\n")
    git(root, "init", "-q", "-b", "main")
    git(root, "add", ".")
    git(root, "commit", "-qm", "author fixture")
    initial_index = (root / ".git/index").read_bytes()
    library = root.parent / "libskill_fixture.rlib"

    def compile_library():
        result = subprocess.run(["rustc", "--edition=2021", "--crate-type=lib", "--crate-name=skill_fixture",
                                 str(source), "-o", str(library)], capture_output=True, timeout=60)
        assert result.returncode == 0, result.stderr.decode(errors="replace")

    compile_library()
    unscoped, _ = exercise.run(root, ["project", "find", "increment", "--signature"])
    exercise.run(root, ["author", "insert-declaration", unscoped["root"], "--from", str(fragment)], success=False)
    assert source.read_text() == original and not (root / ".fr-history").exists()
    for command in commands(path):
        value = exercise.example(root, path, command)
        if command[1:3] == ["project", "find"]:
            assert value["page"]["total"] == 1
            row = dict(zip(value["columns"], value["rows"][0]))
            exercise.values["<FILE_HANDLE>"] = value["root"]
            manifest = root.parent / "author-batch.json"
            manifest.write_text(json.dumps({
                "operations": [
                    {"op": "replace-body", "handle": row["handle"], "from": str(body)},
                    {"op": "insert-declaration", "handle": value["root"], "from": str(fragment)},
                ],
                "postconditions": {
                    "files-changed": 1,
                    "edits": 2,
                    "changed-operations": 2,
                    "paths-changed": ["src/lib.rs"],
                },
            }))
            exercise.values["<MANIFEST>"] = str(manifest)
            assert "value + 1" in row["source"]["text"]
            assert row["source"]["next_offset"] is None
            assert value["source_budget"]["returned_bytes"] <= 512
        if command[1:3] == ["author", "batch"] and "--save-plan" not in command:
            exercise.values["<PLAN_CONTEXT_BASIS>"] = value["plan_context_basis"]
        if "--save-plan" in command:
            assert value["saved"] and not value["applied"] and source.read_text() == original
            exercise.values["<AUTHOR_TX>"] = str(value["transaction"])
            exercise.values["<TRANSACTION_CONTEXT_BASIS>"] = value["transaction_context_basis"]
        if "--write" in command:
            assert value["applied"]
            if "--no-diff" in command and "--context-basis" not in command:
                assert value["diffs_omitted"]
            if "--context-basis" in command:
                assert value["context_omitted"] == ["changes[].diff"]
                assert all("diff" not in change for change in value["changes"])
    changed = source.read_text()
    assert changed.startswith("//! Skill fixture.") and changed != original
    assert fragment.read_text().strip() in changed
    compile_library()
    caller = root.parent / "caller.rs"
    caller.write_text('fn main() { assert_eq!(skill_fixture::increment_twice(40), 42); }\n')
    binary = root.parent / "caller"
    compiled = subprocess.run(["rustc", "--edition=2021", str(caller), "--extern", f"skill_fixture={library}",
                               "-o", str(binary)], capture_output=True, timeout=60)
    assert compiled.returncode == 0, compiled.stderr.decode(errors="replace")
    assert subprocess.run([str(binary)], capture_output=True, timeout=30).returncode == 0
    exercise.run(root, ["project", "show", exercise.values["<FILE_HANDLE>"]], success=False)
    (root / "unrelated.txt").write_text("Preserve this later edit.\n")
    for action in ("undo", "redo"):
        exercise.run(root, ["history", action, exercise.values["<AUTHOR_TX>"], "--write", "--no-diff"])
        assert source.read_text() == (original if action == "undo" else changed)
        assert (root / "unrelated.txt").read_text() == "Preserve this later edit.\n"
    assert (root / ".git/index").read_bytes() == initial_index


def task_workflow(exercise, root):
    (root / "src").mkdir()
    (root / ".fr").mkdir()
    (root / "src/lib.rs").write_text(
        "pub fn render(value: &str) -> String { value.to_owned() }\n")
    (root / ".fr/checks.json").write_text(json.dumps({"schema": 1, "checks": [{
        "name": "unit", "argv": ["true"], "cwd": ".", "timeout_seconds": 30,
        "covers": ["render behavior"]}]}))
    manifest = root / ".fr-task"
    manifest.write_text(json.dumps({
        "schema": "fr-project-task-1",
        "requests": [
            {"id": "target", "arguments": ["find", "render", "--signature", "--source"]},
            {"id": "callers", "arguments": ["calls", {"request": "target", "pointer": "/rows/0/0"}]},
        ],
        "targets": [{"id": "render-body", "handle": {"request": "target", "pointer": "/rows/0/0"},
                     "op": "replace-body"}],
        "checks": ["unit"],
        "delivery": {"exercise-reversal": True, "patch": "change.patch"},
    }))
    value, _ = exercise.run(root, ["project", "task", "--from", str(manifest)])
    assert value["targets"][0]["eligibility"] == "target-supported"
    assert value["targets"][0]["syntax_preflighted"] is False
    assert value["checks"]["names"] == ["unit"]
    assert value["author_manifest_template"]["operations"][0]["handle"].startswith("frp1:")
    assert value["workflow_manifest_template"]["checks"]["basis"] == value["checks"]["basis"]


def verified_workflow(exercise, root):
    source = root / "app.py"
    source.write_text("def before():\n    return 1\n")
    (root / ".fr").mkdir()
    (root / ".fr/checks.json").write_text(json.dumps({"schema": 1, "checks": [{
        "name": "unit", "argv": [sys.executable, "-B", "-c",
        "compile(open('app.py').read(), 'app.py', 'exec')"],
        "cwd": ".", "timeout_seconds": 30, "covers": ["Python syntax"]}]}))
    listing, _ = exercise.run(root, ["checks"])
    plan, _ = exercise.run(root, ["rename", "before", "after", "--save-plan"])
    transaction = str(plan["transaction"])
    shown, _ = exercise.run(root, ["history", "show", transaction])
    control = root / ".fr-workflow"
    control.write_text(json.dumps({
        "schema": 1,
        "transaction": plan["transaction"],
        "transaction-context-basis": shown["records"][0]["context_basis"],
        "checks": {"basis": listing["basis"], "names": ["unit"]},
        "exercise-reversal": True,
        "patch": {"output": "change.patch"},
        "check-output-bytes": 256,
    }))
    exercise.values["<WORKFLOW_MANIFEST>"] = str(control)
    path = SKILL / "references/workflow.md"
    for command in commands(path):
        value = exercise.example(root, path, command)
        if "--write" not in command:
            assert value["ready"] and not value["executed"]
            assert "before" in source.read_text()
            exercise.values["<WORKFLOW_BASIS>"] = value["workflow_basis"]
        else:
            assert value["passed"] and value["transaction_status"] == "applied"
            assert all(stage["status"] == "passed" for stage in value["stages"])
            assert "checks" in value["reviewed_context_omitted"]
    assert "after" in source.read_text()
    assert (root / "change.patch").read_text().startswith("diff --git ")


def lean_workflow(exercise, root):
    path = SKILL / "references/lean.md"
    (root / "src").mkdir()
    (root / "specs").mkdir()
    (root / "src/lib.rs").write_text(blocks(path, "rust")[0])
    model = root / "specs/Model.lean"
    model.write_text(blocks(path, "lean")[0])
    (root / "specs/lean-toolchain").write_text((ROOT / "kernels/lean-toolchain").read_text())
    (root / "specs/lakefile.toml").write_text('name = "agent_skill_fixture"\nversion = "0.1.0"\ndefaultTargets = ["Model"]\n\n[[lean_lib]]\nname = "Model"\n')
    for command in commands(path):
        value = exercise.example(root, path, command, success="check" not in command)
        if "check" in command:
            assert any(anchor["status"] == "stale" for anchor in value["anchors"])
        if "verify" in command:
            assert value["report"]["obligations"] == 0
            assert len(value["report"]["anchors"]) == 1
            assert all(anchor["status"] == "fresh" and anchor["signature"]["status"] == "fresh"
                       for anchor in value["report"]["anchors"])
            assert value["packages"] and all(package["passed"] for package in value["packages"])
    model.write_text(model.read_text().replace("allowed true = true", "allowed true = false"))
    failed, _ = exercise.run(root, ["spec", "verify", "specs"], success=False)
    assert any(not package["passed"] for package in failed["packages"])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fr", required=True, type=Path, help="Built fr binary to validate.")
    args = parser.parse_args()
    files, entry_bytes, route_bytes = check_bundle()
    exercise = Exercise(str(args.fr.resolve()))
    with tempfile.TemporaryDirectory(prefix="fr-agent-skill-") as directory:
        root = Path(directory)
        (root / "source").mkdir()
        (root / "proof").mkdir()
        (root / "author").mkdir()
        (root / "task").mkdir()
        (root / "workflow").mkdir()
        source_bytes = source_workflow(exercise, root / "source")
        checks_workflow(exercise, root / "source")
        author_workflow(exercise, root / "author")
        task_workflow(exercise, root / "task")
        verified_workflow(exercise, root / "workflow")
        lean_workflow(exercise, root / "proof")
    expected = [(path, tuple(command)) for path in files for command in commands(path)]
    assert sorted(exercise.executed) == sorted(expected), "Every fenced shell example must execute."
    assert exercise.exploration_bytes < source_bytes
    print(json.dumps({"passed": True, "shell_examples": exercise.examples,
                      "entry_bytes": entry_bytes, "reference_bytes": sum(path.stat().st_size for path in files) - entry_bytes,
                      "route_bytes": route_bytes,
                      "exploration_output_bytes": exercise.exploration_bytes,
                      "fixture_source_bytes": source_bytes,
                      "measurement": "UTF-8 bytes on a synthetic fixture, not model tokens or an agent success rate."}))


if __name__ == "__main__":
    main()
