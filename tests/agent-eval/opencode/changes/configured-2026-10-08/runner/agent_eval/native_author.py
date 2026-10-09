"""Bounded public author previews and history application for native trials."""
import base64
import copy
import hashlib
from pathlib import Path
import re

from .study import digest, encode, require
from .workspace_bundle import validate
from .opencode_rehearsal import MAX_OUTPUT, MAX_WORKSPACE
from . import native_mcp as mcp

GUIDANCE = """Optional fr editing: use fr_explore names to obtain a full handle, then behavior
to inspect its source. fr_preview_body replaces that declaration's body and returns a diff.
For Python, supply a relative suite without the def line or outer indentation; for Rust,
supply the complete brace block. Other supported languages follow fr author semantics.
Review the returned diff, then call fr_apply_preview with its preview_id. Preview does not
change source. Any intervening edit invalidates the preview. Ordinary replace_source remains
available. An applied edit is not a passing test or proof.
"""


def schemas():
    return [
        {"name": "fr_preview_body", "description": "Preview a public fr author replace-body operation. Review the diff before applying; source remains unchanged.",
         "inputSchema": {"type": "object", "additionalProperties": False,
             "required": ["path", "handle", "body"], "properties": {
                 "path": {"type": "string", "minLength": 1, "maxLength": 512},
                 "handle": {"type": "string", "minLength": 1, "maxLength": 80},
                 "body": {"type": "string", "minLength": 1, "maxLength": 8192}}}},
        {"name": "fr_apply_preview", "description": "Apply the exact reviewed preview through public fr author save-plan and history apply. Stale previews refuse.",
         "inputSchema": {"type": "object", "additionalProperties": False,
             "required": ["preview_id"], "properties": {
                 "preview_id": {"type": "string", "minLength": 64, "maxLength": 64}}}},
    ]


def patched(raw, diff, path):
    """Replay one complete unified diff, checking every old byte and hunk extent."""
    lines = diff.splitlines(keepends=True)
    require(lines[:2] == [f"--- a/{path}\n", f"+++ b/{path}\n"], "preview diff path differs")
    source, output, cursor, index, hunks = raw.decode().splitlines(keepends=True), [], 0, 2, 0
    while index < len(lines):
        match = re.fullmatch(r"@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@[^\n]*\n", lines[index])
        require(match is not None, "invalid preview hunk")
        old_start, old_count, new_start, new_count = (int(v) if v is not None else 1 for v in match.groups())
        start = old_start - (1 if old_count else 0)
        require(cursor <= start <= len(source), "overlapping or out-of-range preview hunk")
        output.extend(source[cursor:start])
        cursor, index, consumed, produced = start, index + 1, 0, 0
        require(len(output) == new_start - (1 if new_count else 0), "preview new extent differs")
        while index < len(lines) and not lines[index].startswith("@@ "):
            line = lines[index]
            require(line[:1] in (" ", "+", "-"), "invalid preview line")
            kind, text = line[0], line[1:]
            index += 1
            if index < len(lines) and lines[index] == "\\ No newline at end of file\n":
                require(text.endswith("\n"), "invalid missing-newline marker")
                text, index = text[:-1], index + 1
            if kind != "+":
                require(cursor < len(source) and source[cursor] == text, "preview old source differs")
                cursor, consumed = cursor + 1, consumed + 1
            if kind != "-":
                output.append(text)
                produced += 1
        require((consumed, produced) == (old_count, new_count), "preview hunk counts differ")
        hunks += 1
    require(hunks > 0, "empty preview diff")
    return "".join(output + source[cursor:]).encode()


def proposal(files, args, review):
    """Check the public preview's byte claims without executing candidate code."""
    require(review.get("schema") == "fr-author-batch-1" and review.get("query") == "batch"
            and review.get("changed") is True and review.get("applied") is False
            and review.get("saved") is False and review.get("postconditions_held") is True
            and review.get("validation") == "reparse-strict", "incomplete author preview")
    require(re.fullmatch(r"frpb1:[0-9a-f]{64}", review.get("plan_context_basis", "")), "invalid preview basis")
    require(review["files_changed"] == 1 and len(review["steps"]) == 1, "preview scope differs")
    step = review["steps"][0]
    require(step["path"] == args["path"] and step["handle"] == args["handle"]
            and step["operation"] == "replace-body" and step["changed"] is True, "preview operation differs")
    before = base64.b64decode(files[args["path"]]["data"], validate=True)
    after = patched(before, review["diff"], args["path"])
    start, end = step["before_span"]["start"], step["before_span"]["end"]
    size = step["after_bytes"]
    require(type(start) is int and type(end) is int and type(size) is int
            and 0 <= start < end <= len(before) and size >= 0, "invalid body extent")
    require(before[:start] == after[:start] and before[end:] == after[start + size:], "preview changes outside body")
    require(step["before_bytes"] == end - start
            and hashlib.sha256(before[start:end]).hexdigest() == step["before_sha256"]
            and hashlib.sha256(after[start:start + size]).hexdigest() == step["after_sha256"], "preview body identity differs")
    require(after != before, "preview makes no change")
    proposed = copy.deepcopy(files)
    proposed[args["path"]]["data"] = base64.b64encode(after).decode()
    validate(proposed, MAX_WORKSPACE)
    return proposed


