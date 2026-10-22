"""Reject incomplete sessions and false savings even after a manifest is recomputed."""
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch
import zipfile

spec = importlib.util.spec_from_file_location('sessions', Path(__file__).with_name('project-session-cost.py'))
cost = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cost)
sys.path.insert(0, str(cost.ROOT / 'sdk/python/src'))
from fr_ir.context import DirectoryObjectStore, store_merkle_value
from fr_ir.investigation import Dependency, DependencyKind, Evidence, EvidenceKind, TaskPlan, TaskStep, StepState
from fr_ir.runtime import FrRuntimeError


def fixture(root):
    tasks = json.loads(cost.TASKS.read_text())
    reports = root / 'reports'
    reports.mkdir()
    sessions = []
    for project_index, task in enumerate(tasks['projects']):
        directory = root / task['id']
        source = directory / 'source'
        source.mkdir(parents=True)
        files = {task['edit_path']: (task['name'] + '()\n').encode(), 'LICENSE': b'fixture\n'}
        with tarfile.open(source / 'source.tar.gz', 'w:gz') as archive:
            for name, data in files.items():
                info = tarfile.TarInfo('upstream/' + name)
                info.size = len(data)
                archive.addfile(info, io.BytesIO(data))
        original = {name: hashlib.sha256(data).hexdigest() for name, data in files.items()}
        changed = {**original, task['edit_path']: hashlib.sha256(files[task['edit_path']].replace(
            task['name'].encode(), (task['name'] + '_cost_renamed').encode())).hexdigest()}
        cost.save(source / 'source.json', {'repository': task['repository'], 'revision': task['revision'],
            'archive_sha256': cost.file_digest(source / 'source.tar.gz'), 'files': original})
        cost.save(directory / 'task.json', task)
        cost.save(directory / 'request.json', {'name': task['name'], 'question': tasks['question'], 'scope_limits': tasks['scope_limits']})
        for repetition in range(3):
            for condition in cost.CONDITIONS:
                base = directory / f'{repetition}-{condition}'
                base.mkdir()
                saved = condition.startswith('saved-')
                state = base / '.fixture-state'
                stages = []
                for stage in cost.STAGES:
                    output = base / stage
                    output.mkdir()
                    found = {'columns': ['handle'], 'rows': [] if stage == 'edit' else [['frp1:fixture']], 'page': {'next': None}}
                    answer = {'find': found, 'scopes': [] if stage == 'edit' else [{'scope': task['id']}]}
                    calls = []

                    def call(arguments, value):
                        calls.append({'arguments': arguments, 'seconds': .01, 'report': cost.retain(reports, value),
                            'sha256': cost.digest(value), 'canonical_report_bytes': len(cost.encode(value))})

                    retained = saved and stage == 'reuse'
                    invalidated = ['selected'] if saved and stage == 'edit' else []
                    validation = None
                    if saved and stage != 'first':
                        validation = {'invalidated': invalidated, 'complete': retained}
                        call(['project', 'investigate', '--from', 'plan.json'], validation)
                    if not retained:
                        limits = tasks['scope_limits']
                        call(['project', 'find', task['name'], '--source', '--bytes', str(limits['find_bytes']),
                            '--limit', str(limits['find_limit']), '--in', limits['find_scope']], found)
                        for scope in answer['scopes']:
                            call(['project', 'change-scope', 'frp1:fixture'], scope)
                    result = {'answer_digest': cost.digest(answer), 'answer_bytes': len(cost.encode(answer)) + 1,
                        'calls': calls, 'retained': retained, 'invalidated': invalidated, 'validation': validation, 'phases': {}}
                    cost.save(output / 'result.json', result)
                    cost.save(output / 'answer.json', {'report': cost.retain(reports, answer)})
                    stages.append({'stage': stage, 'seconds': .1, 'answer_digest': result['answer_digest'],
                        'answer_bytes': result['answer_bytes'], 'native_calls': len(calls),
                        'native_report_bytes': sum(row['canonical_report_bytes'] for row in calls),
                        'native_cache_bytes': 100, 'saved_bytes': 100 if saved else 0,
                        'source_files': changed if stage == 'edit' else original})
                    if saved:
                        store = DirectoryObjectStore(state / 'objects')
                        evidence = Evidence('answer', EvidenceKind.OBSERVATION, 'a' * 64, True, 'sha256:' + cost.digest(answer))
                        plan = TaskPlan(tasks['question'], ('current discovery',), (TaskStep('selected', task['name'], (
                            Dependency(DependencyKind.WORKSPACE, 'project'), Dependency(DependencyKind.LOOKUP, task['name']),
                            Dependency(DependencyKind.ANALYZER, 'native-implementation')),
                            state=StepState.SATISFIED, evidence=(evidence,), satisfies=('current discovery',)),))
                        receipt = {'plan': plan.store(store), 'answer': store_merkle_value(store, answer).digest}
                        cost.save(output / 'receipt.json', receipt)
                        cost.save(state / 'receipt.json', receipt)
                if saved:
                    with zipfile.ZipFile(base / 'saved-state.zip', 'w') as archive:
                        for path in state.rglob('*'):
                            if path.is_file():
                                archive.write(path, str(path.relative_to(state)))
                    cost.shutil.rmtree(state)
                launches = 0 if condition == 'cli-fresh' else 1 if condition.endswith('-running') else 3
                cost.save(base / 'session.json', {'condition': condition, 'copy_seconds': .01, 'cleanup_seconds': .01,
                    'launches': [{'stopped': True, 'startup_seconds': .01, 'shutdown_seconds': .01}] * launches,
                    'stages': stages, 'source_files': original})
                cost.save(base / 'stdout.txt', {'condition': condition, 'answers': [row['answer_digest'] for row in stages]})
                measured = {'exit_code': 0, 'process_exit_code': 0, 'timed_out': False, 'stop_reason': None,
                    'monitor_error': None, 'launch_error': None, 'elapsed_seconds': 1., 'sampled_cpu_seconds': .1,
                    'sampled_aggregate_rss_bytes': 1000, 'sampled_disk_growth_bytes': 1000, 'transcript_bytes': 100,
                    'limits': {name: tasks['limits'][name] for name in ('wall_seconds', 'cpu_seconds', 'rss_bytes', 'disk_bytes', 'transcript_bytes')}}
                cost.save(base / 'process.json', {'measurement': measured})
                sessions.append({'project': task['id'], 'repetition': repetition, 'condition': condition,
                    'position': (cost.CONDITIONS.index(condition) - project_index - repetition) % 5,
                    'path': str(base.relative_to(root)), 'measurement': measured})
    report = {'schema': 'fr-project-session-cost', 'passed': True, 'source_bindings': {}, 'scope': cost.SCOPE,
        'conditions': list(cost.CONDITIONS), 'stages': list(cost.STAGES), 'tasks': tasks, 'projects': 3,
        'repetitions': 3, 'original_unread': {'original': True}, 'all_projects_familiar': True,
        'new_unread_claim': False, 'setup': {'wheel_build_seconds': .1}, 'sessions': sessions, 'answer_agreement': True}
    report['files'] = cost.inventory(root)
    cost.save(root / 'result.json', report)


