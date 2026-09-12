#!/usr/bin/env python3
"""Compare checked semantic deltas with complete semantic body replacement."""

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
    let stage_three = stage_two - 3;
    let stage_four = stage_three + 4;
    let stage_five = stage_four * 5;
    stage_five
}

#[test]
fn changed_behavior() {
    assert_eq!(summarize(2), 95);
}
"""
POINTER = "/body/0/value/value/value/right"


def compact(value: object) -> str:
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"))


def invoke(binary: Path, root: Path, arguments: list[str], expected: int = 0) -> tuple[dict, int]:
    result = subprocess.run(
        [str(binary), "--json", "--no-cache", "-C", str(root), *arguments],
        check=False,
        capture_output=True,
    )
    if result.returncode != expected:
        raise RuntimeError(f"{arguments}: {result.stdout.decode()} {result.stderr.decode()}")
    return json.loads(result.stdout), len(result.stdout)


def semantic(binary: Path, root: Path) -> tuple[dict, int]:
    return invoke(binary, root, [
        "project", "semantic", "app.rs", "--declaration", "summarize", "--body",
        "--nodes", "128", "--minimal",
    ])


def fixture(root: Path) -> None:
    root.mkdir()
    (root / "app.rs").write_text(SOURCE, encoding="utf-8")


def payloads(report: dict) -> tuple[dict, dict]:
    body = {
        "schema": "fr-semantic-body-1",
        "body": report["model"]["items"][0]["value"]["body"],
    }
    changed = json.loads(compact(body))
    changed["body"][0]["value"]["value"]["value"]["right"] = {"kind": "int", "value": "7"}
    delta = {
        "schema": "fr-semantic-change-1",
        "base": report["body_identity"]["basis"],
        "operations": [{
            "op": "replace",
            "path": POINTER,
            "category": "expression",
            "value": {"kind": "int", "value": "7"},
        }],
    }
    return changed, delta


def exercise(binary: Path, root: Path, operation: str, payload: dict) -> dict:
    input_path = root / "change.json"
    encoded = compact(payload).encode()
    input_path.write_bytes(encoded)
    initial = (root / "app.rs").read_bytes()
    query, query_bytes = semantic(binary, root)
    handle = query["selection"]["handle"]
    preview, preview_bytes = invoke(
        binary, root, ["author", operation, handle, "--from", "change.json"]
    )
    written, write_bytes = invoke(
        binary, root, ["author", operation, handle, "--from", "change.json", "--write"]
    )
    transaction = str(written["transaction"])
    changed = (root / "app.rs").read_bytes()
    test = subprocess.run(
        ["rustc", "--edition=2021", "--test", "app.rs", "-o", "app-test"],
        cwd=root,
        check=False,
        capture_output=True,
    )
    tested = test.returncode == 0 and subprocess.run(
        [str(root / "app-test")], cwd=root, check=False, capture_output=True
    ).returncode == 0
    invoke(binary, root, ["history", "undo", transaction, "--write"])
    undo_exact = (root / "app.rs").read_bytes() == initial
    patch, patch_bytes = invoke(binary, root, ["history", "patch", transaction, "--check"])
    invoke(binary, root, ["history", "redo", transaction, "--write"])
    redo_exact = (root / "app.rs").read_bytes() == changed
    reverse, _ = invoke(
        binary, root, ["history", "patch", transaction, "--reverse", "--check"]
    )
    final, _ = semantic(binary, root)
    return {
        "payload_bytes": len(encoded),
        "semantic_query_bytes": query_bytes,
        "preview_bytes": preview_bytes,
        "write_bytes": write_bytes,
        "patch_report_bytes": patch_bytes,
        "changed": preview["changed"],
        "behavior_passed": tested,
        "patch_checked": patch["matches_patch_basis"] and reverse["matches_patch_basis"],
        "undo_exact": undo_exact,
        "redo_exact": redo_exact,
        "final_body_basis": final["body_identity"]["basis"],
        "final_source_sha256": hashlib.sha256(changed).hexdigest(),
    }


def evaluate(binary: Path) -> dict:
    with tempfile.TemporaryDirectory() as directory:
        base = Path(directory)
        delta_root = base / "delta"
        whole_root = base / "whole"
        fixture(delta_root)
        fixture(whole_root)
        initial, _ = semantic(binary, delta_root)
        whole, delta = payloads(initial)
        delta_result = exercise(binary, delta_root, "edit-body-semantic", delta)
        whole_result = exercise(binary, whole_root, "replace-body-semantic", whole)
        stale_root = base / "stale"
        fixture(stale_root)
        (stale_root / "change.json").write_text(compact(delta), encoding="utf-8")
        (stale_root / "app.rs").write_text(SOURCE.replace("value + 1", "value + 2"), encoding="utf-8")
        current, _ = semantic(binary, stale_root)
        refused, _ = invoke(
            binary,
            stale_root,
            [
                "author", "edit-body-semantic", current["selection"]["handle"],
                "--from", "change.json", "--write",
            ],
            expected=1,
        )
        stale_refused = "base does not match" in refused["error"]["message"]
    equivalent = (
        delta_result["final_body_basis"] == whole_result["final_body_basis"]
        and delta_result["final_source_sha256"] == whole_result["final_source_sha256"]
    )
    return {
        "schema": "fr-semantic-delta-evaluation-1",
        "fixture": "generic six-statement arithmetic pipeline",
        "change": "replace one integer expression",
        "routes": {"semantic-delta": delta_result, "complete-body": whole_result},
        "equivalence": {
            "final_body_identity_equal": equivalent,
            "final_source_identity_equal": equivalent,
        },
        "stale_base_refused_before_write": stale_refused,
        "scope": "Deterministic CLI payload and lifecycle evidence on one generic Rust fixture.",
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
