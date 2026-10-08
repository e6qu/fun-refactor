"""Public check feedback bound to a candidate snapshot and a shared container budget."""
import base64
import hashlib
import os
from pathlib import Path, PurePosixPath
import re
import sys
import tempfile

from . import container_resources, isolated_grade
from .study import digest, encode, number, require
from .workspace_bundle import unpack

MAX_RUNS = 2
RESOURCES = {"schema": container_resources.SCHEMA, "cpu_seconds": 10,
             "memory_bytes": 128 * 1024**2, "pids": 32}
GUIDANCE = """describe_checks returns the frozen public check command and cases.
run_checks executes that check against the current files on an isolated GitHub runner.
You may run it twice. Inspect failures, edit, and check again within the same task budget.
Results name their source snapshot; editing makes an earlier result stale.
Public check success is not a private behavior grade or a proof. Keep uncertainties visible.
"""


def schemas():
    return [{"name": name, "description": description,
             "inputSchema": {"type": "object", "additionalProperties": False, "required": [], "properties": {}}}
            for name, description in (
                ("describe_checks", "Read the frozen public check command, cases and limits without executing code."),
                ("run_checks", "Run the public check on the current source snapshot; at most twice per attempt."))]


def validate(profile):
    limits = isolated_grade.validate(profile)
    require(len(profile["cases"]) == 1, "public feedback needs one bounded check command")
    ceilings = {"wall_seconds": 10, "memory_bytes": RESOURCES["memory_bytes"],
                "scratch_bytes": 1024**2, "output_bytes": 2048, "candidate_bytes": 1024**2}
    require(all(limits[key] <= cap for key, cap in ceilings.items()), "public check exceeds feedback limits")
    require(len(encode(profile)) <= 8192, "public check description exceeds budget")
    return profile


def candidate_identity(files):
    entries, directories = [], set()
    for path, item in files.items():
        raw = base64.b64decode(item["data"], validate=True)
        entries.append({"path": path, "executable": item["executable"],
                        "sha256": hashlib.sha256(raw).hexdigest(), "bytes": len(raw)})
        directories.update(str(p) for p in PurePosixPath(path).parents if str(p) != ".")
    entries.extend({"path": p, "directory": True, "bytes": 0} for p in directories)
    return {"sha256": digest(sorted(entries, key=lambda e: e["path"])), "files": len(entries),
            "bytes": sum(e["bytes"] for e in entries)}


def verify_grade(spec, expected_sha, candidate, grade):
    require(grade["schema"] == "fr-isolated-grade-1" and grade["candidate"] == candidate,
            "graded candidate differs")
    require(grade["image"] == spec["image"] and grade["grader_sha256"] == expected_sha, "grader identity differs")
    require([c["id"] for c in grade["cases"]] == [c["id"] for c in spec["cases"]], "graded case set differs")
    for actual, expected in zip(grade["cases"], spec["cases"]):
        require(type(actual["passed"]) is bool, "invalid case verdict")
        if "execution" not in actual:
            require(actual["passed"] is False and isinstance(actual["failure"], str) and actual["failure"],
                    "missing failed case evidence")
            continue
        for channel in ("stdout", "stderr"):
            raw = base64.b64decode(actual[channel + "_base64"], validate=True)
            require(len(raw) == number(actual[channel + "_bytes"], "stream bytes", integer=True)
                    and hashlib.sha256(raw).hexdigest() == actual[channel + "_sha256"], "case stream identity differs")
        state = actual["container_state"]
        require(type(state["Running"]) is bool and type(state["OOMKilled"]) is bool, "invalid container state")
        number(state["ExitCode"], "container exit code", integer=True)
        passed = (not actual["execution"]["stop_reason"] and not state["Running"]
                  and not state["OOMKilled"] and not state.get("Error")
                  and state["ExitCode"] == expected["exit_code"]
                  and base64.b64decode(actual["stdout_base64"]) == expected["stdout"].encode())
        require(actual["passed"] == passed, "case verdict differs from retained execution")
    passed = all(c["passed"] for c in grade["cases"])
    require(grade["outcome"] == ("passed" if passed else "failed"), "grade outcome differs")
    return passed


def require_runner():
    require(sys.platform == "linux" and os.environ.get("GITHUB_ACTIONS") == "true",
            "public check execution belongs on a Linux GitHub runner")


