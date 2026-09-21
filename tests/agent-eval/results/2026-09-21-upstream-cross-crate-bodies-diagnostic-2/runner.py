#!/usr/bin/env python3
"""Record one guided authored cross-crate Rust body edit on a pinned MIT regex workspace."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "sdk/python/src"))

from fr_ir.guide import (AgentGoal, GoalConstraints, GoalLimits, GoalOperation, GoalSelector,
                         GuideFile, GuideInputs)
from fr_ir.intent_actions import TaggedIntentAction, TaskChangeOperation
from fr_ir.ir import TaskChange, TaskDelivery, TaskTarget
from fr_ir.runtime import FrClient

from agent_eval import regex_workspace


ROOT = Path(__file__).resolve().parents[1]
SYNTAX = "regex-syntax/src/lib.rs"
FACADE = "src/lib.rs"
SOURCES = (SYNTAX, FACADE)
MODEL, EFFORT, TIER = "gpt-5.6-luna", "low", "default"
PATCH = "artifacts/exact-escape.patch"
DELIVERY = TaskDelivery(patch=PATCH, check_output_bytes=2048)
STEPS = ("guide", "reveal-syntax", "reveal-facade", "preview", "review", "execute", "finish")
FILES = ("session.json", "prompt.txt", "events.jsonl", "codex-events.jsonl",
         "codex-stderr.txt", "codex-final.txt", "codex-run.json", "result.json")
CHECK_NAMES = ("upstream", "minimal")
CHECKS = [
    {"name": "upstream",
     "argv": ["cargo", "test", "-p", "regex", "-p", "regex-syntax", "--lib", "--locked", "--offline"],
     "cwd": ".", "timeout_seconds": 180, "covers": ["Pinned regex and regex-syntax library tests"]},
    {"name": "minimal",
     "argv": ["cargo", "check", "-p", "regex", "--no-default-features", "--locked", "--offline"],
     "cwd": ".", "timeout_seconds": 120, "covers": ["Regex facade without default features"]},
]
ORACLE = r'''use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Counting;
static CALLS: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        CALLS.fetch_add(1, Ordering::SeqCst);
        System.alloc(layout)
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        CALLS.fetch_add(1, Ordering::SeqCst);
        System.realloc(ptr, layout, size)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) { System.dealloc(ptr, layout) }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn reference(text: &str) -> String {
    text.chars().flat_map(|c| {
        if "\\.+*?()|[]{}^$#&-~".contains(c) { vec!['\\', c] } else { vec![c] }
    }).collect()
}

fn main() {
    let alphabet = ["", "abc", ".*", "\\", "[a-z]", "é🙂", "π#&-~", "a\n b"];
    let mut cases = 0;
    for a in alphabet { for b in alphabet {
        let input = format!("{a}{b}");
        let expected = reference(&input);
        let actual = regex::escape(&input);
        assert_eq!(actual, expected);
        cases += 1;
    }}
    for input in ["a", ".*", "\\.+*?()|[]{}^$#&-~", "é🙂", "π#&-~", "a\n b"] {
        let expected = reference(input);
        CALLS.store(0, Ordering::SeqCst);
        let actual = regex::escape(input);
        let calls = CALLS.load(Ordering::SeqCst);
        assert_eq!(actual, expected);
        assert_eq!(calls, 1, "{input:?} used {calls} allocation calls");
    }
    let expected = reference(".*é");
    let mut appended = String::from("prefix:");
    regex_syntax::escape_into(".*é", &mut appended);
    assert_eq!(appended, format!("prefix:{expected}"));
    println!("{cases} behavior cases; one allocation per nonempty facade case; append preserved");
}
'''


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def save(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n")


def read(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text())
    if not isinstance(value, dict):
        raise ValueError(f"{path} is not an object")
    return value


def tracked(project: Path) -> dict[str, str]:
    names = subprocess.check_output(["git", "ls-files", "-z"], cwd=project).decode().split("\0")
    return {name: sha((project / name).read_bytes()) for name in names if name and not name.startswith(".fr/")}


def evaluator_hash() -> str:
    return sha(Path(__file__).read_bytes())


def goal() -> AgentGoal:
    return AgentGoal("change", selector=GoalSelector(name="escape_into", scope=SYNTAX, language="rust"),
                     operation=GoalOperation("source-bodies", {"additional": [
                         {"name": "escape", "scope": FACADE, "language": "rust"},
                     ]}),
                     constraints=GoalConstraints(allow_source=True),
                     checks=CHECK_NAMES, delivery=DELIVERY,
                     context=GoalLimits(packet_limit=65536))


def unpack(project: Path) -> None:
    regex_workspace.unpack(project)


def prompt(session: Path) -> str:
    return f"""You are an acceptance agent in the pinned MIT/Apache-licensed regex workspace.

