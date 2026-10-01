"""Source snapshots and exact text delivered by the constrained OpenCode host."""

import base64
import gzip
import hashlib
import io
import tarfile
from pathlib import PurePosixPath

from .source_disclosure import overlap
from .study import digest, encode, require
from .workspace_bundle import pack, validate

LIMIT = 1024**2


def criteria(value, files):
    require(isinstance(value, dict) and 1 <= len(value) <= 12, "invalid explanation rubric")
    for name, claim in value.items():
        require(isinstance(name, str) and name and set(claim) == {"value", "evidence"}, "invalid rubric claim")
        require(type(claim["value"]) in (bool, int, str), "unsupported factual value")
        groups = claim["evidence"]
        require(isinstance(groups, list) and 1 <= len(groups) <= 6, "missing source evidence")
        for group in groups:
            require(isinstance(group, list) and 1 <= len(group) <= 6, "missing source alternatives")
            for anchor in group:
                require(set(anchor) == {"path", "contains"} and anchor["path"] in files, "unknown source anchor")
                require(isinstance(anchor["contains"], str) and 16 <= len(anchor["contains"].encode()) <= 2048,
                        "invalid source anchor")
                require(anchor["contains"].encode() in base64.b64decode(files[anchor["path"]]["data"]),
                        "source anchor is absent")
    return value


def source_bundle(task, base):
    path = base / task["source"]
    if task.get("source_format", "directory") == "directory":
        return pack(path, LIMIT)
    require(task["source_format"] == "tar.gz", "unsupported source format")
    require(path.stat().st_size <= LIMIT, "source archive exceeds limit")
    data = path.read_bytes()
    require(hashlib.sha256(data).hexdigest() == task["archive_sha256"], "source archive changed")
    with gzip.GzipFile(fileobj=io.BytesIO(data)) as compressed:
        expanded = compressed.read(2 * LIMIT + 1)
    require(len(expanded) <= 2 * LIMIT, "expanded archive exceeds limit")
    files, roots, total, entries = {}, set(), 0, 0
    with tarfile.open(fileobj=io.BytesIO(expanded), mode="r:") as archive:
        for member in archive:
            entries += 1
            require(entries <= 200, "archive entry limit")
            parts = PurePosixPath(member.name).parts
            require(parts and not member.name.startswith("/") and ".." not in parts,
                    "unsafe archive path")
            roots.add(parts[0])
            require(len(roots) == 1 and (member.isdir() or member.isfile()), "unsafe archive member")
            if member.isdir():
                continue
            name = "/".join(parts[1:])
            require(name not in files and len(files) < 100 and 0 <= member.size <= LIMIT - total,
                    "archive file or byte limit")
            raw = archive.extractfile(member).read(member.size + 1)
            require(len(raw) == member.size, "archive member size differs")
            total += len(raw)
            files[name] = {"data": base64.b64encode(raw).decode(), "executable": bool(member.mode & 0o111)}
    validate(files, LIMIT)
    require(files, "empty source archive")
    return files


def search(files, needle):
    hits, binary, total = [], 0, 0
    for path, entry in sorted(files.items()):
        try:
            content = base64.b64decode(entry["data"]).decode()
        except UnicodeDecodeError:
            binary += 1
            continue
        offset = 0
        identity = hashlib.sha256(content.encode()).hexdigest()
        for line, raw_line in enumerate(content.splitlines(keepends=True), 1):
            value = raw_line.splitlines()[0]
            if needle in value:
                total += 1
                if len(hits) < 20:
                    hits.append({"path": path, "line": line, "text": value[:256],
                                 "offset": offset, "sha256": identity})
            offset += len(raw_line.encode())
    return {"matches": hits[:20], "omitted_matches": max(0, total - 20),
            "unreadable_files": binary}


