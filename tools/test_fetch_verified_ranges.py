"""Fake curl regressions for bounded range resume; no network or large archives."""
import hashlib
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class VerifiedRanges(unittest.TestCase):
    def test_partial_ranges_resume_and_preserve_exact_bytes(self):
        self.run_case("partial", True)

    def test_wrong_content_range_refuses_without_overwriting_output(self):
        self.run_case("wrong-range", False)

    def test_hash_mismatch_refuses_without_overwriting_output(self):
        self.run_case("corrupt", False)

    def run_case(self, mode, success):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "source"
            source.write_bytes(b"abcdefghijklmnop")
            output = root / "output"
            output.write_bytes(b"keep existing output")
            shim = root / "curl"
            shim.write_text(f"#!{sys.executable}\n" + '''import os,sys
from pathlib import Path
args=sys.argv[1:]
def arg(key): return args[args.index(key)+1]
start,end=map(int,arg('--range').split('-'))
source=Path(os.environ['RANGE_SOURCE']).read_bytes()
mode=os.environ['RANGE_MODE']
part=Path(arg('-o'))
marker=Path(str(part)+'.seen')
first=not marker.exists()
marker.touch()
body=source[start:end+1]
if mode=='partial' and first: body=body[:max(1,len(body)//2)]
if mode=='corrupt': body=b'X'*len(body)
reported=start+1 if mode=='wrong-range' else start
Path(arg('--dump-header')).write_text(f'HTTP/1.1 206 Partial Content\\r\\nContent-Range: bytes {reported}-{end}/{len(source)}\\r\\n')
part.write_bytes(body)
with Path(os.environ['RANGE_LOG']).open('a') as log: log.write(f'{start}-{end}\\n')
print('206',end='')
sys.exit(28 if mode=='partial' and first else 0)
''')
            shim.chmod(0o700)
            sleep = root / "sleep"
            sleep.write_text("#!/bin/sh\nexit 0\n")
            sleep.chmod(0o700)
            log = root / "ranges"
            env = {**os.environ, "PATH": str(root) + os.pathsep + os.environ["PATH"], "RANGE_SOURCE": str(source),
                   "RANGE_MODE": mode, "RANGE_LOG": str(log)}
            result = subprocess.run(["bash", str(ROOT / "tools/fetch-verified-ranges.sh"), "https://fixture.invalid/archive", str(output),
                                     hashlib.sha256(source.read_bytes()).hexdigest(), "16", "2"], env=env,
                                    capture_output=True, text=True, timeout=20)
            if success:
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(output.read_bytes(), source.read_bytes())
                self.assertEqual(sorted(log.read_text().splitlines()), ["0-7", "12-15", "4-7", "8-15"])
            else:
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(output.read_bytes(), b"keep existing output")
            self.assertFalse(list(root.glob("output.part.*")))
            self.assertFalse(list(root.glob("output.assembled.*")))


if __name__ == "__main__":
    unittest.main()
