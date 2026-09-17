#!/usr/bin/env python3
"""Exercise every guided workflow family and measure the discovery it replaces."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[1]
BOUND_FILES = (
    "tools/completion-workflows.py",
    "src/audit.rs",
    "src/capabilities.rs",
    "src/cli.rs",
    "src/project/agent_guide.rs",
    "src/project/agent_intent.rs",
    "src/project/task_change.rs",
    "src/project/migration.rs",
    "src/spec.rs",
)


def canonical(value: object) -> bytes:
    return json.dumps(
        value, ensure_ascii=False, sort_keys=True, separators=(",", ":"), allow_nan=False
    ).encode()


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def source_snapshot(root: Path) -> dict[str, str]:
    ignored = {".fr-history", ".lake", "target"}
    return {
        str(path.relative_to(root)): digest(path.read_bytes())
        for path in sorted(root.rglob("*"))
        if path.is_file() and not ignored.intersection(path.relative_to(root).parts)
    }


def bindings() -> dict[str, str]:
    return {name: digest((ROOT / name).read_bytes()) for name in BOUND_FILES}


def run(
    executable: Path,
    workspace: Path,
    arguments: list[str],
    input_value: object | None = None,
    json_output: bool = True,
) -> tuple[object, dict[str, object]]:
    command = [str(executable), "--no-cache", "-C", str(workspace)]
    if json_output:
        command.append("--json")
    command.extend(arguments)
    input_bytes = None if input_value is None else canonical(input_value)
    completed = subprocess.run(command, input=input_bytes, capture_output=True, timeout=180)
    if completed.returncode:
        raise RuntimeError(
            f"completion workflow command failed: {arguments!r}\n"
            + completed.stderr.decode(errors="replace")
            + completed.stdout.decode(errors="replace")
        )
    response = (
        json.loads(completed.stdout)
        if json_output
        else completed.stdout.decode(errors="replace")
    )
    event = {
        "arguments": arguments,
        "request_bytes": len(canonical({"arguments": arguments})) + len(input_bytes or b""),
        "response_bytes": len(completed.stdout),
        "response_sha256": digest(completed.stdout),
    }
    return response, event


def prepare(executable: Path, root: Path) -> tuple[str, str]:
    (root / "src").mkdir(parents=True)
    (root / "app/api/signals").mkdir(parents=True)
    (root / "src/lib.rs").write_text(
        "pub fn calculate(value: i64) -> i64 { value + 7 }\n"
        "pub fn keep(value: bool) -> bool { value }\n"
    )
    (root / "app/api/signals/route.ts").write_text(
        "export async function GET() { return Response.json({ healthy: true }); }\n"
    )
    (root / "Cargo.toml").write_text("[workspace]\n")
    (root / "rename.recipe").write_text(
        "schema 1\nrecipe rename-calculate {\n"
        "  rename to \"compute\" where name=\"calculate\"\n"
        "  expect matched = 1\n  expect changed = 1\n  expect refusals = 0\n}\n"
    )
    (root / "proof.lean").write_text("rfl\n")
    run(executable, root, ["spec", "init", "specs", "--write"])
    plan, _ = run(
        executable,
        root,
        ["spec", "plan", "src/lib.rs::keep", "--property", "identity"],
    )
    (root / "formal-plan.json").write_bytes(canonical(plan))
    run(
        executable,
        root,
        ["spec", "scaffold", "--from", "formal-plan.json", "--write"],
    )
    goals, _ = run(executable, root, ["spec", "goals", "specs"])
    goal = goals["catalog"][0]
    return goal["spec"], goal["name"]


def goal(purpose: str, selector: dict[str, object], operation: dict[str, object]) -> dict[str, object]:
    return {
        "schema": "fr-agent-goal-1",
        "purpose": purpose,
        "selector": selector,
        "operation": operation,
        "context": {"token_limit": 4096, "packet_limit": 65536},
    }


def cases(proof_path: str, obligation: str) -> list[dict[str, object]]:
    return [
        {
            "id": "understanding",
            "route": "evidence",
            "goal": goal("understand", {"name": "calculate"}, {"kind": "automatic"}),
            "manual_discovery": [["project", "--help"], ["intent", "--help"]],
        },
        {
            "id": "tracing",
            "route": "evidence",
            "goal": goal("trace", {"name": "calculate"}, {"kind": "automatic"}),
            "manual_discovery": [["project", "--help"], ["intent", "--help"]],
        },
        {
            "id": "direct-change",
            "route": "direct-capability",
            "goal": goal(
                "change",
                {"name": "calculate"},
                {"kind": "capability", "capability": "rename", "parameters": {"new_name": "compute"}},
            ),
            "manual_discovery": [["--help"], ["rename", "--help"]],
        },
        {
            "id": "recipe",
            "route": "recipe",
            "goal": goal(
                "change", {"name": "calculate"}, {"kind": "recipe", "verb": "rename"}
            ),
            "manual_discovery": [["recipe", "--help"], ["recipe", "--vocabulary"]],
        },
        {
            "id": "semantic-edit",
            "route": "semantic-scalar",
            "goal": goal(
                "change",
                {"name": "calculate"},
                {"kind": "semantic-scalar", "operation": "set-int", "from": "7", "to": "9"},
            ),
            "manual_discovery": [["author", "guide"], ["author", "semantic-schema", "intent"]],
        },
        {
            "id": "framework-migration",
            "route": "framework-migration",
            "goal": goal(
                "migrate",
                {"path": "app/api/signals/route.ts"},
                {"kind": "framework-migration", "to": "fastapi"},
            ),
            "manual_discovery": [["migrate", "--help"], ["audit", "frameworks"]],
        },
        {
            "id": "proof",
            "route": "proof",
            "goal": goal(
                "prove",
                {"path": proof_path},
                {"kind": "proof", "obligation": obligation},
            ),
            "manual_discovery": [["spec", "--help"], ["audit", "proofs"]],
        },
    ]


def action_arguments(case: dict[str, object], action: dict[str, object], guide: dict[str, object]) -> list[str]:
    replacements = {
        "<recipe-file>": "rename.recipe",
        "<destination>": "converted/signals.py",
        "<tactics-file>": "proof.lean",
    }
    compatible = guide["route"]["evidence"].get("compatible_features", [])
    if compatible:
        replacements["<feature-id>"] = compatible[0]
    return [replacements.get(value, value) for value in action["arguments"]]


def execute_case(executable: Path, workspace: Path, case: dict[str, object]) -> dict[str, object]:
    source_before = source_snapshot(workspace)
    manual_events = []
    for arguments in case["manual_discovery"]:
        json_output = "--help" not in arguments
        _, event = run(executable, workspace, arguments, json_output=json_output)
        manual_events.append(event)

    guide, guide_event = run(
        executable, workspace, ["guide", "--from", "-"], case["goal"]
    )
    if guide["route"]["id"] != case["route"] or guide["state"] == "unsupported":
        raise RuntimeError(f"unexpected guide route for {case['id']}: {guide}")
    action_events = []
    outputs = []
    for action in guide["actions"]:
        arguments = action_arguments(case, action, guide)
        if any(value.startswith("<") and value.endswith(">") for value in arguments):
            continue
        response, event = run(
            executable,
            workspace,
            arguments,
            action.get("input"),
        )
        schema = action.get("output_schema")
        field = action.get("schema_field", "schema")
        if schema and response.get(field) != schema:
            raise RuntimeError(f"action schema mismatch for {case['id']}: {action} {response}")
        action_events.append(event)
        outputs.append({"arguments": arguments, "response_sha256": event["response_sha256"]})
    if not action_events:
        raise RuntimeError(f"guide supplied no executable action for {case['id']}")
    exploratory = [
        event for event in action_events if "--help" in event["arguments"]
        or event["arguments"][:2] == ["recipe", "--vocabulary"]
        or event["arguments"][:2] == ["author", "guide"]
        or event["arguments"][:1] == ["audit"]
    ]
    if exploratory:
        raise RuntimeError(f"guide retained exploratory calls for {case['id']}: {exploratory}")
    source_after = source_snapshot(workspace)
    if source_before != source_after:
        raise RuntimeError(f"preview workflow changed source for {case['id']}")
    return {
        "id": case["id"],
        "route": guide["route"]["id"],
        "state": guide["state"],
        "guide_basis": guide["basis"],
        "guide_object_root": guide["object_root"],
        "manual_discovery": {
            "calls": len(manual_events),
            "request_bytes": sum(event["request_bytes"] for event in manual_events),
            "response_bytes": sum(event["response_bytes"] for event in manual_events),
            "events": manual_events,
        },
        "guided": {
            "guide_calls": 1,
            "guide_request_bytes": guide_event["request_bytes"],
            "guide_response_bytes": guide_event["response_bytes"],
            "executable_actions": len(action_events),
            "post_guide_exploratory_calls": 0,
            "outputs": outputs,
        },
        "omitted_measurements": ["hidden reasoning", "model tokenizer counts", "billed quota"],
        "source_state": {
            "unchanged": True,
            "sha256": digest(canonical(source_after)),
            "files": len(source_after),
        },
    }


def measure(executable: Path) -> dict[str, object]:
    with tempfile.TemporaryDirectory(prefix="fr-completion-workflows-") as directory:
        prepared = Path(directory) / "prepared"
        prepared.mkdir()
        proof_path, obligation = prepare(executable, prepared)
        results = []
        for case in cases(proof_path, obligation):
            workspace = Path(directory) / case["id"]
            shutil.copytree(prepared, workspace, ignore=shutil.ignore_patterns(".fr-history"))
            results.append(execute_case(executable, workspace, case))
        return {
            "schema": "fr-completion-workflows-1",
            "bindings": bindings(),
            "binary_sha256": digest(executable.read_bytes()),
            "workflows": results,
            "totals": {
                "workflow_families": len(results),
                "manual_exploratory_calls": sum(row["manual_discovery"]["calls"] for row in results),
                "post_guide_exploratory_calls": sum(row["guided"]["post_guide_exploratory_calls"] for row in results),
            },
            "claim": "Deterministic command and byte accounting. No model, tokenizer, quota or population claim.",
        }


def audit(value: dict[str, object]) -> None:
    if value.get("schema") != "fr-completion-workflows-1":
        raise RuntimeError("completion workflow schema is stale")
    if value.get("bindings") != bindings():
        raise RuntimeError("completion workflow source bindings are stale")
    expected = [row["id"] for row in cases("<proof-path>", "<obligation>")]
    actual = [row["id"] for row in value.get("workflows", [])]
    if actual != expected:
        raise RuntimeError("completion workflow family coverage is incomplete")
    for row in value["workflows"]:
        if row["manual_discovery"]["calls"] < 1:
            raise RuntimeError("manual route has no measured discovery")
        if row["guided"]["post_guide_exploratory_calls"] != 0:
            raise RuntimeError("guided route retains exploratory discovery")
        if row["guided"]["executable_actions"] < 1:
            raise RuntimeError("guided route has no exercised action")
        if row.get("source_state", {}).get("unchanged") is not True:
            raise RuntimeError("completion preview changed source")
        if row["omitted_measurements"] != [
            "hidden reasoning",
            "model tokenizer counts",
            "billed quota",
        ]:
            raise RuntimeError("completion workflow accounting overstates its measurements")
    totals = value["totals"]
    if totals != {
        "workflow_families": 7,
        "manual_exploratory_calls": sum(row["manual_discovery"]["calls"] for row in value["workflows"]),
        "post_guide_exploratory_calls": 0,
    }:
        raise RuntimeError("completion workflow totals are inconsistent")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fr", type=Path, default=ROOT / "target/debug/fr")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--audit", type=Path)
    arguments = parser.parse_args()
    value = json.loads(arguments.audit.read_text()) if arguments.audit else measure(arguments.fr.resolve())
    audit(value)
    encoded = json.dumps(value, indent=2, ensure_ascii=False) + "\n"
    if arguments.output:
        arguments.output.write_text(encoded)
    else:
        print(encoded, end="")


if __name__ == "__main__":
    main()
