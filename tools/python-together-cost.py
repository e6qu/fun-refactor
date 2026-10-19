#!/usr/bin/env python3
"""Compare asking about two helpers together with separate fresh questions."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import selectors
import statistics
import subprocess
import sys
import tempfile
import time
import traceback

spec = importlib.util.spec_from_file_location('process_cost', Path(__file__).with_name('python-process-cost.py'))
process = importlib.util.module_from_spec(spec)
spec.loader.exec_module(process)
ROOT, CASES = process.ROOT, process.CASES
encode, digest, require, finite = process.encode, process.digest, process.require, process.finite
CONDITIONS = ('separate', 'together')
STAGES = (
    ('first-pair', 'original', ['forwards', 'stopped']),
    ('repeated-pair', 'original', ['forwards', 'stopped']),
    ('reversed-pair', 'original', ['stopped', 'forwards']),
    ('edited-helper', 'edited', ['forwards', 'stopped']),
    ('unknown-startup', 'unknown', ['forwards', 'stopped']),
    ('unknown-reversed', 'unknown', ['stopped', 'forwards']),
    ('restored-helper', 'original', ['forwards', 'stopped']),
)
BINDINGS = ('tools/python-together-cost.py', 'tools/python-together-worker.py',
            'tools/test-python-together-cost.py', '.github/workflows/python-together-cost.yml')
SCOPE = {
    'baseline': 'Two separate fresh dataflow questions in one running installed Python process. Compared with the existing analyze_together API in the same installed client and interpreter. Both find both functions before analysis; no saved setup or whole-answer store.',
    'limits': 'Each separate question receives 4096 steps (1 in the exhausted case) and 1048576 response bytes. The combined request shares that same cap between both answers, not twice the cap. Depth 8, html context, summaries enabled, imports explicit per case. Timings are comparable as equal-answer work only when answers actually agree.',
    'cost': 'One process start through parsed ready, seven submitted pairs through parsed replies, and stop through process exit. Includes imports, both function lookups, initially empty project-cache fill, native processes, analysis, SDK validation, instrumentation, report serialization and reply parsing. Controller fixture edits, evidence persistence and receipt checks are outside those intervals but included in session wall time.',
    'installation': 'Wheel build, installation without dependencies and checked-hash bytecode compilation are recorded separately. Same verified source in both conditions. Does not include provisioning the runner, interpreter or build dependencies, or compiling fr.',
    'bytes': 'report_json_bytes counts compact UTF-8 JSON reserialized from each parsed native report, including both find reports. It is not raw pipe traffic, agent-visible text or a memory measurement.',
    'claims': 'Four disclosed development examples, five repetitions, both orders, edits, unknown startup and exhausted work. Python behavior is checked separately, outside timed sequences. No unfamiliar-library acceptance, general language runtime overhead, memory savings or agent-task speedup claim.',
}


def bindings():
    return process.bindings() | {name: process.file_digest(ROOT / name) for name in BINDINGS}


def semantic(answer):
    return {name: value for name, value in answer.items() if name not in {'execution', 'context_basis'}}


def summary(sessions):
    return {case: {condition: {
        'samples_seconds': [row['total_seconds'] for row in sessions
                            if (row['case'], row['condition']) == (case, condition)],
        'median_seconds': statistics.median(row['total_seconds'] for row in sessions
                                            if (row['case'], row['condition']) == (case, condition))}
        for condition in CONDITIONS} for case in CASES}


def audit(report):
    require(report['schema'] == 'fr-python-together-cost' and report['bindings'] == bindings(),
            'measurement sources changed; collect again')
    require(report['scope'] == SCOPE and report['cases'] == CASES
            and report['stages'] == json.loads(encode(STAGES))
            and report['conditions'] == list(CONDITIONS) and report['repetitions'] == 5, 'scope changed')
    require(report['environment']['build'] == process.BUILD, 'optimized build changed')
    for name in ('binary_sha256', 'wheel_sha256'):
        require(isinstance(report[name], str) and len(report[name]) == 64
                and all(char in '0123456789abcdef' for char in report[name]), 'invalid executable or wheel identity')
    current = {path.relative_to(ROOT / 'sdk/python/src').as_posix(): hashlib.sha256(path.read_bytes()).hexdigest()
               for path in (ROOT / 'sdk/python/src/fr_ir').rglob('*.py')}
    require(report['sdk_files']['source'] == report['sdk_files']['installed']
            and set(current) == set(report['sdk_files']['source']) and all(
                name == 'fr_ir/_version.py' or report['sdk_files']['source'][name] == value
                for name, value in current.items()), 'installed SDK source changed')
    require(set(report['setup_seconds']) == {'wheel_build', 'wheel_install', 'bytecode_compile'}
            and all(finite(value) for value in report['setup_seconds'].values()), 'installation cost missing')
    observations = {case: {state: ({'error': 'NameError', 'returned': None} if state == 'unknown'
                                  else {'error': None, 'returned': [17, 17 if state == 'edited' else 0]})
                          for state in ('original', 'edited', 'unknown')} for case in CASES}
    require(report['python_observations'] == observations, 'Python behavior changed')
    sessions, reports, seen, used, agreement = report['sessions'], report['reports'], set(), set(), {}
    require(len(sessions) == len(CASES) * 2 * 5, 'missing sessions')
    for row in sessions:
        case, condition, repetition = row['case'], row['condition'], row['repetition']
        require(case in CASES and condition in CONDITIONS and type(repetition) is int
                and 0 <= repetition < 5, 'invalid session')
        identity = case, condition, repetition
        require(identity not in seen, 'duplicate session')
        seen.add(identity)
        require(row['position'] == (CONDITIONS.index(condition) - repetition) % 2, 'condition order changed')
        launch = row['launch']
        require(launch['ready'] is True and launch['stopped'] is True
                and {'fr_ir/__init__.py', 'fr_ir/runtime.py', 'fr_ir/flow.py', 'fr_ir/flow_together.py'}
                <= set(launch['module_files']) <= set(current), 'installed process receipt missing')
        require(all(finite(launch[key]) for key in ('startup_seconds', 'shutdown_seconds', 'bootstrap_seconds'))
                and launch['bootstrap_seconds'] <= launch['startup_seconds'] <= 30
                and launch['shutdown_seconds'] <= 30, 'invalid startup cost')
        require(type(row['initial_cache_files']) is int and row['initial_cache_files'] == 0
                and type(row['cache_files']) is int and row['cache_files'] > 0
                and type(row['cache_bytes']) is int and row['cache_bytes'] > 0, 'project cache was primed or unused')
        requests = row['requests']
        require(len(requests) == len(STAGES), 'missing pair')
        for stage, request in enumerate(requests):
            label, state, names = STAGES[stage]
            require(request['stage'] == label and request['names'] == names, 'pair order changed')
            raw = reports[request['report_digest']]
            used.add(request['report_digest'])
            require(digest(raw) == request['report_digest'], 'report content changed')
            require(len(raw) == (1 if condition == 'together' else 2), 'native report count changed')
            if condition == 'together':
                group = raw[0]
                answers = group['answers']
                require(group['schema'] == 'fr-dataflow-together'
                        and type(group['startup']['checks']) is int and group['startup']['checks'] == 1
                        and type(group['startup']['questions']) is int and group['startup']['questions'] == 2
                        and group['startup']['input_digest'] == digest(group['startup']['inputs']),
                        'shared startup receipt changed')
                require(group['budget'] == {'steps': CASES[case]['steps'],
                        'used': sum(answer['execution']['analysis_steps'] for answer in answers),
                        'response_bytes': 1_048_576} and group['budget']['used'] <= CASES[case]['steps'],
                        'shared work limit changed')
            else:
                answers = raw
            require(len(answers) == 2, 'missing answer')
            files = process.sources(case, state)
            for index, (name, answer) in enumerate(zip(names, answers)):
                inputs = answer['inputs']
                require(answer['schema'] == 'fr-dataflow-1'
                        and inputs['source'] == {'path': 'app.py', 'digest': digest(files['app.py'])}
                        and inputs['selection']['name'] == name and inputs['configuration'] == digest({})
                        and inputs['rules'] == digest(json.loads(files['rules.json']) | {'propagators': []})
                        and inputs['budget'] == {'steps': CASES[case]['steps'], 'depth': 8, 'bytes': 1_048_576}
                        and inputs['context'] == 'html' and inputs['summary_mode'] is True
                        and ('modules' in inputs) is CASES[case]['imports']
                        and answer['input_digest'] == digest(inputs), 'source or explicit settings changed')
                if CASES[case]['imports']:
                    require(inputs['modules']['files'] == {path: digest(text) for path, text in files.items()
                            if path.endswith('.py')}, 'imported file changed')
                work = answer['execution']['analysis_steps']
                require(answer['execution']['kind'] == 'analyzed'
                        and type(work) is int and 0 <= work <= CASES[case]['steps']
                        and answer['budget'] == {'steps': CASES[case]['steps'], 'used': work,
                                                 'response_bytes': 1_048_576}, 'answer work changed')
                complete = case != 'unfinished-budget' and state != 'unknown'
                require(answer['complete'] is complete, 'unfinished answer disguised as finished')
                if complete:
                    require(bool(answer['witnesses']) is (name == 'forwards' or state == 'edited'),
                            'answer differs from Python behavior')
                else:
                    reason = 'step-budget' if case == 'unfinished-budget' else 'module-effects-unchecked'
                    require(reason in answer['cutoffs'], 'unfinished reason missing')
                if case == 'unfinished-budget':
                    require(work == (0 if condition == 'together' and index == 1 else 1),
                            'exhausted allocation changed')
                key = case, stage, name, condition if case == 'unfinished-budget' and index == 1 else 'both'
                require(agreement.setdefault(key, semantic(answer)) == semantic(answer),
                        'answers changed across approaches or repetitions')
                if condition == 'together':
                    shared = dict(inputs)
                    del shared['selection']
                    require(shared == group['startup']['inputs'], 'shared dependencies changed')
            calls = request['calls']
            require([call['operation'] for call in calls] == ['find', 'find'] + ['dataflow'] * (
                1 if condition == 'together' else 2), 'native calls missing')
            require(all(finite(call['seconds']) and type(call['report_json_bytes']) is int
                        and call['report_json_bytes'] > 0 for call in calls), 'native call cost missing')
            require([call['report_json_bytes'] for call in calls[2:]] == [len(json.dumps(
                value, ensure_ascii=False, separators=(',', ':')).encode()) for value in raw],
                'native report bytes changed')
            if condition == 'together':
                require(calls[2]['report_json_bytes'] + 512 <= 1_048_576, 'combined output limit exceeded')
            require(finite(request['seconds']) and finite(request['body_seconds'])
                    and sum(call['seconds'] for call in calls) <= request['body_seconds']
                    <= request['seconds'] <= 30, 'request timing boundaries changed')
        total = launch['startup_seconds'] + launch['shutdown_seconds'] + sum(r['seconds'] for r in requests)
        require(finite(row['total_seconds']) and row['total_seconds'] == total
                and finite(row['wall_seconds']) and row['wall_seconds'] >= total, 'session cost omits work')
    require(used == set(reports), 'unused reports')
    require(report['summary'] == summary(sessions), 'summary changed')


class Worker(process.Worker):
    def __init__(self, sdk, project, executable, cache):
        self.errors = tempfile.TemporaryFile()
        self.buffer = b''
        self.process = subprocess.Popen([
            sys.executable, '-I', '-B', str(ROOT / 'tools/python-together-worker.py'),
            '--sdk', str(sdk), '--project', str(project), '--fr', str(executable), '--cache', str(cache)],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.errors)
        self.selector = selectors.DefaultSelector()
        self.selector.register(self.process.stdout, selectors.EVENT_READ)


def collect(args):
    require(sys.platform == 'linux' and os.environ.get('GITHUB_ACTIONS') == 'true', 'collect on GitHub Linux')
    require(args.repetitions == 5, 'supply five repetitions')
    args.output.mkdir(parents=True, exist_ok=False)
    before = bindings()
    (args.output / 'attempt.json').write_bytes(encode({'bindings': before, 'scope': SCOPE}) + b'\n')
    with tempfile.TemporaryDirectory(prefix='fr-python-together-') as temporary:
        base, setup = Path(temporary), {}
        installed = base / 'installed'
        commands = {
            'wheel_build': [sys.executable, '-m', 'build', '--wheel', '--no-isolation',
                            str(ROOT / 'sdk/python'), '--outdir', str(args.output / 'wheel')],
            'wheel_install': None,
            'bytecode_compile': [sys.executable, '-m', 'compileall', '-q', '--invalidation-mode',
                                 'checked-hash', str(installed / 'fr_ir')],
        }
        for name in commands:
            if name == 'wheel_install':
                wheels = list((args.output / 'wheel').glob('*.whl'))
                require(len(wheels) == 1, 'need one wheel')
                commands[name] = [sys.executable, '-m', 'pip', 'install', '--no-deps', '--no-compile',
                                  '--target', str(installed), str(wheels[0])]
            started = time.perf_counter()
            subprocess.run(commands[name], check=True, timeout=120, stdout=sys.stderr,
                           env={**os.environ, 'PYTHONDONTWRITEBYTECODE': '1'})
            setup[name] = time.perf_counter() - started
        copies = {'source': process.sdk_inventory(ROOT / 'sdk/python/src', False),
                  'installed': process.sdk_inventory(installed, True)}
        require(copies['source'] == copies['installed'], 'wheel changed SDK source')
        wheel_digest = hashlib.sha256(wheels[0].read_bytes()).hexdigest()
        (args.output / 'setup.json').write_bytes(encode({
            'seconds': setup, 'sdk_files': copies, 'wheel_sha256': wheel_digest}) + b'\n')
        project, observations = base / 'project', {}
        oracle = ("import builtins,json,sys;sys.path.insert(0,sys.argv[1]);"
                  "builtins.source=lambda:17;builtins.sink=lambda value:value\n"
                  "try:\n import app\n print(json.dumps({'error':None,'returned':[app.forwards(),app.stopped()]}))\n"
                  "except NameError:\n print(json.dumps({'error':'NameError','returned':None}))\n")
        for case in CASES:
            observations[case] = {}
            for state in ('original', 'edited', 'unknown'):
                for path in project.glob('*.py'):
                    path.unlink()
                process.install(project, case, state)
                result = subprocess.run([sys.executable, '-I', '-B', '-c', oracle, str(project)],
                                        check=True, capture_output=True, text=True, timeout=20)
                observations[case][state] = json.loads(result.stdout)
        (args.output / 'python-observations.json').write_bytes(encode(observations) + b'\n')
        sessions, reports = [], {}
        for case in CASES:
            for repetition in range(5):
                offset = repetition % 2
                for position, condition in enumerate(CONDITIONS[offset:] + CONDITIONS[:offset]):
                    cache = base / 'cache' / case / str(repetition) / condition
                    require(not cache.exists(), 'cache primed')
                    row = {'case': case, 'condition': condition, 'repetition': repetition,
                           'position': position, 'requests': [], 'initial_cache_files': 0}
                    wall_started, worker = time.perf_counter(), None
                    try:
                        for label, state, names in STAGES:
                            for path in project.glob('*.py'):
                                path.unlink()
                            process.install(project, case, state)
                            if worker is None:
                                started = time.perf_counter()
                                worker = Worker(installed, project, args.fr.resolve(), cache)
                                ready = worker.read()
                                startup = time.perf_counter() - started
                                require(ready['ready'] is True, 'process not ready')
                                process.verify_modules(ready['modules'], installed, copies['installed'], True)
                                row['launch'] = {'ready': True, 'startup_seconds': startup,
                                                 'bootstrap_seconds': ready['bootstrap_seconds']}
                            question = {'condition': condition, 'names': names, 'steps': CASES[case]['steps'],
                                        'depth': 8, 'bytes': 1_048_576, 'imports': CASES[case]['imports'],
                                        'context': 'html'}
                            started = time.perf_counter()
                            worker.send(question)
                            reply = worker.read()
                            seconds = time.perf_counter() - started
                            raw = reply.pop('reports')
                            key = digest(raw)
                            reports[key] = raw
                            measured = {'stage': label, 'names': names, 'report_digest': key,
                                        'seconds': seconds, **reply}
                            row['requests'].append(measured)
                            with (args.output / 'requests.jsonl').open('ab') as stream:
                                stream.write(encode({'case': case, 'condition': condition, 'repetition': repetition,
                                                     'request': measured, 'reports': raw}) + b'\n')
                        started = time.perf_counter()
                        worker.send({'stop': True})
                        stopped = worker.read()
                        worker.process.wait(timeout=10)
                        shutdown = time.perf_counter() - started
                        require(worker.process.returncode == 0 and stopped['stopped'] is True, 'process failed at exit')
                        process.verify_modules(stopped['modules'], installed, copies['installed'], True)
                        row['launch'].update(stopped=True, shutdown_seconds=shutdown, module_files=sorted({
                            Path(item['file']).relative_to(installed).as_posix() for item in stopped['modules'].values()}))
                    finally:
                        if worker is not None:
                            worker.close()
                    row['wall_seconds'] = time.perf_counter() - wall_started
                    row['total_seconds'] = row['launch']['startup_seconds'] + row['launch']['shutdown_seconds']
                    row['total_seconds'] += sum(request['seconds'] for request in row['requests'])
                    files = [path for path in cache.rglob('*') if path.is_file()]
                    row.update(cache_files=len(files), cache_bytes=sum(path.stat().st_size for path in files))
                    sessions.append(row)
                    with (args.output / 'sessions.jsonl').open('ab') as stream:
                        stream.write(encode(row) + b'\n')
                print(f'measured {case} repetition {repetition + 1}', file=sys.stderr, flush=True)
        require(copies['installed'] == process.sdk_inventory(installed, True), 'installed SDK changed')
    require(before == bindings(), 'sources changed')
    report = {'schema': 'fr-python-together-cost', 'bindings': before, 'scope': SCOPE, 'cases': CASES,
              'stages': json.loads(encode(STAGES)), 'conditions': list(CONDITIONS), 'repetitions': 5,
              'sdk_files': copies, 'setup_seconds': setup, 'wheel_sha256': wheel_digest,
              'binary_sha256': hashlib.sha256(args.fr.read_bytes()).hexdigest(),
              'environment': {'build': process.BUILD, 'python': sys.version, 'platform': platform.platform(),
                              'rustc': subprocess.run(['rustc', '--version'], check=True, capture_output=True,
                                                      text=True, timeout=10).stdout.strip(),
                              'cpu_affinity': sorted(os.sched_getaffinity(0)),
                              'visible_memory_kib': int(next(line.split()[1] for line in
                                                            Path('/proc/meminfo').read_text().splitlines()
                                                            if line.startswith('MemTotal:'))),
                              'cpu_model': next(line.split(':', 1)[1].strip() for line in
                                                Path('/proc/cpuinfo').read_text().splitlines()
                                                if line.startswith('model name'))},
              'python_observations': observations, 'sessions': sessions, 'reports': reports,
              'summary': summary(sessions)}
    (args.output / 'result.json').write_bytes(encode(report) + b'\n')
    audit(report)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument('--output', type=Path)
    mode.add_argument('--audit', type=Path)
    parser.add_argument('--fr', type=Path)
    parser.add_argument('--repetitions', type=int)
    args = parser.parse_args()
    if args.audit:
        audit(json.loads(args.audit.read_bytes()))
        print('checked all paired answers, edits, allocations and process costs')
    else:
        require(args.fr is not None and args.repetitions is not None, 'supply --fr and --repetitions')
        output_existed = args.output.exists()
        try:
            collect(args)
        except Exception:
            if not output_existed and args.output.is_dir():
                (args.output / 'error.txt').write_text(traceback.format_exc())
            raise


if __name__ == '__main__':
    main()
