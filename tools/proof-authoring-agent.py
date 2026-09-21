#!/usr/bin/env python3
"""Retain one guided, reviewed agent-authored Lean proof."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
from typing import Any, TypedDict, cast

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "sdk/python/src"))

from fr_ir.guide import AgentGoal, GoalLimits, GoalOperation, GoalSelector, GuideFile, GuideInputs
from fr_ir.intent_actions import ProofSubmissionOperation, TaggedIntentAction
from fr_ir.ir import TaskDelivery
from fr_ir.runtime import FrClient


ROOT = Path(__file__).resolve().parents[1]
MODEL, EFFORT, TIER = "gpt-5.6-luna", "low", "default"
SOURCE = "src/lib.rs"
SPEC = "specs/FrSpecs/SrcLibRsRender.lean"
OBLIGATION = "renderModel_identity"
PATCH = "artifacts/proof.patch"
CHECK_NAMES = ("lean",)
DELIVERY = TaskDelivery(patch=PATCH, check_output_bytes=4096)
STEPS = ("guide", "task", "check", "preview", "review", "execute", "finish")
FILES = ("session.json", "prompt.txt", "events.jsonl", "codex-events.jsonl",
         "codex-stderr.txt", "codex-final.txt", "codex-run.json", "result.json")
SOURCE_TEXT = "pub fn render(value: bool) -> bool { value }\n"
GITIGNORE = ".cache/\nspecs/.lake/\n"
LEAN_TOOLCHAIN = "leanprover/lean4:v4.28.0\n"
LAKEFILE = '''name = "fr-specs"
version = "0.1.0"
defaultTargets = ["FrSpecs"]

[[lean_lib]]
name = "FrSpecs"
'''
ROOT_LEAN = '''import FrSpecs.SrcLibRsRender
/-
This is the checked root of the project's Lean specification package.
Import each model here so `fr spec verify` builds it.
-/

namespace FrSpecs

end FrSpecs
'''
MODEL_BASELINE = '''namespace FrSpecs

-- fr:generated-begin formal-kernel
-- fr:plan e7c05c680d52d54ce9300f1c037f1fe482a5fb2733ba99853a9c93c2fb022d14
-- fr:spec src/lib.rs::render @ dbb8f9aafedfeafffa7cfe33630e69a4a67d63b6a08db38688cb157dc583b8ac
-- fr:signature value: bool => value: Bool; return: bool => return: Bool
def renderModel (value : Bool) : Bool :=
  value

-- fr:property identity unproved
theorem renderModel_identity (x : Bool) : renderModel x = x := by
  -- fr:proof-begin renderModel_identity
  -- fr:debt renderModel_identity
  sorry
  -- fr:proof-end renderModel_identity
-- fr:generated-end formal-kernel

-- fr:handwritten-begin additional-models-and-proofs
-- fr:handwritten-end additional-models-and-proofs

end FrSpecs
'''
EXPECTED_THEOREM = "theorem renderModel_identity (x : Bool) : renderModel x = x"
EXPECTED_ANSWER = {
    "obligation": OBLIGATION,
    "proved": True,
    "model_theorem_checked": True,
    "implementation_correspondence": False,
    "check": "lean",
}


class Event(TypedDict):
    request: dict[str, Any]
    visible: dict[str, Any]
    full: dict[str, Any] | None
    source_files: dict[str, str]


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def canonical(value: object) -> bytes:
    return json.dumps(value, ensure_ascii=False, sort_keys=True,
                      separators=(",", ":"), allow_nan=False).encode()


def save(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n")


def object_map(value: object, label: str) -> dict[str, Any]:
    if not isinstance(value, dict) or any(not isinstance(key, str) for key in value):
        raise ValueError(f"{label} must be an object")
    return cast(dict[str, Any], value)


def source_files(project: Path) -> dict[str, str]:
    names = subprocess.check_output(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"], cwd=project
    ).decode().split("\0")
    return {
        name: sha((project / name).read_bytes())
        for name in names
        if name
        and not name.startswith(".fr/")
        and not name.startswith(".fr-history/")
        and not name.startswith("artifacts/")
    }


def fixture_revision() -> str:
    return sha(canonical({
        SOURCE: SOURCE_TEXT,
        SPEC: MODEL_BASELINE,
        "specs/FrSpecs.lean": ROOT_LEAN,
        "specs/lakefile.toml": LAKEFILE,
        "specs/lean-toolchain": LEAN_TOOLCHAIN,
        ".gitignore": GITIGNORE,
    }))


def evaluator_hash() -> str:
    return sha(Path(__file__).read_bytes())


def sdk_hash() -> str:
    return sha((ROOT / "sdk/python/src/fr_ir/intent.py").read_bytes())


def checks() -> list[dict[str, object]]:
    return [{
        "name": "lean",
        "argv": ["lake", "build"],
        "cwd": "specs",
        "timeout_seconds": 120,
        "covers": ["Generated Lean package elaborates"],
    }]


def goal() -> AgentGoal:
    return AgentGoal(
        "prove",
        selector=GoalSelector(path=SPEC),
        operation=GoalOperation("proof", {"obligation": OBLIGATION}),
        checks=CHECK_NAMES,
        delivery=DELIVERY,
        context=GoalLimits(packet_limit=16384),
    )


def action(tactics: str) -> TaggedIntentAction:
    operation = ProofSubmissionOperation(OBLIGATION, tactics, CHECK_NAMES, DELIVERY)
    return TaggedIntentAction(operation, diff_bytes=65536, report_bytes=65536)


def run_fr(binary: Path, project: Path, arguments: list[str]) -> dict[str, Any]:
    completed = subprocess.run(
        [str(binary), "--json", "--no-cache", "-C", str(project), *arguments],
        capture_output=True,
        timeout=180,
    )
    if completed.returncode:
        raise RuntimeError(
            f"fixture command failed: {arguments!r}\n"
            + completed.stderr.decode(errors="replace")
            + completed.stdout.decode(errors="replace")
        )
    return object_map(json.loads(completed.stdout), "fixture command output")


def populate(project: Path, binary: Path) -> None:
    project.mkdir(parents=True)
    (project / "src").mkdir()
    (project / ".fr").mkdir()
    (project / "artifacts").mkdir()
    (project / SOURCE).write_text(SOURCE_TEXT)
    (project / ".gitignore").write_text(GITIGNORE)
    save(project / ".fr/checks.json", {"schema": 1, "checks": checks()})
    run_fr(binary, project, ["spec", "init", "--write"])
    plan = run_fr(binary, project, ["spec", "plan", "src/lib.rs::render",
                                    "--property", "identity"])
    plan_path = project / ".fr/formal-plan.json"
    save(plan_path, plan)
    try:
        run_fr(binary, project, ["spec", "scaffold", "--from", ".fr/formal-plan.json", "--write"])
    finally:
        plan_path.unlink(missing_ok=True)
    expected = {
        SOURCE: SOURCE_TEXT,
        SPEC: MODEL_BASELINE,
        "specs/FrSpecs.lean": ROOT_LEAN,
        "specs/lakefile.toml": LAKEFILE,
        "specs/lean-toolchain": LEAN_TOOLCHAIN,
    }
    for name, text in expected.items():
        if (project / name).read_text() != text:
            raise RuntimeError(f"prepared proof fixture changed: {name}")


def prompt(session: Path) -> str:
    return f"""You are an acceptance agent in an unfamiliar checked Lean proof workspace.

