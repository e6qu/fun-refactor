"""Reject missing costs, stale reuse and incomplete saved-work comparisons."""
import copy
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest
import zipfile
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('saved_cost', Path(__file__).with_name('saved-work-cost.py'))
cost = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cost)
from fr_ir.context import DirectoryObjectStore, store_merkle_value
from fr_ir.investigation import Dependency, DependencyKind, Evidence, EvidenceKind, TaskPlan, TaskStep, StepState


def process(limits):
    return {'exit_code': 0, 'process_exit_code': 0, 'stop_reason': None, 'timed_out': False,
        'monitor_error': None, 'launch_error': None, 'elapsed_seconds': 1.0,
        'sampled_aggregate_rss_bytes': 67108864, 'sampled_disk_growth_bytes': 100,
        'limits': {key: limits[key] for key in ('wall_seconds', 'rss_bytes', 'disk_bytes', 'cpu_seconds', 'transcript_bytes')}}


def fixture(directory, tasks):
    task = tasks['projects'][0]
    sources = {task['edit_path']: ('pub fn ' + task['name'] + '() {}\n').encode(), 'LICENSE': b'example\n'}
    source = directory / 'source'
    source.mkdir()
    with tarfile.open(source / 'source.tar.gz', 'w:gz') as archive:
        for name, data in sources.items():
            info = tarfile.TarInfo('upstream/' + name)
            info.size = len(data)
            archive.addfile(info, io.BytesIO(data))
    original = {name: hashlib.sha256(data).hexdigest() for name, data in sources.items()}
    changed = {**original, task['edit_path']: hashlib.sha256(sources[task['edit_path']].replace(
        task['name'].encode(), (task['name'] + '_cost_renamed').encode())).hexdigest()}
    cost.save(source / 'source.json', {'repository': task['repository'], 'revision': task['revision'],
        'files': original, 'archive_sha256': cost.file_digest(source / 'source.tar.gz')})
    request = {'name': task['name'], 'question': tasks['question'], 'scope_limits': tasks['scope_limits']}
    cost.save(directory / 'request.json', request)
    result = {'project': task, 'source_files': original, 'changed_files': changed,
        'source_preparation_seconds': .3, 'source_preparation_cpu_seconds': .02,
        'archive_bytes': (source / 'source.tar.gz').stat().st_size,
        'source_bytes': sum(map(len, sources.values())), 'sessions': []}
    for repetition in range(3):
        for condition in tasks['conditions']:
            session = directory / f'{repetition}-{condition}'
            state = session / 'saved-state'
            previous_storage = {'native_state': 0, 'saved_state': 0}
            stages = []
            cost.save(session / 'edit.json', {'measurement': process(tasks['limits']), 'cpu_seconds': .02, 'matches': 1})
            for stage in tasks['stages']:
                destination = session / stage
                found = {'columns': ['path', 'handle'], 'rows': [] if stage == 'edit' else [[task['edit_path'], 'frp1:example']],
                    'page': {'next': None}}
                scope = {'schema': 'fr-change-scope-1', 'runtime_coverage': False,
                    'dependency': {'kind': 'change-scope', 'key': 'example', 'digest': 'a' * 64},
                    'input_digest': 'a' * 64, 'revision': 'b' * 64, 'indexed_complete': False, 'review_ready': False}
                scope.update({name: [] for name in ('targets', 'consumers', 'references', 'unresolved', 'textual_mentions',
                    'test_candidates', 'check_candidates', 'affected_paths', 'unmapped_paths', 'missing_mapped_paths', 'cutoffs')})
                scope['cutoffs'] = ['fixture limit']
                answer = {'find': found, 'scopes': [] if stage == 'edit' else [scope]}
                retained = condition == 'saved' and stage == 'reuse'
                invalidated = ['selected'] if condition == 'saved' and stage == 'edit' else []
                calls = []

                def call(arguments, report):
                    path = f'calls/{len(calls)}.json'
                    cost.save(destination / path, report)
                    calls.append({'arguments': arguments, 'report': path, 'seconds': .05,
                        'sha256': cost.digest(report), 'canonical_report_bytes': len(cost.encode(report))})

                validation = None
                if condition == 'saved' and stage != 'first':
                    validation = {'invalidated': invalidated, 'complete': retained}
                    call(['project', 'investigate', '--from', 'plan.json'], validation)
                if not retained:
                    call(['project', 'find', task['name'], '--source', '--bytes', str(tasks['scope_limits']['find_bytes']),
                        '--limit', str(tasks['scope_limits']['find_limit']), '--in', tasks['scope_limits']['find_scope']], found)
                    if stage != 'edit':
                        limits = tasks['scope_limits']
                        call(['project', 'change-scope', 'frp1:example', '--depth', str(limits['depth']), '--nodes',
                            str(limits['nodes']), '--references', str(limits['references']), '--bytes', str(limits['max_bytes'])], scope)
                storage = {'native_state': 100, 'saved_state': 0}
                if condition == 'saved':
                    store = DirectoryObjectStore(state / 'objects')
                    evidence = Evidence('answer', EvidenceKind.OBSERVATION, 'c' * 64, True, 'sha256:' + cost.digest(answer))
                    plan = TaskPlan(tasks['question'], ('current discovery',), (TaskStep('selected', task['name'], (
                        Dependency(DependencyKind.WORKSPACE, 'project'), Dependency(DependencyKind.LOOKUP, task['name']),
                        Dependency(DependencyKind.ANALYZER, 'native-implementation')), state=StepState.SATISFIED,
                        evidence=(evidence,), satisfies=('current discovery',)),))
                    receipt = {'plan': plan.store(store), 'answer': store_merkle_value(store, answer).digest}
                    cost.save(state / 'receipt.json', receipt)
                    cost.save(destination / 'receipt.json', receipt)
                    storage['saved_state'] = sum(path.stat().st_size for path in state.rglob('*') if path.is_file())
                raw = {'condition': condition, 'stage': stage, 'answer': answer, 'answer_digest': cost.digest(answer),
                    'answer_bytes': len(cost.encode(answer)) + 1, 'calls': calls, 'retained': retained,
                    'invalidated': invalidated, 'validation': validation,
                    'phases': {} if retained else {'discovery_seconds': .1}, 'source_files': changed if stage == 'edit' else original,
                    'storage_before': previous_storage, 'storage_after': storage, 'worker_seconds': .5, 'worker_cpu_seconds': .01}
                cost.save(destination / 'result.json', raw)
                (destination / 'stdout.txt').write_bytes(cost.encode(answer) + b'\n')
                (destination / 'cpu.txt').write_text('0.01 0.01\n')
                cost.save(destination / 'process.json', {'argv': ['python', 'worker'], 'measurement': process(tasks['limits'])})
                stages.append({'stage': stage, 'path': f'{repetition}-{condition}/{stage}', 'seconds': 1.0,
                    'cpu_seconds': .02, 'worker_cpu_seconds': .01, 'sampled_aggregate_rss_bytes': 67108864,
                    'sampled_disk_growth_bytes': 100, 'answer_digest': raw['answer_digest'], 'answer_bytes': raw['answer_bytes'],
                    'program_bytes': cost.WORKER.stat().st_size, 'invocation_bytes': len(cost.encode(['python', 'worker'])),
                    'request_bytes': len(cost.encode(request)), 'native_calls': len(calls),
                    'native_report_bytes': sum(item['canonical_report_bytes'] for item in calls),
                    'native_seconds': .05 * len(calls), 'storage_before': previous_storage, 'storage_after': storage})
                previous_storage = storage
            if condition == 'saved':
                cost.pack_state(state, session / 'saved-state.zip')
            result['sessions'].append({'repetition': repetition, 'condition': condition,
                'position': (tasks['conditions'].index(condition) - repetition) % 2,
                'copy_seconds': .5, 'copy_cpu_seconds': .01, 'copy_bytes': result['source_bytes'],
                'cleanup_seconds': .2, 'cleanup_cpu_seconds': .01, 'stages': stages, 'total_seconds': 4.7})
    return result


