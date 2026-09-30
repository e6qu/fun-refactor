"""Owned test descendants keep a heartbeat and deliberately ignore TERM."""
from pathlib import Path
import signal
import subprocess
import sys
import time

path = Path(sys.argv[2])
if sys.argv[1] == 'parent':
    subprocess.Popen([sys.executable, __file__, 'child', str(path)])
    time.sleep(30)
else:
    signal.signal(signal.SIGTERM, signal.SIG_IGN)
    deadline = time.monotonic() + 30
    counter = 0
    while time.monotonic() < deadline:
        counter += 1
        path.write_text(str(counter))
        time.sleep(0.01)
