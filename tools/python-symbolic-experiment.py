#!/usr/bin/env python3
"""Compare bounded integer checks without claiming agent savings or source proofs."""
import argparse
import ast
import hashlib
import importlib.metadata
import importlib.util
import json
import os
from pathlib import Path
import platform
import random
import subprocess
import sys
import time
import urllib.request

from agent_eval.bounded_host import run
from python_symbolic_cases import CASES, counterexample, require, wrappers

ROOT = Path(__file__).resolve().parents[1]
REVISION = "24ae1aad60dec4d9d5a0e88b8d8d692ee85f232e"
URL = f"https://codeload.github.com/dgilland/pydash/tar.gz/{REVISION}"
LIMITS = dict(wall_seconds=20, rss_bytes=512 * 1024**2, disk_bytes=16 * 1024**2,
              cpu_limit_seconds=10, transcript_bytes=1024**2)


def save(path, value):
    temporary = path.with_suffix(".tmp")
    temporary.write_text(json.dumps(value, indent=2) + "\n")
    temporary.replace(path)


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def prepare(output):
    # Reuse the established bounded archive admission, never extract arbitrary paths.
    spec = importlib.util.spec_from_file_location("repository_flow", ROOT / "tools/python-repository-flow.py")
    flow = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(flow)
    project = next(item for item in flow.COHORT["projects"] if item["id"] == "pydash")
    require(project["revision"] == REVISION, "pinned project revision changed")
    started = time.monotonic()
    with urllib.request.urlopen(URL, timeout=30) as response:
        archive = response.read(4 * 1024**2 + 1)
    (output / "pydash.tar.gz").write_bytes(archive)
    upstream = output / "upstream"
    upstream.mkdir()
    inventory = flow.snapshot(archive, project, upstream)
    source_path = upstream / "src/pydash/numerical.py"
    source = source_path.read_text()
    declarations = [node for node in ast.parse(source).body
                    if isinstance(node, ast.FunctionDef) and node.name == "clamp"]
    require(len(declarations) == 1 and not declarations[0].decorator_list,
            "clamp declaration is ambiguous")
    declaration = ast.get_source_segment(source, declarations[0])
    contexts = {}
    for context in ("full-package", "isolated-declaration"):
        scope = upstream / "src" if context == "full-package" else output / context
        scope.mkdir(exist_ok=True)
        if context == "full-package":
            # PYTHONPATH selects this original, unmodified package before installed packages.
            import_statement = "from pydash.numerical import clamp\n"
        else:
            # Keep the original body, removing annotations that reference the
            # omitted module's TypeVars. This is explicitly a simpler context.
            body = declaration.split("\n", 1)[1]
            (scope / "pinned_clamp.py").write_text("def clamp(x, lower, upper=None):\n" + body + "\n")
            import_statement = "from pinned_clamp import clamp\n"
        (scope / "checks.py").write_text(wrappers(import_statement))
        (scope / "trace.py").write_text(import_statement + "\ndef entry():\n    return record_value(clamp(emit_value(), 1, 10))\n")
        (scope / "rules.json").write_text(json.dumps({"version": "clamp-experiment", "sources": ["emit_value"],
                                                     "sinks": ["record_value"], "sanitizers": {}, "propagators": []}))
        contexts[context] = scope
    return contexts, {"repository": project["repository"], "revision": REVISION,
                      "archive_sha256": digest(output / "pydash.tar.gz"), "files": inventory,
                      "clamp_source": declaration, "setup_elapsed_seconds": time.monotonic() - started}


def execute(command, directory, name, env, *, cwd=None):
    temporary = directory / "tmp"
    temporary.mkdir(exist_ok=True)
    env = dict(env, TMPDIR=str(temporary))
    with (directory / (name + ".stdout")).open("wb") as stdout, (directory / (name + ".stderr")).open("wb") as stderr:
        result = run(command, b"", stdout, stderr, directory.parent, env=env, cwd=cwd, **LIMITS)
    result["command"] = command
    result["stdout"] = (directory / (name + ".stdout")).read_text()
    result["stderr"] = (directory / (name + ".stderr")).read_text()
    return result


def environment(scope, output):
    # No agent/provider credentials or user's Python customization reach the measured children.
    return {"PATH": os.environ["PATH"], "HOME": str(scope), "LANG": "C.UTF-8", "PYTHONHASHSEED": "0",
            "PYTHONDONTWRITEBYTECODE": "1",
            "PYTHONPATH": os.pathsep.join((str(scope), str(output / "upstream/src")))}


def trace(binary, scope, directory, env):
    found = execute([str(binary), "--json", "-C", str(scope), "project", "find", "entry", "--in", "trace.py"],
                    directory, "find", env)
    require(not found["stop_reason"] and found["exit_code"] == 0, "entry discovery failed")
    report = json.loads(found["stdout"])
    require(len(report["rows"]) == 1 and report["page"]["next"] is None, "entry discovery is ambiguous")
    handle = report["rows"][0][report["columns"].index("handle")]
    result = execute([str(binary), "--json", "-C", str(scope), "project", "dataflow", handle,
                      "--summaries", "--imports", "--rules", str(scope / "rules.json"),
                      "--context", "html", "--steps", "4096", "--depth", "8", "--bytes", "1048576"],
                     directory, "trace", env)
    return {"question": "Possible input-to-output connection; does not check the numeric contract.",
            "discovery": found, "analysis": result}


