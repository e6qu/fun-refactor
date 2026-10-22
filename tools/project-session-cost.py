#!/usr/bin/env python3
"""Measure complete familiar-project sessions through CLI and Python clients."""
import argparse
from contextlib import ExitStack
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import platform
import selectors
import shutil
import statistics
import subprocess
import sys
import tempfile
import time
import traceback
import zipfile

sys.path.insert(0, str(Path(__file__).resolve().parent))
from agent_eval.bounded_host import run as bounded_run
from evidence_basis import native_bindings
from project_change_sources import download, unpack

ROOT = Path(__file__).resolve().parents[1]
CONDITIONS = ('cli-fresh', 'sdk-fresh', 'sdk-running', 'saved-fresh', 'saved-running')
STAGES = ('first', 'reuse', 'edit')
WORKER = ROOT / 'tools/project-session-worker.py'
TASKS = ROOT / 'tests/agent-eval/saved-work-cost/tasks.json'
BINDINGS = ('tools/project-session-cost.py', 'tools/project-session-worker.py',
    'tools/test-project-session-cost.py', 'tools/project_change_sources.py', 'tools/evidence_basis.py',
    'tools/agent_eval/bounded_host.py', 'tools/agent_eval/study.py',
    'tests/agent-eval/saved-work-cost/tasks.json', '.github/workflows/project-session-cost.yml',
    'sdk/python/pyproject.toml')
SCOPE = {
    'question': 'Find declarations and their bounded indexed change scopes, preserving missing and incomplete answers. Same familiar glob, Boltons and JMESPath sources and one-file invalidation probe as the saved-work experiment.',
    'baseline': 'cli-fresh orchestrates ordinary optimized fr commands with standard Python JSON parsing and no fr_ir imports in its measured session. It includes this Python coordinator; this is not a pure Python-versus-Rust runtime estimate.',
    'sdk': 'One installed SDK with checked-hash compiled modules, same interpreter and native executable. Fresh starts one worker per question; running keeps the existing Python client alive for the three questions. Every native request still starts fr. No new daemon or user-facing wrapper.',
    'cache': 'All conditions use an initially empty native project cache and retain it across questions within a session. Saved conditions additionally store and validate answers and task plans. Distinct sessions do not share production caches or saved state.',
    'time': 'Measured process lifetime includes the session coordinator, source copy, first Python start, all three questions, native source edit, validation, report recording, explicit worker shutdown and cleanup. Downloads and SDK build/install/bytecode compilation are separate one-time setup costs. Native builds and runner provisioning are workflow setup.',
    'reports': 'Complete answers stay local. Worker replies contain their digests. Equal evidence reports share one recorded file across conditions, preventing duplicated evidence archives. Hashing and evidence persistence are instrumentation included in time. Conditions rotate by project and repetition; OS caches remain uncontrolled.',
    'resources': 'GNU time records user and system CPU for the session and its waited-for descendants, rounded to 0.01 seconds. The process-group monitor separately samples memory and CPU; its ps CPU clock can round short processes down to whole seconds. Brief peaks can be missed and shared pages counted twice. The outer download/collection/audit process and runner setup are outside session peaks. Source, native cache and saved-answer storage are recorded separately from evidence files.',
    'claims': 'Three repetitions on three familiar projects. Requires all 135 answers to agree, including the changed source. No new unread qualification, live-agent adoption, model-token, billing or CodeGraph savings claim. Compare conditions within this run; earlier timings use different procedures and runners.',
}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def encode(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':'), allow_nan=False).encode()


def digest(value):
    return hashlib.sha256(encode(value)).hexdigest()


def file_digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def finite(value):
    return type(value) in (int, float) and math.isfinite(value) and value >= 0


