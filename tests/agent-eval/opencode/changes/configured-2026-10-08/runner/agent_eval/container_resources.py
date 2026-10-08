"""Shared cgroup-v2 budgets for study tool and grader containers, not the host."""
import os
from pathlib import Path
import re
import sys
import tempfile
import threading
import time

from .isolated_grade import DOCKER, invoke
from .request_gateway import decode
from .study import number, require

SCHEMA = "fr-container-resources-1"
SCOPE = "Agent command and private grader containers; excludes host, Docker daemon, disk and caches."


def limits():
    return {"schema": SCHEMA, "cpu_seconds": 60, "memory_bytes": 256 * 1024**2, "pids": 64}


def validate(value):
    require(isinstance(value, dict) and set(value) == set(limits()) and value["schema"] == SCHEMA,
            "unsupported container resource profile")
    require(number(value["cpu_seconds"], "container CPU seconds", positive=True) <= 600, "container CPU ceiling exceeds 600 seconds")
    require(16 * 1024**2 <= number(value["memory_bytes"], "container memory", integer=True) <= 512 * 1024**2
            and value["memory_bytes"] % 4096 == 0, "container memory must be page-aligned and between 16 and 512 MiB")
    require(8 <= number(value["pids"], "container process limit", integer=True) <= 64, "container process limit must be 8 to 64")
    return value


def slice_name(value):
    require(isinstance(value, str) and re.fullmatch(r"frstudy[0-9a-f]{32}\.slice", value), "use a fresh frstudy<32 hex digits>.slice")
    return value


def unit(name, profile):
    slice_name(name)
    validate(profile)
    return ("[Unit]\nDescription=Isolated study container budget\n[Slice]\n"
            "CPUAccounting=yes\nMemoryAccounting=yes\nCPUQuota=50%\nCPUQuotaPeriodSec=100ms\n"
            f'MemoryMax={profile["memory_bytes"]}\nMemorySwapMax=0\nTasksMax={profile["pids"]}\n')


def fields(path):
    rows = path.read_text().splitlines()
    require(len(rows) <= 128, "excessive cgroup fields")
    result = {}
    for row in rows:
        key, value = row.split()
        require(key not in result, "duplicate cgroup field")
        result[key] = number(int(value), key, integer=True)
    return result


def environment():
    require(sys.platform == "linux", "container resource accounting needs Linux")
    mounts = Path("/proc/self/mountinfo").read_text().splitlines()
    require(any(line.split()[4] == "/sys/fs/cgroup" and " - cgroup2 " in line for line in mounts),
            "host needs the unified cgroup-v2 mount")


