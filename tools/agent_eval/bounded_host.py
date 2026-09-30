"""Sampled POSIX process-group limits for trusted evaluation hosts, not a sandbox."""
from __future__ import annotations

import os
from pathlib import Path
import selectors
import signal
import subprocess
import tempfile
import time

from .study import number, require


def disk_size(root):
    total = 0
    for directory, _, files in os.walk(root, followlinks=False):
        for name in files:
            try:
                path = Path(directory) / name
                if not path.is_symlink():
                    total += path.stat().st_size
            except FileNotFoundError:
                pass
    return total


def cpu_seconds(value):
    days, _, clock = value.rpartition("-")
    result = 0.0
    for part in clock.split(":"):
        result = result * 60 + float(part)
    return result + (int(days) * 86400 if days else 0)


def sample(group):
    rows = subprocess.run(["ps", "-axo", "pid=,pgid=,rss=,time="],
                          capture_output=True, text=True, timeout=2, check=True).stdout
    memory, cpu = 0, {}
    for line in rows.splitlines():
        pid, pgid, rss, elapsed = line.split()
        if int(pgid) == group:
            memory += int(rss) * 1024
            cpu[int(pid)] = cpu_seconds(elapsed)
    return memory, cpu


def stop(group):
    try:
        os.killpg(group, signal.SIGKILL)
    except ProcessLookupError:
        pass


def run(command, prompt, stdout, stderr, workspace, *, wall_seconds,
        rss_bytes=768 * 1024**2, disk_bytes=128 * 1024**2,
        cpu_limit_seconds=600, transcript_bytes=16 * 1024**2):
    """Stop the entire group on a limit, monitor failure, or parent exit.

    Limits are sampled: brief peaks and processes escaping the group need OS
    containment on the runner. CPU totals retain samples from exited children.
    Disk measures workspace growth; files elsewhere require OS quotas.
    """
    require(os.name == "posix", "host process-group limits require POSIX")
    limits = {"wall_seconds": wall_seconds, "rss_bytes": rss_bytes,
              "disk_bytes": disk_bytes, "cpu_seconds": cpu_limit_seconds,
              "transcript_bytes": transcript_bytes}
    for key, value in limits.items():
        number(value, key)
        require(value > 0, f"{key} must be positive")
    started, initial_disk = time.monotonic(), disk_size(workspace)
    reason, error, process = None, None, None
    peak_rss, growth, written, cpu = 0, 0, 0, {}
    code = None
    with tempfile.TemporaryFile() as input_file, selectors.DefaultSelector() as selector:
        input_file.write(prompt)
        input_file.seek(0)
        try:
            process = subprocess.Popen(command, stdin=input_file, stdout=subprocess.PIPE,
                                       stderr=subprocess.PIPE, start_new_session=True)
            selector.register(process.stdout, selectors.EVENT_READ, stdout)
            selector.register(process.stderr, selectors.EVENT_READ, stderr)
            next_sample = 0
            while selector.get_map() or process.poll() is None:
                now = time.monotonic()
                if now - started >= wall_seconds:
                    reason = "wall_seconds"
                if now >= next_sample:
                    memory, current = sample(process.pid)
                    peak_rss = max(peak_rss, memory)
                    for pid, seconds in current.items():
                        cpu[pid] = max(cpu.get(pid, 0), seconds)
                    growth = max(growth, disk_size(workspace) - initial_disk)
                    if peak_rss > rss_bytes:
                        reason = "rss_bytes"
                    elif sum(cpu.values()) > cpu_limit_seconds:
                        reason = "cpu_seconds"
                    elif growth > disk_bytes:
                        reason = "disk_bytes"
                    next_sample = now + 0.1
                if reason:
                    break
                # A completed host must not leave its workers running or holding pipes.
                if process.poll() is not None:
                    stop(process.pid)
                for key, _ in selector.select(0.05):
                    block = os.read(key.fileobj.fileno(), 65536)
                    if not block:
                        selector.unregister(key.fileobj)
                        continue
                    remaining = max(0, transcript_bytes - written)
                    key.data.write(block[:remaining])
                    written += min(len(block), remaining)
                    if len(block) > remaining:
                        reason = "transcript_bytes"
                        break
                if reason:
                    break
        except (OSError, subprocess.SubprocessError, ValueError) as failure:
            error = str(failure)
            reason = "launch_error" if process is None else "monitor_error"
        finally:
            if process is not None:
                stop(process.pid)
                code = process.wait(timeout=5)
                process.stdout.close()
                process.stderr.close()
    try:
        stdout.flush()
        stderr.flush()
        growth = max(growth, disk_size(workspace) - initial_disk)
        if reason is None and growth > disk_bytes:
            reason = "disk_bytes"
    except OSError as failure:
        error, reason = str(failure), "monitor_error"
    return {"exit_code": 124 if reason == "wall_seconds" else (125 if reason and process else code),
            "process_exit_code": code,
            "timed_out": reason == "wall_seconds", "stop_reason": reason,
            "launch_error": error if process is None else None, "monitor_error": error if process else None,
            "elapsed_seconds": time.monotonic() - started, "limits": limits,
            "sampled_aggregate_rss_bytes": peak_rss, "sampled_cpu_seconds": sum(cpu.values()),
            "sampled_disk_growth_bytes": growth, "transcript_bytes": written}
