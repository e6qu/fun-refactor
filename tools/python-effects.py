#!/usr/bin/env python3
"""Compare scalar effect reports with independently declared Python executions."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

from evidence_basis import native_source_bindings

ROOT = Path(__file__).resolve().parents[1]
EXAMPLES = [
    {"id": "explicit-stop", "body": "value = read_user()\nraise ValueError(value)\nsend(value)",
     "sent": [], "raised": "ValueError", "complete": True, "witness": False, "limits": []},
    {"id": "explicit-after-send", "body": "value = read_user()\nsend(value)\nraise ValueError(value)",
     "sent": ["input"], "raised": "ValueError", "complete": True, "witness": True, "limits": []},
    {"id": "implicit-stop", "body": "value = read_user()\n1 // 0\nreturn send(value)",
     "sent": [], "raised": "ZeroDivisionError", "complete": True, "witness": True,
     "limits": [["implicit-exception", "1 // 0"]],
     "difference": "The scalar model continues past division; Python raises first. Its witness is not an executable path."},
    {"id": "handler-forward", "body": "try:\n    1 // 0\nexcept ZeroDivisionError:\n    return send(read_user())",
     "sent": ["input"], "raised": None, "complete": False, "witness": False,
     "limits": [["exception-handler", "try:"]]},
    {"id": "handler-replace", "body": "try:\n    value = read_user()\n    1 // 0\nexcept ZeroDivisionError:\n    value = 0\nreturn send(value)",
     "sent": [0], "raised": None, "complete": False, "witness": False,
     "limits": [["exception-handler", "try:"]]},
    {"id": "shared-write-forward", "body": "values = [0]\nalias = values\nalias[0] = read_user()\nreturn send(values[0])",
     "sent": ["input"], "raised": None, "complete": False, "witness": False,
     "limits": [["shared-write", "alias[0]"], ["shared-read", "values[0]"]]},
    {"id": "shared-write-replace", "body": "values = [read_user()]\nalias = values\nalias[0] = 0\nreturn send(values[0])",
     "sent": [0], "raised": None, "complete": False, "witness": False,
     "limits": [["shared-write", "alias[0]"], ["shared-read", "values[0]"]]},
    {"id": "shared-method", "body": "values = []\nalias = values\nalias.append(read_user())\nreturn send(values[0])",
     "sent": ["input"], "raised": None, "complete": False, "witness": False,
     "limits": [["dynamic-call", "alias.append(read_user())"]]},
    {"id": "finally-overrides-return", "body": "try:\n    return send(read_user())\nfinally:\n    return 0",
     "sent": ["input"], "raised": None, "complete": False, "witness": False,
     "limits": [["exception-handler", "try:"]]},
]


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def bindings():
    paths = [ROOT / "tools/python-effects.py", ROOT / "tools/test-python-effects.py", ROOT / "tools/evidence_basis.py"]
    return {**native_source_bindings(ROOT), **{str(path.relative_to(ROOT)): digest(path) for path in paths}}


def command(argv, directory, timeout):
    result = subprocess.run([str(item) for item in argv], cwd=directory, capture_output=True, timeout=timeout)
    require(len(result.stdout) + len(result.stderr) <= 1024**2, "effect check output exceeds 1 MiB")
    (directory.parent / (directory.name + "-last-command.json")).write_text(json.dumps({"argv": list(map(str, argv)),
        "exit_code": result.returncode, "stdout": result.stdout.decode(errors="replace"),
        "stderr": result.stderr.decode(errors="replace")}, indent=2) + "\n")
    require(result.returncode == 0, "effect check command failed: " + result.stderr.decode(errors="replace")[-8192:])
    return json.loads(result.stdout)


def source(example, renamed):
    text = "# A café keeps UTF-8 byte offsets distinct from character columns.\n"
    text += "def entry():\n" + "".join("    " + line + "\n" for line in example["body"].splitlines())
    return text.replace("read_user", "load_input").replace("send", "emit") if renamed else text


def runtime(program, value, renamed, directory):
    source_name, sink_name = ("load_input", "emit") if renamed else ("read_user", "send")
    driver = ('import json\nsent = []\n' + f'def source(): return {value}\n' +
              'def sink(value): sent.append(value); return value\n' +
              f'namespace = {{{source_name!r}: source, {sink_name!r}: sink}}\n' +
              f'exec({program!r}, namespace)\n' + 'raised = None\ntry:\n    namespace["entry"]()\n' +
              'except Exception as error:\n    raised = type(error).__name__\n' +
              'print(json.dumps({"sent": sent, "raised": raised}))\n')
    return command([sys.executable, "-I", "-B", "-c", driver], directory, 10)


def validate(example, report, executions, program, relative):
    require(report["complete"] == example["complete"], "unexpected scalar model completion")
    require(bool(report["witnesses"]) == example["witness"], "unexpected scalar witness")
    require(report["runtime_effects"]["exception_safety_established"] is False and
            report["runtime_effects"]["shared_mutable_values_supported"] is False,
            "scalar report claims unchecked runtime effects")
    for value, execution in zip((17, 31), executions, strict=True):
        wanted = [value if item == "input" else item for item in example["sent"]]
        require(execution == {"sent": wanted, "raised": example["raised"]}, "independent Python execution disagrees")
    for kind, fragment in example["limits"]:
        sites = [row for row in report["effect_limits"] if row["kind"] == kind]
        require(sites, "unchecked effect has no source location: " + kind)
        if relative != "subject.py":
            fragment = fragment.replace("read_user", "load_input").replace("send", "emit")
        located = False
        for site in sites:
            occurrence = site["occurrence"]
            span = occurrence["location"]["span"]
            start, end = span["start"], span["end"]
            selected = program.encode()[start:end].decode()
            require(occurrence["path"] == relative and selected, "effect location escapes the selected source")
            if fragment in selected:
                located = True
        require(located, "unchecked effect points at the wrong source construct")


def audit(directory):
    result = json.loads((directory / "result.json").read_text())
    require(result["passed"] is True and result["source_bindings"] == bindings(), "effect-check inputs changed")
    require(len(result["cases"]) == len(EXAMPLES) * 2 and result["live_model_calls"] == 0, "effect cases are missing")
    expected = {(example["id"], renamed) for example in EXAMPLES for renamed in (False, True)}
    require({(row["id"], row["renamed_and_moved"]) for row in result["cases"]} == expected, "effect case identities changed")
    for row in result["cases"]:
        example, = [item for item in EXAMPLES if item["id"] == row["id"]]
        for name, fingerprint in row["files"].items():
            path = Path(name)
            require(not path.is_absolute() and all(part not in {".", ".."} for part in path.parts), "unsafe effect file path")
            require(digest(directory / path) == fingerprint, "retained effect file changed")
        work = directory / row["directory"]
        require(all(str(Path(row["directory"]) / name) in row["files"]
                    for name in ("analysis.json", "runtime.json")), "effect records missing from file inventory")
        require(str(Path(row["directory"]) / row["source"]) in row["files"], "source missing from file inventory")
        program = (work / row["source"]).read_text()
        require(program == source(example, row["renamed_and_moved"]), "effect example changed")
        report = json.loads((work / "analysis.json").read_text())
        executions = json.loads((work / "runtime.json").read_text())
        validate(example, report, executions, program, row["source"])
    return result


def collect(binary, directory):
    require(os.environ.get("GITHUB_ACTIONS") == "true", "collect Python effects on GitHub")
    binary, directory = binary.resolve(), directory.resolve()
    directory.mkdir(parents=True, exist_ok=False)
    result = {"passed": False, "source_bindings": bindings(), "cases": [], "live_model_calls": 0,
              "scope": "Known scalar examples, including visible model/runtime disagreement; not complete exception or shared-heap support."}
    try:
        for example in EXAMPLES:
            for renamed in (False, True):
                work = directory / (example["id"] + ("-renamed" if renamed else "-original"))
                relative = "moved/worker.py" if renamed else "subject.py"
                path = work / relative
                path.parent.mkdir(parents=True)
                program = source(example, renamed)
                path.write_text(program)
                rules = {"version": "effects", "sources": ["load_input" if renamed else "read_user"],
                         "sinks": ["emit" if renamed else "send"], "propagators": [], "sanitizers": {}}
                (work / "rules.json").write_text(json.dumps(rules) + "\n")
                found = command([binary, "--json", "--no-cache", "-C", work, "project", "find", "entry", "--in", relative], work, 30)
                require(len(found["rows"]) == 1, "entry must resolve exactly once")
                report = command([binary, "--json", "--no-cache", "-C", work, "project", "dataflow", found["rows"][0][0],
                                  "--rules", work / "rules.json", "--summaries", "--imports", "--standard-library", "builtins",
                                  "--steps", "4096", "--depth", "8", "--bytes", "262144"], work, 30)
                executions = [runtime(program, value, renamed, work) for value in (17, 31)]
                (work / "analysis.json").write_text(json.dumps(report, indent=2) + "\n")
                (work / "runtime.json").write_text(json.dumps(executions, indent=2) + "\n")
                validate(example, report, executions, program, relative)
                files = [path, work / "rules.json", work / "analysis.json", work / "runtime.json"]
                result["cases"].append({"id": example["id"], "renamed_and_moved": renamed,
                    "directory": work.name, "source": relative, "difference": example.get("difference"),
                    "files": {str(path.relative_to(directory)): digest(path) for path in files}})
        result["passed"] = True
    except Exception as error:
        result["error"] = type(error).__name__ + ": " + str(error)
        raise
    finally:
        (directory / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    return audit(directory)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    capture = commands.add_parser("collect")
    capture.add_argument("--binary", type=Path, required=True)
    capture.add_argument("--output", type=Path, required=True)
    check = commands.add_parser("audit")
    check.add_argument("--directory", type=Path, required=True)
    args = parser.parse_args()
    result = collect(args.binary, args.output) if args.command == "collect" else audit(args.directory)
    print(json.dumps({"passed": result["passed"], "examples": len(result["cases"])}))


if __name__ == "__main__":
    main()
