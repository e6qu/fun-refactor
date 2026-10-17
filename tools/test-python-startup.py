"""Challenge startup observations with import-time changes and altered records."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import types
import unittest


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


worker = load('startup_worker', 'python-startup-worker.py')
study = load('startup_study', 'python-startup-investigation.py')


class StartupChecks(unittest.TestCase):
    def execute(self, root, *, traced=True):
        command = [sys.executable, '-B', str(Path(worker.__file__)), '--root', str(root),
                   '--module', 'example.helpers', '--function', 'relay', '--arguments', '[[-4],[0],[17]]']
        if traced:
            command.append('--traced')
        process = subprocess.run(command, capture_output=True, text=True, timeout=10)
        self.assertEqual(process.returncode, 0, process.stderr)
        return json.loads(process.stdout)

    def fixture(self, root, startup='from .helpers import relay\n', extra=''):
        package = root / 'example'
        package.mkdir()
        (package / '__init__.py').write_text(startup)
        (package / 'helpers.py').write_text('def relay(value):\n    return value\n\ndef unused():\n    raise RuntimeError()\n' + extra)

    def helper(self):
        return {'module': 'example.helpers', 'function': 'relay', 'path': 'example/helpers.py',
                'arguments': [[-4], [0], [17]], 'expected': [-4, 0, 17]}

    def test_import_and_call_lines_stay_separate_and_unused_bodies_are_unobserved(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            self.fixture(root)
            observed = self.execute(root)
            study.checked_observation(observed, self.helper(), True, root)
            files = {(row['phase'], row['path']): row for row in observed['execution']}
            self.assertEqual(files['import', 'example/helpers.py']['module_lines'], [1, 4])
            self.assertEqual(files['call', 'example/helpers.py']['calls'], {'relay': 3})
            self.assertEqual(files['call', 'example/helpers.py']['lines'], [2])
            self.assertNotIn(5, [line for row in observed['execution'] for line in row['lines']])

    def test_package_startup_can_replace_a_helper_before_from_import_finishes(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            self.fixture(root, 'from . import helpers\nfrom . import setup\n',
                         '\ndef stopped(value):\n    return 0\n')
            (root / 'example/setup.py').write_text('from . import helpers\nhelpers.relay = helpers.stopped\n')
            plain, traced = self.execute(root, traced=False), self.execute(root)
            self.assertEqual(plain['returned'], [0, 0, 0])
            self.assertEqual(plain['returned'], traced['returned'])
            self.assertEqual(plain['modules'], traced['modules'])
            self.assertIn('example/setup.py', traced['files'])
            calls = [row['calls'] for row in traced['execution'] if row['phase'] == 'call']
            self.assertEqual(calls, [{'stopped': 3}])

    def test_imports_triggered_by_the_call_do_not_become_import_time_modules(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            package = root / 'example'
            package.mkdir()
            (package / '__init__.py').write_text('')
            (package / 'helpers.py').write_text('def relay(value):\n    from . import later\n    return value\n')
            (package / 'later.py').write_text('VALUE = 1\n')
            observed = self.execute(root)
            self.assertNotIn('example.later', observed['modules'])
            self.assertIn('example/later.py', observed['files'])
            self.assertEqual([row['phase'] for row in observed['execution'] if row['path'] == 'example/later.py'], ['call'])

    def test_tracing_limit_fails_instead_of_returning_a_shortened_success(self):
        recorder = worker.Recorder(Path('/tmp'), event_limit=1)
        frame = types.SimpleNamespace(f_code=types.SimpleNamespace(co_filename='/tmp/example.py', co_name='<module>'), f_lineno=1)
        recorder.trace(frame, 'line', None)
        with self.assertRaisesRegex(RuntimeError, 'event limit'):
            recorder.trace(frame, 'line', None)

    def test_untraced_baseline_does_not_claim_observed_lines(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            self.fixture(root)
            plain = self.execute(root, traced=False)
            study.checked_observation(plain, self.helper(), False, root)
            self.assertEqual(plain['events'], 0)
            self.assertEqual(plain['execution'], [])

    def test_altered_outputs_source_hashes_and_trace_records_are_rejected(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            self.fixture(root)
            original = self.execute(root)
            def change_output(value):
                value['returned'][0] = 123
            def change_path(value):
                value['files']['../escape.py'] = {'bytes': 0, 'sha256': '0' * 64}
            def change_hash(value):
                value['files']['example/helpers.py']['sha256'] = '0' * 64
            def omit_trace(value):
                value['execution'] = []
            def duplicate_file(value):
                value['execution'].append(value['execution'][0])
            def invent_line(value):
                value['execution'][0]['lines'] = [999]
            for mutation in (change_output, change_path, change_hash, omit_trace, duplicate_file, invent_line):
                with self.subTest(mutation=mutation.__name__), self.assertRaises(ValueError):
                    value = copy.deepcopy(original)
                    mutation(value)
                    study.checked_observation(value, self.helper(), True, root)
            (root / 'example/helpers.py').write_text('def relay(value):\n    return 0\n')
            with self.assertRaisesRegex(ValueError, 'source identity'):
                study.checked_observation(original, self.helper(), True, root)

    def test_a_resource_stop_cannot_pass_with_valid_looking_output(self):
        for reason in ('wall_seconds', 'rss_bytes', 'cpu_seconds', 'disk_bytes', 'transcript_bytes', 'monitor_error'):
            with self.subTest(reason=reason), self.assertRaisesRegex(ValueError, 'stopped'):
                study.checked_process({'stop_reason': reason, 'exit_code': 0}, '{}')

    def test_missing_process_fields_and_empty_or_malformed_output_cannot_pass(self):
        with self.assertRaises(KeyError):
            study.checked_process({'stop_reason': None, 'exit_code': 0}, '{}')
        good = {'stop_reason': None, 'exit_code': 0, 'process_exit_code': 0, 'monitor_error': None,
                'launch_error': None, 'timed_out': False, 'limits': dict(study.LIMITS)}
        good['limits']['cpu_seconds'] = good['limits'].pop('cpu_limit_seconds')
        for output in ('', 'not json'):
            with self.subTest(output=output), self.assertRaises(ValueError):
                study.checked_process(good, output)
        good['limits']['wall_seconds'] += 1
        with self.assertRaisesRegex(ValueError, 'limits changed'):
            study.checked_process(good, '{}')

    def test_unobserved_startup_and_unread_source_stay_unfinished(self):
        report = {'complete': False, 'cutoffs': ['module-effects-unchecked'],
                  'inputs': {'modules': {'files': {'example/__init__.py': 'digest'}}},
                  'initialization': {'by_file': {'example/__init__.py': {'statements': 3}}, 'omitted': 2,
                     'sites': [{'occurrence': {'path': 'example/__init__.py', 'location': {
                         'range': {'start': {'line': 20}, 'end': {'line': 20}}}}, 'reasons': ['startup-call']}]}}
        observation = {'files': {'example/helpers.py': {'bytes': 17}}, 'execution': []}
        compared = study.comparison(observation, report)
        self.assertFalse(compared['analysis_complete'])
        self.assertEqual(compared['unchecked_statements'], 3)
        self.assertEqual(compared['unshown_unchecked_sites'], 2)
        self.assertEqual(compared['observed_but_unread'], ['example/helpers.py'])
        self.assertFalse(compared['shown_unchecked_sites'][0]['module_line_observed'])

    def test_schedule_contains_each_predeclared_cell_once(self):
        cells = study.schedule()
        self.assertEqual(len(cells), 16)
        self.assertEqual(len(cells), len(set(cells)))
        self.assertEqual(cells, study.schedule())


if __name__ == '__main__':
    unittest.main()
