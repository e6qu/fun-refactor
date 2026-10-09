"""Run black-box cases with expected answers outside the candidate container."""
from __future__ import annotations

import base64
import hashlib
import io
import json
import os
from pathlib import Path
import re
import stat
import tempfile
import uuid

from .bounded_host import run
from .study import digest, number, require
from .request_gateway import decode

DOCKER = ["docker", "--context", "default"]


def snapshot(source, destination, limit):
    require(source.is_dir(), "candidate directory does not exist")
    require(not destination.resolve().is_relative_to(source.resolve()), "snapshot must be outside candidate")
    destination.mkdir(mode=0o755)
    entries, size = [], 0
    for directory, dirs, files in os.walk(source, followlinks=False):
        for name in sorted(dirs + files):
            path = Path(directory) / name
            relative = path.relative_to(source)
            mode = path.lstat().st_mode
            require(not stat.S_ISLNK(mode), "candidate symlinks require a separate snapshot adapter")
            target = destination / relative
            if stat.S_ISDIR(mode):
                target.mkdir(mode=0o755, exist_ok=True)
                entries.append({"path": relative.as_posix(), "directory": True, "bytes": 0})
                require(len(entries) <= 10000, "candidate snapshot exceeds entry limit")
                continue
            require(stat.S_ISREG(mode), "candidate contains a special file")
            require(size + path.stat().st_size <= limit and len(entries) < 10000, "candidate snapshot exceeds limits")
            descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
            with os.fdopen(descriptor, "rb") as stream:
                require(stat.S_ISREG(os.fstat(stream.fileno()).st_mode), "candidate changed file type")
                data = stream.read(limit + 1)
            size += len(data)
            require(size <= limit, "candidate file exceeds snapshot limit")
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(data)
            target.chmod(0o555 if mode & 0o111 else 0o444)
            entries.append({"path": relative.as_posix(), "executable": bool(mode & 0o111),
                            "sha256": hashlib.sha256(data).hexdigest(), "bytes": len(data)})
    require(sum(entry["bytes"] for entry in entries) <= limit, "candidate changed beyond snapshot limit")
    return {"sha256": digest(sorted(entries, key=lambda row: row["path"])), "files": len(entries),
            "bytes": sum(entry["bytes"] for entry in entries)}


def validate(grader):
    require(grader["schema"] == "fr-stdio-grader-1", "unsupported grader schema")
    image = grader["image"]
    require(isinstance(image, str) and re.fullmatch(r"(?:[a-zA-Z0-9./:_-]+@)?sha256:[0-9a-f]{64}", image),
            "grader image must be an immutable local image ID or repository digest")
    command = grader["command"]
    require(isinstance(command, list) and command and all(isinstance(item, str) and item and "\0" not in item for item in command),
            "grader command must be an argument list")
    limits = grader["limits"]
    maximum = {"wall_seconds": 30, "memory_bytes": 512 * 1024**2,
               "scratch_bytes": 64 * 1024**2, "output_bytes": 1024**2, "candidate_bytes": 64 * 1024**2}
    require(set(limits) == set(maximum), "grader limits differ from supported controls")
    for key, ceiling in maximum.items():
        require(number(limits[key], key, integer=True, positive=True) <= ceiling, f"{key} exceeds grader maximum")
    cases = grader["cases"]
    require(isinstance(cases, list) and 1 <= len(cases) <= 100, "grader needs 1 to 100 cases")
    require(len(cases) * limits["output_bytes"] <= 4 * 1024**2, "grader output evidence exceeds 4 MiB")
    require(len(cases) * limits["wall_seconds"] <= 120, "grader cases exceed the total execution budget")
    identities = set()
    for case in cases:
        require(isinstance(case["id"], str) and case["id"] and case["id"] not in identities, "invalid or repeated case ID")
        identities.add(case["id"])
        require(isinstance(case["stdin"], str) and isinstance(case["stdout"], str), "case input and expected output must be UTF-8 text")
        require(len(case["stdin"].encode()) <= limits["output_bytes"] and len(case["stdout"].encode()) <= limits["output_bytes"],
                "case data exceeds output limit")
        require(number(case["exit_code"], "expected exit code", integer=True) <= 255, "invalid expected exit code")
    return limits


def create_command(grader, candidate, name):
    limits = grader["limits"]
    require("," not in str(candidate), "candidate mount path contains a comma")
    return DOCKER + ["create", "--pull=never", "--name", name, "--interactive", "--network=none",
                     "--read-only", "--cap-drop=ALL", "--security-opt=no-new-privileges",
                     "--cpus=0.5", f'--memory={limits["memory_bytes"]}', f'--memory-swap={limits["memory_bytes"]}',
                     "--pids-limit=32", "--user=65534:65534", "--workdir=/workspace", "--env=HOME=/tmp",
                     "--log-driver=none", "--no-healthcheck",
                     f'--tmpfs=/tmp:rw,nosuid,nodev,size={limits["scratch_bytes"]}',
                     "--mount", f"type=bind,source={candidate},target=/workspace,readonly,bind-recursive=disabled",
                     "--entrypoint", grader["command"][0], grader["image"], *grader["command"][1:]]


