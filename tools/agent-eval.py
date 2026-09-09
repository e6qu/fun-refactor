#!/usr/bin/env python3
"""Prepare, instrument and score fresh-agent tasks on a pinned public source release."""

import argparse
import hashlib
import importlib.metadata
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import time


from agent_eval.oracle import verify as verify_strsim
from agent_eval import regex_workspace, regex_escape_len


TRIAL_FILES = ("session.json", "prompt.txt", "events.jsonl", "result.json")
CODEX_PROVENANCE_FILES = (
    "codex-events.jsonl",
    "codex-stderr.txt",
    "codex-final.txt",
    "codex-run.json",
)

ROOT = Path(__file__).resolve().parent.parent
ARCHIVE = ROOT / "tests/agent-eval/strsim-0.11.1.crate"
ARCHIVE_SHA = "7da8b5736845d9f2fcb837ea5d9e2628564b3b043a70948a3f0b778838c5fb4f"
TASKS = {
    "unicode-dice": "Fix Sørensen–Dice similarity for Unicode inputs. Use Unicode scalar-value bigrams and their counts consistently. Preserve whitespace removal, duplicate-bigram multiplicity, equal-input behavior and the public API. Unequal inputs with fewer than two scalars after whitespace removal must score zero. Keep existing ASCII behavior.",
    "normalized-osa": "Add a public normalized_osa_distance(a: &str, b: &str) -> f64 API. Return one minus optimal-string-alignment distance divided by the longer input's Unicode scalar count. Two empty inputs score one. Preserve OSA's restricted transposition semantics and all existing APIs. Match the neighboring normalized APIs; reuse existing distance machinery when suitable.",
}

STRSIM_TASKS = tuple(TASKS)
TASKS[regex_workspace.TASK] = regex_workspace.DESCRIPTION
TASKS[regex_escape_len.TASK] = regex_escape_len.DESCRIPTION
REGEX_TASKS = (regex_workspace.TASK, regex_escape_len.TASK)


def edit_paths(task):
    return regex_escape_len.PATHS if task == regex_escape_len.TASK else ("src/lib.rs",)


def required_checks(task):
    return [check["name"] for check in regex_workspace.CHECKS] if task in REGEX_TASKS else ()


def profile(task):
    if task in REGEX_TASKS:
        return {"project": "rust-lang/regex complete workspace snapshot, without Git history",
                "archive_sha256": regex_workspace.ARCHIVE_SHA, "upstream_commit": regex_workspace.COMMIT,
                "checks": regex_workspace.CHECKS, "dependency_lock_sha256": regex_workspace.LOCK_SHA}
    if task not in STRSIM_TASKS:
        raise ValueError("Unknown evaluation task")
    return {"project": "rapidfuzz/strsim-rs published crate 0.11.1; complete release snapshot, without Git history",
            "archive_sha256": ARCHIVE_SHA, "upstream_commit": "76c5a900e6e12cfc605eee5ab6e36300384c8682",
            "checks": [{"name": "upstream", "argv": ["cargo", "test", "--offline"], "cwd": ".",
                        "timeout_seconds": 120, "covers": ["Unmodified upstream unit, integration and documentation tests"]}]}


def verify(root, task):
    if task == regex_escape_len.TASK:
        return regex_escape_len.verify(root)
    return regex_workspace.verify(root) if task == regex_workspace.TASK else verify_strsim(root, task)


def upstream(root, task):
    results = [process(check["argv"], root / check["cwd"]) for check in profile(task)["checks"]]
    return {"exit_code": next((result["exit_code"] for result in results if result["exit_code"] != 0), 0), "checks": results}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def save(path, value):
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n")


def git(root, *args, data=None, check=True):
    env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
    env.update(GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=os.devnull, GIT_OPTIONAL_LOCKS="0")
    result = subprocess.run(["git", "-c", "user.name=fr acceptance", "-c", "user.email=fr@example.invalid",
                             "-c", "core.hooksPath=/dev/null", *args], cwd=root, env=env,
                            input=data, capture_output=True, timeout=60)
    if check and result.returncode:
        raise RuntimeError(result.stderr.decode(errors="replace"))
    return result


def unpack(destination, task="unicode-dice"):
    if task in REGEX_TASKS:
        return regex_workspace.unpack(destination)
    data = ARCHIVE.read_bytes()
    if digest(data) != ARCHIVE_SHA:
        raise ValueError("Pinned source archive checksum mismatch")
    destination.mkdir()
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
        for member in archive.getmembers():
            parts = Path(member.name).parts
            if parts[0] != "strsim-0.11.1" or ".." in parts or member.issym() or member.islnk():
                raise ValueError("Unsafe source archive member")
            path = destination.joinpath(*parts[1:])
            if member.isdir():
                path.mkdir(parents=True, exist_ok=True)
            elif member.isfile():
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(archive.extractfile(member).read())
                path.chmod(member.mode & 0o777)
            else:
                raise ValueError("Unsupported source archive member")


def snapshot(root):
    names = git(root, "ls-files", "-z").stdout.decode().split("\0")
    return {name: {"sha256": digest((root / name).read_bytes()), "mode": (root / name).stat().st_mode & 0o777}
            for name in names if name}


