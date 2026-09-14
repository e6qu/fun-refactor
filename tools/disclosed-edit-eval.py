#!/usr/bin/env python3
"""Exercise disclosure-bound editing and independently verify edit identities."""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
EDIT_SCHEMA = "fr-disclosed-edit-1"
BODY_SCHEMA = "fr-semantic-body-1"


def encoded(value):
    return json.dumps(value, ensure_ascii=False, separators=(",", ":")).encode()


def digest(value):
    return hashlib.sha256(value).hexdigest()


def hash_json(value):
    return digest(encoded(value))


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


def arguments(value):
    return value["reveal"]["arguments"]


def locator(pointer):
    prefix = "/model/items/0/value/body/0/value"
    assert pointer.startswith(prefix)
    steps = [{
        "category": "statement", "index": 0, "kind": "return",
        "label": None, "role": "statement",
    }, {
        "category": "expression", "index": None, "kind": "binary",
        "label": None, "role": "result",
    }]
    suffix = pointer.removeprefix(prefix)
    if suffix == "/value/left/value/right/value":
        steps.extend([{
            "category": "expression", "index": None, "kind": "binary",
            "label": None, "role": "left",
        }, {
            "category": "expression", "index": None, "kind": "int",
            "label": None, "role": "right",
        }])
    elif suffix == "/value/right/value":
        steps.append({
            "category": "expression", "index": None, "kind": "int",
            "label": None, "role": "right",
        })
    else:
        raise AssertionError(f"unexpected scalar pointer {pointer}")
    return steps


def compile_and_run(project):
    executable = project / "app"
    completed = subprocess.run(
        ["rustc", "--edition=2021", "-o", executable, project / "app.rs"],
        capture_output=True,
        timeout=30,
    )
    assert completed.returncode == 0, completed.stderr
    return subprocess.check_output([executable], timeout=30).decode().strip()