Task: make `regex_syntax::escape_into` reserve the exact additional byte length before appending, including one extra byte for every regex metacharacter. Make the public `regex::escape` facade construct an empty `String`, call `regex_syntax::escape_into` directly and return that buffer. Preserve escaping and append behavior for ASCII and Unicode, public signatures, no-default-features support and every source byte outside these two bodies. Author each complete Rust body, including its outer braces but omitting its declaration signature. Inspect both bounded reveals, the batch preview and the complete review before execution. Report success only after all checks and delivery stages pass.

Use only the instrumented tool below through functions.exec / tools.exec_command. Every command calls this exact step with one JSON request on stdin. Do not read project files directly, run project commands, edit source, browse, delegate, or ask a human to correct the result:
python3 {ROOT / 'tools/upstream-cross-crate-bodies-agent.py'} step {session} --request-stdin <<'FRJSON'
{{"tool":"guide"}}
FRJSON

Allowed requests, in order:
1. {{"tool":"guide"}} selects both exact body targets.
2. {{"tool":"reveal-syntax"}} shows bounded `regex_syntax::escape_into` source.
3. {{"tool":"reveal-facade"}} shows bounded `regex::escape` source.
4. {{"tool":"preview","syntax_body":"<complete escape_into body>","facade_body":"<complete escape body>"}} previews both bodies together. JSON-escape line breaks.
5. {{"tool":"review"}} reuses those exact bodies and returns the complete two-file diff and checks.
6. {{"tool":"execute"}} executes the unchanged review.
7. {{"tool":"finish","answer":{{"files":["{SYNTAX}","{FACADE}"],"checks":["upstream","minimal"],"allocation_calls":1,"append_preserved":true}}}} finishes after execution passes.

