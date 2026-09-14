#!/usr/bin/env python3
"""Exercise structural disclosure capabilities and independently verify their identities."""

import argparse
from collections import deque
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
EDIT_SCHEMA = "fr-disclosed-ir-edit-1"
TREE_SCHEMA = "fr-semantic-merkle-1"
BODY_SCHEMA = "fr-semantic-body-1"
LIMIT = 16384


def encoded(value):
    return json.dumps(value, ensure_ascii=False, separators=(",", ":")).encode()


def digest(value):
    return hashlib.sha256(value).hexdigest()


def hash_json(value):
    return digest(encoded(value))


def merkle(value):
    if value is None:
        return hash_json([TREE_SCHEMA, "null"])
    if isinstance(value, bool):
        return hash_json([TREE_SCHEMA, "bool", value])
    if isinstance(value, (int, float)):
        return hash_json([TREE_SCHEMA, "number", str(value).lower()])
    if isinstance(value, str):
        return hash_json([TREE_SCHEMA, "string", value])
    if isinstance(value, list):
        return hash_json([TREE_SCHEMA, "array", [merkle(item) for item in value]])
    if isinstance(value, dict):
        return hash_json([TREE_SCHEMA, "object", [[key, merkle(value[key])] for key in sorted(value)]])
    raise AssertionError(type(value))


def run(binary, project, arguments, success=True):
    completed = subprocess.run(
        [str(binary), "--json", "--no-cache", "-C", str(project), *arguments],
        capture_output=True,
        timeout=30,
    )
    assert (completed.returncode == 0) == success, (
        arguments,
        completed.stdout,
        completed.stderr,
    )
    return json.loads(completed.stdout), len(completed.stdout)


def compile_and_run(project):
    executable = project.parent / "app"
    completed = subprocess.run(
        ["rustc", "--edition=2021", "-o", executable, project / "app.rs"],
        capture_output=True,
        timeout=30,
    )
    assert completed.returncode == 0, completed.stderr
    return subprocess.check_output([executable], timeout=30).decode().strip()


def related(left, right):
    return left == right or left.startswith(right + "/") or right.startswith(left + "/")


def disclosed_capabilities(binary, project, target, body_basis, specifications):
    initial, size = run(binary, project, [
        "project", "disclose", target, "--token-limit", str(LIMIT), "--profile", "expanded",
    ])
    assert "model" not in initial and "text" not in initial
    wanted = {
        capability_id(initial, target, body_basis, specification):
            "/model/items/0/value" + specification["address"]
        for specification in specifications
    }
    pending = deque()
    for shortcut in initial["semantic_shortcuts"]:
        address = shortcut["address"].split("#", 1)[1]
        if shortcut.get("editable_ir", 0) > 0 and any(
            related(address, wanted_address) for wanted_address in wanted.values()
        ):
            pending.append(shortcut["reveal"]["arguments"])
    seen = set()
    found = []
    sizes = [size]
    while pending:
        command = pending.popleft()
        key = tuple(command)
        if key in seen:
            continue
        seen.add(key)
        report, size = run(binary, project, command)
        sizes.append(size)
        assert size == report["token_budget"]["used_upper_bound"] <= LIMIT
        revealed = report["revealed"]
        parent_address = revealed["address"].split("#", 1)[1]
        for edit in revealed.get("ir_edits", []):
            found.append((parent_address, edit))
        for child in revealed.get("children", []):
            segment = str(child["key"]).replace("~", "~0").replace("/", "~1")
            child_address = parent_address + "/" + segment
            for edit in child.get("ir_edits", []):
                found.append((child_address, edit))
            hole = child.get("hole", {})
            if "reveal" in hole and any(
                related(child_address, wanted_address) for wanted_address in wanted.values()
            ):
                pending.append(hole["reveal"]["arguments"])
        if "continuation" in report:
            pending.append(report["continuation"]["arguments"])
        if wanted.keys() <= {edit["id"] for _, edit in found}:
            break
    unique = {edit["id"]: (address, edit) for address, edit in found if edit["id"] in wanted}
    assert wanted.keys() <= unique.keys()
    return initial, list(unique.values()), sizes


def capability_id(initial, target, body_basis, specification):
    return "frdi1:" + hash_json([
        EDIT_SCHEMA,
        initial["revision"],
        target,
        body_basis,
        specification["address"],
        specification["path"],
        specification["index"],
        specification["category"],
        specification["operation"],
        specification["placement"],
        merkle(specification["current"]),
    ])