def initialize(root):
    git(root, "init", "-q", "-b", "main")
    git(root, "add", ".")
    if (root / "Cargo.lock").is_file():
        git(root, "add", "-f", "Cargo.lock")
    git(root, "commit", "-qm", "Pinned public release with evaluation check declarations")


def prompt(session, task, arm):
    surface = (
        "Use fr for source exploration and edits. Start with the instrumented call "
        "{\"tool\":\"read\",\"path\":\"skill/SKILL.md\",\"start\":1,\"lines\":80}. "
        "Load only needed references. Source read/search/replace tools are unavailable in this arm. "
        "Use fr project handles and saved authoring plans. Write fragments under artifacts/ outside the project."
        if arm == "fr" else
        "Use ordinary files, read, search and replace tools for source exploration and edits. Do not use fr project/author/history commands. The shared fr checks command is available for identical project validation. Export and reverse/reapply your patch through the ordinary Git tools."
    )
    if task == regex_escape_len.TASK and arm == "fr":
        surface += " Before constructing the batch manifest, use the instrumented call {\"tool\":\"read\",\"path\":\"skill/references/author.md\",\"start\":1,\"lines\":160}. Coordinate the edits in one author batch saved transaction, and export, undo and redo that transaction. The manifest must contain both API insertion operations and the regex-syntax escape body replacement, with exact files-changed, edits, changed-operations and paths-changed postconditions. Preview the batch once without a mutation flag, call it once with --save-plan, then apply the saved transaction with history apply --write; do not pass --write to author batch. Retain the first full project context_basis and use it on related project and author calls. Retain the complete author diff and its transaction_context_basis; use that basis to compact forward apply and redo reports. Preview reverse transitions in full."
    return f"""You are an independent acceptance-test agent. Complete this code task in the supplied unfamiliar pinned public project: {TASKS[task]}

{surface}

Only change {', '.join(edit_paths(task))}. The evaluator owns regression oracles; do not read or modify the harness, oracle, other sessions, or hidden tests. You may add a focused function-level implementation but need not alter project documentation for this trial. Do not browse, delegate, commit or push. No human correction is available.

Use only the instrumented tool for all task inspection and work. Invoke it using functions.exec / tools.exec_command:
python3 {ROOT / 'tools/agent-eval.py'} step {session} '<JSON object>'
Use the positional JSON form for simple requests. Use this stdin form for every write request so source apostrophes and shell metacharacters survive unchanged:
python3 {ROOT / 'tools/agent-eval.py'} step {session} --request-stdin <<'FRJSON'
{{"tool":"write","path":"fragment.rs","text":"{{\\n    buf.push('\\\\\\\\');\\n}}"}}
FRJSON
You may batch independent instrumented calls. Do not directly read or write the project or artifacts through other tools. This is a cooperative measurement boundary, not an OS sandbox.

Tool objects:
{{"tool":"files","path":"."}} lists up to 200 paths (baseline only).
{{"tool":"read","path":"README.md","start":1,"lines":80}} reads up to 200 lines. In the fr arm use project show for source; read can access README.md, Cargo.toml, .fr/checks.json and skill/ references.
{{"tool":"search","pattern":"normalized","path":"src"}} runs literal rg with bounded output (baseline only).
{{"tool":"replace","path":"src/lib.rs","old":"unique existing text","new":"replacement"}} requires exactly one match (baseline only).
{{"tool":"append","path":"src/lib.rs","text":"new function text"}} appends source (baseline only).
{{"tool":"write","path":"fragment.rs","text":"{{ replacement block }}"}} writes artifacts/fragment.rs and returns its absolute path (both arms).
{{"tool":"fr","args":["checks"]}} invokes fr with the project root, JSON and no cache. Use this for checks in both arms and project/author/history/git in the fr arm. --help is available.
{{"tool":"export"}} saves and shows a Git diff as artifacts/change.patch (baseline only). In the fr arm, use history patch TX --output ../artifacts/change.patch to retain the patch while returning only its identity and size.
{{"tool":"reverse"}} / {{"tool":"apply"}} reverses/reapplies that saved Git patch (baseline only). Apply refuses until all declared checks pass on the state restored by reverse.
{{"tool":"sentinel"}} adds an unrelated edit after the requested change; it must survive reversal and reapplication.
{{"tool":"receiver"}} checks and applies the saved patch in a clean separate receiver and compares tracked content with your project. It refuses until all declared checks pass after the final redo/apply.
{{"tool":"finish","summary":"..."}} records your final conclusion; independent oracles run later.

Workflow: inspect; list and run declared checks on the original; implement the task; run checks on the change; export the patch; add the sentinel; undo and check; redo and check; verify the receiver; finish. fr arm: preview/save/apply an authoring transaction and use history undo/redo. Run all declared checks together at each validation stage using --run with comma-separated names; every run needs the configuration basis from its listing. Keep project handles revision-bound when using them. Keep tool output bounded and request only relevant context. Leave the requested change applied. Report uncertainty and tool refusals honestly.

Execute each successful workflow step once. Do not repeat a successful skill read, listing, saved plan, mutation, patch export, check, or receiver call. Preserve the order above, including the original-state check before any edit and the final-state check before receiver verification.

For every successful check run, pass --quiet-success --no-declarations --output-bytes 2048 after reviewing the listing. Failure diagnostics stay bounded and visible. The instrumented fr tool disables its fact cache for every arm and stage; do not add a separate cache warm-up or change that policy.

The harness records visible tool payload tokens, calls, latency and final correctness. It does not measure your hidden reasoning, system context or billed tokens. The task directory is {session / 'project'}.
"""


