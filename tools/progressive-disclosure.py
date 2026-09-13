#!/usr/bin/env python3
"""Exercise progressive disclosure and independently verify its Merkle commitments."""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
SCHEMA = "fr-progressive-disclosure-1"
TREE_SCHEMA = "fr-semantic-merkle-1"


def digest(data):
    return hashlib.sha256(data).hexdigest()


def encoded(value):
    return json.dumps(value, ensure_ascii=False, separators=(",", ":")).encode()


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
        return hash_json([TREE_SCHEMA, "array", [merkle(child) for child in value]])
    assert isinstance(value, dict)
    children = [[key, merkle(value[key])] for key in sorted(value)]
    return hash_json([TREE_SCHEMA, "object", children])


def at_pointer(value, pointer):
    for part in pointer.removeprefix("/").split("/"):
        part = part.replace("~1", "/").replace("~0", "~")
        value = value[int(part)] if isinstance(value, list) else value[part]
    return value


def run(binary, project, arguments, success=True):
    result = subprocess.run(
        [str(binary), "--json", "--no-cache", "-C", str(project), *arguments],
        capture_output=True,
        timeout=30,
    )
    assert (result.returncode == 0) == success, (arguments, result.stdout, result.stderr)
    report = json.loads(result.stdout)
    return report, len(result.stdout)


def exact(report):
    return report["reveal"]["arguments"]


def assert_budget(report, size):
    budget = report["token_budget"]
    assert budget["used_upper_bound"] == size <= budget["limit"]
    assert budget["unit"] == "conservative-model-token-upper-bound"
    assert budget["accounting"] == "serialized UTF-8 bytes including trailing newline"


