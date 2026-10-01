"""No provider calls: exercise regular-file capture and its sampled size limit."""

import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


class ExportCapture(unittest.TestCase):
    def invoke(self, source):
        with tempfile.TemporaryDirectory() as tmp:
            binary = Path(tmp) / "fake-opencode"
            binary.write_text("#!" + sys.executable + "\n" + source)
            binary.chmod(0o700)
            return subprocess.run([sys.executable, "-I", "-B", str(Path(__file__).with_name("opencode_export.py")),
                                   str(binary), "export", "ses_control", tmp], capture_output=True, timeout=5)

    def test_export_larger_than_pipe_buffer_remains_complete(self):
        result = self.invoke("import json,os,stat,sys\n"
                             "assert sys.argv[1:] == ['export', 'ses_control']\n"
                             "assert stat.S_ISREG(os.fstat(1).st_mode)\n"
                             "sys.stdout.write(json.dumps({'text': 'x'*120000}))\n")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(json.loads(result.stdout)["text"]), 120000)

    def test_nonzero_exit_never_promotes_partial_export(self):
        result = self.invoke("import sys\nprint('partial')\nsys.exit(7)\n")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(result.stdout, b"")

    def test_export_cap_does_not_limit_existing_database_files(self):
        result = self.invoke("from pathlib import Path\n"
                             "cache = Path(__file__).with_name('existing-cache')\n"
                             "cache.write_bytes(b'x'*(2*1024**2))\n"
                             "with cache.open('r+b') as output:\n"
                             "    output.seek(1024**2)\n"
                             "    output.write(b'updated')\n"
                             "print('{}')\n")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout), {})

    def test_file_limit_refuses_oversized_export(self):
        result = self.invoke("import os\nos.write(1, b'x'*(2*1024**2))\nos.write(1, b'x')\n")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(result.stdout, b"")


if __name__ == "__main__":
    unittest.main()
