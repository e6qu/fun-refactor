#!/usr/bin/env python3
"""Reconstruct and independently verify source-free project evidence disclosure."""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
PROOF_SCHEMA = "fr-merkle-inclusion-1"
OBJECT_SCHEMA = "fr-merkle-object-1"


def hashed(value):
    return hashlib.sha256(json.dumps(
        value, ensure_ascii=False, separators=(",", ":")
    ).encode()).hexdigest()


def binary_root(level):
    level = list(level)
    if not level:
        return hashed([OBJECT_SCHEMA, "empty"])
    while len(level) > 1:
        level = [
            hashed([OBJECT_SCHEMA, "pair", level[index], level[index + 1]])
            if index + 1 < len(level) else level[index]
            for index in range(0, len(level), 2)
        ]
    return level[0]


def object_digest(value):
    if value is None:
        return hashed([OBJECT_SCHEMA, "null"])
    if isinstance(value, bool):
        return hashed([OBJECT_SCHEMA, "bool", value])
    if isinstance(value, (int, float)):
        return hashed([OBJECT_SCHEMA, "number", json.dumps(value).lower()])
    if isinstance(value, str):
        return hashed([OBJECT_SCHEMA, "string", value])
    if isinstance(value, list):
        leaves = [hashed([OBJECT_SCHEMA, "array-entry", index, object_digest(child)])
                  for index, child in enumerate(value)]
        return hashed([OBJECT_SCHEMA, "array", len(value), binary_root(leaves)])
    assert isinstance(value, dict)
    leaves = [hashed([OBJECT_SCHEMA, "object-entry", index, key, object_digest(value[key])])
              for index, key in enumerate(sorted(value))]
    return hashed([OBJECT_SCHEMA, "object", len(value), binary_root(leaves)])


def verify_commitment(digest, proof):
    if proof["schema"] != PROOF_SCHEMA or digest != proof["leaf"]:
        return False
    current = digest
    for step in proof["path"]:
        index, width = step["index"], step["length"]
        if not 0 <= index < width:
            return False
        if step["container"] == "array":
            current = hashed([OBJECT_SCHEMA, "array-entry", index, current])
        else:
            current = hashed([OBJECT_SCHEMA, "object-entry", index, step["key"], current])
        for sibling in step["branch"]:
            expected = "left" if index % 2 else (
                "right" if index + 1 < width else "promote"
            )
            if sibling["side"] != expected:
                return False
            if expected == "left":
                current = hashed([OBJECT_SCHEMA, "pair", sibling["digest"], current])
            elif expected == "right":
                current = hashed([OBJECT_SCHEMA, "pair", current, sibling["digest"]])
            index //= 2
            width = (width + 1) // 2
        if width != 1:
            return False
        current = hashed([OBJECT_SCHEMA, step["container"], step["length"], current])
    return current == proof["root"]


def run(binary, project, arguments, success=True):
    result = subprocess.run(
        [str(binary), "--json", "--no-cache", "-C", str(project), *arguments],
        capture_output=True, timeout=60,
    )
    assert (result.returncode == 0) == success, (arguments, result.stdout, result.stderr)
    return json.loads(result.stdout), len(result.stdout)


def assert_budget(report, size):
    budget = report["token_budget"]
    assert budget["used_upper_bound"] == size <= budget["limit"]


def reconstruct(binary, project, arguments, root, sizes, proofs):
    children = []
    fragments = []
    value = None
    node_leaf = None
    while arguments:
        report, size = run(binary, project, arguments)
        assert_budget(report, size)
        sizes.append(size)
        revealed = report["revealed"]
        proof = revealed["proof"]
        assert proof["root"] == root
        assert verify_commitment(proof["leaf"], proof)
        if node_leaf is None:
            node_leaf = proof["leaf"]
        else:
            assert node_leaf == proof["leaf"]
        proofs.append((proof["leaf"], proof))
        if "value" in revealed:
            value = revealed["value"]
        elif "value_fragment" in revealed:
            fragments.append(revealed["value_fragment"])
        else:
            children.extend(revealed["children"])
        arguments = report.get("continuation", {}).get("arguments")
    if value is not None:
        result = value
    elif fragments:
        result = "".join(fragments)
    else:
        keyed = []
        for child in children:
            child_value = child.get("value")
            if "hole" in child:
                child_value = reconstruct(
                    binary, project, child["hole"]["reveal"]["arguments"], root, sizes, proofs
                )
                assert child["hole"]["object_digest"] == object_digest(child_value)
            keyed.append((child["key"], child_value))
        if all(key.isdigit() for key, _ in keyed):
            result = [value for _, value in sorted(keyed, key=lambda item: int(item[0]))]
        else:
            result = {key: value for key, value in keyed}
    assert object_digest(result) == node_leaf
    return result