def collect(args):
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    contexts, upstream = prepare(output)
    result = {"purpose": "Find which checks help answer a numeric question; no agent or proof claim.",
              "source_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
              "source_files": {str(path.relative_to(ROOT)): digest(path) for path in
                               (Path(__file__).resolve(), ROOT / "tools/python_symbolic_cases.py",
                                ROOT / "tools/python-symbolic-worker.py", ROOT / "tools/agent_eval/bounded_host.py",
                                ROOT / ".github/workflows/python-symbolic.yml")},
              "analyzer_sha256": digest(args.fr.resolve()),
              "python": sys.version, "platform": platform.platform(),
              "packages": sorted((item.metadata["Name"], item.version) for item in importlib.metadata.distributions()),
              "upstream": upstream, "limits": LIMITS, "seed": 731, "repetitions": 2,
              "measurement_scope": "Elapsed includes fresh startup, imports, analysis, output and host monitoring. Sampled CPU/RSS cover the child process group, excluding the monitor. Installation, archive setup, build and replay are recorded separately.",
              "cases": [], "traces": {}}
    save(output / "result.json", result)
    schedule = [(context, case, method, repetition) for context in contexts for case in CASES
                for method in ("examples", "generated", "crosshair") for repetition in range(2)]
    random.Random(731).shuffle(schedule)
    errors = []
    for index, (context, case, method, repetition) in enumerate(schedule):
        scope = contexts[context]
        directory = output / f"attempt-{index:02d}"
        directory.mkdir()
        env = environment(scope, output)
        if method == "crosshair":
            command = [sys.executable, "-m", "crosshair", "check", f"checks.{case}", "--analysis_kind=PEP316",
                       "--report_all", "--per_path_timeout=1", "--per_condition_timeout=3"]
        else:
            command = [sys.executable, "-B", str(ROOT / "tools/python-symbolic-worker.py"), method, str(scope), case]
        observation = execute(command, directory, "check", env, cwd=scope)
        row = {"context": context, "case": case, "method": method, "repetition": repetition,
               "observation": observation, "status": "unfinished", "replay": None}
        result["cases"].append(row)
        # Save every first attempt before interpreting it, including parser errors or stops.
        save(output / "result.json", result)
        try:
            require(not observation["stop_reason"], "measurement stopped: " + str(observation["stop_reason"]))
            if method == "crosshair":
                require(observation["exit_code"] in (0, 1), "CrossHair failed to run")
                if observation["exit_code"] == 1:
                    arguments = counterexample(observation["stdout"], case)
                    replay = execute([sys.executable, "-B", str(ROOT / "tools/python-symbolic-worker.py"), "replay",
                                      str(scope), case, "--arguments", json.dumps(arguments)], directory, "replay", env, cwd=scope)
                    row["replay"] = replay
                    require(not replay["stop_reason"] and replay["exit_code"] == 0,
                            "counterexample replay did not finish")
                    observed = json.loads(replay["stdout"])
                    require(observed["checked"] == 1 and len(observed["failures"]) == 1,
                            "reported counterexample did not reproduce")
                    row["status"] = "reproduced-counterexample"
                else:
                    require("info:" in observation["stdout"], "CrossHair returned no check result")
                    row["status"] = "no-counterexample-reported"
            else:
                require(observation["exit_code"] == 0, "concrete checks failed to run")
                concrete = json.loads(observation["stdout"])
                require(concrete["checked"] > 0, "no admitted concrete inputs checked")
                row["status"] = "counterexample-found" if concrete["failures"] else "sampled-inputs-passed"
            if case == "unchecked_bounds":
                require(row["status"] in ("counterexample-found", "reproduced-counterexample"),
                        "the deliberately overbroad requirement was not challenged")
            else:
                require(row["status"] in ("sampled-inputs-passed", "no-counterexample-reported"),
                        "an expected contract disagrees with runtime")
        except (ValueError, SyntaxError, KeyError, json.JSONDecodeError) as error:
            row["error"] = str(error)
            errors.append(str(error))
        save(output / "result.json", result)
        print(f'{index + 1}/{len(schedule)} {context} {case} {method}: {row["status"]}', flush=True)
    for context, scope in contexts.items():
        directory = output / (context + "-trace")
        directory.mkdir()
        try:
            result["traces"][context] = trace(args.fr.resolve(), scope, directory, environment(scope, output))
        except (ValueError, KeyError, json.JSONDecodeError) as error:
            result["traces"][context] = {"error": str(error)}
            errors.append(str(error))
        save(output / "result.json", result)
    result["errors"] = errors
    save(output / "result.json", result)
    require(not errors, "experiment controls failed; see preserved first attempts")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--fr", type=Path, default=ROOT / "target/debug/fr")
    collect(parser.parse_args())