def trial_names(project, repetitions):
    if project not in ("strsim", "regex", "regex-coordinated") or not 1 <= repetitions <= 8:
        raise ValueError("Choose strsim, regex or regex-coordinated and 1..8 repetitions")
    tasks = STRSIM_TASKS if project == "strsim" else ((regex_workspace.TASK,) if project == "regex" else (regex_escape_len.TASK,))
    return [(f"{task}-{arm}" + (f"-r{repeat}" if repetitions > 1 else ""), task, arm, repeat)
            for repeat in range(1, repetitions + 1) for task in tasks for arm in ("fr", "files")]


def prepare(out, binary, project="strsim", repetitions=1):
    names = trial_names(project, repetitions)
    if any((parent / "Cargo.toml").is_file() for parent in [out, *out.parents]):
        raise ValueError("Prepare outside Cargo projects, for example under /tmp, to avoid inherited workspace membership")
    out.mkdir(parents=True, exist_ok=False)
    save(out / "experiment.json", {"project": project, "repetitions": repetitions, "trials": [name for name, _, _, _ in names]})
    for name, task, arm, repeat in names:
        session = out / name
        session.mkdir()
        prepare_trial(session, binary, task, arm, repeat)
    print(json.dumps({"sessions": [str(out / name) for name, _, _, _ in names]}))


def prepare_trial(session, binary, task, arm, repetition):
    selected = profile(task)
    project = session / "project"
    unpack(project, task)
    (project / ".fr").mkdir()
    save(project / ".fr/checks.json", {"schema": 1, "checks": selected["checks"]})
    initialize(project)
    preflight = upstream(project, task)
    if preflight["exit_code"]:
        raise RuntimeError(f"Upstream preflight failed: {preflight}")
    original_oracle = verify(project, task)
    expected_stage = 2 if task == "unicode-dice" else 1
    if original_oracle["passed"] or original_oracle.get("stage") != expected_stage:
        raise RuntimeError(f"Original oracle failed to establish the expected task baseline: {original_oracle}")
    receiver = session / "receiver"
    ignored = (".git", "target") if task in REGEX_TASKS else (".git", "target", "Cargo.lock")
    shutil.copytree(project, receiver, ignore=shutil.ignore_patterns(*ignored))
    initialize(receiver)
    (session / "artifacts").mkdir()
    shutil.copytree(ROOT / "skills/fr", session / "skill")
    source_files = [path for path in project.rglob("*.rs") if "target" not in path.relative_to(project).parts]
    save(session / "session.json", {
        "schema": "fr-agent-eval-session-1", "task": task, "arm": arm, "repetition": repetition,
        "archive_sha256": selected["archive_sha256"], "upstream_commit": selected["upstream_commit"],
        "dependency_lock_sha256": selected.get("dependency_lock_sha256"),
        "fr": str(binary.resolve()), "binary_sha256": digest(binary.read_bytes()),
        "original": snapshot(project), "index_sha256": digest((project / ".git/index").read_bytes()),
        "receiver_index_sha256": digest((receiver / ".git/index").read_bytes()),
        "source_bytes": sum(path.stat().st_size for path in source_files), "created_at": time.time(),
        "original_oracle": original_oracle, "preflight_passed": True,
    })
    (session / "prompt.txt").write_text(prompt(session, task, arm))


def within(root, name):
    candidate = root / name
    path = candidate.resolve()
    if not path.is_relative_to(root.resolve()):
        raise ValueError("Path leaves the selected directory")
    return path


def process(argv, cwd):
    env = os.environ.copy()
    env.update(CARGO_HOME=str(ROOT / "target/cargo-home"), CARGO_NET_OFFLINE="true")
    result = subprocess.run(argv, cwd=cwd, env=env, capture_output=True, timeout=180)
    out, err = result.stdout, result.stderr
    try:
        value = json.loads(out) if len(out) <= 20000 else None
    except (ValueError, UnicodeDecodeError):
        value = None
    return {"exit_code": result.returncode,
            "result": value if value is not None else out[:20000].decode(errors="replace"),
            "stdout_omitted_bytes": max(0, len(out) - 20000),
            "stderr": err[:4096].decode(errors="replace"), "stderr_omitted_bytes": max(0, len(err) - 4096)}


def category(request):
    kind = request["tool"]
    if kind == "read" and request.get("path", "").startswith("skill/"):
        return "skill"
    if kind in ("files", "read", "search"):
        return "inspection"
    if kind == "fr":
        command = request["args"][0]
        if command == "checks":
            return "checks"
        if command == "project":
            return "inspection"
    return "change_and_delivery"


