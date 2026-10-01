"""Capture OpenCode exports through a bounded regular file before forwarding bytes."""

import os
import subprocess
import sys
import tempfile

LIMIT = 1024**2


def main():
    binary, operation, session, directory = sys.argv[1:]
    if operation != "export":
        return 1
    # A regular file avoids OpenCode's premature pipe exit. Limit this export,
    # not OpenCode's existing database files; the outer host monitors the group.
    with tempfile.NamedTemporaryFile(dir=directory, prefix="opencode-export-", suffix=".json") as output:
        process = subprocess.Popen([binary, "export", session], stdout=output)
        try:
            while True:
                if os.fstat(output.fileno()).st_size > LIMIT:
                    return 1
                try:
                    code = process.wait(timeout=0.01)
                    break
                except subprocess.TimeoutExpired:
                    pass
            if code or os.fstat(output.fileno()).st_size > LIMIT:
                return 1
            output.seek(0)
            raw = output.read(LIMIT + 1)
            if len(raw) > LIMIT:
                return 1
            sys.stdout.buffer.write(raw)
            sys.stdout.buffer.flush()
        finally:
            if process.poll() is None:
                process.kill()
            process.wait()
    return 0


if __name__ == "__main__":
    sys.exit(main())