Task: prove `{OBLIGATION}` in `{SPEC}`. Use the source-free proof task to inspect the exact theorem and proof contract, author a Lean tactic without a leading `by`, check it, preview the unchanged accepted tactic, review the complete proof submission and execute that unchanged review. Report success only after every check and reversible delivery stage passes. Keep the stated implementation-correspondence boundary unchanged.

Use only the instrumented tool below through functions.exec / tools.exec_command. Every command calls this exact step with one JSON request on stdin. Do not read project files directly, run project commands, edit source, browse, delegate, or ask a human to correct the result:
python3 {ROOT / 'tools/proof-authoring-agent.py'} step {session} --request-stdin <<'FRJSON'
{{"tool":"guide"}}
FRJSON

Allowed requests, in order:
1. {{"tool":"guide"}} selects the source-free proof route.
2. {{"tool":"task"}} reveals the exact theorem, proof contract and templates.
3. {{"tool":"check","tactics":"<agent-authored Lean tactics without leading by>"}} checks one authored proof attempt.
4. {{"tool":"preview"}} previews the unchanged accepted tactics.
5. {{"tool":"review"}} binds those tactics, the declared Lean check and patch delivery to one immutable review.
6. {{"tool":"execute"}} executes the unchanged review once.
7. {{"tool":"finish","answer":{{"obligation":"{OBLIGATION}","proved":true,"model_theorem_checked":true,"implementation_correspondence":false,"check":"lean"}}}} finishes after execution passes.