The evaluator separately checks the two-body source boundary, receiver patch replay, offline compilation, 64 independent behavior cases, append semantics and one allocation for every nonempty facade case. Do not claim success if any stage fails.
"""


def prepare(session: Path, binary: Path) -> dict[str, Any]:
    session.mkdir(parents=True, exist_ok=False)
    frozen = session / "fr-bin"
    shutil.copyfile(binary, frozen)
    frozen.chmod(0o555)
    project = session / "project"
    unpack(project)
    (project / ".fr").mkdir()
    save(project / ".fr/checks.json", {"schema": 1, "checks": CHECKS})
    (project / "artifacts").mkdir()
    subprocess.run(["git", "init", "-q"], cwd=project, check=True)
    subprocess.run(["git", "add", "--all"], cwd=project, check=True)
    subprocess.run(["git", "-c", "user.name=Eval", "-c", "user.email=eval@example.invalid",
                    "commit", "-qm", "pinned upstream"], cwd=project, check=True)
    selected = {"schema": "fr-upstream-cross-crate-bodies-session-1", "upstream_commit": regex_workspace.COMMIT,
                "archive_sha256": regex_workspace.ARCHIVE_SHA,
                "lock_sha256": sha((project / "Cargo.lock").read_bytes()),
                "binary": str(frozen), "binary_sha256": sha(frozen.read_bytes()),
                "source_files": tracked(project), "goal": goal().to_data(),
                "evaluator_sha256": evaluator_hash(), "manual_corrections": 0}
    save(session / "session.json", selected)
    (session / "prompt.txt").write_text(prompt(session))
    return selected


def config(session: Path) -> dict[str, Any]:
    value = read(session / "session.json")
    if (value.get("schema") != "fr-upstream-cross-crate-bodies-session-1"
            or value.get("upstream_commit") != regex_workspace.COMMIT
            or value.get("archive_sha256") != regex_workspace.ARCHIVE_SHA
            or value.get("evaluator_sha256") != evaluator_hash() or value.get("goal") != goal().to_data()
            or value.get("manual_corrections") != 0
            or sha(Path(value["binary"]).read_bytes()) != value.get("binary_sha256")
            or sha((session / "project/Cargo.lock").read_bytes()) != value.get("lock_sha256")):
        raise ValueError("session binding changed")
    return value


def events(session: Path) -> list[dict[str, Any]]:
    path = session / "events.jsonl"
    return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []


def authored_bodies(prior: list[dict[str, Any]], request: dict[str, Any]) -> tuple[str, str]:
    if request["tool"] == "preview":
        if set(request) != {"tool", "syntax_body", "facade_body"}:
            raise ValueError("preview needs both authored bodies")
        bodies = (request["syntax_body"], request["facade_body"])
    else:
        if set(request) != {"tool"} or len(prior) < 4:
            raise ValueError("review and execute reuse the previewed bodies")
        bodies = (prior[3]["request"]["syntax_body"], prior[3]["request"]["facade_body"])
    if any(not isinstance(body, str) or not 1 <= len(body.encode()) <= 4096 for body in bodies):
        raise ValueError("each authored body must be bounded UTF-8 text")
    return bodies


def action(targets: list[dict[str, Any]], bodies: tuple[str, str]) -> TaggedIntentAction:
    change = TaskChange([], [TaskTarget("syntax", targets[0]["handle"], "replace-body", fragment=bodies[0]),
                             TaskTarget("facade", targets[1]["handle"], "replace-body", fragment=bodies[1])],
                        {"files-changed": 2, "edits": 2, "paths-changed": list(SOURCES)},
                        list(CHECK_NAMES), DELIVERY)
    return TaggedIntentAction(TaskChangeOperation(change), diff_bytes=65536, report_bytes=65536)


def step(session: Path, request: dict[str, Any]) -> dict[str, Any]:
    selected = config(session)
    prior = events(session)
    if len(prior) >= len(STEPS) or request.get("tool") != STEPS[len(prior)]:
        raise ValueError("tool sequence differs from pinned workflow")
    kind = request["tool"]
    bodies = authored_bodies(prior, request) if kind in ("preview", "review", "execute") else None
    project = session / "project"
    client = FrClient(str(project), executable=selected["binary"])
    guide = client.guide(goal()) if kind != "finish" else None
    full: dict[str, Any] | None = None
    if kind == "guide":
        assert guide is not None
        full = guide.to_data()
        visible = {key: full[key] for key in ("state", "route", "target", "basis")}
        visible["targets"] = full["targets"]
        visible["actions"] = [item.to_data() for item in guide.actions()]
    elif kind in ("reveal-syntax", "reveal-facade"):
        assert guide is not None
        full = client.follow_guide(guide.actions()[0 if kind == "reveal-syntax" else 1]).to_data()
        visible = {"query": full["query"], "source": full["source"], "node": full["node"]}
    elif kind == "preview":
        assert guide is not None and bodies is not None
        inputs = session / "inputs"
        inputs.mkdir(exist_ok=False)
        operations = []
        for index, body in enumerate(bodies):
            path = inputs / f"body-{index}.txt"
            path.write_text(body)
            operations.append({"op": "replace-body", "handle": guide.at(f"/targets/{index}/handle"),
                               "from": str(path)})
        manifest = json.dumps({"operations": operations})
        full = client.follow_guide(guide.actions()[2],
                                   GuideInputs({"input-file": GuideFile("batch.json", manifest)})).to_data()
        visible = {key: full.get(key) for key in ("applied", "changed", "files_changed", "diff")}
    elif kind in ("review", "execute"):
        assert guide is not None and bodies is not None
        review = client.review_guide(guide, action(guide.at("/targets"), bodies))
        if kind == "review":
            full = dict(review.at())
            visible = {"ready": full["ready"], "diff": full["diff"], "checks": full["checks"],
                       "stages": full["stages"], "review_sha256": review.review_sha256}
        else:
            if review.review_sha256 != prior[4]["visible"]["review_sha256"]:
                raise ValueError("review changed before execution")
            full = dict(client.execute_guide(review).to_data())
            visible = {"passed": full["passed"], "transaction": full["transaction"],
                       "stages": [{"stage": item["stage"], "status": item["status"]}
                                  for item in full["workflow"]["stages"]]}
    else:
        answer = request.get("answer")
        if not isinstance(answer, dict) or len(json.dumps(answer)) > 1024:
            raise ValueError("finish answer is invalid")
        visible = {"finished": True, "answer": answer}
    if len(json.dumps(visible).encode()) > 20000:
        raise ValueError("visible report exceeds limit")
    row = {"request": request, "visible": visible, "full": full, "source_files": tracked(project)}
    with (session / "events.jsonl").open("a") as stream:
        stream.write(json.dumps(row, ensure_ascii=False) + "\n")
    return visible


def run(session: Path, codex: Path, timeout: int) -> dict[str, Any]:
    config(session)
    if (session / "events.jsonl").exists() or (session / "codex-run.json").exists():
        raise ValueError("live session must be fresh")
    version = subprocess.check_output([codex, "--version"], text=True).strip()
    command = [str(codex), "exec", "--ephemeral", "--ignore-user-config", "--ignore-rules",
               "--skip-git-repo-check", "--json", "--color", "never", "--sandbox", "workspace-write",
               "--add-dir", str(session), "--model", MODEL,
               "--config", f'model_reasoning_effort="{EFFORT}"',
               "--config", f'service_tier="{TIER}"', "--cd", str(session / "project"),
               "--output-last-message", str(session / "codex-final.txt"), "-"]
    started = time.time()
    with (session / "codex-events.jsonl").open("wb") as stdout, (session / "codex-stderr.txt").open("wb") as stderr:
        try:
            completed = subprocess.run(command, input=(session / "prompt.txt").read_bytes(),
                                       stdout=stdout, stderr=stderr, timeout=timeout)
            exit_code, timed_out = completed.returncode, False
        except subprocess.TimeoutExpired:
            exit_code, timed_out = 124, True
    record = {"schema": "fr-upstream-cross-crate-bodies-run-1", "codex_version": version, "model": MODEL,
              "reasoning_effort": EFFORT, "service_tier": TIER, "ephemeral": True,
              "ignored_user_config": True, "ignored_rules": True, "sandbox": "workspace-write",
              "prompt_sha256": sha((session / "prompt.txt").read_bytes()),
              "events_sha256": sha((session / "codex-events.jsonl").read_bytes()),
              "stderr_sha256": sha((session / "codex-stderr.txt").read_bytes()),
              "elapsed_seconds": time.time() - started, "exit_code": exit_code,
              "timed_out": timed_out, "command": command}
    save(session / "codex-run.json", record)
    return record


def oracle(session: Path, selected: dict[str, Any]) -> dict[str, Any]:
    project = session / "project"
    current = tracked(project)
    changed = sorted(name for name in selected["source_files"] if current.get(name) != selected["source_files"][name])
    syntax = (project / SYNTAX).read_text()
    facade = (project / FACADE).read_text()
    exact_reserve = ("buf.reserve(text.len() + text.chars().filter(|&c| is_meta_character(c)).count());" in syntax
                     or ("let additional = text.len()" in syntax
                         and "buf.reserve(additional);" in syntax))
    source_contract = (changed == sorted(SOURCES)
                       and ".filter(|&c| is_meta_character(c)).count()" in syntax
                       and exact_reserve
                       and "let mut escaped = alloc::string::String::new();" in facade
                       and "regex_syntax::escape_into(pattern, &mut escaped);" in facade
                       and "regex_syntax::escape(pattern)" not in facade)
    with tempfile.TemporaryDirectory(prefix="fr-cross-crate-bodies-receiver-") as temporary:
        receiver = Path(temporary) / "project"
        unpack(receiver)
        patch = project / PATCH
        applied = subprocess.run(["git", "apply", str(patch)], cwd=receiver, capture_output=True, timeout=30)
        replay = applied.returncode == 0 and all(
            sha((receiver / name).read_bytes()) == current[name] for name in selected["source_files"])
        env = os.environ.copy()
        env["CARGO_NET_OFFLINE"] = "true"
        build = subprocess.run(
            ["cargo", "build", "-p", "regex", "-p", "regex-syntax", "--lib", "--locked", "--offline"],
            cwd=receiver, env=env, capture_output=True, timeout=180)
        executed: subprocess.CompletedProcess[bytes] | None = None
        detail = (build.stdout + build.stderr).decode(errors="replace")[-2048:]
        if build.returncode == 0:
            oracle_source = Path(temporary) / "oracle.rs"
            oracle_binary = Path(temporary) / "oracle"
            oracle_source.write_text(ORACLE)
            compiled = subprocess.run(
                ["rustc", "--edition=2021", str(oracle_source),
                 "--extern", f"regex={receiver / 'target/debug/libregex.rlib'}",
                 "--extern", f"regex_syntax={receiver / 'target/debug/libregex_syntax.rlib'}",
                 "-L", f"dependency={receiver / 'target/debug/deps'}", "-o", str(oracle_binary)],
                env=env, capture_output=True, timeout=60)
            detail = (compiled.stdout + compiled.stderr).decode(errors="replace")[-2048:]
            if compiled.returncode == 0:
                executed = subprocess.run([str(oracle_binary)], capture_output=True, timeout=30)
                detail = (executed.stdout + executed.stderr).decode(errors="replace")[-2048:]
        expected = b"64 behavior cases; one allocation per nonempty facade case; append preserved"
        behavior = executed is not None and executed.returncode == 0 and executed.stdout.strip() == expected
        return {"changed_files": changed, "source_contract": source_contract,
                "receiver_patch_replay": replay, "offline_compiled": build.returncode == 0,
                "behavior_64_cases": behavior, "one_allocation_nonempty": behavior,
                "append_preserved": behavior, "oracle_detail": detail}


def score(session: Path) -> dict[str, Any]:
    selected = config(session)
    rows = events(session)
    record = read(session / "codex-run.json")
    codex_rows = [json.loads(line) for line in (session / "codex-events.jsonl").read_text().splitlines() if line]
    commands = [row["item"] for row in codex_rows if row.get("type") == "item.completed"
                and row.get("item", {}).get("type") == "command_execution"]
    usage = next((row["usage"] for row in reversed(codex_rows) if row.get("type") == "turn.completed"), None)
    direct = [item["command"] for item in commands if str(ROOT / "tools/upstream-cross-crate-bodies-agent.py") + " step " not in item.get("command", "")]
    sequence = [row["request"].get("tool") for row in rows]
    guide = rows[0]["full"] if rows else {}
    preview = rows[3]["full"] if len(rows) > 3 else {}
    review = rows[4]["full"] if len(rows) > 4 else {}
    execution = rows[5]["full"] if len(rows) > 5 else {}
    stages = execution.get("workflow", {}).get("stages", []) if execution else []
    expected_stages = ["check-original", "apply", "check-applied", "undo", "check-restored",
                       "redo", "check-applied", "deliver-patch"]
    answer = rows[6]["request"].get("answer", {}) if len(rows) > 6 else {}
    independent = oracle(session, selected) if (session / "project" / PATCH).is_file() else {}
    result = {"schema": "fr-upstream-cross-crate-bodies-result-1", "passed": False, "sequence": sequence,
              "guide_route": guide.get("route", {}).get("id"), "preview": preview,
              "reviewed_diff_sha256": sha(review["diff"].encode()) if review else None,
              "review_ready": review.get("ready") if review else None,
              "execution_passed": execution.get("passed") if execution else None,
              "stages": [(item.get("stage"), item.get("status")) for item in stages],
              "oracle": independent, "answer": answer,
              "codex": {"usage": usage, "command_executions": len(commands),
                        "failed_command_executions": sum(item.get("exit_code") != 0 for item in commands),
                        "direct_project_commands": direct, "billed_quota": {"available": False, "value": None}},
              "manual_corrections": selected["manual_corrections"]}
    result["passed"] = (record["exit_code"] == 0 and not record["timed_out"]
                        and sequence == list(STEPS) and guide.get("state") == "ready"
                        and result["guide_route"] == "source-bodies"
                        and guide.get("route", {}).get("source_required") is True
                        and rows[1]["full"]["source"]["returned_bytes"] <= 4096
                        and rows[2]["full"]["source"]["returned_bytes"] <= 4096
                        and preview.get("applied") is False and preview.get("changed") is True
                        and preview.get("files_changed") == 2
                        and preview.get("diff") == review.get("diff") and review.get("ready") is True
                        and review.get("checks", {}).get("names") == list(CHECK_NAMES)
                        and all(f"--- a/{path}" in review.get("diff", "") for path in SOURCES)
                        and execution.get("passed") is True
                        and [item.get("stage") for item in stages] == expected_stages
                        and all(item.get("status") == "passed" for item in stages)
                        and answer == {"files": list(SOURCES), "checks": list(CHECK_NAMES),
                                       "allocation_calls": 1, "append_preserved": True}
                        and independent.get("source_contract") and independent.get("receiver_patch_replay")
                        and independent.get("offline_compiled") and independent.get("behavior_64_cases")
                        and independent.get("one_allocation_nonempty") and independent.get("append_preserved")
                        and len(commands) == len(STEPS) and not direct
                        and result["codex"]["failed_command_executions"] == 0
                        and isinstance(usage, dict) and selected["manual_corrections"] == 0)
    save(session / "result.json", result)
    return result


def record(session: Path, destination: Path, diagnostic: bool, reason: str | None) -> dict[str, Any]:
    result = read(session / "result.json")
    if not result["passed"] and not diagnostic:
        raise ValueError("failed trial can only be recorded as diagnostic")
    if diagnostic and not reason:
        raise ValueError("diagnostic retention requires --reason")
    destination.mkdir(parents=True, exist_ok=False)
    files = {}
    for name in FILES:
        if (session / name).exists():
            shutil.copyfile(session / name, destination / name)
            files[name] = sha((destination / name).read_bytes())
    manifest = {"schema": "fr-upstream-cross-crate-bodies-manifest-1", "passed": result["passed"],
                "acceptance_evidence": result["passed"] and not diagnostic,
                "diagnostic_reason": reason if diagnostic else None,
                "upstream_commit": regex_workspace.COMMIT, "model": MODEL,
                "evaluator_sha256": evaluator_hash(), "files": files}
    save(destination / "manifest.json", manifest)
    return manifest


def audit(destination: Path) -> dict[str, Any]:
    manifest = read(destination / "manifest.json")
    for name, expected in manifest["files"].items():
        if sha((destination / name).read_bytes()) != expected:
            raise ValueError(f"retained artifact changed: {name}")
    result = read(destination / "result.json")
    if manifest["passed"] != result["passed"] or manifest["acceptance_evidence"] and not result["passed"]:
        raise ValueError("retained outcome classification disagrees with score")
    return {"verified": True, "passed": manifest["passed"], "acceptance_evidence": manifest["acceptance_evidence"]}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    p = sub.add_parser("prepare"); p.add_argument("session", type=Path)
    p.add_argument("--fr", type=Path, default=ROOT / "target/debug/fr")
    p = sub.add_parser("step"); p.add_argument("session", type=Path); p.add_argument("--request-stdin", action="store_true")
    p = sub.add_parser("run"); p.add_argument("session", type=Path)
    p.add_argument("--codex", type=Path, default=Path("codex"))
    p.add_argument("--timeout", type=int, default=1800); p.add_argument("--confirm-agent-spend", action="store_true")
    p = sub.add_parser("score"); p.add_argument("session", type=Path)
    p = sub.add_parser("record"); p.add_argument("session", type=Path); p.add_argument("destination", type=Path)
    p.add_argument("--diagnostic", action="store_true"); p.add_argument("--reason")
    p = sub.add_parser("audit"); p.add_argument("destination", type=Path)
    args = parser.parse_args()
    try:
        if args.command == "prepare": value = prepare(args.session.resolve(), args.fr.resolve())
        elif args.command == "step":
            if not args.request_stdin:
                raise ValueError("step requires --request-stdin")
            value = step(args.session.resolve(), read_stdin())
        elif args.command == "run":
            if not args.confirm_agent_spend:
                raise ValueError("--confirm-agent-spend is required")
            value = run(args.session.resolve(), args.codex, args.timeout)
        elif args.command == "score": value = score(args.session.resolve())
        elif args.command == "record": value = record(args.session.resolve(), args.destination.resolve(), args.diagnostic, args.reason)
        else: value = audit(args.destination.resolve())
    except (OSError, ValueError, KeyError, IndexError, RuntimeError, json.JSONDecodeError,
            subprocess.TimeoutExpired) as error:
        parser.error(str(error))
    print(json.dumps(value, indent=2, ensure_ascii=False))


def read_stdin() -> dict[str, Any]:
    value = json.load(sys.stdin)
    if not isinstance(value, dict):
        raise ValueError("tool request must be an object")
    return value


if __name__ == "__main__":
    main()
