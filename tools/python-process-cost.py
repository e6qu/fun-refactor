#!/usr/bin/env python3
"""Compare SDK source and an installed wheel, with fresh and running Python."""
import argparse
import hashlib
import importlib.util
import json
import math
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

from evidence_basis import native_bindings
from python_repository_basis import file_digest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'tests/agent-eval/python-startup-cost'))
from corpus import CASES, STAGES, expected, install, sources

CONDITIONS = ('source-fresh', 'installed-fresh', 'source-running', 'installed-running')
BUILD = 'cargo build --locked --release --bin fr'
BINDINGS = ('tools/python-process-cost.py', 'tools/python-process-worker.py',
            'tools/test-python-process-cost.py', 'tools/evidence_basis.py',
            'tools/python_repository_basis.py', 'sdk/python/pyproject.toml',
            'tests/agent-eval/python-startup-cost/corpus.py',
            '.github/workflows/python-process-cost.yml')
SCOPE = {
    'baseline': 'Fresh analysis, ordinary optimized fr CLI, project cache enabled and initially empty per condition/case/repetition. No saved startup or whole-answer store. Same seven questions and explicit limits in every condition.',
    'sdk': 'Same interpreter and SDK source. Source has no SDK bytecode and cannot write it. Installed wheel has checked-hash bytecode compiled before measurement. Both use isolated Python with -I -B; -B still permits reading existing bytecode. Standard-library bytecode and OS file caches remain uncontrolled.',
    'total': 'Sum of parent process-start-to-ready, each submitted-question-through-parsed-reply interval, and stop-through-process-exit. Includes interpreter startup, imports, arguments, native calls, SDK validation, instrumentation and JSON output. Excludes controller fixture edits, source/bytecode verification and evidence persistence between requests. Wall time also records these controller costs. No uncounted analysis priming.',
    'phases': 'Worker base imports, argument parsing, SDK imports and request body have separate internal timers. They exclude some startup and output. Parent startup minus bootstrap is a residual containing interpreter startup, output and scheduling; it is not pure interpreter or SDK cost. Each request starts a new native process for find and dataflow even in a running Python program.',
    'setup': 'Wheel build, install and checked-hash bytecode compilation have separate recorded one-time costs. They are outside recurring question totals. This is an installed wheel selected by explicit sys.path in the same interpreter, not a virtual-environment activation comparison.',
    'claims': 'Disclosed development examples, five repetitions. Unknown startup and exhausted analysis limits must stay unfinished. No unfamiliar-library acceptance, memory saving, ordinary-Python translation overhead or agent-task speedup claim.',
}


def encode(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':'), allow_nan=False).encode()


def digest(value):
    return hashlib.sha256(encode(value)).hexdigest()


def require(condition, message):
    if not condition:
        raise ValueError(message)


def bindings():
    sdk = [path.relative_to(ROOT).as_posix() for path in sorted((ROOT / 'sdk/python/src').rglob('*.py'))]
    return native_bindings(ROOT, BINDINGS) | {name: file_digest(ROOT / name) for name in sdk}


def finite(value):
    return type(value) in (int, float) and math.isfinite(value) and value >= 0


def sdk_inventory(root, compiled):
    result = {}
    for path in sorted((root / 'fr_ir').rglob('*.py')):
        cached = Path(importlib.util.cache_from_source(str(path)))
        if compiled:
            data = cached.read_bytes()
            require(data[:4] == importlib.util.MAGIC_NUMBER and int.from_bytes(data[4:8], 'little') == 3
                    and data[8:16] == importlib.util.source_hash(path.read_bytes()),
                    'installed SDK bytecode is missing, stale or not checked-hash')
        else:
            require(not cached.exists(), 'source condition already has SDK bytecode')
        result[path.relative_to(root).as_posix()] = hashlib.sha256(path.read_bytes()).hexdigest()
    require(bool(result), 'empty SDK copy')
    return result