def validate_coordinated_manifest(session, project, config, args):
    if config["task"] != regex_escape_len.TASK or args[:2] != ["author", "batch"]:
        return
    try:
        source = args[args.index("--from") + 1]
    except (ValueError, IndexError):
        raise ValueError("The coordinated author batch requires --from MANIFEST") from None
    manifest = json.loads(within(session / "artifacts", (project / source).resolve()).read_text())
    operations = manifest.get("operations")
    if not isinstance(operations, list):
        raise ValueError("The coordinated manifest requires an operations array")
    kinds = [operation.get("op") for operation in operations if isinstance(operation, dict)]
    postconditions = manifest.get("postconditions")
    expected = {
        "files-changed": 2,
        "edits": len(operations),
        "changed-operations": len(operations),
        "paths-changed": list(edit_paths(config["task"])),
    }
    if (len(operations) < 3 or kinds.count("insert-declaration") < 2
            or "replace-body" not in kinds or postconditions != expected):
        raise ValueError(
            "The one coordinated batch must include both API insertions and the escape body "
            "replacement, with exact files-changed, edits, changed-operations and paths-changed postconditions"
        )


def action(session, config, request):
    project = session / "project"
    kind = request["tool"]
    baseline = config["arm"] == "files"
    if kind in ("files", "search", "replace", "append", "export", "reverse", "apply") and not baseline:
        raise ValueError("This tool belongs to the ordinary-file arm")
    if kind == "files":
        result = process(["rg", "--files", str(within(project, request.get("path", ".")))], project)
        lines = result["result"].splitlines()
        if len(lines) > 200:
            result["result"] = "\n".join(lines[:200])
            result["omitted_paths"] = len(lines) - 200
        return result
    if kind == "read":
        name = request["path"]
        skill = name.startswith("skill/")
        if not baseline and not skill and name not in ("README.md", "Cargo.toml", ".fr/checks.json"):
            raise ValueError("Use fr project show for source in this arm")
        path = within(session if skill else project, name)
        start, count = request.get("start", 1), request.get("lines", 80)
        if start < 1 or not 1 <= count <= 200:
            raise ValueError("Read requires positive start and 1..200 lines")
        lines = path.read_text().splitlines(keepends=True)
        text = "".join(f"{i+1}: {line}" for i, line in enumerate(lines) if start - 1 <= i < start - 1 + count)
        return {"text": text[:20000], "total_lines": len(lines), "next_line": start + count if start + count <= len(lines) else None}
    if kind == "search":
        return process(["rg", "-n", "-F", "--", request["pattern"], str(within(project, request.get("path", ".")))], project)
    if kind in ("replace", "append"):
        if request["path"] not in edit_paths(config["task"]):
            raise ValueError("Trial edits are limited to " + ", ".join(edit_paths(config["task"])))
        path = within(project, request["path"])
        text = path.read_text()
        if kind == "replace":
            old = request["old"]
            if not old or text.count(old) != 1:
                raise ValueError("Replacement needs exactly one nonempty match")
            text = text.replace(old, request["new"], 1)
        else:
            text += request["text"]
        path.write_text(text)
        return {"changed": True, "source_sha256": digest(path.read_bytes())}
    if kind == "write":
        path = within(session / "artifacts", request["path"])
        if not path.parent.is_dir() or path.name == "change.patch":
            raise ValueError("Use an existing artifact directory and a fragment filename")
        path.write_text(request["text"])
        return {"path": str(path), "bytes": path.stat().st_size}
    if kind == "fr":
        args = request["args"]
        if not args or (baseline and args[0] != "checks"):
            raise ValueError("Only fr checks is shared with the ordinary-file arm")
        validate_coordinated_manifest(session, project, config, args)
        if args[:2] == ["history", "redo"]:
            events = [json.loads(line) for line in (session / "events.jsonl").read_text().splitlines()]
            if not current_state_checked(events, snapshot(project), required_checks(config["task"])):
                raise ValueError("Run all declared checks on the state restored by undo before redo")
        if config["binary_sha256"] != digest(Path(config["fr"]).read_bytes()):
            raise ValueError("Trial binary changed after preparation")
        result = process([config["fr"], "--no-cache", "--json", "-C", str(project), *args], project)
        report = result["result"]
        if isinstance(report, dict) and isinstance(report.get("patch"), str) and args[:2] == ["history", "patch"]:
            (session / "artifacts/change.patch").write_text(report["patch"])
            result["patch_artifact"] = "artifacts/change.patch"
        return result
    if kind == "export":
        result = git(project, "diff", "--binary", "--", *edit_paths(config["task"]))
        (session / "artifacts/change.patch").write_bytes(result.stdout)
        return {"patch": result.stdout.decode(), "patch_artifact": "artifacts/change.patch"}
    if kind in ("reverse", "apply"):
        if kind == "apply":
            events = [json.loads(line) for line in (session / "events.jsonl").read_text().splitlines()]
            if not current_state_checked(events, snapshot(project), required_checks(config["task"])):
                raise ValueError("Run all declared checks on the state restored by reverse before reapplying")
        args = ["apply", "--reverse"] if kind == "reverse" else ["apply"]
        result = git(project, *args, data=(session / "artifacts/change.patch").read_bytes(), check=False)
        return {"exit_code": result.returncode, "stderr": result.stderr.decode()}
    if kind == "sentinel":
        (project / "unrelated.txt").write_text("Preserve this independent later edit.\n")
        return {"created": "unrelated.txt"}
    if kind == "receiver":
        events = [json.loads(line) for line in (session / "events.jsonl").read_text().splitlines()]
        if not current_state_checked(events, snapshot(project), required_checks(config["task"])):
            raise ValueError("Run all declared checks on the final reapplied state before receiver verification")
        receiver = session / "receiver"
        patch = (session / "artifacts/change.patch").read_bytes()
        git(receiver, "apply", "--check", "--index", data=patch)
        git(receiver, "apply", data=patch)
        return {"patch_applied": True, "matches": snapshot(receiver) == snapshot(project)}
    if kind == "finish":
        return {"finished": True, "summary": request["summary"]}
    raise ValueError("Unknown instrumented tool")


