"""Pinned source export and disposable, network-free command containers."""
import base64
import hashlib
import io
from pathlib import Path
import re
import tarfile
import uuid

from .isolated_grade import DOCKER, invoke
from .request_gateway import decode
from .study import encode, require
from .workspace_bundle import MAX_BYTES, MAX_FILES, validate


def git_snapshot(repository, revision, directory, *, execute=invoke):
    require(re.fullmatch(r"[0-9a-f]{40}|[0-9a-f]{64}", revision), "revision must be a full Git object ID")
    command = ["git", "--no-optional-locks", "-c", "gc.auto=0", "-c", "maintenance.auto=false",
               "-C", str(Path(repository).resolve())]
    result, actual, _ = execute(command + ["rev-parse", "--verify", revision + "^{commit}"], b"", directory, 10, 4096)
    require(result["exit_code"] == 0 and not result["stop_reason"] and actual.decode().strip() == revision,
            "pinned commit is unavailable")
    result, tree, _ = execute(command + ["ls-tree", "-rz", "--full-tree", revision], b"", directory, 10, 1024**2)
    require(result["exit_code"] == 0 and not result["stop_reason"], "Git tree listing failed")
    expected = set()
    for entry in tree.split(b"\0"):
        if entry:
            metadata, name = entry.split(b"\t", 1)
            require(metadata.split()[0] in {b"100644", b"100755"}, "Git snapshot refuses symlinks and submodules")
            expected.add(name.decode("utf-8"))
    result, archive, _ = execute(command + ["archive", "--format=tar", revision], b"", directory, 15, 16 * 1024**2)
    require(result["exit_code"] == 0 and not result["stop_reason"], "bounded Git export failed")
    files, entries, total = {}, 0, 0
    with tarfile.open(fileobj=io.BytesIO(archive), mode="r:") as source:
        for member in source:
            entries += 1
            require(entries <= MAX_FILES * 2, "Git export entry limit")
            if member.isdir():
                continue
            require(member.isfile() and member.name not in files, "Git export contains a link, special file or duplicate")
            total += member.size
            require(total <= MAX_BYTES and len(files) < MAX_FILES, "Git export byte/file limit")
            files[member.name] = {"data": base64.b64encode(source.extractfile(member).read()).decode(),
                                  "executable": bool(member.mode & 0o111)}
    require(set(files) == expected, "Git archive omitted tracked files")
    validate(files)
    return files


def file_hash(path, maximum):
    require(path.is_file() and not path.is_symlink() and path.stat().st_size <= maximum, "missing or oversized pinned file")
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(65536), b""):
            value.update(block)
    return value.hexdigest()


class ContainerTools:
    """One container per command; only exported regular files persist between calls."""
    def __init__(self, image, limits, directory, binary=None, skill=None, *, execute=invoke):
        require(re.fullmatch(r"(?:[a-zA-Z0-9./:_-]+@)?sha256:[0-9a-f]{64}", image), "tool image must be pinned")
        self.image, self.limits, self.directory = image, limits, directory
        self.binary, self.skill, self.execute = binary, skill, execute
        result, data, _ = execute(DOCKER + ["image", "inspect", "--format", "{{json .Config.Volumes}}", image],
                                  b"", directory, 10, 65536)
        require(result["exit_code"] == 0 and not result["stop_reason"], "tool image unavailable")
        require(decode(data) in (None, {}), "tool image declares writable volumes")

    def command(self, files, arguments, timeout):
        validate(files, self.limits["workspace_bytes"])
        require(set(arguments) == {"argv", "stdin"}, "command needs argv and stdin only")
        require(isinstance(arguments["argv"], list) and 1 <= len(arguments["argv"]) <= 128
                and all(isinstance(arg, str) and "\0" not in arg for arg in arguments["argv"])
                and arguments["argv"][0], "invalid command argv")
        require(isinstance(arguments["stdin"], str) and len(arguments["stdin"].encode()) <= 65536, "command input limit")
        name = "fr-study-" + uuid.uuid4().hex
        limits = {**self.limits, "command_seconds": min(timeout, self.limits["command_seconds"])}
        command = DOCKER + ["create", "--pull=never", "--name", name, "--interactive", "--network=none",
                           "--read-only", "--cap-drop=ALL", "--security-opt=no-new-privileges", "--cpus=0.5",
                           "--memory=256m", "--memory-swap=256m", "--pids-limit=32", "--user=65534:65534",
                           "--log-driver=none", "--no-healthcheck", "--workdir=/workspace",
                           "--tmpfs=/workspace:rw,nosuid,nodev,size=16m,mode=1777",
                           "--tmpfs=/tmp:rw,nosuid,nodev,size=16m,mode=1777"]
        mounts = [(Path(__file__).with_name("tool_worker.py"), "/opt/worker.py"),
                  (Path(__file__).with_name("workspace_bundle.py"), "/opt/workspace_bundle.py")]
        if self.binary is not None:
            mounts.append((self.binary, "/opt/fr/fr"))
        if self.skill is not None:
            mounts.append((self.skill, "/opt/fr-skill"))
        for source, target in mounts:
            require("," not in str(source.resolve()), "mount path contains comma")
            command += ["--mount", f"type=bind,source={source.resolve()},target={target},readonly,bind-recursive=disabled"]
        command += ["--entrypoint", "python3", self.image, "-B", "/opt/worker.py"]
        try:
            result, _, _ = self.execute(command, b"", self.directory, 10, 65536)
            require(result["exit_code"] == 0 and not result["stop_reason"], "tool container creation failed")
            payload = encode({"files": files, **arguments, "limits": limits})
            require(len(payload) <= 16 * 1024**2, "tool input size limit")
            result, data, _ = self.execute(DOCKER + ["start", "--attach", "--interactive", name], payload,
                                           self.directory, limits["command_seconds"] + 5, 16 * 1024**2)
            require(result["exit_code"] == 0 and not result["stop_reason"], "tool container failed or exceeded limits")
            exported = decode(data)
            require(set(exported) == {"files", "result"}, "invalid tool export")
            validate(exported["files"], self.limits["workspace_bytes"])
            require(len(encode(exported["result"])) <= 8 * self.limits["output_bytes"] + 1024, "tool result limit")
            return exported["files"], exported["result"]
        finally:
            removed, _, _ = self.execute(DOCKER + ["rm", "--force", name], b"", self.directory, 5, 65536)
            require(removed["exit_code"] == 0 and not removed["stop_reason"], "tool cleanup failed; stop the worker")