def measure(binary):
    source = (
        "fn score(value: i32) -> i32 { value + 1 + 1 }\n"
        "fn main() { println!(\"{}\", score(3)); }\n"
    )
    with tempfile.TemporaryDirectory(prefix="fr-disclosed-edit-") as temporary:
        project = Path(temporary)
        (project / "app.rs").write_text(source)
        assert compile_and_run(project) == "5"

        found, _ = run(binary, project, ["project", "find", "score"])
        target = found["rows"][0][0]
        semantic, _ = run(
            binary, project, ["project", "semantic", target, "--body", "--nodes", "4096"]
        )
        assert semantic["body_identity"]["schema"] == BODY_SCHEMA
        assert semantic["body_identity"]["source_free"] is True
        body_basis = semantic["body_identity"]["basis"]

        initial, initial_bytes = run(binary, project, [
            "project", "disclose", target, "--token-limit", "16384", "--profile", "expanded",
        ])
        assert "model" not in initial and "text" not in initial
        responses = [initial_bytes]
        edits = []
        for shortcut in initial["semantic_shortcuts"]:
            if shortcut.get("kind") != "int" or shortcut["editable_scalars"] != 1:
                continue
            revealed, size = run(binary, project, arguments(shortcut))
            responses.append(size)
            assert size == revealed["token_budget"]["used_upper_bound"] <= 16384
            for child in revealed["revealed"]["children"]:
                if "edit" not in child:
                    continue
                edit = child["edit"]
                pointer = revealed["revealed"]["address"].split("#", 1)[1] + "/value"
                body_pointer = pointer.removeprefix("/model/items/0/value")
                expected = "frde1:" + hash_json([
                    EDIT_SCHEMA,
                    initial["revision"],
                    target,
                    body_basis,
                    body_pointer,
                    edit["operation"],
                    edit["from"],
                    locator(pointer),
                ])
                assert edit["id"] == expected
                assert edit["preview_template"]["arguments"][4] == expected
                edits.append(edit)
        assert len(edits) == 2 and edits[0]["id"] != edits[1]["id"]

        ambiguous, _ = run(binary, project, [
            "author", "edit-body-scalar", target,
            "--operation", "set-int", "--from", "1", "--to", "7",
        ], success=False)
        assert "found 2" in ambiguous["error"]["message"]

        command = [
            "author", "edit-body-disclosed", target,
            "--edit", edits[0]["id"], "--to", "7",
        ]
        preview, preview_bytes = run(binary, project, command)
        assert preview["query"] == "edit-body-disclosed"
        assert preview["disclosed_edit"]["exact_target"] is True
        assert preview["disclosed_edit"]["refinement_checked"] is True
        assert (project / "app.rs").read_text() == source

        written, _ = run(binary, project, [*command, "--write"])
        transaction = str(written["transaction"])
        assert compile_and_run(project) == "11"
        stale, _ = run(binary, project, command, success=False)
        assert "stale" in stale["error"]["message"]
        reverse, _ = run(binary, project, [
            "history", "patch", transaction, "--reverse", "--check",
        ])
        assert reverse["matches_patch_basis"] is True
        run(binary, project, ["history", "undo", transaction, "--write"])
        assert compile_and_run(project) == "5"
        patch, _ = run(binary, project, ["history", "patch", transaction, "--check"])
        assert patch["matches_patch_basis"] is True
        run(binary, project, ["history", "redo", transaction, "--write"])
        assert compile_and_run(project) == "11"

    return {
        "schema": "fr-disclosed-edit-eval-1",
        "passed": True,
        "tool_sha256": digest(Path(__file__).read_bytes()),
        "fixture_sha256": digest(source.encode()),
        "identity": {
            "independently_verified": len(edits),
            "distinct_ambiguous_values": True,
            "stale_refused": True,
        },
        "flow": {
            "source_disclosed": False,
            "legacy_ambiguous_route_refused": True,
            "preview_read_only": True,
            "typed_intent_refinement_checked": True,
            "patch_checked": True,
            "undo_checked": True,
            "redo_checked": True,
            "behavior": {"original": "5", "changed": "11", "undone": "5", "redone": "11"},
        },
        "budget": {
            "limit": 16384,
            "disclosure_responses": len(responses),
            "maximum_disclosure_response_bytes": max(responses),
            "disclosure_bytes": sum(responses),
            "preview_bytes": preview_bytes,
        },
        "scope": "Generated generic Rust fixture with independent edit-identity calculation, rustc behavior checks and exact history checks; SHA-256 collision resistance, parser, writer, compiler and filesystem behavior remain trusted or tested boundaries.",
    }


def audit(path):
    report = json.loads(path.read_text())
    assert report["schema"] == "fr-disclosed-edit-eval-1" and report["passed"] is True
    assert report["tool_sha256"] == digest(Path(__file__).read_bytes())
    assert report["identity"]["independently_verified"] == 2
    assert report["identity"]["distinct_ambiguous_values"] is True
    assert report["identity"]["stale_refused"] is True
    assert report["flow"]["source_disclosed"] is False
    assert report["flow"]["legacy_ambiguous_route_refused"] is True
    assert report["flow"]["preview_read_only"] is True
    assert report["flow"]["typed_intent_refinement_checked"] is True
    assert report["flow"]["patch_checked"] is True
    assert report["flow"]["undo_checked"] is True
    assert report["flow"]["redo_checked"] is True
    assert report["flow"]["behavior"] == {
        "original": "5", "changed": "11", "undone": "5", "redone": "11",
    }
    assert report["budget"]["maximum_disclosure_response_bytes"] <= report["budget"]["limit"]
    return {"schema": report["schema"], "passed": True, "report": str(path)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fr", type=Path, default=ROOT / "target/debug/fr")
    parser.add_argument("--audit", type=Path)
    args = parser.parse_args()
    report = audit(args.audit.resolve(strict=True)) if args.audit else measure(args.fr.resolve())
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
