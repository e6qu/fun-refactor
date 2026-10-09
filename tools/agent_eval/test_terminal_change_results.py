"""Reject misbound grades and retain failures and unknown costs in comparisons."""
import base64
import copy
import hashlib
import gzip
from pathlib import Path
import runpy
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval import native_checks, terminal_change_results as results, terminal_change_runner as runner
from agent_eval.study import encode, load
from agent_eval.test_terminal_changes import frozen, capture
from agent_eval.workspace_bundle import pack


def fake_grade(candidate, spec_path, sha):
    spec = load(spec_path)
    cases = []
    for case in spec["cases"]:
        output = case["stdout"].encode()
        cases.append({"id": case["id"], "passed": True, "failure": None,
                      "execution": {"stop_reason": None},
                      "container_state": {"Running": False, "OOMKilled": False, "ExitCode": case["exit_code"]},
                      **{channel + key: value for channel, raw in (("stdout", output), ("stderr", b""))
                         for key, value in (("_sha256", hashlib.sha256(raw).hexdigest()), ("_bytes", len(raw)),
                                            ("_base64", base64.b64encode(raw).decode()))}})
    return {"schema": "fr-isolated-grade-1", "candidate": native_checks.candidate_identity(pack(candidate)),
            "grader_sha256": sha, "image": spec["image"], "outcome": "passed", "cases": cases}


class Results(unittest.TestCase):
    def test_retained_runtime_reports_without_checkout_on_import_path(self):
        snapshot = runpy.run_path(str(Path(__file__).parents[1] / 'terminal-change-snapshot.py'))
        inputs = self.root / 'inputs'
        inputs.mkdir()
        (inputs / 'plan.json').write_bytes(encode(self.plan))
        (inputs / 'inputs.json.gz').write_bytes(gzip.compress(encode(self.snapshots), mtime=0))
        retained = self.root / 'runner'
        snapshot['retain'](self.plan, retained)
        result = subprocess.run([sys.executable, '-I', '-B', '-c',
            'import runpy,sys; sys.path.insert(0,sys.argv.pop(1)); sys.argv.pop(0); runpy.run_path(sys.argv[0],run_name="__main__")',
            str(retained), str(retained / 'terminal-change-results.py'), 'report', str(inputs),
            str(self.root / 'unstarted'), '--output', str(self.root / 'isolated.json')],
            cwd=self.root, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(load(self.root / 'isolated.json')['attempts'][0]['outcome'], 'not_started')
        snapshot['verify'](self.plan, retained)
        (retained / 'agent_eval/terminal_changes.py').write_text('changed')
        with self.assertRaisesRegex(ValueError, 'bytes differ'):
            snapshot['verify'](self.plan, retained)

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.plan, self.snapshots = frozen()
        self.cell = self.plan["plan"]["cells"][0]
        folder = self.root / self.cell["id"]
        folder.mkdir()
        process = capture(folder, self.plan, self.snapshots)
        runner.seal(self.plan, self.snapshots, self.cell, folder, process)

    def grade(self):
        with patch.object(native_checks, "require_runner"):
            return results.grade(self.plan, self.snapshots, self.root, execute=fake_grade)

    def test_completed_is_pending_until_identity_checked_grading(self):
        before = results.report(self.plan, self.snapshots, self.root)
        self.assertEqual([r["outcome"] for r in before["attempts"]], ["pending_grading", "not_started"])
        after = results.report(self.plan, self.snapshots, self.root, self.grade())
        self.assertEqual([r["outcome"] for r in after["attempts"]], ["passed", "not_started"])
        self.assertIsNone(after["pairs"][0]["fr_minus_files_result_bytes"])
        self.assertIsNone(after["attempts"][1]["observed"])
        self.assertFalse(after["efficiency_advantage"])
        self.assertIn("unknown", results.markdown(after))

    def test_grading_refuses_local_candidate_execution(self):
        with patch.dict("os.environ", {"GITHUB_ACTIONS": "false"}):
            with self.assertRaisesRegex(ValueError, "GitHub runner"):
                results.grade(self.plan, self.snapshots, self.root, execute=fake_grade)

    def test_forged_submission_case_verdict_and_unstarted_grade_are_rejected(self):
        valid = self.grade()
        mutations = [
            lambda g: g["outcomes"][0].update(submission_sha256="0" * 64),
            lambda g: g["outcomes"][0]["grade"]["candidate"].update(bytes=0),
            lambda g: g["outcomes"][0]["grade"]["cases"][0].update(passed=False),
            lambda g: g["outcomes"][0]["grade"]["cases"].clear(),
            lambda g: g["outcomes"][1].update(outcome="passed"),
            lambda g: g.update(passed=2),
            lambda g: g["outcomes"].reverse()]
        for mutate in mutations:
            changed = copy.deepcopy(valid)
            mutate(changed)
            with self.subTest(mutate=mutate), self.assertRaises(ValueError):
                results.report(self.plan, self.snapshots, self.root, changed)

    def test_failed_capture_keeps_observed_work_and_never_enters_grader(self):
        folder = self.root / self.cell["id"]
        process = load(folder / "process.json")
        process.update(stop_reason="cpu_limit", exit_code=1)
        (folder / "submission.json").unlink()
        (folder / "manifest.json").unlink()
        (folder / "record.json").unlink()
        runner.seal(self.plan, self.snapshots, self.cell, folder, process)
        with patch.object(native_checks, "require_runner"), patch.object(results.isolated_grade, "grade"):
            def forbidden(*_):
                raise AssertionError("failed capture reached grader")
            grades = results.grade(self.plan, self.snapshots, self.root, execute=forbidden)
        report = results.report(self.plan, self.snapshots, self.root, grades)
        self.assertEqual(report["attempts"][0]["outcome"], "failed")
        self.assertGreater(report["totals_including_failures"]["host_calls"], 0)
        self.assertIsNone(report["actual_usd"])


if __name__ == "__main__":
    unittest.main()