def verify_modules(modules, root, files, compiled):
    require({'fr_ir', 'fr_ir.flow', 'fr_ir.runtime'} <= set(modules), 'SDK imports missing')
    for item in modules.values():
        path = Path(item['file']).resolve()
        require(path.is_relative_to(root.resolve()), 'worker loaded a different SDK copy')
        relative = path.relative_to(root.resolve()).as_posix()
        require(relative in files, 'worker loaded unrecorded SDK source')
        require(item['cached'] == importlib.util.cache_from_source(str(path)), 'unexpected bytecode location')
        require(Path(item['cached']).exists() is compiled, 'wrong SDK bytecode condition')


class Worker:
    def __init__(self, sdk, project, executable, cache):
        self.errors = tempfile.TemporaryFile()
        self.buffer = b''
        self.process = subprocess.Popen([
            sys.executable, '-I', '-B', str(ROOT / 'tools/python-process-worker.py'),
            '--sdk', str(sdk), '--project', str(project), '--fr', str(executable), '--cache', str(cache)],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.errors)
        self.selector = selectors.DefaultSelector()
        self.selector.register(self.process.stdout, selectors.EVENT_READ)

    def read(self):
        deadline = time.monotonic() + 30
        while b'\n' not in self.buffer:
            remaining = deadline - time.monotonic()
            require(remaining > 0 and self.selector.select(remaining), 'worker reply exceeded 30 seconds')
            chunk = os.read(self.process.stdout.fileno(), 65_536)
            if not chunk:
                self.errors.seek(0)
                raise ValueError('worker closed its reply pipe: ' + self.errors.read().decode())
            self.buffer += chunk
            require(len(self.buffer) <= 2_097_152, 'worker reply exceeds 2 MiB')
        line, self.buffer = self.buffer.split(b'\n', 1)
        return json.loads(line)

    def send(self, value):
        self.process.stdin.write(encode(value) + b'\n')
        self.process.stdin.flush()

    def close(self):
        if self.process.poll() is None:
            self.process.kill()
        self.process.wait(timeout=10)
        self.process.stdin.close()
        self.process.stdout.close()
        self.selector.close()
        self.errors.close()


def summary(sessions):
    return {case: {condition: {
        'samples_seconds': [row['total_seconds'] for row in sessions
                            if (row['case'], row['condition']) == (case, condition)],
        'median_seconds': statistics.median(row['total_seconds'] for row in sessions
                                            if (row['case'], row['condition']) == (case, condition))}
        for condition in CONDITIONS} for case in CASES}


