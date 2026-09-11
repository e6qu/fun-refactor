#!/usr/bin/env python3
"""Compare source-fragment and semantic-body routes on one exact change."""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile

SOURCE = """pub fn calc(value: i32) -> i32 {
    value + 1
}

pub fn untouched(value: i32) -> i32 {
    value - 1
}
"""
SOURCE_BODY = "{\n    return value * 2;\n}\n"
SEMANTIC_BODY = {
    "schema": "fr-semantic-body-1",
    "body": [{
        "kind": "return",
        "value": {
            "kind": "binary",
            "value": {
                "op": "mul",
                "left": {"kind": "name", "value": "value"},
                "right": {"kind": "int", "value": "2"},
            },
        },
    }],
}


def sha(data):
    return hashlib.sha256(data).hexdigest()


def invoke(binary, root, args):
    result = subprocess.run(
        [str(binary), "--json", "--no-cache", "-C", str(root), *args],
        capture_output=True, timeout=120)
    assert result.returncode == 0, (args, result.stdout, result.stderr)
    assert not result.stderr, result.stderr
    return json.loads(result.stdout), result.stdout


def request_bytes(args):
    return len(json.dumps({"tool": "fr", "args": args}, separators=(",", ":")).encode())


def run_arm(binary, base, semantic):
    root = base / ("semantic" if semantic else "source")
    root.mkdir()
    source = root / "app.rs"
    source.write_text(SOURCE)
    payload = base / ("semantic-body.json" if semantic else "body.rs")
    payload.write_text(
        json.dumps(SEMANTIC_BODY, separators=(",", ":")) if semantic else SOURCE_BODY)

    outputs = 0
    requests = 0
    calls = 0
    semantic_report_bytes = 0
    semantic_nodes = None
    if semantic:
        semantic_args = [
            "project", "semantic", "app.rs", "--declaration", "calc", "--body", "--nodes", "256",
            "--minimal",
        ]
        model, raw = invoke(binary, root, semantic_args)
        assert model["status"] == "returned" and model["source_policy"] == "source-free"
        assert '"source":' not in raw.decode()
        semantic_nodes = model["node_budget"]["required"]
        semantic_report_bytes = len(raw)
        outputs += len(raw)
        requests += request_bytes(semantic_args)
        calls += 1
        handle = model["selection"]["handle"]
        exposed = 0
    else:
        find_args = [
            "project", "find", "calc", "--signature", "--source", "--bytes", "65536",
        ]
        found, raw = invoke(binary, root, find_args)
        outputs += len(raw)
        requests += request_bytes(find_args)
        calls += 1
        row = dict(zip(found["columns"], found["rows"][0]))
        handle = row["handle"]
        exposed = len(row["source"]["text"].encode())

    operation = "replace-body-semantic" if semantic else "replace-body"
    author_args = ["author", operation, handle, "--from", str(payload), "--write"]
    changed, raw = invoke(binary, root, author_args)
    assert changed["applied"] is True
    if semantic:
        assert changed["semantic_input"]["source_free"] is True
        assert changed["semantic_render"]["fidelity"]["carried_verbatim"] == 0
    outputs += len(raw)
    requests += request_bytes(["author", operation, "<HANDLE>", "--from", "<BODY>", "--write"])
    calls += 1
    transaction = str(changed["transaction"])

    for action in ("undo", "redo"):
        args = ["history", action, transaction, "--write", "--no-diff"]
        _, raw = invoke(binary, root, args)
        outputs += len(raw)
        requests += request_bytes(["history", action, "<TX>", "--write", "--no-diff"])
        calls += 1
    patch, raw = invoke(binary, root, ["history", "patch", transaction])
    outputs += len(raw)
    requests += request_bytes(["history", "patch", "<TX>"])
    calls += 1
    final = source.read_bytes()
    assert b"value * 2" in final and b"value - 1" in final
    return {
        "calls": calls,
        "visible_output_bytes": outputs,
        "request_bytes": requests,
        "input_bytes": payload.stat().st_size,
        "source_text_bytes_exposed": exposed,
        "semantic_report_bytes": semantic_report_bytes,
        "semantic_nodes": semantic_nodes,
        "source_sha256": sha(final),
        "patch_sha256": sha(patch["patch"].encode()),
    }


def measure(binary):
    with tempfile.TemporaryDirectory(prefix="fr-semantic-context-") as directory:
        base = Path(directory)
        source = run_arm(binary, base, False)
        semantic = run_arm(binary, base, True)
    assert source["source_sha256"] == semantic["source_sha256"]
    assert source["patch_sha256"] == semantic["patch_sha256"]
    return {
        "schema": "fr-semantic-context-1",
        "passed": True,
        "binary_sha256": sha(binary.read_bytes()),
        "fixture": "generic Rust function-body change with an unrelated declaration",
        "source_fragment": source,
        "semantic_body": semantic,
        "claims": [
            "Both routes produce identical changed source and Git patch bytes.",
            "The semantic route exposes no source text and retains complete typed nodes.",
            "Byte counts describe deterministic CLI payloads, not model tokens or agent success.",
        ],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fr", required=True, type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    report = measure(args.fr.resolve())
    text = json.dumps(report, indent=2) + "\n"
    if args.output:
        args.output.write_text(text)
    else:
        print(text, end="")


if __name__ == "__main__":
    main()