def measure(binary):
    source = (
        "def normalize(value):\n"
        "    clean = value.strip()\n"
        "    stored = clean.lower()\n"
        "    return stored\n\n"
        "def endpoint(raw):\n"
        "    return normalize(raw)\n"
    )
    with tempfile.TemporaryDirectory(prefix="fr-progressive-evidence-") as temporary:
        project = Path(temporary)
        (project / "service.py").write_text(source)
        found, _ = run(binary, project, ["project", "find", "normalize"])
        handle = found["rows"][0][0]
        initial, initial_size = run(binary, project, [
            "project", "disclose", handle, "--view", "evidence", "--depth", "3",
            "--token-limit", "4096",
        ])
        assert_budget(initial, initial_size)
        assert [row["domain"] for row in initial["evidence_catalog"]] == [
            "code_map", "call_traces", "impact", "sources_and_sinks"
        ]
        rendered = json.dumps(initial)
        assert "clean = value" not in rendered and "return normalize" not in rendered
        assert '"proof":' not in rendered
        expanded, expanded_size = run(binary, project, [
            "project", "disclose", handle, "--view", "evidence", "--depth", "3",
            "--profile", "expanded", "--token-limit", "16384", "--proofs",
        ])
        assert_budget(expanded, expanded_size)
        root = expanded["commitment"]["object_root"]
        assert root == initial["commitment"]["object_root"]
        sizes = [initial_size, expanded_size]
        proofs = []
        model = reconstruct(
            binary, project, expanded["frontier"][0]["reveal"]["arguments"],
            root, sizes, proofs,
        )
        assert model["schema"] == "fr-project-evidence-1"
        assert model["target"]["name"] == "normalize"
        assert model["call_traces"]["callers"]["nodes"][1]["symbol"]["name"] == "endpoint"
        assert model["impact"]["items"]
        bindings = [row["binding"]["name"] for row in model["sources_and_sinks"]["flows"]]
        assert bindings == ["value", "clean", "stored"]
        assert any(row["sources"]["boundaries"] for row in model["sources_and_sinks"]["flows"])
        assert any(row["sinks"]["steps"] for row in model["sources_and_sinks"]["flows"])
        assert "clean = value" not in json.dumps(model) and "return normalize" not in json.dumps(model)
        assert object_digest(model) == root

        leaf, valid = proofs[-1]
        tampered = json.loads(json.dumps(valid))
        tampered["root"] = "0" * 64
        assert not verify_commitment(leaf, tampered)

        (project / "service.py").write_text(source.replace("lower()", "upper()"))
        stale, _ = run(
            binary, project, initial["frontier"][0]["reveal"]["arguments"], success=False
        )
        assert "stale" in stale["error"]["message"]

    return {
        "schema": "fr-progressive-evidence-eval-1",
        "passed": True,
        "tool_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "oracle": {
            "complete_tree_reconstructed": True,
            "root_recomputed": True,
            "proofs_verified": len(proofs),
            "tampering_rejected": True,
            "stale_action_refused": True,
            "source_free": True,
            "domains": ["code-map", "call-traces", "impact", "sources-and-sinks"],
        },
        "budget": {
            "compact_limit": 4096,
            "expanded_limit": 16384,
            "responses": len(sizes),
            "maximum_response_bytes": max(sizes),
            "total_response_bytes": sum(sizes),
        },
        "scope": "Generated generic Python fixture; independent binary-Merkle reconstruction and source-free static evidence checks. Parser, analyzer completeness, SHA-256 collision resistance and runtime behavior remain trusted or separately tested.",
    }


def audit(path):
    report = json.loads(path.read_text())
    assert report["schema"] == "fr-progressive-evidence-eval-1" and report["passed"] is True
    assert report["tool_sha256"] == hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
    assert report["oracle"]["complete_tree_reconstructed"] is True
    assert report["oracle"]["proofs_verified"] >= 8
    assert report["oracle"]["tampering_rejected"] is True
    assert report["oracle"]["stale_action_refused"] is True
    assert report["oracle"]["source_free"] is True
    assert report["budget"]["maximum_response_bytes"] <= report["budget"]["expanded_limit"]
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