def disclosed(files, request, result):
    """Validate source bytes against the snapshot, excluding names and signatures."""
    if "error" in result:
        return []
    kind, spans = request["action"], []
    if kind == "read":
        spans.append((result["path"], result["offset"], result["text"]))
    elif kind == "search":
        for hit in result["matches"]:
            raw = base64.b64decode(files[hit["path"]]["data"])
            lines = raw.decode().splitlines(keepends=True)
            require(type(hit["line"]) is int and 0 < hit["line"] <= len(lines), "invalid search line")
            start = len("".join(lines[:hit["line"]-1]).encode())
            spans.append((hit["path"], start, hit["text"]))
    elif kind == "fr" and request["operation"] == "show" and result.get("source"):
        source, node = result["source"], result["node"]
        require(node["handle"] == request["handle"] and source["offset"] == request.get("offset", 0),
                "fr source request differs")
        require(source["span"]["end"] - source["span"]["start"] == source["returned_bytes"]
                == len(source["text"].encode()), "fr source extent differs")
        require(source["span"]["start"] == node["span"]["start"] + source["offset"], "fr source origin differs")
        spans.append((node["path"], source["span"]["start"], source["text"]))
    result_spans = []
    for path, start, content in spans:
        require(path in files and type(start) is int and start >= 0, "unknown source extent")
        raw, snippet = base64.b64decode(files[path]["data"]), content.encode()
        require(raw[start:start+len(snippet)] == snippet, "disclosed source differs from snapshot")
        result_spans.append({"path": path, "sha256": hashlib.sha256(raw).hexdigest(),
                             "start": start, "end": start + len(snippet), "text": content,
                             "via": kind})
    return result_spans


def measure(record, task, replay):
    """Reconstruct requests and delivered text; never trust stored summary counters."""
    files = {name: dict(row) for name, row in task["files"].items()}
    rows, turns = record["trace"], record["turns"]
    cursor, spans, ranges = 0, [], {}
    metrics = dict(tool_calls=0, tool_result_bytes=0, instruction_bytes=0,
                   delivered_tool_result_bytes=0, undelivered_results=0,
                   exact_source_bytes=0, repeated_source_bytes=0, unique_source_bytes=0,
                   read_source_bytes=0, search_source_bytes=0, fr_source_bytes=0,
                   source_pages=0, refused_calls=0, unfinished_calls=0)
    if rows and "prompt" in rows[0]:
        metrics["instruction_bytes"] = len(rows[0]["prompt"].encode())
    expected_prompt = None
    for turn in turns:
        require(cursor < len(rows) and set(rows[cursor]) == {"prompt"}, "missing model prompt")
        prompt = rows[cursor]["prompt"]
        require(isinstance(prompt, str) and (expected_prompt is None or prompt == expected_prompt), "delivered prompt differs")
        if cursor == 0:
            metrics["instruction_bytes"] = len(prompt.encode())
        cursor += 1
        request = turn["action"]
        if request["action"] == "finish":
            require(turn is turns[-1] and cursor == len(rows), "actions after finish")
            break
        metrics["tool_calls"] += 1
        if cursor == len(rows):
            require(record["status"] == "failed", "completed attempt lost tool result")
            metrics["unfinished_calls"] += 1
            break
        row = rows[cursor]
        require(set(row) == {"action", "result"} and row["action"] == request, "trace action differs from model stream")
        result = row["result"]
        # Re-execute only deterministic host operations. fr uses retained stdout.
        observed = replay(files, request, result)
        require(observed == result, "retained tool result differs")
        if "error" in result:
            metrics["refused_calls"] += 1
        next_prompt = "Action result:\n" + encode(result).decode() + "\nReturn the next JSON action."
        reaches_prompt = cursor + 1 < len(rows) and rows[cursor + 1] == {"prompt": next_prompt}
        produced = disclosed(files, request, result)
        delivered = produced if reaches_prompt else []
        if reaches_prompt:
            metrics["delivered_tool_result_bytes"] += len(encode(result))
        else:
            metrics["undelivered_results"] += 1
        spans.extend(delivered)
        for span in delivered:
            size = span["end"] - span["start"]
            metrics["exact_source_bytes"] += size
            metrics[span["via"] + "_source_bytes"] += size
            metrics["repeated_source_bytes"] += overlap(ranges.setdefault((span["path"], span["sha256"]), []),
                                                       span["start"], span["end"])
            metrics["source_pages"] += 1
        metrics["tool_result_bytes"] += len(encode(result))
        expected_prompt = next_prompt
        cursor += 1
    if cursor < len(rows):
        require(record["status"] == "failed" and cursor + 1 == len(rows)
                and set(rows[cursor]) == {"prompt"}, "unaccounted trace rows")
        require(expected_prompt is None or rows[cursor]["prompt"] == expected_prompt, "pending prompt differs")
    require(digest(files) == record["submission_sha256"], "trace does not reproduce submission")
    metrics["unique_source_bytes"] = metrics["exact_source_bytes"] - metrics["repeated_source_bytes"]
    metrics["complete_context_accounting"] = False
    return metrics, spans
