"""Keep concurrent collection and interface failures separate from a passing audit."""
import importlib.util
from pathlib import Path
from threading import Event
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('runner', Path(__file__).with_name('python-contract-runner.py'))
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


class CollectionChecks(unittest.TestCase):
    def test_readonly_work_overlaps_and_both_finish_before_the_audit(self):
        sdk_started, collecting, sdk_done = Event(), Event(), Event()
        def sdk():
            sdk_started.set()
            self.assertTrue(collecting.wait(5))
            sdk_done.set()
        def collect():
            self.assertTrue(sdk_started.wait(5))
            collecting.set()
        def audit():
            self.assertTrue(sdk_done.is_set())
        runner.check(sdk, collect, audit)

    def test_interface_failure_cannot_become_a_successful_report_audit(self):
        def sdk():
            raise RuntimeError('interface failed')
        with self.assertRaises(ExceptionGroup) as caught:
            runner.check(sdk, lambda: None, lambda: self.fail('audit ran after SDK failure'))
        self.assertEqual([str(error) for error in caught.exception.exceptions], ['interface failed'])

    def test_collection_failure_still_joins_the_interface_worker(self):
        release, sdk_done = Event(), Event()
        def sdk():
            self.assertTrue(release.wait(5))
            sdk_done.set()
        def collect():
            release.set()
            raise ValueError('collection failed')
        with self.assertRaises(ExceptionGroup):
            runner.check(sdk, collect, lambda: self.fail('audit ran after collection failure'))
        self.assertTrue(sdk_done.is_set())

    def test_both_failures_remain_available_for_diagnosis(self):
        def sdk():
            raise RuntimeError('interface failed')
        def collect():
            raise ValueError('collection failed')
        with self.assertRaises(ExceptionGroup) as caught:
            runner.check(sdk, collect, lambda: self.fail('audit ran after failures'))
        self.assertEqual({str(error) for error in caught.exception.exceptions},
                         {'interface failed', 'collection failed'})

    def test_a_failed_audit_still_fails_the_job(self):
        def audit():
            raise ValueError('altered report accepted')
        with self.assertRaisesRegex(ValueError, 'altered report accepted'):
            runner.check(lambda: None, lambda: None, audit)


class InterfaceShards(unittest.TestCase):
    def exercise(self, failures):
        started = (Event(), Event())
        commands = []
        environment = {'RAYON_NUM_THREADS': '1'}

        def execute(command, **options):
            slot = 0 if f'sdk/python/tests/test_{runner.SDK_TESTS[0]}.py' in command else 1
            commands.append(command[5:])
            self.assertEqual(options['timeout'], 600)
            self.assertTrue(options['check'])
            self.assertEqual(options['env'], environment)
            started[slot].set()
            self.assertTrue(started[1 - slot].wait(5), 'SDK shards did not overlap')
            options['stdout'].write(f'finished shard {slot}\n')
            if slot in failures:
                raise subprocess.CalledProcessError(1, command)

        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / 'sdk.log'
            with patch.object(runner.subprocess, 'run', side_effect=execute), patch('builtins.print'):
                if failures:
                    with self.assertRaises(ExceptionGroup) as caught:
                        runner.sdk_checks(environment, log)
                    self.assertEqual(len(caught.exception.exceptions), len(failures))
                    self.assertTrue(all(isinstance(error, subprocess.CalledProcessError)
                                        for error in caught.exception.exceptions))
                else:
                    runner.sdk_checks(environment, log)
            text = log.read_text()
            self.assertIn('finished shard 0', text)
            self.assertIn('finished shard 1', text)
        selected = [name for command in commands for name in command]
        self.assertCountEqual(selected, [f'sdk/python/tests/test_{name}.py' for name in runner.SDK_TESTS])
        self.assertEqual(len(selected), len(set(selected)))

    def test_every_interface_suite_runs_once_with_both_logs_retained(self):
        self.exercise(set())

    def test_either_or_both_shard_failures_reject_the_interface_check(self):
        for failures in ({0}, {1}, {0, 1}):
            with self.subTest(failures=failures):
                self.exercise(failures)


if __name__ == '__main__':
    unittest.main()
