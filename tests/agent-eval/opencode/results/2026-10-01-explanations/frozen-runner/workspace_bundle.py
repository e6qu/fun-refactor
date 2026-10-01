"""Bounded regular-file snapshots shared by the host and disposable tool worker."""
import base64
import os
from pathlib import Path, PurePosixPath
import stat

MAX_BYTES = 8 * 1024**2
MAX_FILES = 2000


def check(condition, message):
    if not condition:
        raise ValueError(message)


def validate(files, limit=MAX_BYTES):
    check(isinstance(files, dict) and len(files) <= MAX_FILES, "workspace file limit")
    check(0 < limit <= MAX_BYTES, "invalid workspace byte limit")
    total = 0
    for name, row in files.items():
        path = PurePosixPath(name)
        check(isinstance(name, str) and 0 < len(name) <= 512 and not path.is_absolute()
              and str(path) == name and ".." not in path.parts and name != "."
              and "\\" not in name and "\0" not in name, "unsafe workspace path")
        check(all(str(parent) not in files for parent in path.parents), "workspace path collision")
        check(isinstance(row, dict) and set(row) == {"data", "executable"}
              and type(row["executable"]) is bool and isinstance(row["data"], str), "invalid workspace entry")
        check(len(row["data"]) <= (limit + 2) // 3 * 4, "encoded workspace file limit")
        data = base64.b64decode(row["data"], validate=True)
        total += len(data)
        check(total <= limit, "workspace byte limit")
    return total


def unpack(files, root, limit=MAX_BYTES):
    validate(files, limit)
    check(not root.exists(), "workspace destination must be fresh")
    root.mkdir(parents=True)
    for name, row in files.items():
        path = root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(base64.b64decode(row["data"], validate=True))
        path.chmod(0o755 if row["executable"] else 0o644)


def pack(root, limit=MAX_BYTES):
    files, total, entries = {}, 0, 0
    for directory, dirs, names in os.walk(root, followlinks=False):
        for name in dirs + names:
            entries += 1
            check(entries <= MAX_FILES * 2, "workspace entry limit")
            path = Path(directory) / name
            mode = path.lstat().st_mode
            check(not stat.S_ISLNK(mode), "workspace symlink refused")
            if stat.S_ISDIR(mode):
                continue
            check(stat.S_ISREG(mode), "workspace special file refused")
            descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
            with os.fdopen(descriptor, "rb") as stream:
                check(stat.S_ISREG(os.fstat(stream.fileno()).st_mode), "workspace file changed type")
                data = stream.read(limit - total + 1)
            total += len(data)
            check(total <= limit and len(files) < MAX_FILES, "workspace export limit")
            files[path.relative_to(root).as_posix()] = {
                "data": base64.b64encode(data).decode(), "executable": bool(mode & 0o111)}
    validate(files, limit)
    return files
