#!/usr/bin/env python3
"""Bound generated Lean proofs on macOS and Linux, including their subprocesses."""

import argparse
import fcntl
import os
from pathlib import Path
import selectors
import signal
import subprocess
import sys
import time


class GuardFailure(Exception):
    pass


def group_rss(group):
    result = subprocess.run(
        ["ps", "-axo", "pgid=,rss="], capture_output=True, text=True,
        check=True, timeout=2,
    )
    return sum(int(rss) * 1024 for pgid, rss in
               (line.split() for line in result.stdout.splitlines()) if int(pgid) == group)


def stop_group(child):
    try:
        os.killpg(child.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    child.wait(timeout=5)


def run(args):
    child = None
    output = bytearray()
    peak = 0
    started = time.monotonic()
    group_rss(os.getpgrp())  # Refuse to launch if memory monitoring is unavailable.
    args.lock.parent.mkdir(parents=True, exist_ok=True)
    with args.lock.open("a") as lock, selectors.DefaultSelector() as selector:
        while True:
            try:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
                break
            except BlockingIOError:
                if time.monotonic() - started >= args.seconds:
                    raise GuardFailure("timed out waiting for another proof")
                time.sleep(0.1)
        started = time.monotonic()
        try:
            child = subprocess.Popen(
                args.command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                start_new_session=True, env=dict(os.environ, LEAN_NUM_THREADS="1"),
            )
            selector.register(child.stdout, selectors.EVENT_READ)
            while True:
                rss = group_rss(child.pid)
                peak = max(peak, rss)
                if rss > args.memory_mib * 1024 * 1024:
                    raise GuardFailure(f"process group exceeded {args.memory_mib:g} MiB RSS")
                if time.monotonic() - started >= args.seconds:
                    raise GuardFailure(f"exceeded {args.seconds:g} seconds")
                for key, _ in selector.select(timeout=0.1):
                    chunk = os.read(key.fileobj.fileno(), 65536)
                    if not chunk:
                        selector.unregister(key.fileobj)
                    elif len(output) + len(chunk) > args.output_bytes:
                        raise GuardFailure(f"exceeded {args.output_bytes} output bytes")
                    else:
                        output.extend(chunk)
                if child.poll() is not None and not selector.get_map():
                    return child.returncode if child.returncode >= 0 else 128 - child.returncode
        finally:
            if child is not None:
                stop_group(child)
                child.stdout.close()
            sys.stdout.buffer.write(output)
            print(f"lean-guard: peak sampled RSS {peak / 1024 / 1024:.1f} MiB; "
                  f"elapsed {time.monotonic() - started:.1f}s", file=sys.stderr)


def positive(value):
    import math
    result = float(value)
    if not math.isfinite(result) or result <= 0:
        raise argparse.ArgumentTypeError("must be a finite positive number")
    return result


def interrupted(signum, _frame):
    raise GuardFailure(f"interrupted by signal {signum}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--seconds", type=positive, default=120)
    parser.add_argument("--memory-mib", type=positive, default=1024)
    parser.add_argument("--output-bytes", type=int, default=4 * 1024 * 1024)
    parser.add_argument("--lock", type=Path,
                        default=Path(__file__).resolve().parents[1] / "kernels/.lake/proof.lock")
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    if args.command[:1] == ["--"]:
        args.command.pop(0)
    if not args.command or args.output_bytes <= 0:
        parser.error("a command and a positive output limit are required")
    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGINT, interrupted)
    try:
        return run(args)
    except (GuardFailure, OSError, subprocess.SubprocessError, ValueError) as error:
        print(f"lean-guard: refused: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