def invoke(command, data, directory, timeout, cap):
    stdout, stderr = io.BytesIO(), io.BytesIO()
    result = run(command, data, stdout, stderr, directory, wall_seconds=timeout, transcript_bytes=cap)
    return result, stdout.getvalue(), stderr.getvalue()


def failure_details(process, output, error):
    details = {key: process.get(key) for key in (
        "exit_code", "process_exit_code", "stop_reason", "elapsed_seconds", "launch_error", "monitor_error")}
    for key, value in details.items():
        if isinstance(value, str):
            details[key] = value[:512]
    for name, data in (("stdout", output), ("stderr", error)):
        details[name] = data[:1024].decode("utf-8", errors="replace")
        details[name + "_bytes"] = len(data)
        details[name + "_truncated"] = len(data) > 1024
    return details


def grade(candidate, grader_path, expected_sha256, *, execute=invoke, temporary_parent=None):
    raw = Path(grader_path).read_bytes()
    require(len(raw) <= 1024**2, "grader exceeds size limit")
    require(hashlib.sha256(raw).hexdigest() == expected_sha256, "grader digest differs from frozen task")
    grader = decode(raw)
    limits = validate(grader)
    candidate = Path(candidate).resolve()
    require(not Path(grader_path).resolve().is_relative_to(candidate), "private grader cannot be inside candidate directory")
    results = []
    with tempfile.TemporaryDirectory(prefix="fr-grade-", dir=temporary_parent) as temporary:
        root = Path(temporary)
        root.chmod(0o755)
        inspected, volumes, _ = execute(DOCKER + ["image", "inspect", "--format", "{{json .Config.Volumes}}", grader["image"]],
                                        b"", root, 10, 65536)
        require(inspected["exit_code"] == 0 and not inspected["stop_reason"], "pinned grader image is unavailable")
        require(decode(volumes) in (None, {}), "grader image declares writable volumes")
        staged = root / "candidate"
        retained = snapshot(candidate, staged, limits["candidate_bytes"])
        for case in grader["cases"]:
            name = "fr-grade-" + uuid.uuid4().hex
            verdict = {"id": case["id"], "passed": False, "failure": None}
            try:
                created, creation_output, creation_error = execute(create_command(grader, staged, name), b"", root, 10, 65536)
                require(created["exit_code"] == 0 and not created["stop_reason"],
                        "container creation failed: " + json.dumps(failure_details(created, creation_output, creation_error)))
                execution, output, error = execute(DOCKER + ["start", "--attach", "--interactive", name],
                                                   case["stdin"].encode(), root, limits["wall_seconds"], limits["output_bytes"])
                inspected, state, _ = execute(DOCKER + ["inspect", "--format", "{{json .State}}", name], b"", root, 5, 65536)
                require(inspected["exit_code"] == 0 and not inspected["stop_reason"], "container state unavailable")
                state = json.loads(state)
                verdict.update(execution=execution, container_state=state,
                               stdout_sha256=hashlib.sha256(output).hexdigest(),
                               stderr_sha256=hashlib.sha256(error).hexdigest(), stdout_bytes=len(output), stderr_bytes=len(error),
                               stdout_base64=base64.b64encode(output).decode(), stderr_base64=base64.b64encode(error).decode())
                verdict["passed"] = (not execution["stop_reason"] and not state["Running"]
                                     and not state["OOMKilled"] and not state.get("Error")
                                     and state["ExitCode"] == case["exit_code"] and output == case["stdout"].encode())
                if not verdict["passed"]:
                    verdict["failure"] = "candidate output, exit status or resource limits differ"
            except (OSError, ValueError, KeyError) as error:
                verdict["failure"] = str(error)
            finally:
                removed, cleanup_output, cleanup_error = execute(DOCKER + ["rm", "--force", name], b"", root, 5, 65536)
                require(removed["exit_code"] == 0 and not removed["stop_reason"],
                        "grader container cleanup failed; stop the worker: " + json.dumps({
                            "case": case["id"][:128], "container": name,
                            "prior_failure": verdict["failure"][:1024] if verdict["failure"] else None,
                            "cleanup": failure_details(removed, cleanup_output, cleanup_error)}))
            results.append(verdict)
    return {"schema": "fr-isolated-grade-1", "grader_sha256": expected_sha256, "candidate": retained,
            "image": grader["image"], "outcome": "passed" if all(case["passed"] for case in results) else "failed",
            "cases": results, "scope": "Exact stdout and exit status on pinned black-box cases; no general correctness claim."}