def step(session, request):
    config = json.loads((session / "session.json").read_text())
    before = snapshot(session / "project")
    started = time.time()
    try:
        result = action(session, config, request)
    except (ValueError, OSError, subprocess.SubprocessError, RuntimeError) as error:
        result = {"error": str(error), "exit_code": 1}
    rendered = json.dumps(result, ensure_ascii=False)
    event = {"request": request, "visible": rendered, "started_at": started,
             "elapsed_seconds": time.time() - started, "before": before, "after": snapshot(session / "project"),
             "index_sha256": digest((session / "project/.git/index").read_bytes()),
             "sentinel": (session / "project/unrelated.txt").read_text() if (session / "project/unrelated.txt").is_file() else None}
    with (session / "events.jsonl").open("a") as log:
        log.write(json.dumps(event, ensure_ascii=False) + "\n")
    print(rendered)


def request_input(argument, use_stdin, stream):
    if use_stdin == (argument is not None):
        raise ValueError("provide exactly one request source: positional JSON or --request-stdin")
    value = json.loads(stream.read() if use_stdin else argument)
    if not isinstance(value, dict):
        raise ValueError("request must be a JSON object")
    return value


def tokenizer():
    if importlib.metadata.version("tiktoken") != "0.12.0":
        raise ValueError("Install the pinned tools/agent_eval/requirements.txt")
    os.environ.setdefault("TIKTOKEN_CACHE_DIR", str(ROOT / "target/agent-eval-tokenizer"))
    vocabulary = Path(os.environ["TIKTOKEN_CACHE_DIR"]) / "fb374d419588a4632f3f557e76b4b70aebbca790"
    if not vocabulary.is_file() or digest(vocabulary.read_bytes()) != "446a9538cb6c348e3516120d7c08b09f57c36495e2acfffe59a5bf8b0cfb1a2d":
        raise ValueError("Fetch and verify the pinned o200k_base vocabulary before offline scoring")
    import tiktoken
    return tiktoken.get_encoding("o200k_base")


def workflow(events, original, final, required_checks=()):
    checks = []
    undo, redo, receivers = [], [], []
    for index, event in enumerate(events):
        result = json.loads(event["visible"])
        report = result.get("result")
        if isinstance(report, dict) and report.get("schema") == "fr-checks-1" and report.get("executed"):
            checks.append({"event": index, "passed": report["passed"] and result.get("exit_code") == 0
                           and set(required_checks).issubset({check["name"] for check in report.get("results", []) if check.get("passed")}),
                           "original": event["before"] == event["after"] == original,
                           "final": event["before"] == event["after"] == final})
        tool, args = event["request"].get("tool"), event["request"].get("args", [])[:2]
        sentinel = event["sentinel"] == "Preserve this independent later edit.\n"
        if (tool == "reverse" or args == ["history", "undo"]) and sentinel and event["before"] == final and event["after"] == original:
            undo.append(index)
        if (tool == "apply" or args == ["history", "redo"]) and sentinel and event["before"] == original and event["after"] == final:
            redo.append(index)
        if tool == "receiver" and result.get("patch_applied") and result.get("matches"):
            receivers.append(index)
    initial_checks = [c["event"] for c in checks if c["passed"] and c["original"]]
    final_checks = [c["event"] for c in checks if c["passed"] and c["final"]]
    ordered = any(
        any(c < changed for c in initial_checks)
        and any(changed < c < back for c in final_checks)
        and any(back < c < forward for c in initial_checks)
        and any(forward < c < received for c in final_checks)
        for changed, event in enumerate(events) if event["before"] == original and event["after"] != original
        for back in undo if changed < back
        for forward in redo if back < forward
        for received in receivers if forward < received
    )
    return {"checks": checks, "undo_exact": bool(undo), "redo_exact": bool(redo), "workflow_ordered": ordered}


