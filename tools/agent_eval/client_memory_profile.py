"""Bounded process attribution for hosted, scripted client captures."""
import json
import os
from pathlib import Path
import subprocess
import sys
import time

from . import bounded_host
from .study import digest, encode, number, require

MAX_TRACE = 512 * 1024
MAX_PROCESSES = 64
PHASES = ("capture-startup", "server-startup", "message-in-flight", "answer-received", "export")


def linux_cpu(raw, group, ticks):
    """Read utime + stime, excluding waited-for children, from /proc/PID/stat."""
    require(")" in raw, "invalid Linux process stat")
    fields = raw.rsplit(")", 1)[1].split()  # comm may contain spaces and parentheses.
    require(len(fields) >= 22 and int(fields[2]) == group, "process group changed during CPU sample")
    user, system = int(fields[11]), int(fields[12])
    require(user >= 0 and system >= 0 and ticks > 0, "invalid Linux CPU counters")
    return (user + system) / ticks


def measured_processes(raw, group, *, platform=None, proc=Path("/proc"), ticks=None):
    platform = sys.platform if platform is None else platform
    rows = processes(raw, group)
    if platform != "linux":
        require(platform == "darwin", "unsupported CPU counter platform")
        return rows, {"source": "darwin-ps-time", "quantum_seconds": 0.01}
    ticks = os.sysconf("SC_CLK_TCK") if ticks is None else ticks
    require(type(ticks) is int and ticks > 0, "invalid clock tick rate")
    retained = []
    for row in rows:
        try:
            stat = (proc / str(row["pid"]) / "stat").read_text()
        except (FileNotFoundError, ProcessLookupError):
            continue  # Exited between ps and stat; never substitute a rounded counter.
        row["cpu_seconds"] = linux_cpu(stat, group, ticks)
        retained.append(row)
    return retained, {"source": "linux-proc-stat", "quantum_seconds": 1 / ticks}


def phase(attempt):
    for name, label in (("export.json", "export"), ("terminal.json", "answer-received"),
                        ("request.json", "message-in-flight"), ("server.stdout", "server-startup")):
        if (attempt / name).exists():
            return label
    return "capture-startup"


def processes(raw, group):
    result = []
    for line in raw.splitlines():
        pid, pgid, rss, elapsed, executable = line.strip().split(None, 4)
        if int(pgid) != group:
            continue
        name = Path(executable).name
        require(len(name.encode()) <= 128, "process name exceeds profile budget")
        row = {"pid": int(pid), "rss_bytes": int(rss) * 1024,
               "cpu_seconds": bounded_host.cpu_seconds(elapsed), "executable": name}
        require(row["pid"] > 0 and row["rss_bytes"] >= 0, "invalid process sample")
        number(row["cpu_seconds"], "process CPU")
        result.append(row)
        require(len(result) <= MAX_PROCESSES, "process count exceeds profile budget")
    require(len({row["pid"] for row in result}) == len(result), "duplicate process sample")
    return sorted(result, key=lambda row: row["pid"])


class Sampler:
    def __init__(self, root):
        self.attempt = root / "attempts/value-review-0"
        self.path = root / "profile.jsonl"
        self.started = time.monotonic()
        self.written = 0

    def __call__(self, group):
        before, started = phase(self.attempt), time.monotonic() - self.started
        raw = subprocess.run(["ps", "-axo", "pid=,pgid=,rss=,time=,comm="],
                             capture_output=True, text=True, timeout=2, check=True).stdout
        rows, counter = measured_processes(raw, group)
        record = {"group": group, "started_seconds": started,
                  "finished_seconds": time.monotonic() - self.started,
                  "cpu_counter": counter, "phase_before": before, "phase_after": phase(self.attempt), "processes": rows}
        data = encode(record) + b"\n"
        require(self.written + len(data) <= MAX_TRACE, "profile trace exceeds budget")
        with self.path.open("ab") as stream:
            stream.write(data)
        self.written += len(data)
        return sum(row["rss_bytes"] for row in rows), {row["pid"]: row["cpu_seconds"] for row in rows}


def audit(root, process):
    path = root / "profile.jsonl"
    require(path.is_file() and not path.is_symlink() and path.stat().st_size <= MAX_TRACE,
            "invalid process profile")
    raw = path.read_bytes()
    require(raw.endswith(b"\n"), "incomplete process profile")
    samples = [json.loads(line) for line in raw.splitlines()]
    require(samples, "missing process samples")
    peak, cpu, previous, group = None, {}, 0, samples[0]["group"]
    phases = {}
    counter = samples[0].get("cpu_counter")
    if counter is not None:
        require(set(counter) == {"source", "quantum_seconds"}, "invalid CPU counter metadata")
        require(counter["source"] in {"linux-proc-stat", "darwin-ps-time"}, "unknown CPU counter")
        number(counter["quantum_seconds"], "CPU quantum", positive=True)
    for sample in samples:
        require(sample.get("cpu_counter") == counter, "CPU counter changed during capture")
        require(set(sample) == ({"group", "started_seconds", "finished_seconds", "phase_before", "phase_after", "processes"} | ({"cpu_counter"} if counter is not None else set())),
                "unexpected profile fields")
        require(type(group) is int and group > 0 and sample["group"] == group, "profile group changed")
        for key in ("started_seconds", "finished_seconds"):
            number(sample[key], key)
        require(previous <= sample["started_seconds"] <= sample["finished_seconds"], "profile times out of order")
        previous = sample["finished_seconds"]
        require(sample["phase_before"] in PHASES and sample["phase_after"] in PHASES, "unknown capture phase")
        rows = sample["processes"]
        require(isinstance(rows, list) and len(rows) <= MAX_PROCESSES, "invalid process count")
        seen, total = set(), 0
        for row in rows:
            require(set(row) == {"pid", "rss_bytes", "cpu_seconds", "executable"}, "unexpected process fields")
            require(type(row["pid"]) is int and row["pid"] > 0 and row["pid"] not in seen, "invalid process identity")
            seen.add(row["pid"])
            require(type(row["rss_bytes"]) is int and row["rss_bytes"] >= 0, "invalid process RSS")
            name = row["executable"]
            require(isinstance(name, str) and name and Path(name).name == name and len(name.encode()) <= 128,
                    "invalid executable name")
            number(row["cpu_seconds"], "process CPU")
            cpu[row["pid"]] = max(cpu.get(row["pid"], 0), row["cpu_seconds"])
            total += row["rss_bytes"]
        if peak is None or total > peak["rss_bytes"]:
            peak = {"rss_bytes": total, "sample": sample}
        label = sample["phase_before"] if sample["phase_before"] == sample["phase_after"] else "phase-transition"
        phases[label] = max(phases.get(label, 0), total)
    require(peak["rss_bytes"] == process["sampled_aggregate_rss_bytes"], "profile peak differs from guard")
    require(abs(sum(cpu.values()) - process["sampled_cpu_seconds"]) < 1e-6, "profile CPU differs from guard")
    return {"schema": "fr-client-process-profile-1", "samples": len(samples), "trace_sha256": digest(samples),
            "peak": peak, "phase_peak_rss_bytes": phases,
            "scope": "Same samples as the guard; phases inferred from artifact creation, not heap attribution. "
                     "Brief peaks, escaped process groups and shared-page double counting remain possible."}
