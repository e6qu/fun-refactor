"""Challenge startup observations with import-time changes and altered records."""
import copy
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import types
import unittest
from unittest.mock import patch


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
            def change_zero_to_false(value):
                value['returned'][1] = False
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
            for mutation in (change_output, change_zero_to_false, change_path, change_hash,
                             omit_trace, duplicate_file, invent_line):
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
                'launch_error': None, 'timed_out': False, 'limits': dict(study.LIMITS),
                'elapsed_seconds': 0.1, 'sampled_aggregate_rss_bytes': 123,
                'sampled_disk_growth_bytes': 0, 'sampled_cpu_seconds': 0, 'transcript_bytes': 2}
        good['limits']['cpu_seconds'] = good['limits'].pop('cpu_limit_seconds')
        for output in ('', 'not json'):
            with self.subTest(output=output), self.assertRaises(ValueError):
                study.checked_process(good, output)
        good['limits']['wall_seconds'] += 1
        with self.assertRaisesRegex(ValueError, 'limits changed'):
            study.checked_process(good, '{}')

    def test_invalid_or_excessive_resource_measurements_cannot_pass(self):
        value = {'stop_reason': None, 'exit_code': 0, 'process_exit_code': 0, 'monitor_error': None,
                 'launch_error': None, 'timed_out': False,
                 'limits': {'wall_seconds': 20, 'rss_bytes': 512 * 1024**2, 'disk_bytes': 16 * 1024**2,
                            'cpu_seconds': 10, 'transcript_bytes': 1024**2},
                 'elapsed_seconds': 0.1, 'sampled_aggregate_rss_bytes': 1,
                 'sampled_disk_growth_bytes': 0, 'sampled_cpu_seconds': 0, 'transcript_bytes': 2}
        for field in ('elapsed_seconds', 'sampled_aggregate_rss_bytes', 'sampled_disk_growth_bytes',
                      'sampled_cpu_seconds', 'transcript_bytes'):
            for number in (-1, float('nan'), float('inf'), True, 10**12):
                with self.subTest(field=field, number=number), self.assertRaisesRegex(ValueError, 'resource limits'):
                    altered = copy.deepcopy(value)
                    altered[field] = number
                    study.checked_process(altered, '{}')

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

    def test_the_complete_reader_example_changes_the_result_after_the_startup_edit(self):
        text = (Path(__file__).resolve().parents[1] / 'docs/python-package-startup.md').read_text()
        blocks = [part.split('```', 1)[0] for part in text.split('```python\n')[1:]]
        self.assertEqual(len(blocks), 3)
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            (root / 'example').mkdir()
            for filename, source in zip(('helpers.py', 'setup.py', '__init__.py'), blocks, strict=True):
                (root / 'example' / filename).write_text(source)
            self.assertEqual(self.execute(root, traced=False)['returned'], [0, 0, 0])
            self.assertEqual(self.execute(root)['returned'], [0, 0, 0])
            path = root / 'example/__init__.py'
            path.write_text(path.read_text().replace('from . import setup\n', ''))
            self.assertEqual(self.execute(root, traced=False)['returned'], [-4, 0, 17])
            self.assertEqual(self.execute(root)['returned'], [-4, 0, 17])

    def test_saved_collection_audit_rejects_changed_raw_data_claims_inputs_and_missing_cells(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            self.fixture(root)
            helper = dict(self.helper(), id='example', project='example')
            plan = {'helpers': [helper], 'repetitions': 2, 'seed': 1729}
            report = {'complete': False, 'cutoffs': ['module-effects-unchecked'],
                      'inputs': {'modules': {'files': {}}},
                      'initialization': {'sites': [], 'by_file': {}, 'omitted': 0}}
            observations = {traced: self.execute(root, traced=traced) for traced in (False, True)}
            measurement = {'stop_reason': None, 'exit_code': 0, 'process_exit_code': 0,
                           'monitor_error': None, 'launch_error': None, 'timed_out': False,
                           'limits': {'wall_seconds': 20, 'rss_bytes': 512 * 1024**2,
                                      'disk_bytes': 16 * 1024**2, 'cpu_seconds': 10, 'transcript_bytes': 1024**2},
                           'elapsed_seconds': 0.1, 'sampled_aggregate_rss_bytes': 1,
                           'sampled_disk_growth_bytes': 0, 'sampled_cpu_seconds': 0, 'transcript_bytes': 2}
            with patch.object(study, 'PLAN', plan), patch.object(study, 'bindings', return_value={}), \
                 patch.object(study, 'prepare', return_value=({'example': root}, {'example': report})):
                result = {'source_commit': '1' * 40, 'source_files': {}, 'plan': plan, 'python': sys.version,
                          'platform': 'control', 'cpu_model': 'control', 'logical_cpus': 1,
                          'limits': study.LIMITS, 'measurement_scope': study.MEASUREMENT_SCOPE,
                          'attempts': [], 'finished': True}
                for index, (helper_id, traced, repetition) in enumerate(study.schedule()):
                    stdout = root / f'attempt-{index:02d}.stdout'
                    stderr = root / f'attempt-{index:02d}.stderr'
                    stdout.write_text(json.dumps(observations[traced]))
                    stderr.write_text('')
                    measured = dict(measurement, transcript_bytes=stdout.stat().st_size)
                    result['attempts'].append({'helper': helper_id, 'traced': traced, 'repetition': repetition,
                        'measurement': measured, 'comparison': study.comparison(observations[traced], report),
                        'error': None, 'stdout_sha256': study.digest(stdout), 'stderr_sha256': study.digest(stderr)})
                study.save(root / 'result.json', result)
                study.audit(root)
                def remove_cell(value):
                    value['attempts'].pop()
                def duplicate_cell(value):
                    value['attempts'][1] = value['attempts'][0]
                def change_claim(value):
                    value['attempts'][0]['comparison']['analysis_complete'] = True
                def mark_unfinished(value):
                    value['finished'] = False
                def change_inputs(value):
                    value['source_files']['invented.py'] = '0' * 64
                def hide_error(value):
                    value['attempts'][0]['error'] = 'source collection failed'
                def remove_current_field(value):
                    del value['attempts'][0]['stdout_sha256']
                for mutation in (remove_cell, duplicate_cell, change_claim, mark_unfinished,
                                 change_inputs, hide_error, remove_current_field):
                    with self.subTest(mutation=mutation.__name__), self.assertRaises(ValueError):
                        altered = copy.deepcopy(result)
                        mutation(altered)
                        study.save(root / 'result.json', altered)
                        study.audit(root)
                study.save(root / 'result.json', result)
                path = root / 'attempt-00.stdout'
                original = path.read_text()
                path.write_text(original + '\n')
                with self.assertRaisesRegex(ValueError, 'raw observation'):
                    study.audit(root)
                path.write_text(original)
                (root / 'attempt-00.stderr').unlink()
                with self.assertRaises(FileNotFoundError):
                    study.audit(root)

    @unittest.skipUnless(sys.platform == 'linux', 'hosted Linux collection control')
    def test_first_failed_process_is_saved_and_remaining_processes_do_not_consume_the_job_deadline(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            scopes = {project: root for project in study.PLAN['archives']}
            with patch.dict(os.environ, {'GITHUB_ACTIONS': 'true'}), \
                 patch.object(study, 'prepare', return_value=(scopes, {})), \
                 patch.object(study, 'bindings', return_value={}), \
                 patch.object(study, 'run', return_value={'stop_reason': 'wall_seconds', 'exit_code': 124}) as run:
                with self.assertRaisesRegex(ValueError, 'remaining scheduled processes were not started'):
                    study.collect(root / 'result')
                self.assertEqual(run.call_count, 1)
            result = json.loads((root / 'result/result.json').read_bytes())
            self.assertFalse(result['finished'])
            self.assertEqual(len(result['attempts']), 1)
            self.assertIn('stopped or failed', result['attempts'][0]['error'])
            self.assertTrue((root / 'result/attempt-00.stderr').is_file())


if __name__ == '__main__':
    unittest.main()