class ContainerResources:
    """The operator supplies a fresh, empty systemd slice with frozen kernel limits."""
    def __init__(self, profile, name, directory, *, execute=invoke, root=Path("/sys/fs/cgroup")):
        environment()
        import fcntl
        self.profile, self.name, self.directory = dict(validate(profile)), slice_name(name), directory
        self.execute, self.path = execute, root / name
        self.reason, self.monitor_error, self.kill_error = None, None, None
        self.last, self.samples, self.closed = None, 0, False
        self.done, self.lock = threading.Event(), threading.RLock()
        self.thread, self.lease = None, None
        require(self.path.is_dir() and not self.path.is_symlink(), "prepared container slice is missing")
        # flock also excludes a second host that races the empty-slice check.
        descriptor = os.open(Path(tempfile.gettempdir()) / (name + ".lock"), os.O_CREAT | os.O_RDWR | os.O_NOFOLLOW, 0o600)
        self.lease = os.fdopen(descriptor, "r+")
        try:
            fcntl.flock(self.lease, fcntl.LOCK_EX | fcntl.LOCK_NB)
            require((self.path / "memory.max").read_text().strip() == str(profile["memory_bytes"]), "slice memory limit differs")
            require((self.path / "memory.swap.max").read_text().strip() == "0", "slice swap must be disabled")
            require((self.path / "pids.max").read_text().strip() == str(profile["pids"]), "slice process limit differs")
            require((self.path / "cpu.max").read_text().strip() == "50000 100000", "slice CPU rate must be half a core")
            require(os.access(self.path / "cgroup.kill", os.W_OK), "host cannot stop the container slice")
            own = Path("/proc/self/cgroup").read_text()
            require(f"/{name}" not in own, "the host must stay outside the container slice")
            first = self.read()
            require(not any(first[key] for key in ("cpu_usec", "memory_peak_bytes", "populated", "memory_limit_events", "oom_kills", "pids_limit_events")),
                    "container slice is populated or was already used")
            self.last = first
            self.samples = 1
            result, data, _ = execute([sys.executable, "-I", "-B", str(Path(__file__).with_name("docker_info.py"))],
                                      b"", directory, 10, 65536)
            require(result["exit_code"] == 0 and not result["stop_reason"],
                    f'Docker resource preflight failed: exit={result["exit_code"]} stop={result["stop_reason"]}')
            info = decode(data)
            require(info.get("CgroupDriver") == "systemd" and str(info.get("CgroupVersion")) == "2"
                    and not any("rootless" in option for option in info.get("SecurityOptions", [])),
                    "container resources need local rootful Docker with systemd cgroup v2")
            result, data, _ = execute(DOCKER + ["context", "inspect", "default", "--format", "{{json .Endpoints.docker.Host}}"],
                                      b"", directory, 10, 4096)
            require(result["exit_code"] == 0 and not result["stop_reason"] and decode(data) == "unix:///var/run/docker.sock",
                    "container resources require the local default Docker socket")
        except BaseException:
            self.lease.close()
            self.lease = None
            raise

    def read(self):
        cpu = fields(self.path / "cpu.stat")
        memory = fields(self.path / "memory.events")
        pids = fields(self.path / "pids.events")
        return {"cpu_usec": cpu["usage_usec"], "memory_peak_bytes": number(int((self.path / "memory.peak").read_text()), "memory peak", integer=True),
                "memory_limit_events": memory["max"], "oom_kills": memory["oom_kill"],
                "pids_limit_events": pids["max"], "populated": fields(self.path / "cgroup.events")["populated"]}

    def kill(self):
        try:
            (self.path / "cgroup.kill").write_text("1")
        except OSError as error:
            self.kill_error = type(error).__name__

    def sample(self):
        with self.lock:
            try:
                observed = self.read()
                require(self.last is None or all(observed[key] >= self.last[key] for key in observed if key != "populated"),
                        "container counters moved backwards")
                self.last = observed
                self.samples += 1
                if observed["memory_limit_events"] or observed["oom_kills"]:
                    self.reason = self.reason or "memory_bytes"
                elif observed["pids_limit_events"]:
                    self.reason = self.reason or "pids"
                elif observed["cpu_usec"] >= self.profile["cpu_seconds"] * 1000000:
                    self.reason = self.reason or "cpu_seconds"
            except (OSError, ValueError, KeyError) as error:
                self.monitor_error = type(error).__name__
                self.reason = self.reason or "monitor_error"
            if self.reason:
                self.kill()

    def watch(self):
        while not self.done.wait(0.05):
            self.sample()

    def start(self):
        require(self.thread is None and not self.closed, "container monitor already started")
        self.thread = threading.Thread(target=self.watch, name="fr-container-budget", daemon=True)
        self.thread.start()
        return self

    def check(self):
        require(not self.closed, "container monitor already closed")
        self.sample()
        require(self.reason is None and self.kill_error is None, "container resource budget stopped the attempt")

    def invoke(self, command, data, directory, timeout, cap):
        require(command[:len(DOCKER)] == DOCKER, "container resource adapter only accepts Docker commands")
        cleanup = command[3:5] == ["rm", "--force"]
        if not cleanup:
            self.check()
        if command[3] == "create":
            require(not any(arg.startswith("--cgroup-parent") for arg in command), "container parent cannot be overridden")
            command = command[:4] + ["--cgroup-parent=" + self.name] + command[4:]
        result = self.execute(command, data, directory, timeout, cap)
        if command[3] == "create" and result[0]["exit_code"] == 0 and not result[0]["stop_reason"]:
            name = command[command.index("--name") + 1]
            inspected, parent, _ = self.execute(DOCKER + ["inspect", "--format", "{{json .HostConfig.CgroupParent}}", name],
                                                b"", directory, 5, 4096)
            require(inspected["exit_code"] == 0 and not inspected["stop_reason"] and decode(parent) == self.name,
                    "Docker did not retain the required container parent")
        if not cleanup:
            self.check()
        return result

    def finish(self):
        if not self.closed:
            self.done.set()
            if self.thread:
                self.thread.join(timeout=2)
                if self.thread.is_alive():
                    self.reason = self.reason or "monitor_error"
                    self.monitor_error = "ThreadTimeout"
            if self.thread and self.thread.is_alive():
                # Do not block on a lock held by a stalled sampler after its deadline.
                self.kill()
            else:
                self.sample()
                if self.last and self.last["populated"]:
                    self.reason = self.reason or "cleanup_failed"
                    self.kill()
                    deadline = time.monotonic() + 0.5
                    while self.last["populated"] and time.monotonic() < deadline and not self.monitor_error:
                        time.sleep(0.02)
                        self.sample()
            self.closed = True
            if self.lease:
                self.lease.close()
                self.lease = None
        return {"schema": SCHEMA, "limits": self.profile, "slice": self.name, "scope": SCOPE,
                "samples": self.samples, "counters": self.last, "stop_reason": self.reason,
                "monitor_error": self.monitor_error, "kill_error": self.kill_error,
                "complete": self.monitor_error is None and self.kill_error is None and self.last is not None and not self.last["populated"]}


def audit(value, profile):
    """Validate retained kernel counters without substituting them for whole-host RSS."""
    validate(profile)
    require(value["schema"] == SCHEMA and value["limits"] == profile and value["scope"] == SCOPE,
            "container resource evidence differs from frozen profile")
    slice_name(value["slice"])
    number(value["samples"], "resource sample count", integer=True, positive=True)
    counters = value["counters"]
    require(isinstance(counters, dict) and set(counters) == {"cpu_usec", "memory_peak_bytes", "memory_limit_events", "oom_kills", "pids_limit_events", "populated"},
            "container counter fields differ")
    for name, amount in counters.items():
        number(amount, name, integer=True)
    require(counters["populated"] in (0, 1), "invalid populated state")
    require(value["stop_reason"] in {None, "cpu_seconds", "memory_bytes", "pids", "monitor_error", "cleanup_failed"}, "invalid resource stop reason")
    require(type(value["complete"]) is bool and value["complete"] == (value["monitor_error"] is None and value["kill_error"] is None and not counters["populated"]),
            "resource completeness differs")
    stopped = bool(value["stop_reason"] or not value["complete"] or counters["populated"]
                   or counters["memory_limit_events"] or counters["oom_kills"] or counters["pids_limit_events"]
                   or counters["cpu_usec"] >= profile["cpu_seconds"] * 1000000)
    require(not stopped or value["stop_reason"] is not None, "resource limit has no retained stop reason")
    return {"scope": SCOPE, "cpu_seconds": counters["cpu_usec"] / 1000000,
            "memory_peak_bytes": counters["memory_peak_bytes"], "complete": value["complete"],
            "stop_reason": value["stop_reason"], "stopped": stopped}
