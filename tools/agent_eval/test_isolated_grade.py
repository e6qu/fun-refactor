"""Grading protocol tests plus opt-in Docker checks for GitHub workers."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval.isolated_grade import DOCKER, create_command, grade, snapshot, validate


def rubric(image=None):
    return {"schema": "fr-stdio-grader-1", "image": image or "sha256:" + "a" * 64,
            "command": ["python3", "/workspace/answer.py"],
            "limits": {"wall_seconds": 2, "memory_bytes": 64 * 1024**2, "scratch_bytes": 1024**2,
                       "output_bytes": 4096, "candidate_bytes": 1024**2},
            "cases": [{"id": "hidden", "stdin": "6\n", "stdout": "42\n", "exit_code": 0}]}


class Grading(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.candidate = self.root / "submission"
        self.candidate.mkdir()
        (self.candidate / "answer.py").write_text("print(int(input())*7)\n")
        self.grader = self.root / "private.json"
        self.grader.write_text(json.dumps(rubric()))
        self.sha = hashlib.sha256(self.grader.read_bytes()).hexdigest()
        self.calls = []
        self.answer = b"42\n"
        self.stop = None
        self.oom = False

    def execute(self, command, data, directory, timeout, cap):
        self.calls.append((command, data))
        result = {"exit_code": 0, "stop_reason": None}
        if command[3] == "start":
            result["stop_reason"] = self.stop
            return result, self.answer, b""
        if command[3] == "inspect":
            return result, json.dumps({"Running": False, "OOMKilled": self.oom, "Error": "", "ExitCode": 0}).encode(), b""
        return result, b"", b""

    def test_private_expected_output_is_not_sent_to_candidate(self):
        result = grade(self.candidate, self.grader, self.sha, execute=self.execute)
        self.assertEqual(result["outcome"], "passed")
        self.assertEqual([entry[0][3] for entry in self.calls], ["create", "start", "inspect", "rm"])
        self.assertNotIn(str(self.grader), str(self.calls))
        for command, data in self.calls:
            self.assertNotIn("42\n", command)
            self.assertNotEqual(data, b"42\n")
        self.assertEqual(self.calls[1][1], b"6\n")
        for option in ("--network=none", "--read-only", "--cap-drop=ALL", "--security-opt=no-new-privileges", "--cpus=0.5", "--pids-limit=32"):
            self.assertIn(option, self.calls[0][0])

    def test_incorrect_output_timeout_and_oom_cannot_pass(self):
        for answer, stop, oom in ((b"wrong\n", None, False), (b"42\n", "wall_seconds", False), (b"42\n", None, True)):
            with self.subTest(stop=stop, oom=oom):
                self.answer, self.stop, self.oom = answer, stop, oom
                result = grade(self.candidate, self.grader, self.sha, execute=self.execute)
                self.assertEqual(result["outcome"], "failed")
                self.assertEqual(self.calls[-1][0][3:5], ["rm", "--force"])

    def test_cleanup_failure_refuses_result(self):
        def fail(command, *args):
            if command[3] == "rm":
                return {"exit_code": 1, "stop_reason": None}, b"", b""
            return self.execute(command, *args)
        with self.assertRaisesRegex(ValueError, "cleanup failed"):
            grade(self.candidate, self.grader, self.sha, execute=fail)

    def test_pinned_grader_and_private_location_are_required(self):
        with self.assertRaisesRegex(ValueError, "digest"):
            grade(self.candidate, self.grader, "b" * 64, execute=self.execute)
        visible = self.candidate / "private.json"
        visible.write_bytes(self.grader.read_bytes())
        with self.assertRaisesRegex(ValueError, "private grader"):
            grade(self.candidate, visible, self.sha, execute=self.execute)
        self.assertEqual(self.calls, [])

    def test_snapshot_binds_content_and_empty_directories(self):
        first = snapshot(self.candidate, self.root / "first", 1024)
        (self.candidate / "empty").mkdir()
        second = snapshot(self.candidate, self.root / "second", 1024)
        self.assertNotEqual(first["sha256"], second["sha256"])
        (self.candidate / "answer.py").write_text("print('wrong')\n")
        third = snapshot(self.candidate, self.root / "third", 1024)
        self.assertNotEqual(second["sha256"], third["sha256"])
        with self.assertRaisesRegex(ValueError, "snapshot exceeds"):
            snapshot(self.candidate, self.root / "small", 1)
        with self.assertRaisesRegex(ValueError, "outside candidate"):
            snapshot(self.candidate, self.candidate / "nested", 1024)

    def test_snapshot_refuses_symlinks_and_special_files(self):
        external = self.candidate / "link"
        external.symlink_to(self.grader)
        with self.assertRaisesRegex(ValueError, "symlinks"):
            snapshot(self.candidate, self.root / "linked", 1024)
        external.unlink()
        os.mkfifo(external)
        with self.assertRaisesRegex(ValueError, "special file"):
            snapshot(self.candidate, self.root / "fifo", 1024)

    def test_unpinned_images_and_unbounded_resources_refuse(self):
        value = rubric()
        value["image"] = "python:latest"
        with self.assertRaisesRegex(ValueError, "immutable"):
            validate(value)
        for field in rubric()["limits"]:
            value = rubric()
            value["limits"][field] = 2**40
            with self.subTest(field=field), self.assertRaisesRegex(ValueError, "maximum"):
                validate(value)


@unittest.skipUnless(os.environ.get("FR_STUDY_TEST_IMAGE"), "Docker checks run on GitHub, not the workstation")
class DockerGrading(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.candidate = self.root / "submission"
        self.candidate.mkdir()
        self.grader = self.root / "private.json"
        self.value = rubric(os.environ["FR_STUDY_TEST_IMAGE"])
        self.value["limits"]["wall_seconds"] = 3

    def evaluate(self, source):
        (self.candidate / "answer.py").write_text(source)
        self.grader.write_text(json.dumps(self.value))
        return grade(self.candidate, self.grader, hashlib.sha256(self.grader.read_bytes()).hexdigest())

    def test_correct_and_incorrect_submission(self):
        self.assertEqual(self.evaluate("print(int(input())*7)\n")["outcome"], "passed")
        self.assertEqual(self.evaluate("print(0)\n")["outcome"], "failed")

    def test_candidate_cannot_read_grader_or_modify_mount(self):
        source = '''from pathlib import Path
import socket
assert not Path(%r).exists()
try:
    Path('/workspace/injected').touch()
except PermissionError:
    pass
except OSError:
    pass
else:
    raise AssertionError('candidate mount was writable')
s = socket.socket()
s.settimeout(0.2)
try:
    s.connect(('1.1.1.1', 443))
except OSError:
    pass
else:
    raise AssertionError('network was available')
print(int(input())*7)
''' % str(self.grader)
        self.assertEqual(self.evaluate(source)["outcome"], "passed")
        self.assertFalse((self.candidate / "injected").exists())

    def test_timeout_removes_container(self):
        result = self.evaluate("import time\ntime.sleep(60)\n")
        self.assertEqual(result["outcome"], "failed")
        self.assertEqual(result["cases"][0]["execution"]["stop_reason"], "wall_seconds")
        containers = subprocess.run(DOCKER + ["ps", "-aq", "--filter", "name=fr-grade-"], capture_output=True, text=True, check=True, timeout=10)
        self.assertEqual(containers.stdout.strip(), "")


if __name__ == "__main__":
    unittest.main()