def current_state_checked(events, state, required):
    mutations = [index for index, event in enumerate(events) if event["before"] != event["after"]]
    if not mutations:
        return False
    for event in events[mutations[-1] + 1:]:
        payload = json.loads(event["visible"])
        report = payload.get("result")
        if (payload.get("exit_code") == 0 and isinstance(report, dict)
                and report.get("schema") == "fr-checks-1" and report.get("executed")
                and report.get("passed") and event["before"] == event["after"] == state
                and set(required).issubset({check["name"] for check in report.get("results", []) if check.get("passed")})):
            return True
    return False


def coordinated_batch(events):
    saved = []
    for event in events:
        payload = json.loads(event["visible"])
        report = payload.get("result")
        if payload.get("exit_code") == 0 and isinstance(report, dict) and report.get("saved") is True:
            saved.append((event, report))
    if len(saved) != 1:
        return False
    event, report = saved[0]
    if (event["request"].get("args", [])[:2] != ["author", "batch"]
            or report.get("schema") != "fr-author-batch-1" or report.get("files_changed") != 2
            or report.get("applied") is not False
            or type(report.get("transaction")) is not int or report["transaction"] <= 0):
        return False
    transaction = str(report["transaction"])
    for action in ("apply", "undo", "redo", "patch"):
        matching = [entry for entry in events if entry["request"].get("args", [])[:3] == ["history", action, transaction]]
        if not any((action == "patch" or "--write" in entry["request"]["args"])
                   and json.loads(entry["visible"]).get("exit_code") == 0 for entry in matching):
            return False
    return True


def score(session):
    config = json.loads((session / "session.json").read_text())
    events = [json.loads(line) for line in (session / "events.jsonl").read_text().splitlines()]
    if not events:
        raise ValueError("A trial needs recorded tool events before scoring")
    enc = tokenizer()
    count = lambda text: len(enc.encode(text, disallowed_special=()))
    original, final = config["original"], snapshot(session / "project")
    changed = sorted(name for name in set(original) | set(final) if original.get(name) != final.get(name))
    result = {
        "schema": "fr-agent-eval-result-1", "task": config["task"], "arm": config["arm"],
        "archive_sha256": config["archive_sha256"], "binary_sha256": config["binary_sha256"],
        "tokenizer": {"package": "tiktoken", "version": "0.12.0", "encoding": "o200k_base",
                      "vocabulary_sha256": "446a9538cb6c348e3516120d7c08b09f57c36495e2acfffe59a5bf8b0cfb1a2d"},
        "tool_calls": len(events), "visible_output_tokens": sum(count(e["visible"]) for e in events),
        "output_tokens_by_category": {kind: sum(count(e["visible"]) for e in events if category(e["request"]) == kind)
                                      for kind in ("skill", "inspection", "checks", "change_and_delivery")},
        "prompt_tokens": count((session / "prompt.txt").read_text()),
        "tool_request_tokens": sum(count(json.dumps(e["request"], ensure_ascii=False)) for e in events),
        "visible_output_bytes": sum(len(e["visible"].encode()) for e in events),
        "source_bytes": config["source_bytes"],
        "tool_seconds": round(sum(e["elapsed_seconds"] for e in events), 3),
        "elapsed_seconds": round(events[-1]["started_at"] + events[-1]["elapsed_seconds"] - events[0]["started_at"], 3),
        "refusals_or_failures": sum(bool(json.loads(e["visible"]).get("error")) or json.loads(e["visible"]).get("exit_code", 0) != 0 for e in events),
        "source_edit_steps": sum(e["before"] != e["after"] for e in events),
        "manual_corrections": config.get("manual_corrections", 0), "changed_paths": changed,
        **workflow(events, original, final, required_checks(config["task"])),
        "index_unchanged": all(e["index_sha256"] == config["index_sha256"] for e in events),
        "receiver_index_unchanged": digest((session / "receiver/.git/index").read_bytes()) == config["receiver_index_sha256"],
        "receiver_matches": snapshot(session / "receiver") == final,
        "oracle": verify(session / "project", config["task"]),
        "original_oracle": config["original_oracle"],
        "receiver_oracle": verify(session / "receiver", config["task"]),
        "finished": json.loads(events[-1]["visible"]).get("finished", False),
        "measurement_scope": "Prompt and instrumented tool payloads; excludes system context, hidden reasoning, framing, caching and billed token usage. Cooperative isolation. One scored session.",
        "repetition": config.get("repetition", 1),
    }
    result["context_tokens"] = result["prompt_tokens"] + result["visible_output_tokens"]
    if config["task"] == regex_escape_len.TASK and config["arm"] == "fr":
        result["coordinated_batch"] = coordinated_batch(events)
    result["passed"] = (
        all(result[key] for key in ("workflow_ordered", "undo_exact", "redo_exact", "index_unchanged", "receiver_index_unchanged", "receiver_matches", "finished"))
        and changed == sorted(edit_paths(config["task"])) and not result["original_oracle"]["passed"]
        and result["original_oracle"].get("stage") == (2 if config["task"] == "unicode-dice" else 1)
        and result["oracle"]["passed"] and result["receiver_oracle"]["passed"]
        and result.get("coordinated_batch", True)
    )
    save(session / "result.json", result)
    print(json.dumps(result, indent=2, ensure_ascii=False))


