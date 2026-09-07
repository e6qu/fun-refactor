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
import tarfile
import time

from agent_eval.oracle import verify

ROOT = Path(__file__).resolve().parent.parent
ARCHIVE = ROOT / "tests/agent-eval/strsim-0.11.1.crate"
ARCHIVE_SHA = "7da8b5736845d9f2fcb837ea5d9e2628564b3b043a70948a3f0b778838c5fb4f"
TASKS = {
    "unicode-dice": "Fix Sørensen–Dice similarity for Unicode inputs. Use Unicode scalar-value bigrams and their counts consistently. Preserve whitespace removal, duplicate-bigram multiplicity, equal-input behavior and the public API. Unequal inputs with fewer than two scalars after whitespace removal must score zero. Keep existing ASCII behavior.",
    "normalized-osa": "Add a public normalized_osa_distance(a: &str, b: &str) -> f64 API. Return one minus optimal-string-alignment distance divided by the longer input's Unicode scalar count. Two empty inputs score one. Preserve OSA's restricted transposition semantics and all existing APIs. Match the neighboring normalized APIs; reuse existing distance machinery when suitable.",
}


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


def unpack(destination):
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
    git(root, "commit", "-qm", "Pinned public release with evaluation check declarations")


def prompt(session, task, arm):
    surface = (
        "Use fr for source exploration and edits. Start by reading skill/SKILL.md through the read tool; load its references only as needed. Source read/search/replace tools are unavailable in this arm. Use fr project handles and saved authoring plans. Write fragments under artifacts/ outside the project."
        if arm == "fr" else
        "Use ordinary files, read, search and replace tools for source exploration and edits. Do not use fr project/author/history commands. The shared fr checks command is available for identical project validation. Export and reverse/reapply your patch through the ordinary Git tools."
    )
    return f"""You are an independent acceptance-test agent. Complete this code task in the supplied unfamiliar published project: {TASKS[task]}

{surface}

Only change src/lib.rs. The evaluator owns regression oracles; do not read or modify the harness, oracle, other sessions, or hidden tests. You may add a focused function-level implementation but need not alter project documentation for this trial. Do not browse, delegate, commit or push. No human correction is available.

Use only the instrumented tool for all task inspection and work. Invoke it using functions.exec / tools.exec_command:
python3 {ROOT / 'tools/agent-eval.py'} step {session} '<JSON object>'
Use proper shell quoting for JSON (a single quote inside JSON needs shell escaping). You may batch independent instrumented calls. Do not directly read or write the project or artifacts through other tools. This is a cooperative measurement boundary, not an OS sandbox.

Tool objects:
{{"tool":"files","path":"."}} lists up to 200 paths (baseline only).
{{"tool":"read","path":"README.md","start":1,"lines":80}} reads up to 200 lines. In the fr arm use project show for source; read can access README.md, Cargo.toml, .fr/checks.json and skill/ references.
{{"tool":"search","pattern":"normalized","path":"src"}} runs literal rg with bounded output (baseline only).
{{"tool":"replace","path":"src/lib.rs","old":"unique existing text","new":"replacement"}} requires exactly one match (baseline only).
{{"tool":"append","path":"src/lib.rs","text":"new function text"}} appends source (baseline only).
{{"tool":"write","path":"fragment.rs","text":"{{ replacement block }}"}} writes artifacts/fragment.rs and returns its absolute path (both arms).
{{"tool":"fr","args":["checks"]}} invokes fr with the project root, JSON and no cache. Use this for checks in both arms and project/author/history/git in the fr arm. --help is available.
{{"tool":"export"}} saves and shows a Git diff as artifacts/change.patch (baseline only). In the fr arm, fr history patch TX automatically saves its returned patch there.
{{"tool":"reverse"}} / {{"tool":"apply"}} reverses/reapplies that saved Git patch (baseline only).
{{"tool":"sentinel"}} adds an unrelated edit after the requested change; it must survive reversal and reapplication.
{{"tool":"receiver"}} checks and applies the saved patch in a clean separate receiver and compares tracked content with your project.
{{"tool":"finish","summary":"..."}} records your final conclusion; independent oracles run later.

Workflow: inspect; list and run declared checks on the original; implement the task; run checks on the change; export the patch; add the sentinel; undo and check; redo and check; verify the receiver; finish. fr arm: preview/save/apply an authoring transaction and use history undo/redo. Every check run needs the configuration basis from its listing. Refresh handles after source changes. Keep tool output bounded and request only relevant context. Leave the requested change applied. Report uncertainty and tool refusals honestly.

The harness records visible tool payload tokens, calls, latency and final correctness. It does not measure your hidden reasoning, system context or billed tokens. The task directory is {session / 'project'}.
"""


