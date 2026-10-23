#!/usr/bin/env python3
"""Check declared Rust/Python translations against independent integer arithmetic."""
import argparse
import hashlib
import itertools
import json
import os
from pathlib import Path
import subprocess
import sys

from evidence_basis import native_source_bindings

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "tests/agent-eval/checked-translation"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def bindings():
    names = ["tools/evidence_basis.py", "tools/checked-translation.py", "tools/test-checked-translation.py"]
    paths = [ROOT / name for name in names]
    paths += sorted(FIXTURE.iterdir())
    return {**native_source_bindings(ROOT),
            **{str(path.relative_to(ROOT)): digest(path) for path in paths if path.is_file()}}


def coverage(contract):
    spans = contract["exhaustive"]
    return {"exhaustive_pairs": (spans["items"][1] - spans["items"][0] + 1) *
                               (spans["size"][1] - spans["size"][0] + 1),
            "boundary_pairs": len(contract["boundary_pairs"]), "directions": len(contract["directions"]),
            "renamed_and_moved": True}


def cases(contract):
    spans = [range(contract["exhaustive"][name][0], contract["exhaustive"][name][1] + 1)
             for name in contract["parameters"]]
    values = list(itertools.product(*spans)) + [tuple(pair) for pair in contract["boundary_pairs"]]
    require(len(values) == len(set(values)), "duplicate declared input pair")
    require(all(type(n) is int and -(2**63) <= n < 2**63 for pair in values for n in pair),
            "inputs must be signed 64-bit integers")
    return values