def replay(directory):
    import tempfile
    manifest = json.loads((directory / "manifest.json").read_text())
    for name, sha in manifest["files"].items():
        if digest(within(directory, name).read_bytes()) != sha:
            raise ValueError(f"Recorded evidence checksum mismatch: {name}")
    results = []
    with tempfile.TemporaryDirectory(prefix="fr-agent-replay-") as tmp:
        for trial in manifest["trials"]:
            path = within(directory, trial)
            config = json.loads((path / "session.json").read_text())
            recorded = json.loads((path / "result.json").read_text())
            if not recorded["passed"]:
                raise ValueError(f"Recorded trial failed acceptance: {trial}")
            events = [json.loads(line) for line in (path / "events.jsonl").read_text().splitlines()]
            root = Path(tmp) / trial
            unpack(root, config["task"])
            if verify(root, config["task"])["passed"]:
                raise ValueError("The unmodified project unexpectedly satisfies the task oracle")
            initialize(root)
            original = snapshot(root)
            original_index = (root / ".git/index").read_bytes()
            patch = (path / "change.patch").read_bytes()
            git(root, "apply", "--check", "--index", data=patch)
            git(root, "apply", data=patch)
            actual = snapshot(root)
            changed = sorted(name for name in original if original[name] != actual.get(name))
            if changed != sorted(edit_paths(config["task"])) or any(actual[name] != events[-1]["after"].get(name) for name in changed):
                raise ValueError("Patch does not reproduce the recorded agent result")
            upstream_result = upstream(root, config["task"])
            oracle = verify(root, config["task"])
            (root / "unrelated.txt").write_text("Preserve this independent later edit.\n")
            git(root, "apply", "--reverse", data=patch)
            if snapshot(root) != original:
                raise ValueError("Reverse patch did not restore original bytes")
            git(root, "apply", data=patch)
            if snapshot(root) != actual or (root / "unrelated.txt").read_text() != "Preserve this independent later edit.\n":
                raise ValueError("Patch reapplication lost source or the unrelated edit")
            if (root / ".git/index").read_bytes() != original_index:
                raise ValueError("Patch workflow changed the index")
            observed = workflow(events, config["original"], events[-1]["after"], required_checks(config["task"]))
            if config["task"] == regex_escape_len.TASK and config["arm"] == "fr":
                observed["coordinated_batch"] = coordinated_batch(events)
                if not observed["coordinated_batch"]:
                    raise ValueError("Coordinated task requires one reviewed batch transaction")
            if any(observed[key] != recorded[key] for key in observed):
                raise ValueError("Recorded workflow score disagrees with its transcript")
            if not recorded["passed"] or not observed["workflow_ordered"] or upstream_result["exit_code"] or not oracle["passed"]:
                raise ValueError(f"Acceptance replay failed: {trial}: {oracle}: {upstream_result}")
            results.append({"trial": trial, "passed": True, "oracle": oracle})
    return {"passed": True, "trials": results,
            "scope": "Replay recorded patches, tests, oracles and transition evidence; does not rerun an autonomous agent or retokenize payloads."}


def audit_tokens(directory):
    manifest = json.loads((directory / "manifest.json").read_text())
    enc = tokenizer()
    count = lambda text: len(enc.encode(text, disallowed_special=()))
    audited = []
    for trial in manifest["trials"]:
        path = within(directory, trial)
        result = json.loads((path / "result.json").read_text())
        events = [json.loads(line) for line in (path / "events.jsonl").read_text().splitlines()]
        prompt_tokens = count((path / "prompt.txt").read_text())
        output_tokens = sum(count(e["visible"]) for e in events)
        request_tokens = sum(count(json.dumps(e["request"], ensure_ascii=False)) for e in events)
        expected = {"prompt_tokens": prompt_tokens, "visible_output_tokens": output_tokens,
                    "tool_request_tokens": request_tokens, "context_tokens": prompt_tokens + output_tokens,
                    "tool_calls": len(events)}
        if any(result[key] != value for key, value in expected.items()):
            raise ValueError(f"Token accounting does not match the retained transcript: {trial}")
        audited.append({"trial": trial, **expected})
    return {"passed": True, "trials": audited}


