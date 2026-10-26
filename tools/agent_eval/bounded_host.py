"""Sampled POSIX process-group limits for trusted evaluation hosts, not a sandbox."""
from __future__ import annotations

import os
import ctypes
import errno
from pathlib import Path
import selectors
import signal
import subprocess
import sys
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


class DarwinTaskInfo(ctypes.Structure):
    _fields_ = [(name, ctypes.c_uint64) for name in
                ('virtual_size', 'resident_size', 'total_user', 'total_system', 'threads_user', 'threads_system')]
    _fields_ += [(name, ctypes.c_int32) for name in
                 ('policy', 'faults', 'pageins', 'cow_faults', 'messages_sent', 'messages_received',
                  'syscalls_mach', 'syscalls_unix', 'csw', 'threadnum', 'numrunning', 'priority')]


class DarwinTimebase(ctypes.Structure):
    _fields_ = [('numer', ctypes.c_uint32), ('denom', ctypes.c_uint32)]


def darwin_sample(group):
    """Read the selected group directly; a CPU throttle can pause a ps subprocess past its deadline."""
    library = ctypes.CDLL('/usr/lib/libproc.dylib', use_errno=True)
    library.proc_listpids.argtypes = [ctypes.c_uint32, ctypes.c_uint32, ctypes.c_void_p, ctypes.c_int]
    library.proc_listpids.restype = ctypes.c_int
    library.proc_pidinfo.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_uint64, ctypes.c_void_p, ctypes.c_int]
    library.proc_pidinfo.restype = ctypes.c_int
    system = ctypes.CDLL('/usr/lib/libSystem.B.dylib', use_errno=True)
    system.mach_timebase_info.argtypes = [ctypes.POINTER(DarwinTimebase)]
    system.mach_timebase_info.restype = ctypes.c_int
    timebase = DarwinTimebase()
    require(system.mach_timebase_info(ctypes.byref(timebase)) == 0 and timebase.numer > 0 and timebase.denom > 0,
            'cannot read the macOS CPU timebase')
    pids = (ctypes.c_int * 4096)()
    ctypes.set_errno(0)
    size = library.proc_listpids(2, group, pids, ctypes.sizeof(pids))
    error = ctypes.get_errno()
    if size < 0 or (size == 0 and error):
        raise OSError(error or errno.EIO, 'cannot list the monitored macOS process group')
    require(size < ctypes.sizeof(pids) and size % ctypes.sizeof(ctypes.c_int) == 0,
            'macOS process-group inventory is truncated or malformed')
    memory, cpu = 0, {}
    for pid in pids[:size // ctypes.sizeof(ctypes.c_int)]:
        require(pid > 0, 'invalid macOS process-group member')
        info = DarwinTaskInfo()
        ctypes.set_errno(0)
        size = library.proc_pidinfo(pid, 4, 0, ctypes.byref(info), ctypes.sizeof(info))
        error = ctypes.get_errno()
        if size == 0 and error == errno.ESRCH:
            continue
        if size != ctypes.sizeof(info):
            raise OSError(error or errno.EIO, 'cannot sample a monitored macOS process')
        memory += info.resident_size
        cpu[pid] = (info.total_user + info.total_system) * timebase.numer / timebase.denom / 1_000_000_000
    return memory, cpu


def sample(group):
    if sys.platform == 'darwin':
        return darwin_sample(group)
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
        cpu_limit_seconds=600, transcript_bytes=16 * 1024**2, env=None, cwd=None):
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
                                       stderr=subprocess.PIPE, start_new_session=True, env=env, cwd=cwd)
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