def prepare(out, binary):
    if any((parent / "Cargo.toml").is_file() for parent in [out, *out.parents]):
        raise ValueError("Prepare outside Cargo projects, for example under /tmp, to avoid inherited workspace membership")
    out.mkdir(parents=True, exist_ok=False)
    for task in TASKS:
        for arm in ("fr", "files"):
            session = out / f"{task}-{arm}"
            session.mkdir()
            project = session / "project"
            unpack(project)
            (project / ".fr").mkdir()
            save(project / ".fr/checks.json", {"schema": 1, "checks": [
                {"name": "upstream", "argv": ["cargo", "test", "--offline"], "cwd": ".",
                 "timeout_seconds": 120, "covers": ["Unmodified upstream unit, integration and documentation tests"]}
            ]})
            initialize(project)
            preflight = process(["cargo", "test", "--offline"], project)
            if preflight["exit_code"]:
                raise RuntimeError(f"Upstream preflight failed: {preflight}")
            receiver = session / "receiver"
            shutil.copytree(project, receiver, ignore=shutil.ignore_patterns(".git", "target", "Cargo.lock"))
            initialize(receiver)
            (session / "artifacts").mkdir()
            shutil.copytree(ROOT / "skills/fr", session / "skill")
            source_files = list(project.rglob("*.rs"))
            save(session / "session.json", {
                "schema": "fr-agent-eval-session-1", "task": task, "arm": arm,
                "archive_sha256": ARCHIVE_SHA, "upstream_commit": "76c5a900e6e12cfc605eee5ab6e36300384c8682",
                "fr": str(binary.resolve()), "binary_sha256": digest(binary.read_bytes()),
                "original": snapshot(project), "index_sha256": digest((project / ".git/index").read_bytes()),
                "receiver_index_sha256": digest((receiver / ".git/index").read_bytes()),
                "source_bytes": sum(path.stat().st_size for path in source_files),
                "created_at": time.time(),
                "original_oracle": verify(project, task),
                "preflight_passed": True,
            })
            (session / "prompt.txt").write_text(prompt(session, task, arm))
    print(json.dumps({"sessions": [str(path) for path in sorted(out.iterdir())]}))


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
        if request["path"] != "src/lib.rs":
            raise ValueError("Trial edits are limited to src/lib.rs")
        path = project / request["path"]
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
        if config["binary_sha256"] != digest(Path(config["fr"]).read_bytes()):
            raise ValueError("Trial binary changed after preparation")
        result = process([config["fr"], "--no-cache", "--json", "-C", str(project), *args], project)
        report = result["result"]
        if isinstance(report, dict) and isinstance(report.get("patch"), str) and args[:2] == ["history", "patch"]:
            (session / "artifacts/change.patch").write_text(report["patch"])
            result["patch_artifact"] = "artifacts/change.patch"
        return result
    if kind == "export":
        result = git(project, "diff", "--binary", "--", "src/lib.rs")
        (session / "artifacts/change.patch").write_bytes(result.stdout)
        return {"patch": result.stdout.decode(), "patch_artifact": "artifacts/change.patch"}
    if kind in ("reverse", "apply"):
        args = ["apply", "--reverse"] if kind == "reverse" else ["apply"]
        result = git(project, *args, data=(session / "artifacts/change.patch").read_bytes(), check=False)
        return {"exit_code": result.returncode, "stderr": result.stderr.decode()}
    if kind == "sentinel":
        (project / "unrelated.txt").write_text("Preserve this independent later edit.\n")
        return {"created": "unrelated.txt"}
    if kind == "receiver":
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