def measure(binary):
    source = (
        "def normalize(records):\n"
        "    result = []\n"
        "    for record in records:\n"
        "        if not record['enabled']:\n"
        "            continue\n"
        "        name = record['name'].strip().lower()\n"
        "        tags = []\n"
        "        for tag in record['tags']:\n"
        "            clean = tag.strip().lower()\n"
        "            if clean and clean not in tags:\n"
        "                tags.append(clean)\n"
        "        entry = {'name': name, 'tags': tags}\n"
        "        if record.get('priority'):\n"
        "            entry['priority'] = int(record['priority'])\n"
        "        result.append(entry)\n"
        "    result.sort(key=lambda item: item['name'])\n"
        "    return result\n"
    )
    with tempfile.TemporaryDirectory(prefix="fr-progressive-disclosure-") as temporary:
        project = Path(temporary)
        (project / "service.py").write_text(source)
        names, _ = run(binary, project, ["project", "explore", "normalize"])
        target = names["rows"][0]["handle"]

        semantic, semantic_bytes = run(
            binary, project, ["project", "semantic", target, "--body", "--nodes", "4096"]
        )
        model = semantic["model"]
        semantic_root = merkle(model)
        source_text = source.rstrip("\n")
        source_root = hash_json([TREE_SCHEMA, "source", list(source_text.encode())])
        root = hash_json([
            TREE_SCHEMA, "view", semantic["revision"], target, semantic_root, source_root
        ])
        basis = "frdv1:" + hash_json([
            SCHEMA, semantic["revision"], target, semantic["semantic_basis"], root, "compact", 4096
        ])

        initial, initial_bytes = run(
            binary, project, ["project", "disclose", target, "--token-limit", "4096"]
        )
        assert_budget(initial, initial_bytes)
        assert initial["view_basis"] == basis
        assert initial["commitment"]["root"] == root
        assert initial["commitment"]["semantic_root"] == semantic_root
        assert initial["commitment"]["source_root"] == source_root
        assert "model" not in initial and source_text not in json.dumps(initial)
        semantic_hole, source_hole = initial["frontier"]
        expected_hole = "frh1:" + hash_json([
            SCHEMA, basis, "/model", semantic_root
        ])
        assert semantic_hole["id"] == expected_hole

        shortcut = initial["semantic_shortcuts"][0]
        shortcut_pointer = shortcut["address"].split("#", 1)[1]
        shortcut_digest = merkle(at_pointer({"model": model}, shortcut_pointer))
        assert shortcut["hole"] == "frh1:" + hash_json([
            SCHEMA, basis, shortcut_pointer, shortcut_digest
        ])
        shortcut_report, shortcut_bytes = run(binary, project, shortcut["reveal"]["arguments"])
        assert_budget(shortcut_report, shortcut_bytes)
        assert shortcut_report["commitment"] == initial["commitment"]
        assert "text" not in shortcut_report["revealed"]

        revealed, reveal_bytes = run(binary, project, exact(semantic_hole))
        assert_budget(revealed, reveal_bytes)
        assert revealed["commitment"] == initial["commitment"]
        for child in revealed["revealed"]["children"]:
            key = child["key"]
            actual_digest = child.get("digest") or child["hole"]["digest"]
            assert actual_digest == merkle(model[key])
        assert "text" not in revealed["revealed"]

        reconstructed = ""
        source_pages = 0
        action = exact(source_hole)
        response_sizes = [initial_bytes, shortcut_bytes, reveal_bytes]
        while action:
            page, size = run(binary, project, action)
            assert_budget(page, size)
            assert page["commitment"] == initial["commitment"]
            reconstructed += page["revealed"]["text"]
            response_sizes.append(size)
            source_pages += 1
            frontier = page["frontier"]
            action = exact(frontier[0]) if frontier else None
        assert reconstructed == source_text

        (project / "service.py").write_text(source.replace("return result", "return tuple(result)"))
        stale, _ = run(binary, project, exact(semantic_hole), success=False)
        assert "stale" in stale["error"]["message"]

    changed = json.loads(json.dumps(model))
    changed["items"] = list(reversed(changed["items"])) + [{"kind": "synthetic-oracle-change"}]
    assert merkle(changed) != semantic_root
    return {
        "schema": "fr-progressive-disclosure-eval-1",
        "passed": True,
        "tool_sha256": digest(Path(__file__).read_bytes()),
        "fixture_sha256": digest(source.encode()),
        "oracle": {
            "semantic_root_verified": True,
            "source_root_verified": True,
            "combined_root_verified": True,
            "root_hole_verified": True,
            "shortcut_hole_verified": True,
            "child_digests_verified": len(revealed["revealed"]["children"]),
            "hidden_change_detected": True,
        },
        "flow": {
            "initial_source_free": True,
            "semantic_reveal_source_free": True,
            "exact_actions_followed": 3 + source_pages,
            "source_pages": source_pages,
            "source_reconstructed": True,
            "stale_action_refused": True,
        },
        "budget": {
            "limit": 4096,
            "responses_checked": len(response_sizes),
            "maximum_response_bytes": max(response_sizes),
            "initial_response_bytes": initial_bytes,
            "complete_semantic_response_bytes": semantic_bytes,
        },
        "scope": "Generated generic Python fixture; deterministic agent-style continuations and independent SHA-256 oracle, without a live model or cryptographic collision proof.",
    }


def audit(path):
    report = json.loads(path.read_text())
    assert report["schema"] == "fr-progressive-disclosure-eval-1" and report["passed"] is True
    assert report["tool_sha256"] == digest(Path(__file__).read_bytes())
    assert report["oracle"]["semantic_root_verified"] is True
    assert report["oracle"]["source_root_verified"] is True
    assert report["oracle"]["combined_root_verified"] is True
    assert report["oracle"]["shortcut_hole_verified"] is True
    assert report["oracle"]["hidden_change_detected"] is True
    assert report["flow"]["initial_source_free"] is True
    assert report["flow"]["semantic_reveal_source_free"] is True
    assert report["flow"]["source_reconstructed"] is True
    assert report["flow"]["stale_action_refused"] is True
    assert report["budget"]["maximum_response_bytes"] <= report["budget"]["limit"]
    assert report["budget"]["initial_response_bytes"] < report["budget"]["complete_semantic_response_bytes"]
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
