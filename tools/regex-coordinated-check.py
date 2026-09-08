#!/usr/bin/env python3
"""Rehearse the coordinated regex task and reject incorrect implementations."""

import argparse
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import tempfile

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("agent_eval_harness", ROOT / "tools/agent-eval.py")
harness = importlib.util.module_from_spec(spec)
spec.loader.exec_module(harness)
TASK = harness.regex_escape_len.TASK
LENGTH_BODY = "{ pattern.len() + pattern.chars().filter(|&c| is_meta_character(c)).count() }"
FACADE_BODY = "{ regex_syntax::escape_len(pattern) }"
ESCAPE_BODY = "{ let mut quoted = String::with_capacity(escape_len(text)); escape_into(text, &mut quoted); quoted }"
FRAGMENTS = {
    "length.txt": "/// Returns the byte length of the escaped pattern without allocating.\npub fn escape_len(pattern: &str) -> usize " + LENGTH_BODY,
    "facade.txt": "/// Returns the byte length of the escaped pattern without allocating.\npub fn escape_len(pattern: &str) -> usize " + FACADE_BODY,
    "escape.txt": ESCAPE_BODY,
}


def check(binary):
    with tempfile.TemporaryDirectory(prefix="fr-regex-coordinated-") as tmp:
        session = Path(tmp).resolve()
        harness.prepare_trial(session, binary, TASK, "fr", 1)
        config = json.loads((session / "session.json").read_text())
        project = session / "project"

        def request(value):
            output = io.StringIO()
            with contextlib.redirect_stdout(output):
                harness.step(session, value)
            result = json.loads(output.getvalue())
            assert not result.get("exit_code", 0) and not result.get("error"), result
            assert not result.get("stdout_omitted_bytes", 0), result
            return result

        def fr(*args):
            return request({"tool": "fr", "args": list(args)})["result"]

        basis = fr("checks")["basis"]

        def checks():
            report = fr("checks", "--run", "upstream,minimal", "--basis", basis, "--quiet-success", "--no-declarations")
            assert report["passed"] and report["not_run"] == [], report

        checks()
        selected = {}
        for path in harness.edit_paths(TASK):
            found = fr("project", "find", "escape", "--in", path, "--source", "--signature", "--bytes", "2048")
            assert found["page"]["total"] == 1 and found["page"]["next"] is None
            row = dict(zip(found["columns"], found["rows"][0]))
            assert row["source"]["next_offset"] is None
            selected[path] = (found["root"], row["handle"])
        inputs = {name: request({"tool": "write", "path": name, "text": text})["path"] for name, text in FRAGMENTS.items()}
        manifest = {"operations": [
            {"op": "insert-declaration", "handle": selected["regex-syntax/src/lib.rs"][0], "from": inputs["length.txt"]},
            {"op": "insert-declaration", "handle": selected["src/lib.rs"][0], "from": inputs["facade.txt"]},
            {"op": "replace-body", "handle": selected["regex-syntax/src/lib.rs"][1], "from": inputs["escape.txt"]},
        ]}
        manifest_path = request({"tool": "write", "path": "batch.json", "text": json.dumps(manifest)})["path"]
        saved = fr("author", "batch", "--from", manifest_path, "--save-plan", "--diff-bytes", "65536")
        assert saved["saved"] and not saved["applied"] and saved["files_changed"] == 2 and isinstance(saved["diff"], str)
        assert harness.snapshot(project) == config["original"]
        transaction = str(saved["transaction"])
        fr("history", "apply", transaction, "--write", "--no-diff")
        checks()
        oracle = harness.verify(project, TASK)
        assert oracle["passed"], oracle
        final = harness.snapshot(project)
        changed = sorted(name for name in final if final[name] != config["original"][name])
        assert changed == sorted(harness.edit_paths(TASK))
        patch = fr("history", "patch", transaction)["patch"]
        request({"tool": "sentinel"})
        fr("history", "undo", transaction, "--write", "--no-diff")
        assert harness.snapshot(project) == config["original"]
        checks()
        fr("history", "redo", transaction, "--write", "--no-diff")
        assert harness.snapshot(project) == final
        checks()
        assert request({"tool": "receiver"})["matches"]
        receiver_oracle = harness.verify(session / "receiver", TASK)
        assert receiver_oracle["passed"], receiver_oracle
        request({"tool": "finish", "summary": "Controlled rehearsal complete; no autonomous agent participated."})
        events = [json.loads(line) for line in (session / "events.jsonl").read_text().splitlines()]
        observed = harness.workflow(events, config["original"], final, harness.required_checks(TASK))
        assert all(observed[key] for key in ("undo_exact", "redo_exact", "workflow_ordered"))
        assert harness.coordinated_batch(events)
        assert all(event["index_sha256"] == config["index_sha256"] for event in events)
        assert harness.digest((session / "receiver/.git/index").read_bytes()) == config["receiver_index_sha256"]
        assert (project / "unrelated.txt").read_text() == "Preserve this independent later edit.\n"
        correct = {path: (project / path).read_bytes() for path in harness.edit_paths(TASK)}
        rejected = {}
        mutations = {
            "unicode_scalar_count": ("regex-syntax/src/lib.rs", LENGTH_BODY, LENGTH_BODY.replace("pattern.len()", "pattern.chars().count()")),
            "missed_metacharacters": ("regex-syntax/src/lib.rs", LENGTH_BODY, "{ pattern.len() }"),
            "allocating_length": ("regex-syntax/src/lib.rs", LENGTH_BODY, "{ let mut buf = String::new(); escape_into(pattern, &mut buf); buf.len() }"),
            "no_preallocation": ("regex-syntax/src/lib.rs", ESCAPE_BODY, ESCAPE_BODY.replace("String::with_capacity(escape_len(text))", "String::new()")),
            "facade_disagrees": ("src/lib.rs", FACADE_BODY, "{ pattern.len() }"),
        }
        for name, (path, before, after) in mutations.items():
            try:
                text = correct[path].decode()
                assert text.count(before) == 1
                (project / path).write_bytes(text.replace(before, after).encode())
                outcome = harness.verify(project, TASK)
                assert not outcome["passed"] and outcome["stage"] == 2, outcome
                rejected[name] = outcome
            finally:
                (project / path).write_bytes(correct[path])
        assert harness.snapshot(project) == final
        assert harness.digest(binary.read_bytes()) == config["binary_sha256"]
        sources = (Path(__file__), ROOT / "tools/agent-eval.py", ROOT / "tools/agent_eval/regex_workspace.py", ROOT / "tools/agent_eval/regex_escape_len.py")
        return {"passed": True, "run_kind": "controlled-rehearsal", "task": TASK,
                "binary_sha256": config["binary_sha256"], "binary_path": str(binary), "source_bytes": config["source_bytes"],
                "archive_sha256": config["archive_sha256"], "dependency_lock_sha256": config["dependency_lock_sha256"],
                "measurement_files": {str(path.relative_to(ROOT)): harness.digest(path.read_bytes()) for path in sources},
                "original_oracle": config["original_oracle"], "oracle": oracle, "receiver_oracle": receiver_oracle,
                "negative_controls": rejected, "workflow": observed, "coordinated_batch": True,
                "changed_paths": changed, "original": config["original"], "final": final, "patch": patch,
                "events": [{key: event[key] for key in ("request", "visible", "index_sha256")} for event in events],
                "checked_states": ["original", "changed", "undone", "redone"], "indexes_unchanged": True,
                "scope": "Prescribed real-workspace implementation and workflow, with independent finite oracles and negative controls. No autonomous agents, context-efficiency measurement, latency comparison or formal verification claim."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fr", type=Path, default=ROOT / "target/debug/fr")
    args = parser.parse_args()
    print(json.dumps(check(args.fr.resolve()), indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