def audit(report):
    require(report['schema'] == 'fr-python-process-cost', 'unexpected report format')
    require(report['bindings'] == bindings(), 'measurement sources changed; collect again')
    require(report['scope'] == SCOPE and report['cases'] == CASES
            and report['stages'] == json.loads(encode(STAGES)), 'measurement scope changed')
    require(report['repetitions'] == 5 and report['conditions'] == list(CONDITIONS), 'conditions changed')
    require(report['environment']['build'] == BUILD, 'optimized build changed')
    require(len(report['binary_sha256']) == 64 and all(c in '0123456789abcdef'
            for c in report['binary_sha256']), 'invalid binary identity')
    copies = report['sdk_files']
    require(copies['source'] == copies['installed'] and bool(copies['source']), 'SDK copies differ')
    require(all(isinstance(value, str) and len(value) == 64 and all(c in '0123456789abcdef' for c in value)
                for value in copies['source'].values()), 'invalid SDK source digest')
    current = {path.relative_to(ROOT / 'sdk/python/src').as_posix(): hashlib.sha256(path.read_bytes()).hexdigest()
               for path in (ROOT / 'sdk/python/src/fr_ir').rglob('*.py')}
    require(set(copies['source']) == set(current) and all(
        name == 'fr_ir/_version.py' or copies['source'][name] == value for name, value in current.items()),
        'SDK source receipt changed')
    require(set(report['setup_seconds']) == {'wheel_build', 'wheel_install', 'bytecode_compile'}
            and all(finite(value) for value in report['setup_seconds'].values()), 'installation cost missing')
    sessions, answers, seen, agreement = report['sessions'], report['answers'], set(), {}
    require(len(sessions) == len(CASES) * len(CONDITIONS) * 5, 'missing or extra sessions')
    used = set()
    for row in sessions:
        case, condition, repetition = row['case'], row['condition'], row['repetition']
        require(case in CASES and condition in CONDITIONS and type(repetition) is int
                and 0 <= repetition < 5, 'invalid session identity')
        identity = case, condition, repetition
        require(identity not in seen, 'duplicate session')
        seen.add(identity)
        require(row['position'] == (CONDITIONS.index(condition) - repetition) % len(CONDITIONS),
                'condition order changed')
        launches = row['launches']
        require(len(launches) == (1 if condition.endswith('running') else len(STAGES)), 'process count changed')
        for launch in launches:
            require(launch['ready'] is True and launch['stopped'] is True, 'process did not complete')
            require(set(launch['timing']) == {'base_import_seconds', 'arguments_seconds',
                    'sdk_import_seconds', 'bootstrap_seconds'}, 'startup phases missing')
            require(all(finite(value) for value in launch['timing'].values())
                    and finite(launch['startup_seconds']) and finite(launch['shutdown_seconds']), 'invalid startup timing')
            require(sum(launch['timing'][name] for name in ('base_import_seconds', 'arguments_seconds',
                    'sdk_import_seconds')) <= launch['timing']['bootstrap_seconds']
                    <= launch['startup_seconds'] <= 30, 'startup timing boundaries changed')
            require(launch['shutdown_seconds'] <= 30, 'shutdown exceeded deadline')
            require({'fr_ir/__init__.py', 'fr_ir/flow.py', 'fr_ir/runtime.py'} <= set(launch['module_files'])
                    <= set(copies['source'])
                    and launch['sdk_copy'] == condition.split('-')[0]
                    and launch['bytecode'] is condition.startswith('installed'), 'SDK import receipt changed')
        requests = row['requests']
        require(len(requests) == len(STAGES), 'question missing')
        for stage, request in enumerate(requests):
            require(request['stage'] == STAGES[stage][0], 'question order changed')
            answer = answers[request['answer_digest']]
            used.add(request['answer_digest'])
            require(digest(answer) == request['answer_digest'], 'answer content changed')
            wanted = expected(case, stage)
            require(answer['complete'] is wanted['complete'], 'completion changed')
            inputs = answer['inputs']
            files = sources(case, STAGES[stage][2])
            require(inputs['source'] == {'path': 'app.py', 'digest': digest(files['app.py'])}
                    and inputs['configuration'] == digest({})
                    and inputs['selection']['name'] == STAGES[stage][1]
                    and inputs['budget'] == {'steps': CASES[case]['steps'], 'depth': 8, 'bytes': 1_048_576}
                    and inputs['context'] == 'html' and inputs['summary_mode'] is True
                    and ('modules' in inputs) is CASES[case]['imports']
                    and answer['input_digest'] == digest(inputs), 'question or analysis settings changed')
            if CASES[case]['imports']:
                require(inputs['modules']['files'] == {name: digest(text) for name, text in files.items()
                        if name.endswith('.py')}, 'imported sources changed')
            if wanted['complete']:
                require(bool(answer['witnesses']) is (wanted['python']['returned'] == 17), 'flow result changed')
            else:
                reason = 'step-budget' if case == 'unfinished-budget' else 'module-effects-unchecked'
                require(reason in answer['cutoffs'], 'unfinished reason changed')
            question = case, stage
            require(agreement.setdefault(question, request['answer_digest']) == request['answer_digest'],
                    'SDK copy or process lifetime changed answers')
            require(finite(request['seconds']) and finite(request['body_seconds'])
                    and request['body_seconds'] <= request['seconds'] <= 30, 'invalid request timing')
            calls = request['calls']
            require([call['operation'] for call in calls] == ['find', 'dataflow']
                    and all(finite(call['seconds']) for call in calls)
                    and sum(call['seconds'] for call in calls) <= request['body_seconds'], 'native work missing')
            require(type(request['analysis_steps']) is int and request['analysis_steps'] > 0,
                    'analysis work missing')
        total = sum(item['startup_seconds'] + item['shutdown_seconds'] for item in launches)
        total += sum(item['seconds'] for item in requests)
        require(finite(row['total_seconds']) and row['total_seconds'] == total
                and finite(row['wall_seconds']) and row['wall_seconds'] >= total, 'session cost omits work')
        require(type(row['cache_files']) is int and row['cache_files'] > 0
                and type(row['cache_bytes']) is int and row['cache_bytes'] > 0, 'project cache did not populate')
        require(row['initial_cache_files'] == 0, 'project cache was primed')
    require(used == set(answers), 'unused answers')
    require(report['summary'] == summary(sessions), 'summary changed')