class Accounting(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name)
        self.tasks = json.loads(cost.TASKS.read_text())
        self.report = fixture(self.directory, self.tasks)

    def audit(self):
        cost.audit_project(self.report, self.directory, self.tasks)

    def test_complete_accounting_preserves_unfinished_scopes(self):
        self.audit()

    def test_missing_or_duplicate_repetitions_fail(self):
        self.report['sessions'][1] = copy.deepcopy(self.report['sessions'][0])
        with self.assertRaisesRegex(ValueError, 'duplicate'):
            self.audit()

    def test_missing_stage_fails(self):
        self.report['sessions'][0]['stages'].pop()
        with self.assertRaisesRegex(ValueError, 'stages'):
            self.audit()

    def test_session_cannot_omit_copy_and_edit(self):
        self.report['sessions'][0]['total_seconds'] = 3.0
        with self.assertRaisesRegex(ValueError, 'total omits'):
            self.audit()

    def test_complete_program_request_is_counted(self):
        self.report['sessions'][0]['stages'][0]['program_bytes'] = 0
        with self.assertRaisesRegex(ValueError, 'overhead'):
            self.audit()

    def test_process_memory_is_aggregate(self):
        self.report['sessions'][0]['stages'][0]['sampled_aggregate_rss_bytes'] = 0
        with self.assertRaisesRegex(ValueError, 'resource'):
            self.audit()

    def test_native_calls_cannot_disappear(self):
        self.report['sessions'][0]['stages'][0]['native_calls'] = 0
        with self.assertRaisesRegex(ValueError, 'native work'):
            self.audit()

    def test_failed_measurement_cannot_pass(self):
        value = process(self.tasks['limits'])
        value['timed_out'] = True
        with self.assertRaisesRegex(ValueError, 'failed measurement'):
            cost.check_measurement(value, self.tasks['limits'])

    def test_limits_cannot_be_extended(self):
        value = process(self.tasks['limits'])
        value['limits']['wall_seconds'] += 1
        with self.assertRaisesRegex(ValueError, 'limits changed'):
            cost.check_measurement(value, self.tasks['limits'])

    def test_explicit_cache_and_worker_settings_reach_the_process(self):
        output, workspace = self.directory / 'measurement', self.directory / 'workspace'

        def run(*args, **kwargs):
            self.assertEqual(kwargs['env']['FUN_REFACTOR_CACHE'], str(workspace / 'project/.fr/cost-cache'))
            self.assertEqual(kwargs['env']['RAYON_NUM_THREADS'], '1')
            self.assertEqual(kwargs['env']['PYTHONHASHSEED'], '0')
            (output / 'cpu.txt').write_text('0.01 0.01\n')
            return process(self.tasks['limits'])

        with patch.object(cost, 'bounded_run', side_effect=run):
            cost.measured(['example'], output, workspace, self.tasks['limits'])

    def test_edit_history_is_state_and_other_hidden_files_are_source(self):
        spec = importlib.util.spec_from_file_location('saved_worker', cost.WORKER)
        worker = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(worker)
        for name, data in {'.fr/cache': b'facts', '.fr-history/state.json': b'undo',
                '.github/workflow.yml': b'source', 'app.py': b'function'}.items():
            path = self.directory / 'project' / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
        self.assertEqual(set(worker.files(self.directory / 'project')), {'.github/workflow.yml', 'app.py'})
        self.assertEqual(worker.native_size(self.directory / 'project'), 9)

    def test_stale_answer_cannot_be_reused(self):
        path = self.directory / '0-saved/edit/result.json'
        value = json.loads(path.read_text())
        value['retained'] = True
        cost.save(path, value)
        with self.assertRaisesRegex(ValueError, 'reuse claim'):
            self.audit()

    def test_actual_stdout_is_counted(self):
        (self.directory / '0-saved/first/stdout.txt').write_text('{}\n')
        with self.assertRaisesRegex(ValueError, 'answer differs'):
            self.audit()

    def test_saved_evidence_cannot_lose_its_analyzer_dependency(self):
        base = self.directory / '0-saved'
        store = DirectoryObjectStore(base / 'saved-state/objects')
        receipt = json.loads((base / 'first/receipt.json').read_text())
        plan = TaskPlan.restore(store, receipt['plan'])
        value = plan.to_data()
        value['steps'][0]['inputs'].pop()
        receipt['plan'] = TaskPlan.from_data(value).store(store)
        cost.save(base / 'first/receipt.json', receipt)
        cost.pack_state(base / 'saved-state', base / 'saved-state.zip')
        with self.assertRaisesRegex(ValueError, 'dependencies'):
            self.audit()

    def test_prior_reserved_attempt_blocks_new_first_claim(self):
        environment = {'GITHUB_RUN_ID': '20', 'GITHUB_RUN_ATTEMPT': '1'}
        runs = {'workflow_runs': [{'id': 10, 'head_sha': 'x', 'conclusion': 'failure', 'html_url': 'url'}]}
        jobs = {'jobs': [{'name': 'Reserved project', 'steps': [{'name':
            'Evaluate the reserved project without changing the frozen procedure', 'started_at': 'date', 'conclusion': 'failure'}]}]}
        data = io.BytesIO()
        with zipfile.ZipFile(data, 'w') as archive:
            archive.writestr('freeze.json', '{}')
            archive.writestr('reserved.json', '{}')
        artifacts = {'artifacts': [{'name': 'saved-work-reserved'}]}
        with patch.dict(cost.os.environ, environment), patch.object(cost, 'api', side_effect=[runs, jobs, artifacts]), \
                patch.object(cost, 'artifact_bytes', return_value=data.getvalue()):
            with self.assertRaisesRegex(ValueError, 'already opened'):
                cost.previous_attempts()

    def test_failed_development_attempt_is_retained_without_consuming_project(self):
        environment = {'GITHUB_RUN_ID': '20', 'GITHUB_RUN_ATTEMPT': '1'}
        runs = {'workflow_runs': [{'id': 10, 'head_sha': 'x', 'conclusion': 'failure', 'html_url': 'url'}]}
        jobs = {'jobs': [{'name': 'Reserved project', 'started_at': 'date', 'conclusion': 'skipped', 'steps': []}]}
        with patch.dict(cost.os.environ, environment), patch.object(cost, 'api', side_effect=[runs, jobs]):
            self.assertEqual(cost.previous_attempts()[0]['reserved_opened'], False)

    def test_queued_job_timestamp_does_not_mean_source_was_opened(self):
        self.assertFalse(cost.opened_source(10, [{'name': 'Reserved project', 'started_at': 'date',
            'conclusion': 'cancelled', 'steps': []}]))

    def test_failed_preflight_does_not_consume_unread_project(self):
        jobs = [{'name': 'Reserved project', 'steps': [{'name':
            'Evaluate the reserved project without changing the frozen procedure', 'started_at': 'date', 'conclusion': 'failure'}]}]
        data = io.BytesIO()
        with zipfile.ZipFile(data, 'w') as archive:
            archive.writestr('saved-work/freeze.json', '{}')
            archive.writestr('saved-work/glob/result.json', '{}')
        with patch.object(cost, 'api', return_value={'artifacts': [{'name': 'saved-work-reserved'}]}), \
                patch.object(cost, 'artifact_bytes', return_value=data.getvalue()):
            self.assertFalse(cost.opened_source(10, jobs))

    def test_packed_state_preserves_bytes_with_existing_limits(self):
        source = self.directory / '0-saved/saved-state'
        restored = self.directory / 'restored-state'
        cost.unpack_state(self.directory / '0-saved/saved-state.zip', restored, self.tasks['limits'])
        self.assertEqual(cost.inventory(source), cost.inventory(restored))
        bad = self.directory / 'bad-state.zip'
        with zipfile.ZipFile(bad, 'w') as archive:
            archive.writestr('../escape', '{}')
        with self.assertRaisesRegex(ValueError, 'unsafe saved state'):
            cost.unpack_state(bad, self.directory / 'bad-state', self.tasks['limits'])

    def test_authenticated_artifact_download_checks_original_digest(self):
        data = b'original archive'
        artifact = {'id': 5, 'size_in_bytes': len(data), 'expired': False,
            'digest': 'sha256:' + hashlib.sha256(data).hexdigest()}

        def download(command, **kwargs):
            self.assertEqual(command, ['gh', 'api', 'repos/example/project/actions/artifacts/5/zip'])
            self.assertEqual(kwargs['timeout'], 30)
            kwargs['stdout'].write(data)

        with patch.dict(cost.os.environ, {'GITHUB_REPOSITORY': 'example/project'}), \
                patch.object(cost.subprocess, 'run', side_effect=download):
            self.assertEqual(cost.artifact_bytes(artifact), data)
            artifact['digest'] = 'sha256:' + '0' * 64
            with self.assertRaisesRegex(ValueError, 'artifact changed'):
                cost.artifact_bytes(artifact)

    def test_rerun_cannot_claim_first_evaluation(self):
        with patch.dict(cost.os.environ, {'GITHUB_RUN_ATTEMPT': '2'}):
            with self.assertRaisesRegex(ValueError, 'rerun'):
                cost.previous_attempts()


if __name__ == '__main__':
    unittest.main()