def save(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(encode(value) + b'\n')


def inventory(root):
    return {str(path.relative_to(root)): file_digest(path) for path in sorted(root.rglob('*')) if path.is_file()}


def size(root):
    return sum(path.stat().st_size for path in root.rglob('*') if path.is_file()) if root.exists() else 0


def bindings():
    sdk = [str(path.relative_to(ROOT)) for path in sorted((ROOT / 'sdk/python/src').rglob('*.py'))]
    return native_bindings(ROOT, (*BINDINGS, *sdk))


def retain(reports, value):
    identity = digest(value)
    path, data = reports / (identity + '.json'), encode(value) + b'\n'
    if path.exists():
        require(path.read_bytes() == data, 'conflicting recorded report')
    else:
        path.write_bytes(data)
    return identity


def native(binary, project, arguments, reports):
    started = time.monotonic()
    result = subprocess.run([str(binary), '--json', '-C', str(project), *arguments],
        capture_output=True, timeout=20)
    elapsed = time.monotonic() - started
    require(result.returncode == 0, 'native question failed: ' + result.stderr.decode(errors='replace'))
    value = json.loads(result.stdout)
    return value, {'arguments': arguments, 'seconds': elapsed, 'report': retain(reports, value),
                   'sha256': digest(value), 'canonical_report_bytes': len(encode(value))}


def cli_question(binary, project, request, reports, output):
    output.mkdir()
    limits = request['scope_limits']
    found, call = native(binary, project, ['project', 'find', request['name'], '--source', '--bytes',
        str(limits['find_bytes']), '--limit', str(limits['find_limit']), '--in', limits['find_scope']], reports)
    require(found['page']['next'] is None, 'find requires another page')
    calls, scopes = [call], []
    for raw in found['rows']:
        row = dict(zip(found['columns'], raw))
        scope, call = native(binary, project, ['project', 'change-scope', row['handle'], '--depth', str(limits['depth']),
            '--nodes', str(limits['nodes']), '--references', str(limits['references']), '--bytes', str(limits['max_bytes'])], reports)
        scopes.append(scope)
        calls.append(call)
    answer = {'find': found, 'scopes': scopes}
    save(output / 'answer.json', {'report': retain(reports, answer)})
    result = {'condition': 'fresh', 'answer_digest': digest(answer), 'answer_bytes': len(encode(answer)) + 1,
              'calls': calls, 'invalidated': [], 'retained': False, 'validation': None, 'phases': {}}
    save(output / 'result.json', result)
    return result


class Worker:
    def __init__(self, args, project, state, output):
        self.stderr = (output / 'worker.stderr').open('wb')
        self.process = subprocess.Popen([sys.executable, '-I', '-B', str(WORKER), '--sdk', str(args.sdk),
            '--project', str(project), '--fr', str(args.fr), '--state', str(state), '--request', str(args.request),
            '--reports', str(args.reports), '--condition', 'saved' if args.condition.startswith('saved-') else 'fresh'],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.stderr)
        self.selector = selectors.DefaultSelector()
        self.selector.register(self.process.stdout, selectors.EVENT_READ)
        self.buffer = b''

    def read(self):
        deadline = time.monotonic() + 20
        while b'\n' not in self.buffer:
            require(self.selector.select(max(0, deadline - time.monotonic())), 'worker reply exceeded the request deadline')
            chunk = os.read(self.process.stdout.fileno(), 65536)
            require(chunk, 'worker exited before its reply')
            self.buffer += chunk
            require(len(self.buffer) <= 65536, 'worker reply exceeds 64 KiB; full answers belong in local reports')
        line, self.buffer = self.buffer.split(b'\n', 1)
        return json.loads(line)

    def send(self, value):
        self.process.stdin.write(encode(value) + b'\n')
        self.process.stdin.flush()

    def close(self):
        if self.process.poll() is None:
            self.process.kill()
        self.process.wait(timeout=5)
        self.process.stdin.close()
        self.process.stdout.close()
        self.selector.close()
        self.stderr.close()


def session(args):
    task = json.loads(args.task.read_text())
    request = json.loads(args.request.read_text())
    project, state = args.workspace / 'project', args.workspace / 'state'
    require(not project.exists() and not state.exists(), 'a previous session was not cleaned up')
    os.environ['FUN_REFACTOR_CACHE'] = str(project / '.fr/cost-cache')
    started = time.monotonic()
    shutil.copytree(args.source, project)
    state.mkdir()
    copy_seconds = time.monotonic() - started
    source_files = inventory(args.source)
    original = (project / task['edit_path']).read_bytes()
    before, after = task['name'].encode(), (task['name'] + '_cost_renamed').encode()
    require(original.count(before) > 0 and after not in original, 'source edit is unavailable')
    launches, stages, active = [], [], None

    def launch(label):
        folder = args.output / label
        folder.mkdir()
        started = time.monotonic()
        worker = Worker(args, project, state, folder)
        try:
            ready = worker.read()
            require(ready['ready'] is True and Path(ready['sdk_runtime']).is_relative_to(args.sdk), 'different SDK loaded')
        except BaseException:
            worker.close()
            raise
        launches.append({'startup_seconds': time.monotonic() - started, 'ready': ready})
        return worker

    def stop(worker):
        started = time.monotonic()
        worker.send({'stop': True})
        require(worker.read() == {'stopped': True}, 'worker did not acknowledge shutdown')
        require(worker.process.wait(timeout=5) == 0, 'worker shutdown failed')
        launches[-1].update(shutdown_seconds=time.monotonic() - started, stopped=True)
        worker.close()

    try:
        if args.condition.endswith('-running'):
            active = launch('running-client')
        for stage in STAGES:
            if stage == 'edit':
                edit_started = time.monotonic()
                edit, _ = native(args.fr, project, ['file', 'replace', task['edit_path'], '--before', before.decode(),
                    '--after', after.decode(), '--matches', str(original.count(before)), '--write'], args.reports)
                save(args.output / 'edit.json', {'report': retain(args.reports, edit),
                    'seconds': time.monotonic() - edit_started, 'matches': original.count(before)})
            output, started = args.output / stage, time.monotonic()
            if args.condition == 'cli-fresh':
                result = cli_question(args.fr, project, request, args.reports, output)
            else:
                if active is None:
                    active = launch(stage + '-client')
                active.send({'stage': stage, 'output': str(output)})
                reply = active.read()
                result = json.loads((output / 'result.json').read_text())
                require(reply == {'stage': stage, 'answer_digest': result['answer_digest']}, 'worker reply differs from recorded answer')
                if args.condition.endswith('-fresh'):
                    stop(active)
                    active = None
            stages.append({'stage': stage, 'seconds': time.monotonic() - started,
                'answer_digest': result['answer_digest'], 'answer_bytes': result['answer_bytes'],
                'native_calls': len(result['calls']), 'native_report_bytes': sum(row['canonical_report_bytes'] for row in result['calls']),
                'native_cache_bytes': size(project / '.fr'), 'saved_bytes': size(state)})
            expected = {**source_files, **({task['edit_path']: hashlib.sha256(original.replace(before, after)).hexdigest()} if stage == 'edit' else {})}
            current = {name: value for name, value in inventory(project).items()
                       if Path(name).parts[0] not in ('.fr', '.fr-history')}
            require(current == expected, 'a question changed source or used the wrong source state')
            stages[-1]['source_files'] = current
        if active is not None:
            stop(active)
            active = None
        if args.condition.startswith('saved-'):
            with zipfile.ZipFile(args.output / 'saved-state.zip', 'w', compression=zipfile.ZIP_DEFLATED) as archive:
                for path in sorted(state.rglob('*')):
                    if path.is_file():
                        archive.write(path, str(path.relative_to(state)))
        cleanup = time.monotonic()
        shutil.rmtree(project)
        shutil.rmtree(state)
        save(args.output / 'session.json', {'condition': args.condition, 'copy_seconds': copy_seconds,
            'cleanup_seconds': time.monotonic() - cleanup, 'launches': launches, 'stages': stages,
            'source_files': source_files, 'workspace': str(project)})
        print(json.dumps({'condition': args.condition, 'answers': [row['answer_digest'] for row in stages]}), flush=True)
    finally:
        if active is not None:
            active.close()


def original_reference():
    spec = importlib.util.spec_from_file_location('saved_answer_comparison', ROOT / 'tools/saved-answer-comparison.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.original_reference()


def collect(args):
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'collect on GitHub runners')
    args.output.mkdir(parents=True, exist_ok=False)
    reports = args.output / 'reports'
    reports.mkdir()
    tasks, before = json.loads(TASKS.read_text()), bindings()
    records, setup = [], {}
    save(args.output / 'freeze.json', {'source_bindings': before, 'source_commit': os.environ['GITHUB_SHA'],
        'binary_sha256': file_digest(args.fr), 'conditions': CONDITIONS, 'scope': SCOPE})
    try:
        with tempfile.TemporaryDirectory(prefix='fr-project-sessions-') as temporary:
            root = Path(temporary)
            workspace, sdk = root / 'workspace', root / 'installed'
            workspace.mkdir()
            wheel = root / 'wheel'
            for name, command in (
                ('wheel_build_seconds', [sys.executable, '-m', 'pip', 'wheel', '--no-deps', '--no-build-isolation',
                                        '--wheel-dir', str(wheel), str(ROOT / 'sdk/python')]),
            ):
                started = time.monotonic()
                subprocess.run(command, check=True, timeout=60)
                setup[name] = time.monotonic() - started
            package, = wheel.glob('*.whl')
            started = time.monotonic()
            subprocess.run([sys.executable, '-m', 'pip', 'install', '--no-deps', '--no-compile',
                            '--target', str(sdk), str(package)], check=True, timeout=60)
            setup['wheel_install_seconds'] = time.monotonic() - started
            import compileall
            import py_compile
            started = time.monotonic()
            require(compileall.compile_dir(str(sdk / 'fr_ir'), quiet=1,
                invalidation_mode=py_compile.PycInvalidationMode.CHECKED_HASH), 'SDK compilation failed')
            setup['bytecode_compile_seconds'] = time.monotonic() - started
            for path in (sdk / 'fr_ir').rglob('*.py'):
                data = Path(importlib.util.cache_from_source(str(path))).read_bytes()
                require(data[:4] == importlib.util.MAGIC_NUMBER and int.from_bytes(data[4:8], 'little') == 3
                        and data[8:16] == importlib.util.source_hash(path.read_bytes()), 'SDK bytecode differs')
            source_inventory = inventory(ROOT / 'sdk/python/src/fr_ir')
            installed_inventory = {name: value for name, value in inventory(sdk / 'fr_ir').items() if name.endswith('.py')}
            require({name: value for name, value in source_inventory.items() if name.endswith('.py')} == installed_inventory,
                    'installed SDK source differs')
            for project_index, task in enumerate(tasks['projects']):
                directory = args.output / task['id']
                directory.mkdir()
                preparation = time.monotonic()
                download(task, directory / 'source', tasks['limits'])
                setup[task['id'] + '_download_seconds'] = time.monotonic() - preparation
                source = directory / 'source/original'
                request = {key: tasks[key] for key in ('question', 'scope_limits')}
                request['name'] = task['name']
                save(directory / 'request.json', request)
                save(directory / 'task.json', task)
                for repetition in range(tasks['repetitions']):
                    offset = (project_index + repetition) % len(CONDITIONS)
                    order = CONDITIONS[offset:] + CONDITIONS[:offset]
                    for position, condition in enumerate(order):
                        output = directory / f'{repetition}-{condition}'
                        output.mkdir()
                        command = [sys.executable, '-I', '-B', str(Path(__file__).resolve()), '--session',
                            '--condition', condition, '--task', str(directory / 'task.json'),
                            '--request', str(directory / 'request.json'), '--source', str(source), '--sdk', str(sdk),
                            '--fr', str(args.fr), '--reports', str(reports), '--workspace', str(workspace), '--output', str(output)]
                        command = ['/usr/bin/time', '-f', '%U %S', '-o', str(output / 'cpu.txt'), *command]
                        with (output / 'stdout.txt').open('wb') as stdout, (output / 'stderr.txt').open('wb') as stderr:
                            measured = bounded_run(command, b'', stdout, stderr, workspace,
                                wall_seconds=tasks['limits']['wall_seconds'], cpu_limit_seconds=tasks['limits']['cpu_seconds'],
                                rss_bytes=tasks['limits']['rss_bytes'], disk_bytes=tasks['limits']['disk_bytes'],
                                transcript_bytes=tasks['limits']['transcript_bytes'],
                                env={**os.environ, 'PYTHONHASHSEED': '0', 'RAYON_NUM_THREADS': '1'})
                        save(output / 'process.json', {'argv': command, 'measurement': measured})
                        records.append({'project': task['id'], 'repetition': repetition, 'condition': condition,
                                        'position': position, 'path': str(output.relative_to(args.output)),
                                        'measurement': measured, 'cpu_seconds': None})
                        save(args.output / 'progress.json', {'completed': records, 'setup': setup})
                        require(measured['exit_code'] == 0 and measured['stop_reason'] is None,
                                'session failed: ' + str(output) + '; inspect retained diagnostics')
                        cpu = [float(value) for value in (output / 'cpu.txt').read_text().split()]
                        require(len(cpu) == 2 and all(finite(value) for value in cpu), 'missing complete CPU accounting')
                        records[-1]['cpu_seconds'] = sum(cpu)
                        require(sum(cpu) <= tasks['limits']['cpu_seconds'], 'complete session CPU exceeded its limit')
                        save(args.output / 'progress.json', {'completed': records, 'setup': setup})
                shutil.rmtree(source)
        result = {'schema': 'fr-project-session-cost', 'passed': True, 'source_bindings': before,
            'source_commit': os.environ['GITHUB_SHA'], 'run_id': int(os.environ['GITHUB_RUN_ID']),
            'binary_sha256': file_digest(args.fr), 'environment': {'python': platform.python_version(), 'platform': platform.platform()},
            'scope': SCOPE, 'conditions': CONDITIONS, 'stages': STAGES, 'tasks': tasks,
            'projects': 3, 'repetitions': 3, 'answer_agreement': True, 'all_projects_familiar': True,
            'new_unread_claim': False, 'original_unread': original_reference(), 'setup': setup, 'sessions': records}
        require(bindings() == before, 'measurement source changed')
        result['files'] = {name: value for name, value in inventory(args.output).items() if name != 'result.json'}
        save(args.output / 'result.json', result)
        audit(args.output)
    except BaseException:
        save(args.output / 'failure.json', {'traceback': traceback.format_exc(), 'sessions': records, 'setup': setup})
        raise


def audit(directory):
    with ExitStack() as resources:
        return audit_contents(directory, resources)


def audit_contents(directory, resources):
    if (directory / 'evidence.zip').is_file():
        outer = json.loads((directory / 'result.json').read_text())
        require(outer['files'] == {'evidence.zip': file_digest(directory / 'evidence.zip')}, 'retained archive changed')
        with tempfile.TemporaryDirectory(prefix='fr-session-audit-') as temporary:
            destination = Path(temporary)
            with zipfile.ZipFile(directory / 'evidence.zip') as archive:
                entries = archive.infolist()
                require(len(entries) <= 4096 and sum(entry.file_size for entry in entries) <= 67108864,
                        'retained evidence exceeds its budget')
                names = set()
                for entry in entries:
                    path = Path(entry.filename)
                    require(not path.is_absolute() and '..' not in path.parts and not entry.is_dir()
                            and path.as_posix() == entry.filename and entry.filename not in names, 'unsafe retained entry')
                    names.add(entry.filename)
                    target = destination / path
                    target.parent.mkdir(parents=True, exist_ok=True)
                    target.write_bytes(archive.read(entry))
            inner = json.loads((destination / 'result.json').read_text())
            require(outer == {**inner, 'files': outer['files'], 'collection_sha256': file_digest(destination / 'result.json')},
                    'retained summary differs from its collection')
            return audit(destination)
    report = json.loads((directory / 'result.json').read_text())
    sys.path.insert(0, str(ROOT / 'sdk/python/src'))
    from fr_ir.context import DirectoryObjectStore, restore_stored_value
    from fr_ir.investigation import TaskPlan, StepState
    require(report['schema'] == 'fr-project-session-cost' and report['passed'] is True, 'incomplete session comparison')
    require(report['source_bindings'] == bindings() and report['scope'] == SCOPE, 'measurement sources or scope changed')
    require(report['conditions'] == list(CONDITIONS) and report['stages'] == list(STAGES), 'condition or stage changed')
    require(report['tasks'] == json.loads(TASKS.read_text()) and report['projects'] == 3 and report['repetitions'] == 3,
            'project task plan changed')
    require(report['original_unread'] == original_reference() and report['all_projects_familiar'] is True
            and report['new_unread_claim'] is False, 'unread-project qualification changed')
    require(report['files'] == {name: value for name, value in inventory(directory).items()
                               if name != 'result.json'}, 'recorded evidence changed')
    require(all(finite(value) for value in report['setup'].values()), 'invalid setup cost')
    source_files, edited_files = {}, {}
    for task in report['tasks']['projects']:
        base = directory / task['id']
        source = json.loads((base / 'source/source.json').read_text())
        require(source['repository'] == task['repository'] and source['revision'] == task['revision']
                and source['archive_sha256'] == file_digest(base / 'source/source.tar.gz'), 'source identity differs')
        require(json.loads((base / 'task.json').read_text()) == task and json.loads((base / 'request.json').read_text()) == {
            'name': task['name'], 'question': report['tasks']['question'], 'scope_limits': report['tasks']['scope_limits']},
            'question changed')
        with tempfile.TemporaryDirectory(prefix='fr-session-source-') as temporary:
            tree = Path(temporary) / 'source'
            unpack(base / 'source/source.tar.gz', tree, report['tasks']['limits'])
            require(inventory(tree) == source['files'], 'original source inventory differs')
            path = tree / task['edit_path']
            before = path.read_bytes()
            path.write_bytes(before.replace(task['name'].encode(), (task['name'] + '_cost_renamed').encode()))
            edited_files[task['id']] = inventory(tree)
            require(edited_files[task['id']] != source['files'], 'source edit had no effect')
        source_files[task['id']] = source['files']
    objects = {}
    for path in (directory / 'reports').glob('*.json'):
        value = json.loads(path.read_text())
        require(path.stem == digest(value), 'a recorded report has the wrong fingerprint')
        objects[path.stem] = value
    require(len(report['sessions']) == 45, 'missing sessions')
    seen, agreement, summary = set(), {}, {}
    for row in report['sessions']:
        project, repetition, condition = row['project'], row['repetition'], row['condition']
        require(project in {task['id'] for task in report['tasks']['projects']} and condition in CONDITIONS
                and type(repetition) is int and 0 <= repetition < 3, 'invalid session identity')
        identity = project, repetition, condition
        require(identity not in seen and row['path'] == f'{project}/{repetition}-{condition}', 'duplicate or unsafe session')
        seen.add(identity)
        project_index = [task['id'] for task in report['tasks']['projects']].index(project)
        require(row['position'] == (CONDITIONS.index(condition) - project_index - repetition) % len(CONDITIONS),
                'condition order changed')
        base = directory / row['path']
        process = json.loads((base / 'process.json').read_text())['measurement']
        require(process == row['measurement'] and process['exit_code'] == 0 and process['stop_reason'] is None,
                'session did not finish within its limits')
        expected_limits = report['tasks']['limits']
        cpu = [float(value) for value in (base / 'cpu.txt').read_text().split()]
        require(len(cpu) == 2 and all(finite(value) for value in cpu) and finite(row['cpu_seconds'])
                and row['cpu_seconds'] == sum(cpu) and sum(cpu) <= expected_limits['cpu_seconds'], 'complete CPU accounting differs')
        require(process['process_exit_code'] == 0 and process['timed_out'] is False
                and process['monitor_error'] is None and process['launch_error'] is None
                and process['limits'] == {name: expected_limits[name] for name in
                    ('wall_seconds', 'cpu_seconds', 'rss_bytes', 'disk_bytes', 'transcript_bytes')}, 'measurement limits changed')
        for field, limit in (('elapsed_seconds', 'wall_seconds'), ('sampled_cpu_seconds', 'cpu_seconds'),
            ('sampled_aggregate_rss_bytes', 'rss_bytes'), ('sampled_disk_growth_bytes', 'disk_bytes'), ('transcript_bytes', 'transcript_bytes')):
            require(finite(process[field]) and process[field] <= expected_limits[limit], 'resource limit exceeded')
        session_value = json.loads((base / 'session.json').read_text())
        require(session_value['condition'] == condition and [stage['stage'] for stage in session_value['stages']] == list(STAGES),
                'session stages changed')
        launches = session_value['launches']
        require(session_value['source_files'] == source_files[project] and finite(session_value['copy_seconds'])
                and finite(session_value['cleanup_seconds']), 'session source or setup differs')
        expected_launches = 0 if condition == 'cli-fresh' else 1 if condition.endswith('-running') else 3
        require(len(launches) == expected_launches and all(item['stopped'] is True for item in launches), 'wrong process lifecycle')
        require(all(finite(item['startup_seconds']) and finite(item['shutdown_seconds']) for item in launches), 'invalid client cost')
        expected_stdout = {'condition': condition, 'answers': [stage['answer_digest'] for stage in session_value['stages']]}
        require(json.loads((base / 'stdout.txt').read_text()) == expected_stdout, 'session replies differ')
        saved_state = None
        if condition.startswith('saved-'):
            saved_state = resources.enter_context(tempfile.TemporaryDirectory(prefix='fr-session-state-'))
            with zipfile.ZipFile(base / 'saved-state.zip') as archive:
                entries = archive.infolist()
                require({entry.filename for entry in entries} == {'receipt.json', 'objects/objects.sqlite3'}
                        and len(entries) == 2 and sum(entry.file_size for entry in entries) <= expected_limits['disk_bytes'],
                        'unexpected saved state')
                archive.extractall(saved_state)
            store = DirectoryObjectStore(Path(saved_state) / 'objects')
        for stage in session_value['stages']:
            stage_root = base / stage['stage']
            result = json.loads((stage_root / 'result.json').read_text())
            require(stage['source_files'] == (edited_files[project] if stage['stage'] == 'edit' else source_files[project]),
                    'wrong source state')
            answer = objects[json.loads((stage_root / 'answer.json').read_text())['report']]
            require(answer['find']['page']['next'] is None and len(answer['find']['rows']) == len(answer['scopes']),
                    'missing discovery page or scope')
            if stage['stage'] == 'first':
                require(bool(answer['find']['rows']), 'empty initial discovery')
            require(stage['answer_digest'] == result['answer_digest'] == digest(answer)
                    and stage['answer_bytes'] == result['answer_bytes'] == len(encode(answer)) + 1, 'answer differs')
            key = project, repetition, stage['stage']
            agreement.setdefault(key, set()).add(stage['answer_digest'])
            require(len(agreement[key]) == 1, 'CLI, SDK or saved answers disagree')
            require(result['retained'] is (condition.startswith('saved-') and stage['stage'] == 'reuse'), 'wrong reuse claim')
            require(result['invalidated'] == (['selected'] if condition.startswith('saved-') and stage['stage'] == 'edit' else []),
                    'wrong edit invalidation')
            require(finite(stage['seconds']) and all(finite(value) for value in result['phases'].values()), 'invalid stage cost')
            require(stage['native_calls'] == len(result['calls']) and stage['native_report_bytes'] == sum(
                item['canonical_report_bytes'] for item in result['calls']), 'native accounting differs')
            for call in result['calls']:
                value = objects[call['report']]
                require(finite(call['seconds']) and call['sha256'] == digest(value)
                        and call['canonical_report_bytes'] == len(encode(value)), 'native report differs')
            find_calls = [objects[call['report']] for call in result['calls'] if call['arguments'][:2] == ['project', 'find']]
            scope_calls = [objects[call['report']] for call in result['calls'] if call['arguments'][:2] == ['project', 'change-scope']]
            if result['retained']:
                require(not find_calls and not scope_calls, 'reuse performed fresh discovery')
            else:
                require(find_calls == [answer['find']] and scope_calls == answer['scopes'], 'discovery evidence differs')
                task, = [task for task in report['tasks']['projects'] if task['id'] == project]
                limits = report['tasks']['scope_limits']
                expected = ['project', 'find', task['name'], '--source', '--bytes', str(limits['find_bytes']),
                            '--limit', str(limits['find_limit']), '--in', limits['find_scope']]
                require([call['arguments'] for call in result['calls'] if call['arguments'][:2] == ['project', 'find']] == [expected],
                        'native discovery question changed')
            if saved_state is not None:
                task, = [task for task in report['tasks']['projects'] if task['id'] == project]
                receipt = json.loads((stage_root / 'receipt.json').read_text())
                plan = TaskPlan.restore(store, receipt['plan'])
                require(restore_stored_value(store, receipt['answer']) == answer and plan.goal == report['tasks']['question']
                        and len(plan.steps) == 1 and plan.steps[0].state == StepState.SATISFIED
                        and plan.steps[0].question == task['name']
                        and [(item.kind.value, item.key) for item in plan.steps[0].inputs] == [
                            ('workspace', 'project'), ('lookup', task['name']), ('analyzer', 'native-implementation')]
                        and len(plan.steps[0].evidence) == 1
                        and plan.steps[0].evidence[0].reference == 'sha256:' + digest(answer), 'stored evidence differs')
            validations = [objects[call['report']] for call in result['calls']
                           if call['arguments'][:2] == ['project', 'investigate'] and '--transition' not in call['arguments']]
            if condition.startswith('saved-') and stage['stage'] != 'first':
                require(validations == [result['validation']] and result['validation']['invalidated'] == result['invalidated']
                        and result['validation']['complete'] is result['retained'], 'native validation differs')
            else:
                require(not validations and result['validation'] is None, 'unexpected saved validation')
        summary.setdefault(project, {}).setdefault(condition, []).append(process['elapsed_seconds'])
    require(len(agreement) == 27 and report['answer_agreement'] is True, 'missing answer comparisons')
    for project in summary:
        for repetition in range(3):
            require(agreement[project, repetition, 'first'] == agreement[project, repetition, 'reuse']
                    and agreement[project, repetition, 'edit'] != agreement[project, repetition, 'first'],
                    'unchanged answer differs or source edit had no effect on the answer')
    return {project: {condition: {'samples_seconds': samples, 'median_seconds': statistics.median(samples)}
                     for condition, samples in conditions.items()} for project, conditions in summary.items()}


def retain_collection(source, destination):
    audit(source)
    destination.mkdir(parents=True, exist_ok=False)
    with zipfile.ZipFile(destination / 'evidence.zip', 'w', compression=zipfile.ZIP_DEFLATED) as archive:
        for path in sorted(source.rglob('*')):
            if path.is_file():
                archive.write(path, str(path.relative_to(source)))
    report = json.loads((source / 'result.json').read_text())
    save(destination / 'result.json', {**report, 'files': {'evidence.zip': file_digest(destination / 'evidence.zip')},
                                     'collection_sha256': file_digest(source / 'result.json')})
    return audit(destination)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument('--collect', action='store_true')
    mode.add_argument('--session', action='store_true')
    mode.add_argument('--audit', action='store_true')
    mode.add_argument('--retain', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--fr', type=Path)
    parser.add_argument('--condition', choices=CONDITIONS)
    for name in ('task', 'request', 'source', 'sdk', 'reports', 'workspace'):
        parser.add_argument('--' + name, type=Path)
    args = parser.parse_args()
    if args.session:
        require(all(getattr(args, name) is not None for name in ('condition', 'task', 'request', 'source', 'sdk', 'fr', 'reports', 'workspace')),
                'submit every session input explicitly')
        session(args)
    elif args.collect:
        require(args.fr is not None, 'collection requires the compiled executable')
        args.fr = args.fr.resolve()
        collect(args)
    elif args.retain is not None:
        print(json.dumps(retain_collection(args.retain, args.output), indent=2))
    else:
        print(json.dumps(audit(args.output), indent=2))