def fixture_specifications(body):
    return {
        "replace": {
            "address": "/body/3/value/value", "path": "/body/3/value/value", "index": None,
            "category": "expression", "operation": "replace", "placement": "replace",
            "current": body[3]["value"]["value"],
            "value": {"kind": "int", "value": "7"}, "behavior": "7",
        },
        "delete": {
            "address": "/body/0", "path": "/body/0", "index": None,
            "category": "statement", "operation": "delete-statement", "placement": "delete",
            "current": body[0], "value": None, "behavior": "1",
        },
        "insert-before": {
            "address": "/body/0", "path": "/body", "index": 0,
            "category": "statement", "operation": "insert-statement", "placement": "before",
            "current": body, "value": {"kind": "comment", "value": "before"}, "behavior": "1",
        },
        "append-empty": {
            "address": "/body/1/value/then", "path": "/body/1/value/then", "index": 0,
            "category": "statement", "operation": "insert-statement", "placement": "append",
            "current": body[1]["value"]["then"],
            "value": {"kind": "comment", "value": "inside"}, "behavior": "1",
        },
    }


def fixture_body_for_rust_identity():
    """Reproduce typed Rust field order rather than report-map key order."""
    def condition(operator):
        return {"kind": "binary", "value": {
            "op": operator,
            "left": {"kind": "name", "value": "value"},
            "right": {"kind": "int", "value": "0"},
        }}

    return [
        {"kind": "comment", "value": "marker"},
        {"kind": "if", "value": {
            "condition": condition("gt"), "then": [], "otherwise": [],
        }},
        {"kind": "if", "value": {
            "condition": condition("lt"), "then": [], "otherwise": [],
        }},
        {"kind": "let", "value": {
            "name": "current", "ty": None,
            "value": {"kind": "int", "value": "1"}, "mutable": False,
        }},
        {"kind": "return", "value": {"kind": "name", "value": "current"}},
    ]


def semantic_operation(specification):
    operation = {"op": specification["operation"], "path": specification["path"]}
    if specification["operation"] == "replace":
        operation.update(category=specification["category"], value=specification["value"])
    elif specification["operation"] == "insert-statement":
        operation.update(index=specification["index"], value=specification["value"])
    return operation