def collect(args):
    require(sys.platform == 'linux' and os.environ.get('GITHUB_ACTIONS') == 'true', 'collect on GitHub Linux')
    require(args.repetitions == 5, 'supply five repetitions')
    args.output.mkdir(parents=True, exist_ok=False)
    before = bindings()
    (args.output / 'attempt.json').write_bytes(encode({'bindings': before, 'scope': SCOPE}) + b'\n')
    with tempfile.TemporaryDirectory(prefix='fr-python-process-') as temporary:
        base = Path(temporary)
        installed = base / 'installed'
        setup = {}
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
                require(len(wheels) == 1, 'need exactly one built wheel')
                commands[name] = [sys.executable, '-m', 'pip', 'install', '--no-deps', '--no-compile',
                                  '--target', str(installed), str(wheels[0])]
            started = time.perf_counter()
            subprocess.run(commands[name], check=True, timeout=120, stdout=sys.stderr,
                           env={**os.environ, 'PYTHONDONTWRITEBYTECODE': '1'})
            setup[name] = time.perf_counter() - started
        sdk_roots = {'source': ROOT / 'sdk/python/src', 'installed': installed}
        copies = {name: sdk_inventory(path, name == 'installed') for name, path in sdk_roots.items()}
        require(copies['source'] == copies['installed'], 'wheel changed SDK source')
        wheel_digest = hashlib.sha256(wheels[0].read_bytes()).hexdigest()
        (args.output / 'setup.json').write_bytes(encode({
            'seconds': setup, 'sdk_files': copies, 'wheel_sha256': wheel_digest}) + b'\n')
        sessions, answers = [], {}
        project = base / 'project'
        for case in CASES:
            for repetition in range(5):
                offset = repetition % len(CONDITIONS)
                order = CONDITIONS[offset:] + CONDITIONS[:offset]
                for position, condition in enumerate(order):
                    sdk_name = condition.split('-')[0]
                    sdk, running = sdk_roots[sdk_name], condition.endswith('running')
                    cache = base / 'cache' / case / str(repetition) / condition
                    require(not cache.exists(), 'cache was primed')
                    row = {'case': case, 'condition': condition, 'repetition': repetition,
                           'position': position, 'launches': [], 'requests': [], 'initial_cache_files': 0}
                    wall_started = time.perf_counter()
                    worker = None
                    try:
                        for stage, (name, function, state) in enumerate(STAGES):
                            for path in project.glob('*.py'):
                                path.unlink()
                            install(project, case, state)
                            if worker is None:
                                started = time.perf_counter()
                                worker = Worker(sdk, project, args.fr.resolve(), cache)
                                ready = worker.read()
                                startup = time.perf_counter() - started
                                require(ready['ready'] is True, 'worker did not become ready')
                                verify_modules(ready['modules'], sdk, copies[sdk_name], sdk_name == 'installed')
                                launch = {'ready': True, 'timing': ready['timing'], 'startup_seconds': startup,
                                          'sdk_copy': sdk_name, 'bytecode': sdk_name == 'installed'}
                            request = {'function': function, 'steps': CASES[case]['steps'], 'depth': 8,
                                       'bytes': 1_048_576, 'imports': CASES[case]['imports']}
                            started = time.perf_counter()
                            worker.send(request)
                            reply = worker.read()
                            seconds = time.perf_counter() - started
                            answer = reply.pop('answer')
                            key = digest(answer)
                            answers[key] = answer
                            measured = {'stage': name, 'answer_digest': key, 'seconds': seconds, **reply}
                            row['requests'].append(measured)
                            with (args.output / 'requests.jsonl').open('ab') as stream:
                                stream.write(encode({'case': case, 'condition': condition, 'repetition': repetition,
                                                     'request': measured, 'answer': answer}) + b'\n')
                            if not running or stage == len(STAGES) - 1:
                                started = time.perf_counter()
                                worker.send({'stop': True})
                                stopped = worker.read()
                                worker.process.wait(timeout=10)
                                shutdown = time.perf_counter() - started
                                require(worker.process.returncode == 0 and stopped['stopped'] is True,
                                        'worker did not stop cleanly')
                                verify_modules(stopped['modules'], sdk, copies[sdk_name], sdk_name == 'installed')
                                launch.update(stopped=True, shutdown_seconds=shutdown,
                                              module_files=sorted({Path(item['file']).relative_to(sdk).as_posix()
                                                                   for item in stopped['modules'].values()}))
                                row['launches'].append(launch)
                                worker.close()
                                worker = None
                    finally:
                        if worker is not None:
                            worker.close()
                    row['wall_seconds'] = time.perf_counter() - wall_started
                    row['total_seconds'] = sum(item['startup_seconds'] + item['shutdown_seconds']
                                               for item in row['launches'])
                    row['total_seconds'] += sum(item['seconds'] for item in row['requests'])
                    files = [path for path in cache.rglob('*') if path.is_file()]
                    row.update(cache_files=len(files), cache_bytes=sum(path.stat().st_size for path in files))
                    sessions.append(row)
                    with (args.output / 'sessions.jsonl').open('ab') as stream:
                        stream.write(encode(row) + b'\n')
                print(f'measured {case} repetition {repetition + 1}', file=sys.stderr, flush=True)
        require(copies == {name: sdk_inventory(path, name == 'installed')
                           for name, path in sdk_roots.items()}, 'SDK changed during collection')
    require(before == bindings(), 'measurement sources changed')
    report = {'schema': 'fr-python-process-cost', 'bindings': before, 'scope': SCOPE,
              'cases': CASES, 'stages': json.loads(encode(STAGES)), 'conditions': list(CONDITIONS),
              'repetitions': 5, 'sdk_files': copies, 'setup_seconds': setup,
              'wheel_sha256': wheel_digest,
              'binary_sha256': hashlib.sha256(args.fr.read_bytes()).hexdigest(),
              'environment': {'build': BUILD, 'python': sys.version, 'platform': platform.platform(),
                              'python_cache_tag': sys.implementation.cache_tag,
                              'rustc': subprocess.run(['rustc', '--version'], check=True, capture_output=True,
                                                      text=True, timeout=10).stdout.strip(),
                              'cpu_count': os.cpu_count(), 'cpu_affinity': sorted(os.sched_getaffinity(0)),
                              'visible_memory_kib': int(next(line.split()[1] for line in
                                                            Path('/proc/meminfo').read_text().splitlines()
                                                            if line.startswith('MemTotal:'))),
                              'cpu_model': next(line.split(':', 1)[1].strip() for line in
                                                Path('/proc/cpuinfo').read_text().splitlines()
                                                if line.startswith('model name'))},
              'sessions': sessions, 'answers': answers, 'summary': summary(sessions)}
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
        print('checked every process, question, source change, budget and cross-condition answer')
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