The evaluator independently recreates the unproved package, replays the reviewed patch, runs strict `fr spec verify`, requires a fresh source anchor with zero obligations and zero proof debt, and confirms that only the named proof region changed. Do not claim success if any stage fails.
"""


def prepare(session: Path, binary: Path) -> dict[str, Any]:
    session.mkdir(parents=True, exist_ok=False)
    frozen = session / "fr-bin"
    shutil.copyfile(binary, frozen)
    frozen.chmod(0o555)
    project = session / "project"
    populate(project, frozen)
    subprocess.run(["git", "init", "-q"], cwd=project, check=True)
    subprocess.run(["git", "add", "--all"], cwd=project, check=True)
    subprocess.run(["git", "-c", "user.name=Eval", "-c", "user.email=eval@example.invalid",
                    "commit", "-qm", "pinned unproved Lean fixture"], cwd=project, check=True)
    selected = {
        "schema": "fr-proof-authoring-agent-session-1",
        "fixture_revision": fixture_revision(),
        "binary": str(frozen),
        "binary_sha256": sha(frozen.read_bytes()),
        "source_files": source_files(project),
        "goal": goal().to_data(),
        "evaluator_sha256": evaluator_hash(),
        "sdk_intent_sha256": sdk_hash(),
        "manual_corrections": 0,
    }
    save(session / "session.json", selected)
    (session / "prompt.txt").write_text(prompt(session))
    return selected


def config(session: Path) -> dict[str, Any]:
    value = object_map(json.loads((session / "session.json").read_text()), "session")
    if (value.get("schema") != "fr-proof-authoring-agent-session-1"
            or value.get("fixture_revision") != fixture_revision()
            or value.get("goal") != goal().to_data()
            or value.get("evaluator_sha256") != evaluator_hash()
            or value.get("sdk_intent_sha256") != sdk_hash()
            or value.get("manual_corrections") != 0
            or sha(Path(value["binary"]).read_bytes()) != value.get("binary_sha256")):
        raise ValueError("session binding changed")
    return value


def events(session: Path) -> list[Event]:
    path = session / "events.jsonl"
    if not path.exists():
        return []
    rows: list[Event] = []
    for line in path.read_text().splitlines():
        row = object_map(json.loads(line), "event")
        object_map(row.get("request"), "event request")
        object_map(row.get("visible"), "event response")
        rows.append(cast(Event, row))
    return rows


def authored_tactics(prior: list[Event]) -> str:
    if len(prior) < 3:
        raise ValueError("proof tactics have not been authored")
    tactics = prior[2]["request"].get("tactics")
    if not isinstance(tactics, str):
        raise ValueError("proof tactics must be a string")
    return tactics


def step(session: Path, request: dict[str, Any]) -> dict[str, Any]:
    selected = config(session)
    prior = events(session)
    if len(prior) >= len(STEPS) or request.get("tool") != STEPS[len(prior)]:
        raise ValueError("tool sequence differs from the pinned workflow")
    project = session / "project"
    client = FrClient(str(project), executable=selected["binary"])
    kind = request["tool"]
    full: dict[str, Any] | None = None
    if kind == "guide":
        guide = client.guide(goal())
        full = dict(guide.to_data())
        visible = {key: full[key] for key in ("state", "route", "target", "basis", "checks")}
        visible["actions"] = [item.to_data() for item in guide.actions()]
    elif kind == "task":
        guide = client.guide(goal())
        full = dict(client.follow_guide(guide.actions()[0]).to_data())
        visible = {key: full[key] for key in ("schema", "goal", "contract", "templates",
                                               "object_digest", "token_budget")}
    elif kind == "check":
        tactics = request.get("tactics")
        if not isinstance(tactics, str):
            raise ValueError("check requires agent-authored tactics")
        guide = client.guide(goal())
        proof = GuideFile("proof.lean", tactics)
        full = dict(client.follow_guide(
            guide.actions()[1], GuideInputs({"tactics-file": proof})).to_data())
        visible = {key: full[key] for key in ("schema", "goal_id", "proof_digest", "checker",
                                               "passed", "diagnostics", "receipt", "token_budget")}
    elif kind == "preview":
        tactics = authored_tactics(prior)
        if prior[2]["full"] is None or prior[2]["full"].get("passed") is not True:
            raise ValueError("proof attempt must pass before preview")
        guide = client.guide(goal())
        proof = GuideFile("proof.lean", tactics)
        full = dict(client.follow_guide(
            guide.actions()[2], GuideInputs({"tactics-file": proof})).to_data())
        visible = {key: full[key] for key in ("applied", "diff", "goal_id", "obligation",
                                               "proof_digest", "receipt", "schema")}
    elif kind in ("review", "execute"):
        tactics = authored_tactics(prior)
        guide = client.guide(goal())
        review = client.review_guide(guide, action(tactics))
        operation = dict(review.at())
        if kind == "review":
            full = operation
            visible = {
                "ready": operation["ready"],
                "diff": operation["diff"],
                "checks": operation["checks"],
                "claims": operation["claims"],
                "proof": operation["proof"],
                "proof_validation": operation["proof_validation"],
                "stages": operation["stages"],
                "review_sha256": review.review_sha256,
            }
        else:
            if review.review_sha256 != prior[4]["visible"]["review_sha256"]:
                raise ValueError("review changed before execution")
            full = dict(client.execute_guide(review).to_data())
            visible = {
                "passed": full["passed"],
                "claims": full["claims"],
                "proof": full["proof"],
                "proof_validation": full["proof_validation"],
                "transaction": full["transaction"],
                "stages": [{"stage": item["stage"], "status": item["status"]}
                           for item in full["workflow"]["stages"]],
                "patch": full["workflow"]["stages"][-1]["result"],
            }
    else:
        answer = object_map(request.get("answer"), "answer")
        if len(json.dumps(answer).encode()) > 2048:
            raise ValueError("answer is too large")
        visible = {"finished": True, "answer": answer}
    if len(json.dumps(visible).encode()) > 30000:
        raise ValueError("visible report exceeds limit")
    row: Event = {"request": request, "visible": visible, "full": full,
                  "source_files": source_files(project)}
    with (session / "events.jsonl").open("a") as stream:
        stream.write(json.dumps(row, ensure_ascii=False) + "\n")
    return visible


def codex_command(codex: Path, session: Path, model: str, effort: str, tier: str) -> list[str]:
    return [str(codex), "exec", "--ephemeral", "--ignore-user-config", "--ignore-rules",
            "--skip-git-repo-check", "--json", "--color", "never", "--sandbox", "workspace-write",
            "--add-dir", str(session), "--model", model,
            "--config", f'model_reasoning_effort="{effort}"',
            "--config", f'service_tier="{tier}"', "--cd", str(session / "project"),
            "--output-last-message", str(session / "codex-final.txt"), "-"]


def run(session: Path, codex: Path, model: str, effort: str, tier: str,
        timeout: int) -> dict[str, Any]:
    config(session)
    if any((session / name).exists() for name in
           ("events.jsonl", "codex-events.jsonl", "codex-run.json")):
        raise ValueError("live session must be fresh")
    version = subprocess.check_output([codex, "--version"], text=True).strip()
    command = codex_command(codex, session, model, effort, tier)
    started, timed_out = time.time(), False
    with (session / "codex-events.jsonl").open("wb") as stdout, (
            session / "codex-stderr.txt").open("wb") as stderr:
        try:
            completed = subprocess.run(command, input=(session / "prompt.txt").read_bytes(),
                                       stdout=stdout, stderr=stderr, timeout=timeout)
            exit_code = completed.returncode
        except subprocess.TimeoutExpired:
            exit_code, timed_out = 124, True
    record = {
        "schema": "fr-proof-authoring-agent-run-1",
        "codex_version": version,
        "model": model,
        "reasoning_effort": effort,
        "service_tier": tier,
        "ephemeral": True,
        "ignored_user_config": True,
        "ignored_rules": True,
        "sandbox": "workspace-write",
        "prompt_sha256": sha((session / "prompt.txt").read_bytes()),
        "events_sha256": sha((session / "codex-events.jsonl").read_bytes()),
        "stderr_sha256": sha((session / "codex-stderr.txt").read_bytes()),
        "elapsed_seconds": time.time() - started,
        "exit_code": exit_code,
        "timed_out": timed_out,
        "command": command,
    }
    save(session / "codex-run.json", record)
    return record


def proof_region(model: str) -> str | None:
    begin = f"  -- fr:proof-begin {OBLIGATION}\n"
    end = f"  -- fr:proof-end {OBLIGATION}\n"
    if model.count(begin) != 1 or model.count(end) != 1:
        return None
    return model.split(begin, 1)[1].split(end, 1)[0]


def independent_oracle(project: Path, original: dict[str, str], patch: Path,
                       binary: Path, tactics: str) -> dict[str, Any]:
    current = source_files(project)
    changed = sorted(name for name in set(original) | set(current)
                     if original.get(name) != current.get(name))
    with tempfile.TemporaryDirectory(prefix="fr-proof-authoring-receiver-") as temporary:
        receiver = Path(temporary) / "project"
        populate(receiver, binary)
        applied = subprocess.run(["git", "apply", str(patch)], cwd=receiver,
                                 capture_output=True, timeout=30)
        replay = applied.returncode == 0
        model = (receiver / SPEC).read_text() if replay else ""
        region = proof_region(model)
        source_preserved = (receiver / SOURCE).read_text() == SOURCE_TEXT
        verified = subprocess.run(
            [str(binary), "--json", "--no-cache", "-C", str(receiver),
             "spec", "verify", "specs"],
            capture_output=True,
            timeout=180,
        ) if replay else None
        detail = ((applied.stdout + applied.stderr).decode(errors="replace")[-4096:]
                  if verified is None else
                  (verified.stdout + verified.stderr).decode(errors="replace")[-8192:])
        report: dict[str, Any] = {}
        if verified is not None and verified.returncode == 0:
            report = object_map(json.loads(verified.stdout), "strict verification")
        verification = report.get("report", {}) if report else {}
        packages = report.get("packages", []) if report else []
        exact_tactics = region == "".join(f"  {line}\n" for line in tactics.strip().splitlines())
        return {
            "changed_files": changed,
            "source_preserved": source_preserved,
            "receiver_patch_replay": replay,
            "authored_tactics_preserved": exact_tactics,
            "strict_verify": bool(report)
            and verification.get("obligations") == 0
            and verification.get("debts") == []
            and len(verification.get("anchors", [])) == 1
            and verification["anchors"][0].get("status") == "fresh"
            and len(packages) == 1
            and packages[0].get("passed") is True,
            "no_proof_debt": "sorry" not in model and "fr:debt" not in model,
            "oracle_detail": detail,
        }


def score(session: Path) -> dict[str, Any]:
    selected = config(session)
    rows = events(session)
    run_record = object_map(json.loads((session / "codex-run.json").read_text()), "run")
    codex_rows = [json.loads(line) for line in
                  (session / "codex-events.jsonl").read_text().splitlines() if line]
    commands = [row["item"] for row in codex_rows if row.get("type") == "item.completed"
                and row.get("item", {}).get("type") == "command_execution"]
    usage = next((row["usage"] for row in reversed(codex_rows)
                  if row.get("type") == "turn.completed"), None)
    command_prefix = str(ROOT / "tools/proof-authoring-agent.py") + " step "
    direct = [item["command"] for item in commands
              if command_prefix not in item.get("command", "")]
    sequence = [row["request"].get("tool") for row in rows]
    guide = object_map(rows[0]["full"] if rows else {}, "guide")
    task = object_map(rows[1]["full"] if len(rows) > 1 else {}, "task")
    checked = object_map(rows[2]["full"] if len(rows) > 2 else {}, "checked proof")
    preview = object_map(rows[3]["full"] if len(rows) > 3 else {}, "preview")
    review = object_map(rows[4]["full"] if len(rows) > 4 else {}, "review")
    execution = object_map(rows[5]["full"] if len(rows) > 5 else {}, "execution")
    tactics = rows[2]["request"].get("tactics", "") if len(rows) > 2 else ""
    stages = execution.get("workflow", {}).get("stages", []) if execution else []
    expected_stages = ["check-original", "apply", "check-applied", "undo", "check-restored",
                       "redo", "check-applied", "deliver-patch"]
    answer = rows[6]["request"].get("answer", {}) if len(rows) > 6 else {}
    patch = session / "project" / PATCH
    oracle = independent_oracle(session / "project", selected["source_files"], patch,
                                Path(selected["binary"]), tactics) \
        if patch.is_file() and isinstance(tactics, str) else {}
    goal_task = task.get("goal", {}) if task else {}
    proof_validation = review.get("proof_validation", {}) if review else {}
    result = {
        "schema": "fr-proof-authoring-agent-result-1",
        "passed": False,
        "sequence": sequence,
        "guide_route": guide.get("route", {}).get("id") if guide else None,
        "authored_tactics": tactics,
        "reviewed_diff_sha256": sha(review["diff"].encode()) if review else None,
        "review_ready": review.get("ready") if review else None,
        "execution_passed": execution.get("passed") if execution else None,
        "stages": [(item.get("stage"), item.get("status")) for item in stages],
        "oracle": oracle,
        "answer": answer,
        "codex": {
            "usage": usage,
            "command_executions": len(commands),
            "failed_command_executions": sum(item.get("exit_code") != 0 for item in commands),
            "direct_project_commands": direct,
            "billed_quota": {"available": False, "value": None},
        },
        "manual_corrections": selected["manual_corrections"],
    }
    result["passed"] = (
        run_record["exit_code"] == 0 and not run_record["timed_out"]
        and sequence == list(STEPS)
        and result["guide_route"] == "proof"
        and guide.get("state") == "ready"
        and guide.get("route", {}).get("source_required") is False
        and guide.get("route", {}).get("evidence", {}).get("obligation") == OBLIGATION
        and task.get("schema") == "fr-proof-task-1"
        and goal_task.get("name") == OBLIGATION
        and goal_task.get("theorem") == EXPECTED_THEOREM
        and task.get("contract", {}).get("author") == "agent"
        and task.get("contract", {}).get("checker") == LEAN_TOOLCHAIN.strip()
        and isinstance(tactics, str) and bool(tactics.strip())
        and checked.get("passed") is True and checked.get("diagnostics") == []
        and checked.get("proof_digest") == preview.get("proof_digest")
        and checked.get("receipt") == preview.get("receipt")
        and preview.get("applied") is False
        and preview.get("diff") == review.get("diff")
        and review.get("ready") is True
        and review.get("checks", {}).get("names") == list(CHECK_NAMES)
        and review.get("claims", {}).get("model_theorem_checked") is True
        and review.get("claims", {}).get("implementation_correspondence") is False
        and proof_validation.get("strict_correspondence") is True
        and proof_validation.get("remaining_obligations") == 0
        and execution.get("passed") is True
        and execution.get("proof", {}).get("proof_digest") == checked.get("proof_digest")
        and execution.get("claims") == review.get("claims")
        and [item.get("stage") for item in stages] == expected_stages
        and all(item.get("status") == "passed" for item in stages)
        and answer == EXPECTED_ANSWER
        and oracle.get("changed_files") == [SPEC]
        and all(oracle.get(name) is True for name in (
            "source_preserved", "receiver_patch_replay", "authored_tactics_preserved",
            "strict_verify", "no_proof_debt"))
        and len(commands) == len(STEPS) and not direct
        and result["codex"]["failed_command_executions"] == 0
        and isinstance(usage, dict)
        and selected["manual_corrections"] == 0
    )
    save(session / "result.json", result)
    return result


def record(session: Path, destination: Path, diagnostic: bool,
           reason: str | None) -> dict[str, Any]:
    result = object_map(json.loads((session / "result.json").read_text()), "result")
    if not result["passed"] and not diagnostic:
        raise ValueError("failed trial can only be recorded as diagnostic")
    if diagnostic and not reason:
        raise ValueError("diagnostic retention requires --reason")
    destination.mkdir(parents=True, exist_ok=False)
    files: dict[str, str] = {}
    for name in FILES:
        if (session / name).exists():
            shutil.copyfile(session / name, destination / name)
            files[name] = sha((destination / name).read_bytes())
    run_record = json.loads((session / "codex-run.json").read_text())
    manifest = {
        "schema": "fr-proof-authoring-agent-manifest-1",
        "passed": result["passed"],
        "acceptance_evidence": result["passed"] and not diagnostic,
        "diagnostic_reason": reason if diagnostic else None,
        "fixture_revision": fixture_revision(),
        "model": run_record["model"],
        "evaluator_sha256": evaluator_hash(),
        "files": files,
    }
    save(destination / "manifest.json", manifest)
    return manifest


def audit(destination: Path) -> dict[str, Any]:
    manifest = object_map(json.loads((destination / "manifest.json").read_text()), "manifest")
    for name, expected in object_map(manifest["files"], "files").items():
        if sha((destination / name).read_bytes()) != expected:
            raise ValueError(f"retained artifact changed: {name}")
    result = json.loads((destination / "result.json").read_text())
    if (manifest["passed"] != result["passed"]
            or manifest["acceptance_evidence"] and not result["passed"]):
        raise ValueError("retained outcome classification disagrees with score")
    return {"verified": True, "passed": manifest["passed"],
            "acceptance_evidence": manifest["acceptance_evidence"]}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="action", required=True)
    prepared = sub.add_parser("prepare")
    prepared.add_argument("session", type=Path)
    prepared.add_argument("--fr", type=Path, default=ROOT / "target/debug/fr")
    stepped = sub.add_parser("step")
    stepped.add_argument("session", type=Path)
    stepped.add_argument("--request-stdin", action="store_true")
    running = sub.add_parser("run")
    running.add_argument("session", type=Path)
    running.add_argument("--codex", type=Path, default=Path("codex"))
    running.add_argument("--model", default=MODEL)
    running.add_argument("--effort", default=EFFORT)
    running.add_argument("--service-tier", default=TIER)
    running.add_argument("--timeout", type=int, default=1800)
    running.add_argument("--confirm-agent-spend", action="store_true")
    sub.add_parser("score").add_argument("session", type=Path)
    recorded = sub.add_parser("record")
    recorded.add_argument("session", type=Path)
    recorded.add_argument("destination", type=Path)
    recorded.add_argument("--diagnostic", action="store_true")
    recorded.add_argument("--reason")
    sub.add_parser("audit").add_argument("destination", type=Path)
    args = parser.parse_args()
    try:
        if args.action == "prepare":
            result = prepare(args.session.resolve(), args.fr.resolve())
        elif args.action == "step":
            if not args.request_stdin:
                raise ValueError("step requires --request-stdin")
            result = step(args.session.resolve(), object_map(json.load(sys.stdin), "tool request"))
        elif args.action == "run":
            if not args.confirm_agent_spend:
                raise ValueError("--confirm-agent-spend is required")
            result = run(args.session.resolve(), args.codex, args.model, args.effort,
                         args.service_tier, args.timeout)
        elif args.action == "score":
            result = score(args.session.resolve())
        elif args.action == "record":
            result = record(args.session.resolve(), args.destination.resolve(),
                            args.diagnostic, args.reason)
        else:
            result = audit(args.destination.resolve())
    except (OSError, ValueError, KeyError, IndexError, RuntimeError, json.JSONDecodeError,
            subprocess.TimeoutExpired) as error:
        parser.error(str(error))
    print(json.dumps(result, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