def measure(binary):
    source = (
        "fn adjust(value: i32) -> i32 { // marker\n"
        "if value > 0 { } if value < 0 { } let current = 1; current }\n"
        "fn main() { println!(\"{}\", adjust(2)); }\n"
    )
    results = {}
    all_sizes = []
    duplicate_empty_ids = []
    for name in ["replace", "delete", "insert-before", "append-empty"]:
        with tempfile.TemporaryDirectory(prefix="fr-disclosed-ir-edit-") as temporary:
            outer = Path(temporary)
            project = outer / "workspace"
            project.mkdir()
            (project / "app.rs").write_text(source)
            assert compile_and_run(project) == "1"
            found, _ = run(binary, project, ["project", "find", "adjust"])
            target = found["rows"][0][0]
            semantic, _ = run(
                binary, project, ["project", "semantic", target, "--body", "--nodes", "4096"]
            )
            body = semantic["model"]["items"][0]["value"]["body"]
            expected_body = fixture_body_for_rust_identity()
            assert body == expected_body
            body_value = {"schema": BODY_SCHEMA, "body": expected_body}
            body_basis = "frsb1:" + hash_json(body_value)
            assert semantic["body_identity"]["basis"] == body_basis
            specifications = fixture_specifications(body)
            specification = specifications[name]
            requested_specifications = [specification]
            if name == "append-empty":
                second = dict(specification)
                second.update(address="/body/2/value/then", path="/body/2/value/then",
                              current=body[2]["value"]["then"])
                requested_specifications.append(second)
            initial, capabilities, sizes = disclosed_capabilities(
                binary, project, target, body_basis, requested_specifications
            )
            all_sizes.extend(sizes)
            expected = capability_id(initial, target, body_basis, specification)
            matches = [(address, edit) for address, edit in capabilities if edit["id"] == expected]
            assert len(matches) == 1
            address, edit = matches[0]
            assert address == "/model/items/0/value" + specification["address"], (
                name, address, specification["address"], edit
            )
            assert edit["current_commitment"]["digest"] == merkle(specification["current"])
            if name == "append-empty":
                duplicate_empty_ids = [expected, capability_id(initial, target, body_basis, second)]
                assert duplicate_empty_ids[0] != duplicate_empty_ids[1]
                assert {identity for _, item in capabilities for identity in [item["id"]]}.issuperset(duplicate_empty_ids)

            request = {"edit": expected}
            if specification["value"] is not None:
                request["value"] = specification["value"]
                (outer / "node.json").write_bytes(encoded(specification["value"]))
            command = ["author", "edit-body-disclosed-ir", target, "--edit", expected]
            if specification["value"] is not None:
                command.extend(["--from", "../node.json"])
            capability_preview, preview_bytes = run(binary, project, command)
            assert (project / "app.rs").read_text() == source

            change = {
                "schema": "fr-semantic-change-1", "base": body_basis,
                "operations": [semantic_operation(specification)],
            }
            (outer / "change.json").write_bytes(encoded(change))
            explicit_preview, _ = run(
                binary, project, ["author", "edit-body-semantic", target, "--from", "../change.json"]
            )
            assert capability_preview["diff"] == explicit_preview["diff"]
            assert capability_preview["body"]["after_sha256"] == explicit_preview["body"]["after_sha256"]

            written, _ = run(binary, project, [*command, "--write"])
            transaction = str(written["transaction"])
            assert compile_and_run(project) == specification["behavior"]
            stale, _ = run(binary, project, command, success=False)
            assert "stale" in stale["error"]["message"]
            reverse, _ = run(binary, project, ["history", "patch", transaction, "--reverse", "--check"])
            assert reverse["matches_patch_basis"] is True
            run(binary, project, ["history", "undo", transaction, "--write"])
            assert (project / "app.rs").read_text() == source
            forward, _ = run(binary, project, ["history", "patch", transaction, "--check"])
            assert forward["matches_patch_basis"] is True
            run(binary, project, ["history", "redo", transaction, "--write"])
            assert compile_and_run(project) == specification["behavior"]
            results[name] = {
                "identity_independently_verified": True,
                "explicit_change_equivalent": True,
                "behavior": specification["behavior"],
                "disclosure_responses": len(sizes),
                "disclosure_bytes": sum(sizes),
                "preview_bytes": preview_bytes,
            }

    return {
        "schema": "fr-disclosed-ir-edit-eval-1",
        "passed": True,
        "tool_sha256": digest(Path(__file__).read_bytes()),
        "fixture_sha256": digest(source.encode()),
        "operations": results,
        "identity": {"same_shaped_empty_lists_distinct": len(set(duplicate_empty_ids)) == 2},
        "flow": {
            "source_disclosed": False, "stale_refused": True, "preview_read_only": True,
            "patch_checked": True, "undo_checked": True, "redo_checked": True,
        },
        "budget": {
            "limit": LIMIT, "disclosure_responses": len(all_sizes),
            "maximum_disclosure_response_bytes": max(all_sizes),
            "disclosure_bytes": sum(all_sizes),
        },
        "scope": "Generated generic Rust fixture with independent capability and Merkle calculation, explicit semantic-change equivalence, rustc behavior checks and exact history checks; SHA-256 collision resistance, parser, writer, compiler and filesystem behavior remain trusted or tested boundaries.",
    }


def audit(path):
    report = json.loads(path.read_text())
    assert report["schema"] == "fr-disclosed-ir-edit-eval-1" and report["passed"] is True
    assert report["tool_sha256"] == digest(Path(__file__).read_bytes())
    assert set(report["operations"]) == {"replace", "delete", "insert-before", "append-empty"}
    assert all(item["identity_independently_verified"] for item in report["operations"].values())
    assert all(item["explicit_change_equivalent"] for item in report["operations"].values())
    assert report["identity"]["same_shaped_empty_lists_distinct"] is True
    assert report["flow"] == {
        "source_disclosed": False, "stale_refused": True, "preview_read_only": True,
        "patch_checked": True, "undo_checked": True, "redo_checked": True,
    }
    assert report["budget"]["maximum_disclosure_response_bytes"] <= report["budget"]["limit"]
    return {"schema": report["schema"], "passed": True, "report": str(path)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fr", type=Path, default=ROOT / "target/debug/fr")
    parser.add_argument("--audit", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    report = audit(args.audit.resolve(strict=True)) if args.audit else measure(args.fr.resolve())
    rendered = json.dumps(report, indent=2) + "\n"
    if args.output:
        args.output.write_text(rendered)
    else:
        print(rendered, end="")


if __name__ == "__main__":
    main()