def record(sessions, directory, pilots=None, execution_note=None, implementation_commit=None):
    experiment = sessions / "experiment.json"
    design = json.loads(experiment.read_text()) if experiment.is_file() else {
        "project": "strsim", "repetitions": 1, "trials": [name for name, _, _, _ in trial_names("strsim", 1)]}
    expected = [name for name, _, _, _ in trial_names(design["project"], design["repetitions"])]
    if design["trials"] != expected:
        raise ValueError("Experiment does not contain every planned paired repetition")
    scores = {}
    for name in expected:
        result = json.loads((sessions / name / "result.json").read_text())
        if not isinstance(result.get("passed"), bool):
            raise ValueError(f"Score every completed trial before recording evidence: {name}")
        if result["passed"] and not (sessions / name / "artifacts/change.patch").is_file():
            raise ValueError(f"Passing trial lacks its exported patch: {name}")
        scores[name] = result["passed"]
    directory.mkdir(parents=True, exist_ok=False)
    trials = []
    for name in expected:
        source, destination = sessions / name, directory / name
        destination.mkdir()
        for filename in TRIAL_FILES:
            shutil.copyfile(source / filename, destination / filename)
        for filename in CODEX_PROVENANCE_FILES:
            path = source / filename
            if path.is_file():
                shutil.copyfile(path, destination / filename)
        patch = source / "artifacts/change.patch"
        if patch.is_file():
            shutil.copyfile(patch, destination / "change.patch")
        trials.append(name)
    config = json.loads((sessions / expected[0] / "session.json").read_text())
    selected = profile(config["task"])
    shutil.copytree(sessions / expected[0] / "skill", directory / "skill")
    save(directory / "experiment.json", design)
    pilot_names = []
    if pilots:
        for source in sorted(pilots.iterdir()):
            if not (source / "events.jsonl").is_file():
                continue
            destination = directory / "pilots" / source.name
            destination.mkdir(parents=True)
            for filename in TRIAL_FILES:
                if not (source / filename).is_file():
                    continue
                shutil.copyfile(source / filename, destination / filename)
            for filename in CODEX_PROVENANCE_FILES:
                path = source / filename
                if path.is_file():
                    shutil.copyfile(path, destination / filename)
            pilot_names.append(source.name)
    implementation = git(
        ROOT,
        "rev-parse",
        "--verify",
        f"{implementation_commit or 'HEAD'}^{{commit}}",
    ).stdout.decode().strip()
    save(directory / "manifest.json", {
        "schema": "fr-agent-eval-evidence-1", "trials": trials,
        "project": selected["project"], "upstream_commit": selected["upstream_commit"], "archive_sha256": selected["archive_sha256"],
        "dependency_lock_sha256": selected.get("dependency_lock_sha256"),
        "implementation_commit": implementation,
        "agent_execution": execution_note or "Runtime provenance not supplied; consult individual trial transcripts.",
        "acceptance": {
            "passed": all(scores.values()),
            "failed_trials": [name for name, passed in scores.items() if not passed],
        },
        "pilots": {"interrupted": pilot_names, "reason": "Cargo inherited the containing fr workspace; no valid baseline build. Restarted outside Cargo projects after preflight." if pilot_names else None, "included_in_scored_trials": False},
        "versions": {tool: subprocess.check_output([tool, "--version"], text=True).strip() for tool in ("rustc", "cargo", "git", "python3")},
        "evaluator_files": {str(path.relative_to(ROOT)): digest(path.read_bytes()) for path in
                            [Path(__file__), ROOT / "tools/agent_eval/oracle.py", ROOT / "tools/agent_eval/regex_workspace.py", ROOT / "tools/agent_eval/regex_escape_len.py"]},
        "files": {str(path.relative_to(directory)): digest(path.read_bytes()) for path in sorted(directory.rglob("*")) if path.is_file()},
    })
    print(json.dumps({"recorded": str(directory), "trials": trials, "interrupted_pilots": pilot_names}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    prepare_parser = commands.add_parser("prepare")
    prepare_parser.add_argument("--out", type=Path, required=True)
    prepare_parser.add_argument("--fr", type=Path, default=ROOT / "target/debug/fr")
    prepare_parser.add_argument("--project", choices=("strsim", "regex", "regex-coordinated"), default="strsim")
    prepare_parser.add_argument("--repetitions", type=int, default=1)
    step_parser = commands.add_parser("step")
    step_parser.add_argument("session", type=Path)
    step_parser.add_argument("request", nargs="?")
    step_parser.add_argument("--request-stdin", action="store_true")
    score_parser = commands.add_parser("score")
    score_parser.add_argument("session", type=Path)
    replay_parser = commands.add_parser("replay")
    replay_parser.add_argument("directory", type=Path)
    audit_parser = commands.add_parser("audit-tokens")
    audit_parser.add_argument("directory", type=Path)
    record_parser = commands.add_parser("record")
    record_parser.add_argument("sessions", type=Path)
    record_parser.add_argument("directory", type=Path)
    record_parser.add_argument("--pilots", type=Path)
    record_parser.add_argument("--execution-note", help="Actual agent runtime, isolation and intervention details.")
    record_parser.add_argument(
        "--implementation-commit",
        help="Commit used to build the evaluated fr binary; defaults to HEAD.",
    )
    args = parser.parse_args()
    if args.command == "prepare":
        prepare(args.out.resolve(), args.fr.resolve(), args.project, args.repetitions)
    elif args.command == "step":
        step(args.session.resolve(), request_input(args.request, args.request_stdin, sys.stdin))
    elif args.command == "score":
        score(args.session.resolve())
    elif args.command == "replay":
        print(json.dumps(replay(args.directory.resolve()), indent=2))
    elif args.command == "audit-tokens":
        print(json.dumps(audit_tokens(args.directory.resolve()), indent=2))
    else:
        record(
            args.sessions.resolve(),
            args.directory.resolve(),
            args.pilots.resolve() if args.pilots else None,
            args.execution_note,
            args.implementation_commit,
        )


if __name__ == "__main__":
    main()
