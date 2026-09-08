#!/usr/bin/env python3
"""Prepare dependencies or rehearse the regex workspace task without an agent runtime."""

import argparse
import importlib.util
import json
from pathlib import Path
import tempfile

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("agent_eval_harness", ROOT / "tools/agent-eval.py")
harness = importlib.util.module_from_spec(spec)
spec.loader.exec_module(harness)
TASK = harness.regex_workspace.TASK
FUNCTION = "pub fn escape_into(pattern: &str, buf: &mut alloc::string::String) { regex_syntax::escape_into(pattern, buf); }"
FRAGMENT = "/// Appends the escaped pattern to the buffer without replacing its contents.\n" + FUNCTION


def check(binary):
    with tempfile.TemporaryDirectory(prefix="fr-regex-rehearsal-") as tmp:
        session = Path(tmp).resolve()
        harness.prepare_trial(session, binary, TASK, "fr", 1)
        config = json.loads((session / "session.json").read_text())
        project = session / "project"

        def request(value):
            result = harness.action(session, config, value)
            assert not result.get("exit_code", 0), result
            return result

        def fr(*args):
            return request({"tool": "fr", "args": list(args)})["result"]

        basis = fr("checks")["basis"]

        def checks():
            report = fr("checks", "--run", "upstream,minimal", "--basis", basis, "--quiet-success")
            assert report["passed"] and report["not_run"] == [], report

        checks()
        selected = fr("project", "map", "src/lib.rs", "--depth", "0", "--fields", "handle", "--limit", "1")
        handle = selected["rows"][0][0]
        fragment = request({"tool": "write", "path": "function.rs", "text": FRAGMENT})["path"]
        saved = fr("author", "insert-declaration", handle, "--from", fragment, "--save-plan")
        assert saved["saved"] and not saved["applied"] and saved["documentation"]["kind"] == "outer-doc-comments"
        tx = str(saved["transaction"])
        fr("history", "apply", tx, "--write", "--no-diff")
        checks()
        oracle = harness.verify(project, TASK)
        assert oracle["passed"], oracle
        final = harness.snapshot(project)
        fr("history", "patch", tx)
        request({"tool": "sentinel"})
        fr("history", "undo", tx)
        fr("history", "undo", tx, "--write", "--no-diff")
        assert harness.snapshot(project) == config["original"]
        checks()
        fr("history", "redo", tx)
        fr("history", "redo", tx, "--write", "--no-diff")
        checks()
        assert harness.snapshot(project) == final
        assert request({"tool": "receiver"})["matches"]
        receiver_oracle = harness.verify(session / "receiver", TASK)
        assert receiver_oracle["passed"], receiver_oracle
        assert (project / "unrelated.txt").read_text() == "Preserve this independent later edit.\n"
        assert harness.digest((project / ".git/index").read_bytes()) == config["index_sha256"]
        assert harness.digest((session / "receiver/.git/index").read_bytes()) == config["receiver_index_sha256"]
        source = project / "src/lib.rs"
        correct = source.read_text()
        rejected = {}
        for name, body in {
            "clears_prefix": "buf.clear(); regex_syntax::escape_into(pattern, buf);",
            "omits_escaping": "buf.push_str(pattern);",
            "allocates_intermediate": "buf.push_str(&regex_syntax::escape(pattern));",
        }.items():
            source.write_text(correct.replace(FUNCTION, FUNCTION.replace("regex_syntax::escape_into(pattern, buf);", body)))
            outcome = harness.verify(project, TASK)
            assert not outcome["passed"] and outcome["stage"] == 2, outcome
            rejected[name] = outcome
        source.write_text(correct)
        assert harness.snapshot(project) == final
        return {"passed": True, "run_kind": "controlled-rehearsal", "task": TASK,
                "source_bytes": config["source_bytes"], "binary_sha256": config["binary_sha256"],
                "archive_sha256": config["archive_sha256"], "dependency_lock_sha256": config["dependency_lock_sha256"],
                "original_oracle": config["original_oracle"], "oracle": oracle, "receiver_oracle": receiver_oracle,
                "negative_controls": rejected, "checked_states": ["original", "changed", "undone", "redone"],
                "undo_redo_exact": True, "patch_receiver_matches": True, "indexes_unchanged": True,
                "scope": "Prescribed implementation and workflow, no autonomous agents or context-efficiency measurement."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    unpack = commands.add_parser("unpack")
    unpack.add_argument("directory", type=Path)
    rehearsal = commands.add_parser("check")
    rehearsal.add_argument("--fr", type=Path, default=ROOT / "target/debug/fr")
    args = parser.parse_args()
    if args.command == "unpack":
        harness.unpack(args.directory.resolve(), TASK)
    else:
        print(json.dumps(check(args.fr.resolve()), indent=2))


if __name__ == "__main__":
    main()