def tokenizer():
    if importlib.metadata.version("tiktoken") != "0.12.0":
        raise ValueError("Install the pinned tools/agent_eval/requirements.txt")
    os.environ.setdefault("TIKTOKEN_CACHE_DIR", str(ROOT / "target/agent-eval-tokenizer"))
    vocabulary = Path(os.environ["TIKTOKEN_CACHE_DIR"]) / "fb374d419588a4632f3f557e76b4b70aebbca790"
    if not vocabulary.is_file() or digest(vocabulary.read_bytes()) != "446a9538cb6c348e3516120d7c08b09f57c36495e2acfffe59a5bf8b0cfb1a2d":
        raise ValueError("Fetch and verify the pinned o200k_base vocabulary before offline scoring")
    import tiktoken
    return tiktoken.get_encoding("o200k_base")


def workflow(events, original, final):
    checks = []
    undo, redo, receivers = [], [], []
    for index, event in enumerate(events):
        result = json.loads(event["visible"])
        report = result.get("result")
        if isinstance(report, dict) and report.get("schema") == "fr-checks-1" and report.get("executed"):
            checks.append({"event": index, "passed": report["passed"] and result.get("exit_code") == 0,
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
        **workflow(events, original, final),
        "index_unchanged": all(e["index_sha256"] == config["index_sha256"] for e in events),
        "receiver_index_unchanged": digest((session / "receiver/.git/index").read_bytes()) == config["receiver_index_sha256"],
        "receiver_matches": snapshot(session / "receiver") == final,
        "oracle": verify(session / "project", config["task"]),
        "original_oracle": config["original_oracle"],
        "receiver_oracle": verify(session / "receiver", config["task"]),
        "finished": json.loads(events[-1]["visible"]).get("finished", False),
        "measurement_scope": "Prompt and instrumented tool payloads; excludes system context, hidden reasoning, framing, caching and billed token usage. Cooperative isolation. One trial per arm and task.",
    }
    result["context_tokens"] = result["prompt_tokens"] + result["visible_output_tokens"]
    result["passed"] = (
        all(result[key] for key in ("workflow_ordered", "undo_exact", "redo_exact", "index_unchanged", "receiver_index_unchanged", "receiver_matches", "finished"))
        and changed == ["src/lib.rs"] and not result["original_oracle"]["passed"]
        and result["original_oracle"].get("stage") == {"unicode-dice": 2, "normalized-osa": 1}[config["task"]]
        and result["oracle"]["passed"] and result["receiver_oracle"]["passed"]
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
            events = [json.loads(line) for line in (path / "events.jsonl").read_text().splitlines()]
            root = Path(tmp) / trial
            unpack(root)
            original = (root / "src/lib.rs").read_bytes()
            if verify(root, config["task"])["passed"]:
                raise ValueError("The unmodified project unexpectedly satisfies the task oracle")
            initialize(root)
            original_index = (root / ".git/index").read_bytes()
            patch = (path / "change.patch").read_bytes()
            git(root, "apply", "--check", "--index", data=patch)
            git(root, "apply", data=patch)
            actual = (root / "src/lib.rs").read_bytes()
            if digest(actual) != events[-1]["after"]["src/lib.rs"]["sha256"]:
                raise ValueError("Patch does not reproduce the recorded agent result")
            upstream = process(["cargo", "test", "--offline"], root)
            oracle = verify(root, config["task"])
            (root / "unrelated.txt").write_text("Preserve this independent later edit.\n")
            git(root, "apply", "--reverse", data=patch)
            if (root / "src/lib.rs").read_bytes() != original:
                raise ValueError("Reverse patch did not restore original bytes")
            git(root, "apply", data=patch)
            if (root / "src/lib.rs").read_bytes() != actual or (root / "unrelated.txt").read_text() != "Preserve this independent later edit.\n":
                raise ValueError("Patch reapplication lost source or the unrelated edit")
            if (root / ".git/index").read_bytes() != original_index:
                raise ValueError("Patch workflow changed the index")
            observed = workflow(events, config["original"], events[-1]["after"])
            if any(observed[key] != recorded[key] for key in observed):
                raise ValueError("Recorded workflow score disagrees with its transcript")
            if not recorded["passed"] or not observed["workflow_ordered"] or upstream["exit_code"] or not oracle["passed"]:
                raise ValueError(f"Acceptance replay failed: {trial}: {oracle}: {upstream}")
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


def record(sessions, directory, pilots=None):
    directory.mkdir(parents=True, exist_ok=False)
    trials = []
    for task in TASKS:
        for arm in ("fr", "files"):
            name = f"{task}-{arm}"
            source, destination = sessions / name, directory / name
            result = json.loads((source / "result.json").read_text())
            if not result["passed"]:
                raise ValueError(f"Only complete passing acceptance bundles can seed regression replay: {name}")
            destination.mkdir()
            for filename in ("session.json", "prompt.txt", "events.jsonl", "result.json"):
                shutil.copyfile(source / filename, destination / filename)
            shutil.copyfile(source / "artifacts/change.patch", destination / "change.patch")
            trials.append(name)
    shutil.copytree(sessions / "unicode-dice-fr/skill", directory / "skill")
    pilot_names = []
    if pilots:
        for source in sorted(pilots.iterdir()):
            if not (source / "events.jsonl").is_file():
                continue
            destination = directory / "pilots" / source.name
            destination.mkdir(parents=True)
            for filename in ("session.json", "prompt.txt", "events.jsonl"):
                shutil.copyfile(source / filename, destination / filename)
            pilot_names.append(source.name)
    save(directory / "manifest.json", {
        "schema": "fr-agent-eval-evidence-1", "trials": trials,
        "project": "rapidfuzz/strsim-rs published crate 0.11.1; complete release snapshot, without Git history",
        "upstream_commit": "76c5a900e6e12cfc605eee5ab6e36300384c8682", "archive_sha256": ARCHIVE_SHA,
        "implementation_commit": git(ROOT, "rev-parse", "HEAD").stdout.decode().strip(),
        "agent_execution": "Four fresh collaboration agents, fork_turns=none, inherited parent model and effort, no overrides or task corrections. Cooperative tool boundary.",
        "pilots": {"interrupted": pilot_names, "reason": "Cargo inherited the containing fr workspace; no valid baseline build. Restarted all trials with fresh agents outside Cargo projects after adding preflight.", "included_in_scored_trials": False},
        "versions": {tool: subprocess.check_output([tool, "--version"], text=True).strip() for tool in ("rustc", "cargo", "git", "python3")},
        "evaluator_files": {str(path.relative_to(ROOT)): digest(path.read_bytes()) for path in
                            [Path(__file__), ROOT / "tools/agent_eval/oracle.py"]},
        "files": {str(path.relative_to(directory)): digest(path.read_bytes()) for path in sorted(directory.rglob("*")) if path.is_file()},
    })
    print(json.dumps({"recorded": str(directory), "trials": trials, "interrupted_pilots": pilot_names}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    prepare_parser = commands.add_parser("prepare")
    prepare_parser.add_argument("--out", type=Path, required=True)
    prepare_parser.add_argument("--fr", type=Path, default=ROOT / "target/debug/fr")
    step_parser = commands.add_parser("step")
    step_parser.add_argument("session", type=Path)
    step_parser.add_argument("request", type=json.loads)
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
    args = parser.parse_args()
    if args.command == "prepare":
        prepare(args.out.resolve(), args.fr.resolve())
    elif args.command == "step":
        step(args.session.resolve(), args.request)
    elif args.command == "score":
        score(args.session.resolve())
    elif args.command == "replay":
        print(json.dumps(replay(args.directory.resolve()), indent=2))
    elif args.command == "audit-tokens":
        print(json.dumps(audit_tokens(args.directory.resolve()), indent=2))
    else:
        record(args.sessions.resolve(), args.directory.resolve(), args.pilots.resolve() if args.pilots else None)


if __name__ == "__main__":
    main()
