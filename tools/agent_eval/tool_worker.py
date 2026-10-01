"""Runs only inside a disposable container; never import this as a host executor."""
import json
import os
from pathlib import Path
import selectors
import signal
import subprocess
import sys
import tempfile
import time

from workspace_bundle import pack, unpack, check


def execute(argv, supplied, seconds, cap):
    check(isinstance(argv, list) and 1 <= len(argv) <= 128
          and all(isinstance(arg, str) and "\0" not in arg for arg in argv) and argv[0], "invalid command argv")
    check(isinstance(supplied, str) and len(supplied.encode()) <= 65536, "command input limit")
    output = {"stdout": bytearray(), "stderr": bytearray()}
    stop = None
    with tempfile.TemporaryFile() as source, selectors.DefaultSelector() as selector:
        source.write(supplied.encode())
        source.seek(0)
        process = subprocess.Popen(argv, stdin=source, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                   cwd="/workspace/project", start_new_session=True,
                                   env={"PATH": "/opt/fr:/usr/local/bin:/usr/bin:/bin", "HOME": "/tmp",
                                        "TMPDIR": "/tmp", "RAYON_NUM_THREADS": "1", "CARGO_BUILD_JOBS": "1"})
        started = time.monotonic()
        def kill():
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        try:
            selector.register(process.stdout, selectors.EVENT_READ, "stdout")
            selector.register(process.stderr, selectors.EVENT_READ, "stderr")
            while selector.get_map() or process.poll() is None:
                if time.monotonic() - started >= seconds:
                    stop = "wall_seconds"
                    break
                if process.poll() is not None:
                    kill()
                for key, _ in selector.select(0.05):
                    data = os.read(key.fileobj.fileno(), 16384)
                    if not data:
                        selector.unregister(key.fileobj)
                    else:
                        remaining = cap - sum(map(len, output.values()))
                        output[key.data].extend(data[:remaining])
                        if len(data) > remaining:
                            stop = "output_bytes"
                            break
                if stop:
                    break
        finally:
            kill()
            code = process.wait(timeout=2)
            process.stdout.close()
            process.stderr.close()
    return {"exit_code": code, "stop_reason": stop,
            **{key: value.decode("utf-8", errors="replace") for key, value in output.items()}}


def main():
    request = json.loads(sys.stdin.buffer.read(16 * 1024**2 + 1))
    limits = request["limits"]
    root = Path("/workspace/project")
    unpack(request["files"], root, limits["workspace_bytes"])
    try:
        result = execute(request["argv"], request["stdin"], limits["command_seconds"], limits["output_bytes"])
    except OSError as error:
        result = {"exit_code": None, "stop_reason": "launch_error", "stdout": "", "stderr": type(error).__name__}
    print(json.dumps({"files": pack(root, limits["workspace_bytes"]), "result": result}))


if __name__ == "__main__":
    main()
