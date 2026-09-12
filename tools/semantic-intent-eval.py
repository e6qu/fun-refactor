#!/usr/bin/env python3
"""Compare whole-body, pointer-delta, direct-intent and Python-intent routes."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
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
    report = json.loads(result.stdout)
    measured = result.stdout
    if isinstance(report, dict) and isinstance(report.get("receiving_root"), str):
        encoded_root = json.dumps(report["receiving_root"], ensure_ascii=False)[1:-1].encode()
        measured = measured.replace(encoded_root, b"$ROOT")
    return report, len(measured)


def semantic(binary: Path, root: Path, mode: str = "body") -> tuple[dict, int]:
    arguments = [
        "project", "semantic", "app.rs", "--declaration", "summarize", "--body",
        "--nodes", "128", "--minimal",
    ]
    if mode == "pointers":
        arguments.append("--pointers")
    elif mode == "locators":
        arguments.extend([
            "--locators", "--locators-only", "--locator-op", "set-int", "--locator-from", "1"
        ])
    return invoke(binary, root, arguments)


def fixture(root: Path) -> None:
    root.mkdir()
    (root / "app.rs").write_text(SOURCE, encoding="utf-8")


def body_and_delta(report: dict) -> tuple[dict, dict]:
    body = {"schema": "fr-semantic-body-1", "body": report["model"]["items"][0]["value"]["body"]}
    changed = json.loads(compact(body))
    changed["body"][0]["value"]["value"]["value"]["right"] = {"kind": "int", "value": "7"}
    delta = {
        "schema": "fr-semantic-change-1",
        "base": report["body_identity"]["basis"],
        "operations": [{
            "op": "replace", "path": POINTER, "category": "expression",
            "value": {"kind": "int", "value": "7"},
        }],
    }
    return changed, delta


def direct_intent(report: dict) -> dict:
    matches = [
        row for row in report["body_locators"]["rows"]
        if row["operation"] == "set-int" and row["from"] == "1"
    ]
    if len(matches) != 1:
        raise RuntimeError(f"expected one integer intent target, found {len(matches)}")
    row = matches[0]
    return {
        "schema": "fr-semantic-intent-1",
        "base": report["body_identity"]["basis"],
        "operations": [{"op": "set-int", "target": row["target"], "from": "1", "to": "7"}],
    }


def sdk_intent(repository: Path, root: Path, basis: str, target: list[dict]) -> tuple[bytes, int]:
    producer = f'''from fr_ir import Intent, LocatorStep, SemanticIntent
target = [LocatorStep(**step) for step in {target!r}]
SemanticIntent("{basis}", [Intent.SetInt(target, "1", "7")]).write("change.json", indent=None)
'''
    environment = os.environ.copy()
    environment["PYTHONPATH"] = str(repository / "sdk/python/src")
    result = subprocess.run(
        ["python3", "-c", producer], cwd=root, env=environment, check=False, capture_output=True
    )
    if result.returncode:
        raise RuntimeError(result.stderr.decode())
    return (root / "change.json").read_bytes(), len(producer.encode())


def exercise(
    binary: Path,
    root: Path,
    operation: str,
    payload: bytes,
    query_mode: str,
    producer_bytes: int = 0,
) -> dict:
    (root / "change.json").write_bytes(payload)
    initial = (root / "app.rs").read_bytes()
    query, query_bytes = semantic(binary, root, query_mode)
    handle = query["selection"]["handle"]
    preview, preview_bytes = invoke(binary, root, ["author", operation, handle, "--from", "change.json"])
    written, write_bytes = invoke(
        binary, root, ["author", operation, handle, "--from", "change.json", "--write"]
    )
    transaction = str(written["transaction"])
    changed = (root / "app.rs").read_bytes()
    compiled = subprocess.run(
        ["rustc", "--edition=2021", "--test", "app.rs", "-o", "app-test"],
        cwd=root,
        check=False,
        capture_output=True,
    )
    tested = compiled.returncode == 0 and subprocess.run(
        [str(root / "app-test")], cwd=root, check=False, capture_output=True
    ).returncode == 0
    invoke(binary, root, ["history", "undo", transaction, "--write"])
    undo_exact = (root / "app.rs").read_bytes() == initial
    patch, patch_bytes = invoke(binary, root, ["history", "patch", transaction, "--check"])
    invoke(binary, root, ["history", "redo", transaction, "--write"])
    redo_exact = (root / "app.rs").read_bytes() == changed
    reverse, _ = invoke(binary, root, ["history", "patch", transaction, "--reverse", "--check"])
    final, _ = semantic(binary, root)
    measured = query_bytes + len(payload) + producer_bytes + preview_bytes + write_bytes + patch_bytes
    return {
        "payload_bytes": len(payload),
        "producer_bytes": producer_bytes,
        "semantic_query_bytes": query_bytes,
        "preview_bytes": preview_bytes,
        "write_bytes": write_bytes,
        "patch_report_bytes": patch_bytes,
        "measured_context_bytes": measured,
        "changed": preview["changed"],
        "behavior_passed": tested,
        "patch_checked": patch["matches_patch_basis"] and reverse["matches_patch_basis"],
        "undo_exact": undo_exact,
        "redo_exact": redo_exact,
        "final_body_basis": final["body_identity"]["basis"],
        "final_source_sha256": hashlib.sha256(changed).hexdigest(),
    }


def evaluate(binary: Path, repository: Path) -> dict:
    with tempfile.TemporaryDirectory() as directory:
        base = Path(directory)
        roots = {name: base / name for name in ("whole", "delta", "direct-intent", "python-intent")}
        for root in roots.values():
            fixture(root)
        body_report, _ = semantic(binary, roots["whole"])
        locator_report, _ = semantic(binary, roots["direct-intent"], "locators")
        whole, delta = body_and_delta(body_report)
        intent = direct_intent(locator_report)
        target = intent["operations"][0]["target"]
        sdk_payload, producer_bytes = sdk_intent(
            repository, roots["python-intent"], intent["base"], target
        )
        routes = {
            "complete-body": exercise(
                binary, roots["whole"], "replace-body-semantic", compact(whole).encode(), "body"
            ),
            "pointer-delta": exercise(
                binary, roots["delta"], "edit-body-semantic", compact(delta).encode(), "pointers"
            ),
            "direct-intent": exercise(
                binary, roots["direct-intent"], "edit-body-intent", compact(intent).encode(), "locators"
            ),
            "python-intent": exercise(
                binary, roots["python-intent"], "edit-body-intent", sdk_payload, "locators", producer_bytes
            ),
        }
    body_ids = {result["final_body_basis"] for result in routes.values()}
    source_ids = {result["final_source_sha256"] for result in routes.values()}
    return {
        "schema": "fr-semantic-intent-evaluation-1",
        "fixture": "generic six-statement arithmetic pipeline",
        "change": "replace one integer scalar",
        "routes": routes,
        "equivalence": {
            "final_body_identity_equal": len(body_ids) == 1,
            "final_source_identity_equal": len(source_ids) == 1,
        },
        "scope": "Deterministic CLI and Python payload plus lifecycle evidence on one generic Rust fixture.",
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--fr", type=Path, default=Path("target/debug/fr"))
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    repository = Path(__file__).resolve().parent.parent
    report = evaluate(args.fr.resolve(), repository)
    rendered = json.dumps(report, indent=2) + "\n"
    if args.output:
        args.output.write_text(rendered, encoding="utf-8")
    else:
        print(rendered, end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