class DockerChecks:
    def __init__(self, name, directory, *, execute=isolated_grade.invoke):
        require_runner()
        self.name, self.directory = container_resources.slice_name(name), Path(directory)
        self.execute = execute

    def invoke(self, command, data, directory, timeout, cap):
        require(command[:3] == isolated_grade.DOCKER, "public check backend only accepts Docker commands")
        if command[3] == "create":
            require(not any(arg.startswith("--cgroup-parent") for arg in command), "container parent cannot be overridden")
            command = command[:4] + ["--cgroup-parent=" + self.name,
                                     "--label=fr.native-check=" + self.name] + command[4:]
        result = self.execute(command, data, directory, timeout, cap)
        if command[3] == "create" and result[0]["exit_code"] == 0 and not result[0]["stop_reason"]:
            name = command[command.index("--name") + 1]
            state, parent, _ = self.execute(isolated_grade.DOCKER + ["inspect", "--format", "{{json .HostConfig.CgroupParent}}", name],
                                             b"", directory, 5, 4096)
            require(state["exit_code"] == 0 and not state["stop_reason"]
                    and isolated_grade.decode(parent) == self.name, "public check container parent differs")
        return result

    def run(self, files, profile):
        with tempfile.TemporaryDirectory(prefix="fr-public-check-", dir=self.directory) as temporary:
            root = Path(temporary)
            unpack(files, root / "source", 1024**2)
            path = root / "check.json"
            raw = encode(profile)
            path.write_bytes(raw)
            return isolated_grade.grade(root / "source", path, hashlib.sha256(raw).hexdigest(),
                                        execute=self.invoke, temporary_parent=self.directory)

    def cleanup(self):
        state, output, _ = self.execute(isolated_grade.DOCKER + ["ps", "--all", "--quiet", "--no-trunc",
            "--filter", "label=fr.native-check=" + self.name], b"", self.directory, 5, 4096)
        require(state["exit_code"] == 0 and not state["stop_reason"], "public check cleanup inventory failed")
        names = output.decode().splitlines()
        require(len(names) <= MAX_RUNS and len(set(names)) == len(names)
                and all(re.fullmatch(r"[0-9a-f]{64}", name) for name in names), "unexpected public check cleanup inventory")
        for name in names:
            result, _, _ = self.execute(isolated_grade.DOCKER + ["rm", "--force", name], b"", self.directory, 5, 4096)
            require(result["exit_code"] == 0 and not result["stop_reason"], "public check cleanup failed; stop worker")


class Checks:
    def __init__(self, machine, profile, execute=None):
        self.machine, self.profile, self.execute = machine, validate(profile), execute
        self.sha, self.runs, self.latest = hashlib.sha256(encode(profile)).hexdigest(), 0, None

    def describe(self):
        return {"profile_sha256": self.sha, "profile": self.profile, "max_runs": MAX_RUNS}

    def run(self, retained=None):
        require(self.runs < MAX_RUNS, "public check call budget exhausted")
        self.runs += 1
        basis = {"schema": "fr-native-public-check-1", "run": self.runs,
                 "profile_sha256": self.sha, "source_sha256": digest(self.machine.files)}
        if self.machine.replay:
            require(isinstance(retained, dict) and all(retained.get(k) == v for k, v in basis.items()),
                    "public check result identity differs")
            result = retained
        else:
            try:
                require(self.execute is not None, "public check backend unavailable")
                grade = self.execute(self.machine.files, self.profile)
                result = {**basis, "status": grade["outcome"], "grade": grade}
            except (OSError, ValueError, KeyError) as error:
                result = {**basis, "status": "unavailable", "failure": str(error)[:256]}
        if result["status"] == "unavailable":
            require(set(result) == set(basis) | {"status", "failure"}
                    and isinstance(result["failure"], str) and 0 < len(result["failure"]) <= 256,
                    "invalid unavailable check")
        else:
            require(set(result) == set(basis) | {"status", "grade"}, "unexpected public check evidence")
            passed = verify_grade(self.profile, self.sha, candidate_identity(self.machine.files), result["grade"])
            require(result["status"] == ("passed" if passed else "failed"), "public check status differs")
        require(len(encode(result)) <= 16384, "public check result exceeds budget")
        self.latest = {**basis, "status": result["status"]}
        return result

    def summary(self):
        return {"runs": self.runs, "latest": self.latest,
                "current": self.latest is not None and self.latest["source_sha256"] == digest(self.machine.files),
                "offline_semantics_reexecuted": False}