def validate_saved(saved, review):
    require(saved.get("saved") is True and saved.get("applied") is False
            and saved.get("plan_context_basis") == review["plan_context_basis"]
            and type(saved.get("transaction")) is int and saved["transaction"] > 0
            and re.fullmatch(r"frtb2:[0-9a-f]{64}", saved.get("transaction_context_basis", "")), "invalid saved author plan")


class Author:
    def __init__(self, machine):
        self.machine, self.pending = machine, None

    def command(self, args):
        machine = self.machine
        raw = machine.execute([str(machine.binary), "--json", "-C", str(machine.workspace()), *args], b"", "fr")
        require(len(raw) <= MAX_OUTPUT, "author output exceeds budget")
        result = mcp.decode(raw)
        require("error" not in result, "public author command refused")
        return result

    def manifest(self, args):
        machine = self.machine
        machine.workspace()
        root = Path(machine.temporary.name)
        fragment, manifest = root / "body.txt", root / "author.json"
        fragment.write_text(args["body"])
        manifest.write_bytes(encode({"operations": [{"op": "replace-body", "handle": args["handle"], "from": str(fragment)}],
            "postconditions": {"files-changed": 1, "paths-changed": [args["path"]]}}))
        return ["author", "batch", "--from", str(manifest), "--diff-bytes", "8192"]

    def preview(self, args, retained):
        machine = self.machine
        self.pending = None
        require(args["path"] in machine.files and "\n" not in args["path"] and "\r" not in args["path"], "preview needs an existing source path")
        require(re.fullmatch(r"frp1:[0-9a-f]{32}:[0-9a-f]{1,16}", args["handle"]), "preview needs a full handle")
        require("\0" not in args["body"], "invalid body fragment")
        if machine.replay and "error" in retained:
            # A CLI refusal cannot be re-proved without the frozen executable.
            # Keep its transcript; it creates no proposal and changes no source.
            return retained
        review = retained["review"] if machine.replay else self.command(self.manifest(args))
        proposed = proposal(machine.files, args, review)
        result = {"preview_id": digest({"source": digest(machine.files), "arguments": args, "review": review}), "review": review}
        require(len(encode(result)) <= MAX_OUTPUT, "preview exceeds disclosure budget")
        self.pending = {"id": result["preview_id"], "source": digest(machine.files), "arguments": copy.deepcopy(args),
                        "review": review, "files": proposed}
        return result

    def apply(self, args, retained):
        machine, pending = self.machine, self.pending
        require(pending is not None and args["preview_id"] == pending["id"], "unknown preview")
        require(digest(machine.files) == pending["source"], "stale preview")
        if machine.replay and "error" in retained:
            self.pending = None
            return retained
        try:
            if machine.replay:
                saved, applied = retained["saved"], retained["applied"]
            else:
                saved = self.command(self.manifest(pending["arguments"]) + ["--save-plan", "--plan-basis", pending["review"]["plan_context_basis"]])
                validate_saved(saved, pending["review"])
                applied = self.command(["history", "apply", str(saved["transaction"]), "--write", "--no-diff",
                                        "--context-basis", saved["transaction_context_basis"]])
                # History may write its own metadata. Only frozen regular source
                # files can enter the submission, and every one must match preview.
                root = machine.workspace_path()
                for name, row in pending["files"].items():
                    path = root / name
                    require(path.is_file() and not path.is_symlink()
                            and path.stat().st_size == len(base64.b64decode(row["data"]))
                            and path.read_bytes() == base64.b64decode(row["data"])
                            and bool(path.stat().st_mode & 0o111) == row["executable"], "applied source differs from preview")
            validate_saved(saved, pending["review"])
            require(applied.get("action") == "apply" and applied.get("applied") is True
                    and applied.get("transaction") == saved["transaction"]
                    and applied.get("context_basis") == saved["transaction_context_basis"]
                    and [c["path"] for c in applied["changes"]] == [pending["arguments"]["path"]], "history application differs")
            result = {"preview_id": pending["id"], "before_sha256": pending["source"],
                      "submission_sha256": digest(pending["files"]), "saved": saved, "applied": applied}
            require(len(encode(result)) <= MAX_OUTPUT, "apply result exceeds disclosure budget")
            machine.files, machine.edits = pending["files"], machine.edits + 1
            machine.materialized = digest(machine.files)
            return result
        finally:
            self.pending = None
            # On refusal, discard the disposable materialization before reuse;
            # machine.files is only committed after all checks succeed.
            if machine.files != pending["files"]:
                machine.materialized = None
