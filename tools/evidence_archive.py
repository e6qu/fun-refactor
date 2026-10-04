"""Stream lossless historical evidence archives with explicit size and identity checks."""
import gzip
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import tempfile

MAX_ARCHIVE = 8 * 1024**2
MAX_ORIGINAL = 64 * 1024**2
CHUNK = 128 * 1024


def require(condition, message):
    if not condition:
        raise ValueError(message)


def safe(root, name):
    require(isinstance(name, str) and name, "missing archive path")
    relative = PurePosixPath(name)
    require(not relative.is_absolute() and ".." not in relative.parts and str(relative) == name,
            "unsafe archive path")
    path = root / name
    require(path.resolve().is_relative_to(root.resolve()) and not path.is_symlink(), "archive path escapes root")
    return path


def catalog(root, path):
    require(path.is_file() and not path.is_symlink() and path.stat().st_size <= 65536, "invalid archive catalog")
    value = json.loads(path.read_text())
    require(set(value) == {"schema", "source_revision", "entries"}
            and value["schema"] == "fr-evidence-archives-1", "unsupported archive catalog")
    require(isinstance(value["source_revision"], str)
            and re.fullmatch(r"[0-9a-f]{40}", value["source_revision"]), "invalid source revision")
    entries = value["entries"]
    require(isinstance(entries, list) and 1 <= len(entries) <= 64, "invalid archive count")
    names = set()
    for entry in entries:
        require(isinstance(entry, dict) and set(entry) == {
            "path", "archive", "bytes", "sha256", "git_blob", "archive_bytes", "archive_sha256"},
            "invalid archive entry")
        safe(root, entry["path"])
        safe(root, entry["archive"])
        require(entry["path"].endswith(".json") and entry["archive"] == entry["path"] + ".gz",
                "archive must preserve the original JSON path")
        require(entry["path"] not in names, "duplicate archive entry")
        names.add(entry["path"])
        for field, limit in (("bytes", MAX_ORIGINAL), ("archive_bytes", MAX_ARCHIVE)):
            require(type(entry[field]) is int and 0 < entry[field] <= limit, "archive size exceeds limit")
        for field, length in (("sha256", 64), ("archive_sha256", 64), ("git_blob", 40)):
            require(isinstance(entry[field], str) and re.fullmatch("[0-9a-f]{" + str(length) + "}", entry[field]),
                    "invalid archive identity")
    require(sum(e["bytes"] for e in entries) <= 1024**3, "archive collection exceeds limit")
    return value


def transfer(root, entry, output=None):
    path = safe(root, entry["archive"])
    require(path.is_file() and path.stat().st_size == entry["archive_bytes"] <= MAX_ARCHIVE,
            "compressed archive size differs")
    require(type(entry["bytes"]) is int and 0 < entry["bytes"] <= MAX_ORIGINAL, "expanded archive exceeds limit")
    compressed = hashlib.sha256()
    compressed_bytes = 0
    with path.open("rb") as stream:
        while chunk := stream.read(min(CHUNK, entry["archive_bytes"] - compressed_bytes + 1)):
            compressed_bytes += len(chunk)
            require(compressed_bytes <= entry["archive_bytes"], "compressed archive grew beyond declared size")
            compressed.update(chunk)
        require(compressed_bytes == entry["archive_bytes"], "compressed archive size changed")
        require(compressed.hexdigest() == entry["archive_sha256"], "compressed archive identity differs")
        stream.seek(0)
        original = hashlib.sha256()
        blob = hashlib.sha1(b"blob " + str(entry["bytes"]).encode() + b"\0")
        count = 0
        with gzip.GzipFile(fileobj=stream, mode="rb") as decoded:
            while chunk := decoded.read(min(CHUNK, entry["bytes"] - count + 1)):
                count += len(chunk)
                require(count <= entry["bytes"], "expanded archive exceeds declared size")
                original.update(chunk)
                blob.update(chunk)
                if output is not None:
                    output.write(chunk)
    require(count == entry["bytes"], "expanded archive size differs")
    require(original.hexdigest() == entry["sha256"] and blob.hexdigest() == entry["git_blob"],
            "restored evidence identity differs")
    return count


def restore(root, entry, destination):
    destination = Path(destination)
    require(not destination.exists() and not destination.is_symlink(), "restore destination already exists")
    descriptor, name = tempfile.mkstemp(prefix=".fr-evidence-", dir=destination.parent)
    temporary = Path(name)
    try:
        with os.fdopen(descriptor, "wb") as stream:
            transfer(root, entry, stream)
            stream.flush()
            os.fsync(stream.fileno())
        os.link(temporary, destination)
    finally:
        temporary.unlink()
    return destination