def expected(pairs):
    # This mathematical formula deliberately differs from both implementation bodies.
    # Python integers do not overflow when items + size - 1 exceeds i64.
    return "".join(str(-1 if items < 0 or size <= 0 else (items + size - 1) // size) + "\n"
                   for items, size in pairs).encode()


def run(argv, directory, data, timeout, transcript):
    process = subprocess.run([str(item) for item in argv], cwd=directory, input=data,
                             capture_output=True, timeout=timeout)
    require(len(process.stdout) + len(process.stderr) <= 1024**2, "command output exceeds 1 MiB")
    transcript.append({"argv": [str(item) for item in argv], "exit_code": process.returncode,
                       "stdout": process.stdout.decode(errors="replace"),
                       "stderr": process.stderr.decode(errors="replace")})
    require(process.returncode == 0, "command failed: " + " ".join(map(str, argv)) + "\n" +
            process.stderr.decode(errors="replace")[-8192:])
    return process.stdout


def execute(source, function, data, transcript):
    if source.suffix == ".rs":
        driver = source.with_suffix(".driver.rs")
        driver.write_text('#[path = ' + json.dumps(str(source)) + '] mod subject;\n' +
            'use std::io::{self, BufRead};\nfn main() {\n' +
            'for line in io::stdin().lock().lines() {\nlet line = line.unwrap();\n' +
            'let values: Vec<i64> = line.split_whitespace().map(|v| v.parse().unwrap()).collect();\n' +
            f'println!("{{}}", subject::{function}(values[0], values[1]));\n' + '}}\n')
        binary = source.with_suffix(".bin")
        run(["rustc", "--edition=2021", "--crate-name", "translation_check", "-C", "overflow-checks=yes",
             "-C", "opt-level=0", driver, "-o", binary], source.parent, b"", 30, transcript)
        try:
            return run([binary], source.parent, data, 30, transcript)
        finally:
            binary.unlink(missing_ok=True)
    driver = ('import runpy, sys\n' + f'function = runpy.run_path({str(source)!r})[{function!r}]\n' +
              'for line in sys.stdin:\n' + '    items, size = map(int, line.split())\n' +
              '    result = function(items, size)\n' +
              '    if type(result) is not int: raise TypeError("non-integer result")\n' +
              '    print(result)\n')
    return run([sys.executable, "-I", "-B", "-c", driver], source.parent, data, 30, transcript)


def audit(directory):
    contract = json.loads((FIXTURE / "contract.json").read_text())
    pairs = cases(contract)
    result = json.loads((directory / "result.json").read_text())
    require(result["schema"] == "fr-checked-translation" and result["passed"] is True and
            result["source_bindings"] == bindings(), "translation inputs changed")
    require(result["coverage"] == coverage(contract), "translation coverage changed")
    require(result["input_pairs"] == [list(pair) for pair in pairs], "declared inputs were omitted or changed")
    require(result["live_model_calls"] == 0 and result["runtime_benchmark"] is False, "unsupported comparison claim")
    answer = expected(pairs)
    wanted = {(direction, renamed) for direction in contract["directions"] for renamed in (False, True)}
    require(len(result["translations"]) == len(wanted) and
            {(row["direction"], row["renamed_and_moved"]) for row in result["translations"]} == wanted,
            "translation directions or renamed examples missing")
    for row in result["translations"]:
        require(row["passed"] is True and row["checked_inputs"] == len(pairs), "translation did not finish")
        for name, fingerprint in row["files"].items():
            path = Path(name)
            require(not path.is_absolute() and all(part not in {".", ".."} for part in path.parts), "unsafe record path")
            require(digest(directory / path) == fingerprint, "retained translation file changed")
        for key in ("source_answers", "translated_answers"):
            require(row[key] in row["files"], "answer is not part of the retained file inventory")
            require((directory / row[key]).read_bytes() == answer, "answers disagree with independent arithmetic")
        require(row["commands"] in row["files"], "commands are not part of the retained file inventory")
        commands = json.loads((directory / row["commands"]).read_text())
        require(commands and all(command["exit_code"] == 0 for command in commands), "translation command failed")
    return result


def collect(binary, directory):
    require(os.environ.get("GITHUB_ACTIONS") == "true", "collect translation results on GitHub")
    directory.mkdir(parents=True, exist_ok=False)
    directory = directory.resolve()
    binary = binary.resolve()
    contract = json.loads((FIXTURE / "contract.json").read_text())
    pairs = cases(contract)
    data = "".join(f"{items} {size}\n" for items, size in pairs).encode()
    answer = expected(pairs)
    result = {"schema": "fr-checked-translation", "passed": False, "source_bindings": bindings(), "input_pairs": pairs,
              "source_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True, timeout=5).strip(),
              "coverage": coverage(contract),
              "live_model_calls": 0, "runtime_benchmark": False, "translations": [],
              "scope": contract["scope"], "trusted": ["fr translation and parser", "rustc with overflow checks",
                  "Python integer arithmetic and interpreter", "collector and retained-output audit"]}
    active_work, transcript = None, []
    try:
        versions = []
        result["versions"] = {name: run(argv, directory, b"", 30, versions).decode().strip()
                              for name, argv in (("fr", [binary, "--version"]), ("rustc", ["rustc", "--version"]),
                                                 ("python", [sys.executable, "--version"]))}
        result["binary_sha256"] = digest(binary)
        for direction in contract["directions"]:
            source_ext, target_ext, target_lang = (("rs", "py", "python") if direction == "rust-to-python"
                                                  else ("py", "rs", "rust"))
            for renamed in (False, True):
                work = directory / (direction + ("-renamed/moved" if renamed else "-original"))
                work.mkdir(parents=True)
                active_work = work
                function = "total_pages" if renamed else contract["function"]
                source, target = work / ("calculation." + source_ext), work / ("translated." + target_ext)
                source.write_text((FIXTURE / ("pages." + source_ext)).read_text().replace(contract["function"], function))
                transcript = []
                found = run([binary, "--json", "--no-cache", "-C", work, "project", "find", function,
                             "--in", source.name, "--signature"], work, b"", 30, transcript)
                require(len(json.loads(found)["rows"]) == 1, "fr must find the exact source function")
                command = [binary, "--json", "--no-cache", "-C", work, "translate", source, target_lang, "--out", target]
                preview = run(command, work, b"", 30, transcript)
                require(not target.exists(), "translation preview wrote its destination")
                report = json.loads(run([*command, "--write"], work, b"", 30, transcript))
                require(report["carried_verbatim"] == 0 and report["signatures_complete"] == 1 and
                        report["signatures_with_foreign_types"] == 0, "translation carried unsupported code")
                (work / "preview.json").write_bytes(preview)
                source_answers, translated_answers = work / "source-answers.txt", work / "translated-answers.txt"
                source_answers.write_bytes(execute(source, function, data, transcript))
                translated_answers.write_bytes(execute(target, function, data, transcript))
                require(source_answers.read_bytes() == answer and translated_answers.read_bytes() == answer,
                        "implementation differs from independent arithmetic")
                commands = work / "commands.json"
                commands.write_text(json.dumps(transcript, indent=2) + "\n")
                names = [source, target, work / "preview.json", source_answers, translated_answers, commands]
                row = {"direction": direction, "renamed_and_moved": renamed, "passed": True,
                       "checked_inputs": len(pairs), "source_answers": str(source_answers.relative_to(directory)),
                       "translated_answers": str(translated_answers.relative_to(directory)),
                       "commands": str(commands.relative_to(directory)),
                       "files": {str(path.relative_to(directory)): digest(path) for path in names}}
                result["translations"].append(row)
        result["passed"] = True
    except Exception as error:
        result["error"] = type(error).__name__ + ": " + str(error)
        raise
    finally:
        if active_work is not None:
            (active_work / "commands.json").write_text(json.dumps(transcript, indent=2) + "\n")
        (directory / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    return audit(directory)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    subcommands = parser.add_subparsers(dest="command", required=True)
    capture = subcommands.add_parser("collect")
    capture.add_argument("--binary", type=Path, required=True)
    capture.add_argument("--output", type=Path, required=True)
    check = subcommands.add_parser("audit")
    check.add_argument("--directory", type=Path, required=True)
    args = parser.parse_args()
    result = collect(args.binary, args.output) if args.command == "collect" else audit(args.directory)
    print(json.dumps({"passed": result["passed"], "translations": len(result["translations"]),
                      "inputs_per_translation": len(result["input_pairs"])}))


if __name__ == "__main__":
    main()