class SessionAccounting(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name) / 'raw'
        self.root.mkdir()
        fixture(self.root)
        for name, value in (('bindings', {}), ('original_reference', {'original': True})):
            mocked = patch.object(cost, name, return_value=value)
            mocked.start()
            self.addCleanup(mocked.stop)

    def update(self, name, change):
        path = self.root / name
        value = json.loads(path.read_text())
        change(value)
        cost.save(path, value)
        report = json.loads((self.root / 'result.json').read_text())
        report['files'] = {key: value for key, value in cost.inventory(self.root).items() if key != 'result.json'}
        cost.save(self.root / 'result.json', report)

    def test_complete_sessions_and_retained_archive(self):
        result = cost.audit(self.root)
        self.assertEqual(set(result), {'glob', 'boltons', 'jmespath'})
        self.assertEqual(result, cost.retain_collection(self.root, self.root.parent / 'retained'))

    def test_missing_session(self):
        self.update('result.json', lambda value: value['sessions'].pop())
        with self.assertRaisesRegex(ValueError, 'missing sessions'):
            cost.audit(self.root)

    def test_missing_shutdown(self):
        self.update('glob/0-sdk-running/session.json', lambda value: value['launches'][0].update(stopped=False))
        with self.assertRaisesRegex(ValueError, 'lifecycle'):
            cost.audit(self.root)

    def test_false_reuse(self):
        self.update('glob/0-sdk-fresh/reuse/result.json', lambda value: value.update(retained=True))
        with self.assertRaisesRegex(ValueError, 'reuse claim'):
            cost.audit(self.root)

    def test_stale_source_after_edit(self):
        self.update('glob/0-cli-fresh/session.json', lambda value: value['stages'][2].update(source_files=value['source_files']))
        with self.assertRaisesRegex(ValueError, 'source state'):
            cost.audit(self.root)

    def test_missing_native_validation(self):
        self.update('glob/0-saved-running/reuse/result.json', lambda value: value.update(validation=None))
        with self.assertRaisesRegex(ValueError, 'native validation'):
            cost.audit(self.root)

    def test_stored_answer_does_not_match(self):
        self.update('glob/0-saved-running/edit/receipt.json', lambda value: value.update(answer='0' * 64))
        with self.assertRaises(FrRuntimeError):
            cost.audit(self.root)


if __name__ == '__main__':
    unittest.main()
