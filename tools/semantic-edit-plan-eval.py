#!/usr/bin/env python3
"""Compare explicit semantic-intent and direct scalar-plan authoring."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile


SOURCE = """pub fn summarize(value: i32) -> i32 {
    let stage_one = value + 1;
    let stage_two = stage_one * 2;
    stage_two
}

#[test]
fn changed_behavior() {
    assert_eq!(summarize(2), 18);
}
"""


def compact(value: object) -> bytes:
    return json.dumps(value, ensure_ascii=False, separators=(",", ":")).encode()


def invoke(binary: Path, root: Path, arguments: list[str]) -> tuple[dict, int]:
    result = subprocess.run(
        [str(binary), "--json", "--no-cache", "-C", str(root), *arguments],
        check=False,
        capture_output=True,
    )
    if result.returncode:
        raise RuntimeError(f"{arguments}: {result.stdout.decode()} {result.stderr.decode()}")
    return json.loads(result.stdout), len(result.stdout)


def fixture(root: Path) -> None:
    root.mkdir()
    (root / "app.rs").write_text(SOURCE, encoding="utf-8")


def behavior(root: Path) -> bool:
    compiled = subprocess.run(
        ["rustc", "--edition=2021", "--test", "app.rs", "-o", "app-test"],
        cwd=root,
        check=False,
        capture_output=True,
    )
    return compiled.returncode == 0 and subprocess.run(
        [str(root / "app-test")], cwd=root, check=False, capture_output=True
    ).returncode == 0


def lifecycle(binary: Path, root: Path, written: dict, initial: bytes) -> dict:
    transaction = str(written["transaction"])
    changed = (root / "app.rs").read_bytes()
    passed = behavior(root)
    invoke(binary, root, ["history", "undo", transaction, "--write"])
    undo_exact = (root / "app.rs").read_bytes() == initial
    forward, _ = invoke(binary, root, ["history", "patch", transaction, "--check"])
    invoke(binary, root, ["history", "redo", transaction, "--write"])
    redo_exact = (root / "app.rs").read_bytes() == changed
    reverse, _ = invoke(binary, root, ["history", "patch", transaction, "--reverse", "--check"])
    final, _ = invoke(
        binary,
        root,
        ["project", "semantic", ".", "--declaration", "summarize", "--body", "--nodes", "64", "--minimal"],
    )
    return {
        "behavior_passed": passed,
        "undo_exact": undo_exact,
        "redo_exact": redo_exact,
        "patch_checked": forward["matches_patch_basis"] and reverse["matches_patch_basis"],
        "final_body_basis": final["body_identity"]["basis"],
        "final_source_sha256": hashlib.sha256(changed).hexdigest(),
    }


def explicit_route(binary: Path, root: Path) -> dict:
    initial = (root / "app.rs").read_bytes()
    query, query_bytes = invoke(binary, root, [
        "project", "semantic", "app.rs", "--declaration", "summarize", "--body",
        "--locators", "--locators-only", "--locator-op", "set-int", "--locator-from", "1",
        "--nodes", "64", "--minimal",
    ])
    rows = query["body_locators"]["rows"]
    if len(rows) != 1:
        raise RuntimeError(f"expected one locator, found {len(rows)}")
    intent = {
        "schema": "fr-semantic-intent-1",
        "base": query["body_identity"]["basis"],
        "operations": [{"op": "set-int", "target": rows[0]["target"], "from": "1", "to": "7"}],
    }
    payload = compact(intent)
    intent_path = root.parent / "explicit-intent.json"
    intent_path.write_bytes(payload)
    handle = query["selection"]["handle"]
    preview, preview_bytes = invoke(
        binary, root, ["author", "edit-body-intent", handle, "--from", str(intent_path)]
    )
    written, write_bytes = invoke(binary, root, [
        "author", "edit-body-intent", handle, "--from", str(intent_path), "--write",
        "--plan-basis", preview["plan_context_basis"],
    ])
    result = {
        "commands_before_lifecycle": 3,
        "query_bytes": query_bytes,
        "payload_bytes": len(payload),
        "preview_bytes": preview_bytes,
        "write_bytes": write_bytes,
        "measured_context_bytes": query_bytes + len(payload) + preview_bytes + write_bytes,
        "intent_sha256": preview["semantic_intent"]["sha256"],
        "intent_basis": preview["semantic_intent"]["basis"],
        "compiled_change_sha256": preview["semantic_intent"]["compiled_change_sha256"],
    }
    result.update(lifecycle(binary, root, written, initial))
    return result


def scalar_route(binary: Path, root: Path) -> dict:
    initial = (root / "app.rs").read_bytes()
    preview, preview_bytes = invoke(binary, root, [
        "author", "edit-body-scalar", ".", "--declaration", "summarize",
        "--operation", "set-int", "--from", "1", "--to", "7",
    ])
    written, write_bytes = invoke(binary, root, [
        "author", "edit-body-scalar", ".", "--declaration", "summarize",
        "--operation", "set-int", "--from", "1", "--to", "7",
        "--write", "--plan-basis", preview["plan_context_basis"],
    ])
    result = {
        "commands_before_lifecycle": 2,
        "query_bytes": 0,
        "payload_bytes": 0,
        "preview_bytes": preview_bytes,
        "write_bytes": write_bytes,
        "measured_context_bytes": preview_bytes + write_bytes,
        "intent_sha256": preview["semantic_edit_plan"]["intent_sha256"],
        "intent_basis": preview["semantic_edit_plan"]["intent_basis"],
        "compiled_change_sha256": preview["semantic_edit_plan"]["compiled_change_sha256"],
    }
    result.update(lifecycle(binary, root, written, initial))
    return result


def evaluate(binary: Path) -> dict:
    with tempfile.TemporaryDirectory() as directory:
        base = Path(directory)
        explicit = base / "explicit"
        scalar = base / "scalar"
        fixture(explicit)
        fixture(scalar)
        routes = {
            "explicit-intent": explicit_route(binary, explicit),
            "scalar-plan": scalar_route(binary, scalar),
        }
    return {
        "schema": "fr-semantic-edit-plan-evaluation-1",
        "fixture": "generic three-statement Rust arithmetic pipeline",
        "change": "replace one uniquely matched integer scalar",
        "routes": routes,
        "equivalence": {
            "intent_identity_equal": len({route["intent_basis"] for route in routes.values()}) == 1,
            "compiled_change_identity_equal": len({route["compiled_change_sha256"] for route in routes.values()}) == 1,
            "final_body_identity_equal": len({route["final_body_basis"] for route in routes.values()}) == 1,
            "final_source_identity_equal": len({route["final_source_sha256"] for route in routes.values()}) == 1,
        },
        "scope": "Deterministic source-free discovery, authoring and lifecycle evidence on one generic Rust fixture; no agent or population claim.",
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--fr", type=Path, default=Path("target/debug/fr"))
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    report = evaluate(args.fr.resolve())
    rendered = json.dumps(report, indent=2) + "\n"
    if args.output:
        args.output.write_text(rendered, encoding="utf-8")
    else:
        print(rendered, end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
