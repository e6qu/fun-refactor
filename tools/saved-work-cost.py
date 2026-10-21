"""Compare saved answers with fresh discovery on identical changed projects."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import resource
import shutil
import statistics
import subprocess
import sys
import tempfile
import time
import urllib.request
import zipfile

from evidence_basis import native_source_bindings
from project_change_sources import digest as file_digest, download, save
from agent_eval.bounded_host import run as bounded_run

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'sdk/python/src'))
TASKS = ROOT / 'tests/agent-eval/saved-work-cost/tasks.json'
WORKER = ROOT / 'tools/saved-work-worker.py'
BINDINGS = ('tools/saved-work-cost.py', 'tools/saved-work-worker.py', 'tools/test-saved-work-cost.py',
    'tools/project_change_sources.py', 'tools/evidence_basis.py', 'tools/agent_eval/bounded_host.py',
    'tools/agent_eval/study.py', 'tests/agent-eval/saved-work-cost/tasks.json',
    '.github/workflows/saved-work-cost.yml')
BUILD = 'cargo build --locked --release --bin fr'


def encode(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':'), allow_nan=False).encode()


def digest(value):
    return hashlib.sha256(encode(value)).hexdigest()


def require(condition, message):
    if not condition:
        raise ValueError(message)


def bindings():
    names = list(BINDINGS) + [str(p.relative_to(ROOT)) for p in (ROOT / 'sdk/python/src').rglob('*.py')]
    return native_source_bindings(ROOT) | {name: file_digest(ROOT / name) for name in names}


def finite(value):
    return type(value) in (int, float) and math.isfinite(value) and value >= 0


def integer(value):
    return type(value) is int and value >= 0


def inventory(directory):
    return {str(path.relative_to(directory)): file_digest(path)
            for path in sorted(directory.rglob('*')) if path.is_file()}


def cpu_now():
    return sum(getattr(resource.getrusage(kind), field)
        for kind in (resource.RUSAGE_SELF, resource.RUSAGE_CHILDREN) for field in ('ru_utime', 'ru_stime'))


def api(path):
    request = urllib.request.Request('https://api.github.com/repos/' + os.environ['GITHUB_REPOSITORY'] + '/' + path,
        headers={'Authorization': 'Bearer ' + os.environ['GH_TOKEN'], 'Accept': 'application/vnd.github+json'})
    with urllib.request.urlopen(request, timeout=30) as response:
        return json.load(response)


def previous_attempts():
    require(os.environ['GITHUB_RUN_ATTEMPT'] == '1', 'a rerun cannot claim a first reserved evaluation')
    current, attempts, page = int(os.environ['GITHUB_RUN_ID']), [], 1
    while True:
        batch = api(f'actions/workflows/saved-work-cost.yml/runs?per_page=100&page={page}')['workflow_runs']
        for run in batch:
            if run['id'] >= current:
                continue
            jobs, job_page = [], 1
            while True:
                rows = api(f'actions/runs/{run["id"]}/jobs?filter=all&per_page=100&page={job_page}')[ 'jobs']
                jobs.extend(rows)
                if len(rows) < 100:
                    break
                job_page += 1
            consumed = any(job['name'] == 'Reserved project' and job['started_at'] is not None
                and job['conclusion'] != 'skipped' for job in jobs)
            attempts.append({'run_id': run['id'], 'head_sha': run['head_sha'], 'conclusion': run['conclusion'],
                'url': run['html_url'], 'reserved_opened': consumed})
        if len(batch) < 100:
            break
        page += 1
    require(not any(row['reserved_opened'] for row in attempts),
        'reserved project was already opened; preserve that result and choose another unread project')
    return sorted(attempts, key=lambda row: row['run_id'])


def measured(command, output, workspace, limits):
    output.mkdir(parents=True, exist_ok=False)
    timed = ['/usr/bin/time', '--output', str(output / 'cpu.txt'), '--format', '%U %S', *command]
    with (output / 'stdout.txt').open('wb') as stdout, (output / 'stderr.txt').open('wb') as stderr:
        result = bounded_run(timed, b'', stdout, stderr, workspace,
            wall_seconds=limits['wall_seconds'], cpu_limit_seconds=limits['cpu_seconds'],
            rss_bytes=limits['rss_bytes'], disk_bytes=limits['disk_bytes'],
            transcript_bytes=limits['transcript_bytes'], cwd=ROOT,
            env={**os.environ, 'PYTHONDONTWRITEBYTECODE': '1', 'PYTHONHASHSEED': '0',
                'RAYON_NUM_THREADS': '1', 'FUN_REFACTOR_CACHE': str(workspace / 'project/.fr/cost-cache')})
    save(output / 'process.json', {'argv': command, 'measurement': result})
    require(result['stop_reason'] is None and result['exit_code'] == 0, 'measured process failed: ' + str(output))
    cpu = sum(float(value) for value in (output / 'cpu.txt').read_text().split())
    require(finite(cpu), 'invalid process CPU')
    return result, cpu


def collect_project(task, output, binary, tasks):
    output.mkdir(parents=True, exist_ok=False)
    preparation, preparation_cpu = time.monotonic(), cpu_now()
    download(task, output / 'source', tasks['limits'])
    source = output / 'source/original'
    require(not any((source / name).exists() for name in ('.fr', '.fr-history')),
        'upstream source already has native cache or history state')
    source_files = inventory(source)
    source_bytes = sum(path.stat().st_size for path in source.rglob('*') if path.is_file())
    original = (source / task['edit_path']).read_bytes()
    old, new = task['name'].encode(), (task['name'] + '_cost_renamed').encode()
    matches = original.count(old)
    require(matches > 0 and new not in original, 'declared source edit is unavailable')
    changed_files = {**source_files, task['edit_path']: hashlib.sha256(original.replace(old, new)).hexdigest()}
    preparation_seconds = time.monotonic() - preparation
    preparation_cpu_seconds = cpu_now() - preparation_cpu
    request = {key: tasks[key] for key in ('question', 'scope_limits')}
    request['name'] = task['name']
    save(output / 'request.json', request)
    sessions = []
    with tempfile.TemporaryDirectory(prefix='fr-saved-cost-') as temporary:
        workspace = Path(temporary)
        project = workspace / 'project'
        for repetition in range(tasks['repetitions']):
            order = tasks['conditions'][repetition % 2:] + tasks['conditions'][:repetition % 2]
            for position, condition in enumerate(order):
                copied, copied_cpu = time.monotonic(), cpu_now()
                require(not project.exists(), 'previous condition was not cleaned up')
                shutil.copytree(source, project)
                copy_seconds = time.monotonic() - copied
                copy_cpu_seconds = cpu_now() - copied_cpu
                session = output / f'{repetition}-{condition}'
                state = workspace / f'{repetition}-{condition}-state'
                state.mkdir()
                stages = []
                for stage in tasks['stages']:
                    if stage == 'edit':
                        edit, cpu = measured([str(binary), '--json', '-C', str(project), 'file', 'replace',
                            task['edit_path'], '--before', old.decode(), '--after', new.decode(),
                            '--matches', str(matches), '--write'], session / 'source-edit', workspace, tasks['limits'])
                        save(session / 'edit.json', {'measurement': edit, 'cpu_seconds': cpu, 'matches': matches})
                    destination = session / stage
                    command = [sys.executable, '-B', str(WORKER), '--project', str(project), '--fr', str(binary),
                        '--request', str(output / 'request.json'), '--state', str(state),
                        '--cache', str(project / '.fr/cost-cache'),
                        '--output', str(destination), '--condition', condition, '--stage', stage]
                    process, cpu = measured(command, destination, workspace, tasks['limits'])
                    value = json.loads((destination / 'result.json').read_text())
                    require(value['source_files'] == (changed_files if stage == 'edit' else source_files),
                        'question changed project source')
                    if condition == 'saved':
                        save(destination / 'receipt.json', json.loads((state / 'receipt.json').read_text()))
                    stages.append({'stage': stage, 'path': str(destination.relative_to(output)),
                        'seconds': process['elapsed_seconds'], 'cpu_seconds': cpu,
                        'worker_cpu_seconds': value['worker_cpu_seconds'],
                        'sampled_aggregate_rss_bytes': process['sampled_aggregate_rss_bytes'],
                        'sampled_disk_growth_bytes': process['sampled_disk_growth_bytes'],
                        'answer_digest': value['answer_digest'], 'answer_bytes': value['answer_bytes'],
                        'program_bytes': WORKER.stat().st_size,
                        'invocation_bytes': len(encode(command)), 'request_bytes': len(encode(request)),
                        'native_calls': len(value['calls']),
                        'native_report_bytes': sum(call['canonical_report_bytes'] for call in value['calls']),
                        'native_seconds': sum(call['seconds'] for call in value['calls']),
                        'storage_before': value['storage_before'], 'storage_after': value['storage_after']})
                if condition == 'saved':
                    shutil.copytree(state, session / 'saved-state')
                cleaned, cleaned_cpu = time.monotonic(), cpu_now()
                shutil.rmtree(state)
                shutil.rmtree(project)
                cleanup_seconds, cleanup_cpu_seconds = time.monotonic() - cleaned, cpu_now() - cleaned_cpu
                sessions.append({'repetition': repetition, 'condition': condition, 'position': position,
                    'copy_seconds': copy_seconds, 'copy_cpu_seconds': copy_cpu_seconds,
                    'cleanup_seconds': cleanup_seconds, 'cleanup_cpu_seconds': cleanup_cpu_seconds,
                    'copy_bytes': source_bytes, 'stages': stages,
                    'total_seconds': copy_seconds + cleanup_seconds + sum(row['seconds'] for row in stages)
                        + edit['elapsed_seconds']})
    shutil.rmtree(source)
    result = {'project': task, 'source_files': source_files, 'changed_files': changed_files,
        'source_preparation_seconds': preparation_seconds,
        'source_preparation_cpu_seconds': preparation_cpu_seconds,
        'archive_bytes': (output / 'source/source.tar.gz').stat().st_size,
        'source_bytes': source_bytes,
        'sessions': sessions}
    save(output / 'result.json', result)
    return result


def summaries(projects):
    return {project['project']['id']: {condition: {stage: {
        'median_seconds': statistics.median(row['seconds'] for session in project['sessions']
            if session['condition'] == condition for row in session['stages'] if row['stage'] == stage),
        'samples_seconds': [row['seconds'] for session in project['sessions'] if session['condition'] == condition
            for row in session['stages'] if row['stage'] == stage]}
        for stage in ('first', 'reuse', 'edit')} for condition in ('saved', 'fresh')} for project in projects}


def audit_project(value, directory, tasks):
    from fr_ir.change_scope import ChangeScope
    from fr_ir.context import DirectoryObjectStore, restore_stored_value
    from fr_ir.investigation import TaskPlan, StepState
    from fr_ir.runtime import FrReport
    task = value['project']
    require(task in tasks['projects'], 'unexpected project')
    require(finite(value['source_preparation_seconds']) and finite(value['source_preparation_cpu_seconds']),
        'missing source preparation')
    require(integer(value['archive_bytes']) and integer(value['source_bytes']), 'missing source storage')
    source = json.loads((directory / 'source/source.json').read_text())
    require(source['repository'] == task['repository'] and source['revision'] == task['revision'], 'source identity differs')
    require(source['files'] == value['source_files'], 'source inventory differs')
    require(source['archive_sha256'] == file_digest(directory / 'source/source.tar.gz'), 'source archive differs')
    from project_change_sources import unpack
    with tempfile.TemporaryDirectory(prefix='fr-cost-source-audit-') as temporary:
        tree = Path(temporary) / 'source'
        unpack(directory / 'source/source.tar.gz', tree, tasks['limits'])
        require(inventory(tree) == value['source_files'], 'archive inventory differs')
        require(value['archive_bytes'] == (directory / 'source/source.tar.gz').stat().st_size
            and value['source_bytes'] == sum(path.stat().st_size for path in tree.rglob('*') if path.is_file()),
            'source storage differs')
        path = tree / task['edit_path']
        before = path.read_bytes()
        path.write_bytes(before.replace(task['name'].encode(), (task['name'] + '_cost_renamed').encode()))
        require(inventory(tree) == value['changed_files'] and value['changed_files'] != value['source_files'],
            'single source edit differs')
    require(len(value['sessions']) == 6, 'missing sessions')
    seen, answers = set(), {}
    for session in value['sessions']:
        repetition, condition = session['repetition'], session['condition']
        require(type(repetition) is int and 0 <= repetition < 3 and condition in tasks['conditions'], 'session identity')
        require((repetition, condition) not in seen, 'duplicate session')
        seen.add((repetition, condition))
        require(session['position'] == (tasks['conditions'].index(condition) - repetition) % 2, 'condition order')
        require(finite(session['copy_seconds']) and finite(session['copy_cpu_seconds'])
            and session['copy_bytes'] == value['source_bytes'], 'missing checkout copy cost')
        require(finite(session['cleanup_seconds']) and finite(session['cleanup_cpu_seconds']), 'missing cleanup cost')
        require([row['stage'] for row in session['stages']] == tasks['stages'], 'missing stages')
        edit_path = directory / f'{repetition}-{condition}/edit.json'
        edit = json.loads(edit_path.read_text())
        check_measurement(edit['measurement'], tasks['limits'])
        require(finite(edit['cpu_seconds']) and integer(edit['matches']) and edit['matches'] > 0, 'missing edit cost')
        require(math.isclose(session['total_seconds'], session['copy_seconds'] + session['cleanup_seconds']
            + edit['measurement']['elapsed_seconds']
            + sum(row['seconds'] for row in session['stages'])), 'total omits preparation or edit')
        for row in session['stages']:
            stage = row['stage']
            require(row['path'] == f'{repetition}-{condition}/{stage}', 'unsafe stage path')
            base = directory / row['path']
            raw = json.loads((base / 'result.json').read_text())
            process = json.loads((base / 'process.json').read_text())
            check_measurement(process['measurement'], tasks['limits'])
            require(row['seconds'] == process['measurement']['elapsed_seconds']
                and finite(raw['worker_seconds']) and raw['worker_seconds'] <= row['seconds'], 'wall time omits worker')
            require(row['cpu_seconds'] == sum(float(x) for x in (base / 'cpu.txt').read_text().split())
                and finite(row['worker_cpu_seconds']) and row['worker_cpu_seconds'] == raw['worker_cpu_seconds']
                and row['worker_cpu_seconds'] <= row['cpu_seconds'] + .021, 'CPU omits worker or children')
            for name in ('sampled_aggregate_rss_bytes', 'sampled_disk_growth_bytes'):
                require(integer(row[name]) and row[name] == process['measurement'][name], 'resource measurement differs')
            require(row['answer_digest'] == raw['answer_digest'] == digest(raw['answer'])
                and row['answer_bytes'] == raw['answer_bytes'] == len(encode(raw['answer'])) + 1
                and (base / 'stdout.txt').read_bytes() == encode(raw['answer']) + b'\n', 'answer differs')
            require(raw['condition'] == condition and raw['stage'] == stage, 'worker identity differs')
            require(raw['source_files'] == value['changed_files' if stage == 'edit' else 'source_files'], 'wrong source state')
            request = json.loads((directory / 'request.json').read_text())
            require(request == {'name': task['name'], 'question': tasks['question'], 'scope_limits': tasks['scope_limits']},
                'different question or limits')
            require(row['program_bytes'] == WORKER.stat().st_size and row['invocation_bytes'] == len(encode(process['argv']))
                and row['request_bytes'] == len(encode(request)), 'request overhead omitted')
            require(row['native_calls'] == len(raw['calls']) and row['native_report_bytes'] == sum(
                call['canonical_report_bytes'] for call in raw['calls']) and math.isclose(row['native_seconds'],
                sum(call['seconds'] for call in raw['calls'])) and row['native_seconds'] <= row['seconds'],
                'native work accounting differs')
            for index, call in enumerate(raw['calls']):
                require(call['report'] == f'calls/{index}.json' and finite(call['seconds']), 'invalid native call')
                report = json.loads((base / call['report']).read_text())
                require(digest(report) == call['sha256'] and len(encode(report)) == call['canonical_report_bytes'],
                    'native report differs')
            require(raw['retained'] is (condition == 'saved' and stage == 'reuse'), 'incorrect reuse claim')
            require(raw['invalidated'] == (['selected'] if condition == 'saved' and stage == 'edit' else []),
                'incorrect invalidation claim')
            discoveries = [call for call in raw['calls'] if call['arguments'][:2] == ['project', 'find']]
            require(len(discoveries) == (0 if raw['retained'] else 1), 'missing or repeated discovery')
            if discoveries:
                require(discoveries[0]['arguments'] == ['project', 'find', task['name'], '--source', '--bytes',
                    str(tasks['scope_limits']['find_bytes']), '--limit', str(tasks['scope_limits']['find_limit']),
                    '--in', tasks['scope_limits']['find_scope']], 'different native question')
                require(json.loads((base / discoveries[0]['report']).read_text()) == raw['answer']['find'],
                    'answer does not match discovery')
            found = raw['answer']['find']
            require(found['page']['next'] is None and len(found['rows']) <= tasks['scope_limits']['find_limit'],
                'unfinished discovery page')
            scopes = raw['answer']['scopes']
            require(len(scopes) == len(found['rows']), 'missing change scopes')
            scope_calls = [call for call in raw['calls'] if call['arguments'][:2] == ['project', 'change-scope']]
            require(len(scope_calls) == (0 if raw['retained'] else len(scopes)), 'repeated or missing scopes')
            for index, scope in enumerate(scopes):
                ChangeScope(FrReport(scope, ()))
                if not raw['retained']:
                    limits = tasks['scope_limits']
                    declaration = dict(zip(found['columns'], found['rows'][index]))
                    require(scope_calls[index]['arguments'] == ['project', 'change-scope', declaration['handle'],
                        '--depth', str(limits['depth']), '--nodes', str(limits['nodes']), '--references',
                        str(limits['references']), '--bytes', str(limits['max_bytes'])]
                        and json.loads((base / scope_calls[index]['report']).read_text()) == scope,
                        'scope differs from bounded native request')
            require(all(finite(n) for n in raw['phases'].values())
                and sum(raw['phases'].values()) <= raw['worker_seconds'], 'phase time accounting differs')
            require(('discovery_seconds' in raw['phases']) is (not raw['retained']), 'discovery timing missing')
            if condition == 'saved':
                receipt = json.loads((base / 'receipt.json').read_text())
                objects = DirectoryObjectStore(directory / f'{repetition}-{condition}/saved-state/objects')
                plan = TaskPlan.restore(objects, receipt['plan'])
                require(restore_stored_value(objects, receipt['answer']) == raw['answer'], 'stored answer differs')
                require(plan.goal == tasks['question'] and len(plan.steps) == 1
                    and plan.steps[0].question == task['name'] and plan.steps[0].state == StepState.SATISFIED
                    and [(item.kind.value, item.key) for item in plan.steps[0].inputs] == [
                        ('workspace', 'project'), ('lookup', task['name']), ('analyzer', 'native-implementation')]
                    and len(plan.steps[0].evidence) == 1
                    and plan.steps[0].evidence[0].reference == 'sha256:' + raw['answer_digest'],
                    'stored answer has incomplete dependencies or evidence')
                if stage != 'first':
                    validations = [json.loads((base / call['report']).read_text()) for call in raw['calls']
                        if call['arguments'][:2] == ['project', 'investigate'] and '--transition' not in call['arguments']]
                    require(validations == [raw['validation']]
                        and raw['validation']['invalidated'] == raw['invalidated']
                        and raw['validation']['complete'] is raw['retained'], 'validation differs from native report')
            if stage == 'first':
                require(bool(raw['answer']['find']['rows']), 'empty first question')
            for name in ('storage_before', 'storage_after'):
                require(row[name] == raw[name] and set(row[name]) == {'native_state', 'saved_state'}
                    and all(integer(n) for n in row[name].values()), 'storage accounting missing')
            if stage == 'first':
                require(row['storage_before'] == {'native_state': 0, 'saved_state': 0}, 'unmeasured priming')
            if condition == 'fresh':
                require(row['storage_after']['saved_state'] == 0, 'fresh route retained saved objects')
            answers[repetition, condition, stage] = raw['answer']
        require(answers[repetition, condition, 'first'] == answers[repetition, condition, 'reuse'], 'unchanged answer differs')
        require(answers[repetition, condition, 'first'] != answers[repetition, condition, 'edit'], 'edit did not affect answer')
    for repetition in range(3):
        for stage in tasks['stages']:
            require(answers[repetition, 'saved', stage] == answers[repetition, 'fresh', stage], 'saved and fresh disagree')


def check_measurement(value, limits):
    require(value['exit_code'] == 0 and value['process_exit_code'] == 0 and value['stop_reason'] is None
        and value['timed_out'] is False and value['monitor_error'] is None and value['launch_error'] is None,
        'failed measurement cannot pass')
    expected = {key: limits[key] for key in ('wall_seconds', 'rss_bytes', 'disk_bytes', 'cpu_seconds', 'transcript_bytes')}
    require(value['limits'] == expected, 'measurement limits changed')
    require(finite(value['elapsed_seconds']) and 0 < value['elapsed_seconds'] <= limits['wall_seconds'], 'invalid elapsed time')
    require(integer(value['sampled_aggregate_rss_bytes']) and 0 < value['sampled_aggregate_rss_bytes'] <= limits['rss_bytes'],
        'invalid aggregate memory')
    require(integer(value['sampled_disk_growth_bytes']) and value['sampled_disk_growth_bytes'] <= limits['disk_bytes'],
        'invalid disk growth')


def audit(directory):
    report = json.loads((directory / 'result.json').read_text())
    tasks = json.loads(TASKS.read_text())
    require(report['schema'] == 'fr-saved-work-cost' and report['passed'] is True, 'incomplete comparison')
    require(report['source_bindings'] == bindings() and report['tasks'] == tasks, 'measurement sources changed')
    require(report['freeze']['source_bindings'] == bindings()
        and report['freeze']['source_commit'] == report['source_commit']
        and report['freeze']['binary_sha256'] == report['binary_sha256'], 'freeze differs from measurement')
    require(report['environment']['build'] == BUILD, 'build differs')
    require(report['reserved']['first_evaluation'] is True and report['reserved']['project'] == 'jmespath'
        and report['reserved']['run_id'] == report['freeze']['run_id']
        and not any(row['reserved_opened'] for row in report['reserved']['previous_attempts']), 'reserved evaluation was reused')
    require(report['repetitions'] == 3 and report['projects'] == 3 and report['answer_agreement'] is True,
        'completion counts differ')
    require(report['files'] == {'measurements.zip': file_digest(directory / 'measurements.zip')}, 'archive differs')
    with tempfile.TemporaryDirectory(prefix='fr-cost-audit-') as temporary:
        base = Path(temporary)
        with zipfile.ZipFile(directory / 'measurements.zip') as archive:
            names, total = set(), 0
            for entry in archive.infolist():
                name = Path(entry.filename)
                require(not name.is_absolute() and '..' not in name.parts and entry.filename not in names
                    and not entry.is_dir() and entry.file_size <= 2097152
                    and (entry.external_attr >> 16) & 0o170000 in (0, 0o100000), 'unsafe archive entry')
                names.add(entry.filename)
                total += entry.file_size
                require(total <= 67108864 and len(names) <= 8192, 'evidence archive budget exceeded')
                destination = base / name
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes(archive.read(entry))
        receipt = json.loads((base / 'inventory.json').read_text())
        require(receipt == {key: value for key, value in inventory(base).items() if key != 'inventory.json'}, 'raw inventory differs')
        require(json.loads((base / 'freeze.json').read_text()) == report['freeze'], 'raw freeze differs')
        projects = []
        for task in tasks['projects']:
            value = json.loads((base / task['id'] / 'result.json').read_text())
            audit_project(value, base / task['id'], tasks)
            projects.append(value)
        require(report['summary'] == summaries(projects), 'summary differs from raw runs')
    return report


def collect(args):
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'collect only on GitHub runners')
    tasks = json.loads(TASKS.read_text())
    binary = args.fr.resolve()
    before = bindings()
    if args.collect == 'development':
        args.output.mkdir(parents=True, exist_ok=False)
        save(args.output / 'freeze.json', {'source_commit': os.environ['GITHUB_SHA'], 'source_bindings': before,
            'binary_sha256': file_digest(binary), 'run_id': int(os.environ['GITHUB_RUN_ID']),
            'environment': {'build': BUILD, 'python': platform.python_version(), 'platform': platform.platform(),
                'cache': 'Explicit per-condition project/.fr/cost-cache; initially empty',
                'RAYON_NUM_THREADS': '1', 'PYTHONHASHSEED': '0',
                'rustc': subprocess.check_output(['rustc', '--version'], text=True).strip()}})
    else:
        freeze = json.loads((args.output / 'freeze.json').read_text())
        require(freeze['source_commit'] == os.environ['GITHUB_SHA'] and freeze['source_bindings'] == before
            and freeze['binary_sha256'] == file_digest(binary), 'procedure changed before reserved project')
        attempts = previous_attempts()
        save(args.output / 'reserved.json', {'project': 'jmespath', 'first_evaluation': True,
            'run_id': int(os.environ['GITHUB_RUN_ID']), 'previous_attempts': attempts})
    for task in tasks['projects']:
        if task['role'] == ('development' if args.collect == 'development' else 'reserved'):
            value = collect_project(task, args.output / task['id'], binary, tasks)
            audit_project(value, args.output / task['id'], tasks)
    require(bindings() == before, 'collection changed its implementation')
    if args.collect == 'reserved':
        freeze = json.loads((args.output / 'freeze.json').read_text())
        save(args.output / 'inventory.json', inventory(args.output))
        destination = args.output.parent / (args.output.name + '-retained')
        destination.mkdir(exist_ok=False)
        with zipfile.ZipFile(destination / 'measurements.zip', 'w', compression=zipfile.ZIP_DEFLATED) as archive:
            for path in sorted(args.output.rglob('*')):
                if path.is_file():
                    archive.write(path, str(path.relative_to(args.output)))
        projects = [json.loads((args.output / task['id'] / 'result.json').read_text()) for task in tasks['projects']]
        save(destination / 'result.json', {'schema': 'fr-saved-work-cost', 'passed': True, 'tasks': tasks,
            'source_commit': freeze['source_commit'], 'source_bindings': before, 'freeze': freeze,
            'binary_sha256': freeze['binary_sha256'], 'environment': freeze['environment'],
            'reserved': json.loads((args.output / 'reserved.json').read_text()),
            'repetitions': 3, 'projects': 3, 'answer_agreement': True, 'summary': summaries(projects),
            'files': {'measurements.zip': file_digest(destination / 'measurements.zip')}})
        audit(destination)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument('--collect', choices=('development', 'reserved'))
    mode.add_argument('--audit', type=Path)
    parser.add_argument('--fr', type=Path)
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    if args.audit:
        audit(args.audit)
    else:
        require(args.fr is not None and args.output is not None, 'collection needs --fr and --output')
        collect(args)
