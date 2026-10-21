#!/usr/bin/env python3
"""Measure fresh questions, saved Python preparation and saved whole answers."""
from __future__ import annotations

import argparse
from contextlib import contextmanager
import copy
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import resource
import statistics
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / 'tests/agent-eval/python-startup-cost'
sys.path[:0] = [str(ROOT / 'sdk/python/src'), str(FIXTURE)]
from corpus import CASES, STAGES, STRATEGIES, expected, install, sources
from evidence_basis import native_bindings
from python_repository_basis import file_digest as python_file_digest
from fr_ir.context import DirectoryObjectStore
from fr_ir.flow import FlowAnalysis, FlowCache
from fr_ir.flow_startup import StartupCache
from fr_ir.runtime import FrClient, FrReport

BINDINGS = ('tools/python-startup-cost.py', 'tools/test-python-startup-cost.py',
            'tools/evidence_basis.py', 'tools/python_repository_basis.py',
            'tests/agent-eval/python-startup-cost/corpus.py',
            'tools/project-profile.rs',
            '.github/workflows/python-startup-cost.yml')
FIELDS = ('elapsed_seconds', 'worker_seconds', 'native_seconds', 'store_seconds',
          'native_calls', 'canonical_reply_bytes', 'store_gets', 'store_puts',
          'store_read_bytes', 'store_offered_bytes', 'added_files', 'added_bytes',
          'stored_files', 'stored_bytes', 'analysis_steps',
          'worker_peak_rss_bytes', 'native_peak_rss_bytes',
          'find_seconds', 'inputs_seconds', 'answer_seconds',
          'project_cache_files', 'project_cache_bytes',
          'project_cache_added_files', 'project_cache_added_bytes')
ENGINES = ('cli', 'profile')
CACHE_MODES = ('disabled', 'enabled')
PHASES = {'root', 'scan', 'cache_open', 'index', 'project', 'query', 'verify', 'serialize', 'drop'}
CONSTRUCTION = {'manifests', 'setup', 'source_read', 'source_digest', 'source_lines',
                'hierarchy', 'symbol_digest', 'reference_digest', 'finish'}
BUILD = 'cargo build --locked --release --bin fr --example project-profile'
SCOPE = {
    'elapsed_seconds': 'Parent timer around a new Python worker through parsed JSON reply. Includes Python/SDK startup, finding the function, native process startup, file checks, import resolution, parsing, analysis or validation, saved-record reads/writes and manifest pointer I/O.',
    'excluded': 'Checkout, compilation, dependency installation, fixture edits, ordinary Python observation, store inventory, report comparison and audit. Worker reply serialization/transfer, shared measurement imports and instrumentation are included. Each sequence explicitly enables or disables the project cache, isolated by approach and repetition and initially empty. No uncounted priming. OS file caches are neither flushed nor controlled.',
    'attribution': 'Ordinary CLI calls time finding the function, input identity and answering separately, including client parsing. Separate profiling sequences use the existing library profiler with construction timers. Their phase times exclude process startup, argument parsing and outer reply output. The profiling client constructs the current required project identity before strict SDK validation, outside native phase timers. Index includes extraction or cached facts and resolution; query includes import loading, parsing and analysis or validation, without separate import/parser timers. Profiling totals are diagnostic, not CLI latency.',
    'bytes': 'Canonical JSON reply bytes, not exact wire bytes. Profiling counts the adapted SDK report including its constructed project identity, excluding the diagnostic envelope. Store reads and offered writes count repeated object operations. Added bytes count database growth; offered writes can verify existing objects. Manifest pointer bytes are outside object totals but their I/O is timed. Temporary native reuse files are timed but not retained in the object inventory.',
    'memory': 'Separate worker and largest-child lifetime RSS high-water marks, not sampled aggregate RSS. Python runtime and fr are separate processes.',
    'python': 'Ordinary Python executes the same selected function with source returning 17 and sink returning its argument. It checks one concrete result, not all input-flow paths. Its elapsed time is contextual, not an equivalent analysis baseline.',
    'claims': 'Optimized build, disclosed synthetic development examples only. No unfamiliar package, agent task, paid study, full-package acceptance or general speedup claim. The earlier development-build collection is historical, not a controlled build-profile comparison.',
}


def encode(value):
    return json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(',', ':'), allow_nan=False).encode()


def digest(value):
    return hashlib.sha256(encode(value)).hexdigest()


def bindings():
    sdk = [str(path.relative_to(ROOT)) for path in sorted((ROOT / 'sdk/python/src').rglob('*.py'))]
    return native_bindings(ROOT, BINDINGS) | {name: python_file_digest(ROOT / name) for name in sdk}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def evidence(value):
    answer = copy.deepcopy(value)
    del answer['execution']
    answer.pop('context_basis', None)
    return answer


def inventory(root):
    files = [path for path in (root / 'objects').rglob('*') if path.is_file()]
    return len(files), sum(path.stat().st_size for path in files)


def project_inventory(root):
    files = [path for path in root.rglob('*') if path.is_file()]
    return len(files), sum(path.stat().st_size for path in files)


class MeasuredStore:
    def __init__(self, root):
        self.store = DirectoryObjectStore(root)
        self.gets = self.puts = self.read_bytes = self.offered_bytes = 0
        self.seconds = 0.0

    @contextmanager
    def read(self):
        started = time.perf_counter()
        try:
            with self.store.read() as fetch:
                self.seconds += time.perf_counter() - started
                try:
                    yield lambda key: self._get(fetch, key)
                finally:
                    started = time.perf_counter()
        finally:
            self.seconds += time.perf_counter() - started

    def _get(self, fetch, key):
        self.gets += 1
        started = time.perf_counter()
        try:
            value = fetch(key)
            if value is not None:
                self.read_bytes += len(encode(value))
            return value
        finally:
            self.seconds += time.perf_counter() - started

    def put_many(self, records):
        self.puts += len(records)
        started = time.perf_counter()
        try:
            self.offered_bytes += sum(len(encode(value)) for value in records.values())
            self.store.put_many(records)
        finally:
            self.seconds += time.perf_counter() - started


class MeasuredClient(FrClient):
    def __init__(self, project, executable, engine, cache_mode):
        super().__init__(project, executable=executable, timeout=20, max_output_bytes=1_048_576)
        self.calls = self.reply_bytes = 0
        self.seconds = 0.0
        self.engine, self.cache_mode = engine, cache_mode
        self.requests = []

    def project(self, *arguments):
        started = time.perf_counter()
        cache_args = ['--no-cache'] if self.cache_mode == 'disabled' else []
        profile = None
        if self.engine == 'cli':
            report = self.call(*cache_args, 'project', *arguments)
        else:
            process = subprocess.run([self.executable, '-C', str(self.root), *cache_args,
                                      '--construction', *arguments],
                                     capture_output=True, timeout=self.timeout, check=True)
            require(len(process.stdout) <= self.max_output_bytes
                    and len(process.stderr) <= self.max_output_bytes, 'profiler output exceeds budget')
            profile = json.loads(process.stdout)
            value = json.loads(profile.pop('report_stdout'))
            require('context_basis' not in value, 'profiler unexpectedly packaged a CLI project identity')
            value['context_basis'] = 'frcb1:' + digest([
                'fr-context-1', value['revision'], value['handle_prefix'], value['coverage']])
            report = FrReport(value, ('project', *arguments))
        elapsed = time.perf_counter() - started
        self.seconds += elapsed
        self.calls += 1
        reply_bytes = len(encode(report.to_data()))
        self.reply_bytes += reply_bytes
        operation = 'find' if arguments[0] == 'find' else 'inputs' if '--inputs-only' in arguments else 'answer'
        self.requests.append({'operation': operation, 'seconds': elapsed,
                              'reply_bytes': reply_bytes, 'profile': profile})
        return report


def worker(args):
    started = time.perf_counter()
    # Every child uses this explicit location; it is outside the scanned project.
    os.environ['FUN_REFACTOR_CACHE'] = str(args.work / 'project-cache')
    client = MeasuredClient(args.project, str(args.fr.resolve()), args.engine, args.project_cache)
    target = client.project('find', args.function).definition_target().handle
    settings = CASES[args.case]
    options = dict(rules=args.project / 'rules.json', context='html', steps=settings['steps'],
                   depth=8, max_bytes=1_048_576, imports=settings['imports'])
    startup_checks = None
    startup_reused = answer_reused = False
    store = None
    if args.strategy == 'fresh':
        arguments = ['dataflow', target, '--rules', str(options['rules']), '--context', 'html',
                     '--steps', str(settings['steps']), '--depth', '8', '--bytes', '1048576', '--summaries']
        if settings['imports']:
            arguments.append('--imports')
        result = FlowAnalysis(client.project(*arguments))
        result.initialization
        result.graphs
        result.witnesses
        if settings['imports']:
            result.dependencies
        if result.report.at('/function_summaries'):
            result.summaries
    else:
        store = MeasuredStore(args.work / 'objects')
        cache_type = StartupCache if args.strategy == 'saved-startup' else FlowCache
        pointer = args.work / 'head'
        cache = cache_type.restore(store, pointer.read_text()) if pointer.exists() else cache_type(store)
        if args.strategy == 'saved-startup':
            prepared = cache.analyze(client, target, standard_library=(), **options)
            result = prepared.answer
            startup_checks, startup_reused = prepared.checks, prepared.reused
        else:
            result = cache.analyze(client, target, standard_library=(), summaries=True, **options)
            answer_reused = result.reused
        pointer.write_text(cache.persist())
    value = result.report.to_data()
    elapsed = time.perf_counter() - started
    scale = 1 if sys.platform == 'darwin' else 1024
    return {'answer': evidence(value), 'startup_reused': startup_reused,
            'answer_reused': answer_reused, 'startup_checks': startup_checks,
            'calls': client.requests,
            'metrics': {'worker_seconds': elapsed, 'native_seconds': client.seconds,
                        **{name + '_seconds': sum(call['seconds'] for call in client.requests
                                                  if call['operation'] == name)
                           for name in ('find', 'inputs', 'answer')},
                        'store_seconds': 0.0 if store is None else store.seconds,
                        'native_calls': client.calls, 'canonical_reply_bytes': client.reply_bytes,
                        'store_gets': 0 if store is None else store.gets,
                        'store_puts': 0 if store is None else store.puts,
                        'store_read_bytes': 0 if store is None else store.read_bytes,
                        'store_offered_bytes': 0 if store is None else store.offered_bytes,
                        'analysis_steps': value['execution']['analysis_steps'],
                        'worker_peak_rss_bytes': resource.getrusage(resource.RUSAGE_SELF).ru_maxrss * scale,
                        'native_peak_rss_bytes': resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss * scale}}


def invoke(args, project, work, case, function, strategy):
    before = inventory(work)
    cache_before = project_inventory(work / 'project-cache')
    command = [sys.executable, '-B', str(Path(__file__).resolve()), '--worker', '--fr', str(args.fr.resolve()),
               '--project', str(project), '--work', str(work), '--case', case, '--function', function,
               '--strategy', strategy, '--engine', args.engine, '--project-cache', args.project_cache]
    started = time.perf_counter()
    process = subprocess.run(command, capture_output=True, text=True, timeout=30)
    require(process.returncode == 0, f'worker failed ({case}/{function}/{strategy}): {process.stderr}')
    result = json.loads(process.stdout)
    elapsed = time.perf_counter() - started
    after = inventory(work)
    cache_after = project_inventory(work / 'project-cache')
    result['metrics'].update(elapsed_seconds=elapsed, stored_files=after[0], stored_bytes=after[1],
                             added_files=after[0] - before[0], added_bytes=after[1] - before[1],
                             project_cache_files=cache_after[0], project_cache_bytes=cache_after[1],
                             project_cache_added_files=cache_after[0] - cache_before[0],
                             project_cache_added_bytes=cache_after[1] - cache_before[1])
    return result


def observe(project, function):
    program = ("import sys,json,builtins; sys.path.insert(0,sys.argv[1]); "
               "builtins.source=lambda:17; builtins.sink=lambda value:value; "
               "exec(\"try:\\n import app\\n value={'error':None,'returned':getattr(app,sys.argv[2])()}"
               "\\nexcept Exception as error:\\n value={'error':type(error).__name__,'returned':None}\"); "
               "print(json.dumps(value))")
    started = time.perf_counter()
    process = subprocess.run([sys.executable, '-I', '-B', '-c', program, str(project), function],
                             check=True, capture_output=True, text=True, timeout=20)
    return {'result': json.loads(process.stdout), 'elapsed_seconds': time.perf_counter() - started}


def summarize(rows):
    result = {}
    for case in CASES:
        result[case] = {}
        for stage, _, _ in STAGES:
            result[case][stage] = {}
            for strategy in STRATEGIES:
                selected = [row for row in rows if (row['case'], row['stage'], row['strategy']) == (case, stage, strategy)]
                result[case][stage][strategy] = {
                    name: {'median': statistics.median(row['metrics'][name] for row in selected),
                           'min': min(row['metrics'][name] for row in selected),
                           'max': max(row['metrics'][name] for row in selected)} for name in FIELDS}
    return result


def sequence_totals(rows, repetitions):
    result = {}
    for case in CASES:
        result[case] = {}
        for strategy in STRATEGIES:
            values = [sum(row['metrics']['elapsed_seconds'] for row in rows
                          if (row['case'], row['strategy'], row['repetition']) == (case, strategy, repetition))
                      for repetition in range(repetitions)]
            result[case][strategy] = {'samples_seconds': values, 'median_seconds': statistics.median(values)}
    return result


def measure(args):
    require(args.repetitions == 5 and args.profile_repetitions == 3,
            'use five CLI repetitions and three separate profiling repetitions')
    require(sys.platform == 'linux', 'collect on a Linux GitHub runner')
    before = bindings()
    args.output.mkdir(parents=True, exist_ok=False)
    reports, settings, observations = {}, {}, []
    for engine in ENGINES:
        for mode in CACHE_MODES:
            condition = argparse.Namespace(**vars(args))
            condition.engine, condition.project_cache = engine, mode
            condition.fr = args.fr if engine == 'cli' else args.profiler
            condition.repetitions = args.repetitions if engine == 'cli' else args.profile_repetitions
            condition.output = args.output / f'{engine}-{mode}'
            condition.output.mkdir()
            settings[engine, mode] = condition
            reports[engine, mode] = {'samples': [], 'answers': {}}
            (condition.output / 'attempt.json').write_bytes(encode({
                'bindings': before, 'repetitions': condition.repetitions, 'engine': engine,
                'project_cache': mode, 'scope': SCOPE}) + b'\n')
    with tempfile.TemporaryDirectory(prefix='fr-startup-cost-') as directory:
        base = Path(directory)
        project = base / 'project'
        for case in CASES:
            for repetition in range(args.repetitions):
                offset = repetition % len(STRATEGIES)
                order = STRATEGIES[offset:] + STRATEGIES[:offset]
                engines = ENGINES if repetition % 2 == 0 else ENGINES[::-1]
                modes = CACHE_MODES if repetition % 2 == 0 else CACHE_MODES[::-1]
                for engine in engines:
                    for mode_position, mode in enumerate(modes):
                        condition = settings[engine, mode]
                        if repetition >= condition.repetitions:
                            continue
                        report = reports[engine, mode]
                        for position, strategy in enumerate(order):
                            for stage, (name, function, state) in enumerate(STAGES):
                                # Remove the previous case's imported helper before installing this case.
                                for path in project.glob('*.py'):
                                    path.unlink()
                                install(project, case, state)
                                work = base / f'{engine}-{mode}' / case / str(repetition) / strategy
                                result = invoke(condition, project, work, case, function, strategy)
                                answer = result.pop('answer')
                                key = digest(answer)
                                report['answers'][key] = answer
                                row = {'case': case, 'repetition': repetition, 'position': position,
                                       'cache_position': mode_position, 'stage': name,
                                       'strategy': strategy, 'answer_digest': key, **result}
                                report['samples'].append(row)
                                with (condition.output / 'requests.jsonl').open('ab') as stream:
                                    stream.write(encode({'request': row, 'answer': answer}) + b'\n')
                print(f'measured {case} repetition {repetition + 1}', file=sys.stderr, flush=True)
            for name, function, state in STAGES:
                for path in project.glob('*.py'):
                    path.unlink()
                install(project, case, state)
                observations.append({'case': case, 'stage': name, **observe(project, function)})
    require(before == bindings(), 'measurement sources changed')
    environment = {'platform': platform.platform(), 'python': sys.version,
                              'cpu_model': next(line.split(':', 1)[1].strip() for line in
                                                Path('/proc/cpuinfo').read_text().splitlines()
                                                if line.startswith('model name')),
                              'cpu_count': os.cpu_count(), 'cpu_affinity': sorted(os.sched_getaffinity(0)),
                              'visible_memory_kib': int(next(line.split()[1] for line in
                                                            Path('/proc/meminfo').read_text().splitlines()
                                                            if line.startswith('MemTotal:'))),
                              'rustc': subprocess.run(['rustc', '--version'], check=True, capture_output=True,
                                                      text=True, timeout=10).stdout.strip(),
                              'build': BUILD}
    for (engine, mode), report in reports.items():
        condition = settings[engine, mode]
        report.update(schema='fr-python-startup-cost', bindings=before,
                      binary_sha256=hashlib.sha256(condition.fr.read_bytes()).hexdigest(),
                      engine=engine, project_cache=mode, environment=environment,
                      repetitions=condition.repetitions, scope=SCOPE, cases=CASES,
                      stages=STAGES, strategies=STRATEGIES, observations=observations,
                      summary=summarize(report['samples']),
                      sequence_totals=sequence_totals(report['samples'], condition.repetitions))
        audit(report)
        (condition.output / 'result.json').write_bytes(encode(report) + b'\n')
    audit_set(args.output)


def audit(report):
    require(report['schema'] == 'fr-python-startup-cost', 'unexpected measurement schema')
    require(report['bindings'] == bindings(), 'measurement source changed; collect again')
    require(report['scope'] == SCOPE and report['cases'] == CASES, 'measurement scope changed')
    require(report['stages'] == [list(stage) for stage in STAGES] or report['stages'] == STAGES,
            'question sequence changed')
    require(list(report['strategies']) == list(STRATEGIES), 'strategies changed')
    require(report['engine'] in ENGINES and report['project_cache'] in CACHE_MODES, 'missing execution settings')
    require(report['environment']['build'] == BUILD, 'optimized build settings changed')
    require(isinstance(report['binary_sha256'], str) and len(report['binary_sha256']) == 64
            and all(char in '0123456789abcdef' for char in report['binary_sha256']), 'invalid binary digest')
    repetitions = report['repetitions']
    require(type(repetitions) is int and 3 <= repetitions <= 9, 'invalid repetition count')
    rows = report['samples']
    require(len(rows) == len(CASES) * len(STAGES) * len(STRATEGIES) * repetitions, 'missing or extra requests')
    require(report['summary'] == summarize(rows), 'summary does not match raw samples')
    require(report['sequence_totals'] == sequence_totals(rows, repetitions), 'sequence totals changed')
    used_answers, seen = set(), set()
    for row in rows:
        case, strategy, stage_name, repetition = row['case'], row['strategy'], row['stage'], row['repetition']
        require(case in CASES and strategy in STRATEGIES and type(repetition) is int
                and 0 <= repetition < repetitions, 'invalid request identity')
        stage = [item[0] for item in STAGES].index(stage_name)
        identity = case, repetition, strategy, stage_name
        require(identity not in seen, 'duplicate request')
        seen.add(identity)
        require(row['position'] == (STRATEGIES.index(strategy) - repetition) % len(STRATEGIES),
                'strategy order changed')
        require(row['cache_position'] == (CACHE_MODES.index(report['project_cache']) - repetition) % 2,
                'project cache order changed')
        wanted = expected(case, stage)
        key = row['answer_digest']
        answer = report['answers'][key]
        used_answers.add(key)
        require(digest(answer) == key, 'answer content digest changed')
        require(answer['complete'] is wanted['complete'], 'completion changed')
        inputs = answer['inputs']
        files = sources(case, STAGES[stage][2])
        require(inputs['source'] == {'path': 'app.py', 'digest': digest(files['app.py'])}
                and inputs['configuration'] == digest({})
                and inputs['selection']['name'] == STAGES[stage][1]
                and inputs['budget'] == {'steps': CASES[case]['steps'], 'depth': 8, 'bytes': 1_048_576}
                and inputs['context'] == 'html' and inputs['summary_mode'] is True
                and ('modules' in inputs) is CASES[case]['imports']
                and answer['input_digest'] == digest(inputs), 'source, function or analysis settings changed')
        if CASES[case]['imports']:
            require(inputs['modules']['files'] == {name: digest(contents) for name, contents in files.items()
                                                  if name.endswith('.py')}, 'imported file set changed')
        require(row['startup_reused'] is (strategy == 'saved-startup' and wanted['startup_reused']),
                'startup reuse changed')
        require(row['answer_reused'] is (strategy == 'saved-answer' and wanted['answer_reused']),
                'whole-answer reuse changed')
        require(row['startup_checks'] == (int(not wanted['startup_reused']) if strategy == 'saved-startup' else None),
                'startup check count changed')
        if wanted['complete']:
            require(bool(answer['witnesses']) is (wanted['python']['returned'] == 17), 'input-flow result changed')
        else:
            require(bool(answer['cutoffs']), 'unfinished answer has no explanation')
            require(('step-budget' in answer['cutoffs']) if case == 'unfinished-budget' else
                    ('module-effects-unchecked' in answer['cutoffs']), 'unfinished reason changed')
        metrics = row['metrics']
        require(set(metrics) == set(FIELDS), 'missing or extra metrics')
        require(all(type(value) in (int, float) and math.isfinite(value) and value >= 0
                    for value in metrics.values()), 'invalid measurement')
        require(all(type(value) is int for name, value in metrics.items() if not name.endswith('_seconds')),
                'counts and byte totals must be integers')
        require(metrics['native_calls'] == (3 if strategy == 'saved-answer' else 2), 'native process count changed')
        calls = row['calls']
        operations = ['find', 'inputs', 'answer'] if strategy == 'saved-answer' else ['find', 'answer']
        require([call['operation'] for call in calls] == operations, 'native request order changed')
        require(all(type(call['seconds']) in (int, float) and math.isfinite(call['seconds'])
                    and 0 <= call['seconds'] <= 20 and type(call['reply_bytes']) is int
                    and call['reply_bytes'] > 0 for call in calls), 'invalid native request measurement')
        require(metrics['native_seconds'] == sum(call['seconds'] for call in calls)
                and metrics['canonical_reply_bytes'] == sum(call['reply_bytes'] for call in calls),
                'native request totals changed')
        for operation in ('find', 'inputs', 'answer'):
            require(metrics[operation + '_seconds'] == sum(call['seconds'] for call in calls
                                                         if call['operation'] == operation),
                    'operation timing changed')
        for position, call in enumerate(calls):
            profile = call['profile']
            if report['engine'] == 'cli':
                require(profile is None, 'CLI time contains profiling')
                continue
            require(profile['schema'] == 'fr-project-profile-1', 'unexpected profiling report')
            phases, construction = profile['phases_seconds'], profile['construction_seconds']
            require(set(phases) == PHASES and set(construction) == CONSTRUCTION, 'profiling phases changed')
            require(all(type(value) in (int, float) and math.isfinite(value) and value >= 0
                        for value in [*phases.values(), *construction.values(), profile['measured_seconds']]),
                    'invalid phase timing')
            require(sum(phases.values()) <= profile['measured_seconds'] <= call['seconds']
                    and sum(construction.values()) <= phases['project'], 'phase timing exceeds parent')
            counters = ('fact_cache_hits', 'resolution_cache_hits', 'resolution_cache_misses')
            if report['project_cache'] == 'disabled':
                require(all(profile[name] is None for name in counters), 'disabled cache was used')
            else:
                require(all(type(profile[name]) is int and profile[name] >= 0 for name in counters),
                        'enabled cache did not report counters')
                require(type(profile['indexed_files']) is int and profile['indexed_files'] > 0
                        and profile['fact_cache_hits'] <= profile['indexed_files'], 'invalid fact cache hits')
                if position > 0 or stage in (1, 2, 5, 6):
                    require(profile['fact_cache_hits'] == profile['indexed_files'],
                            'unchanged files missed the project cache')
                elif stage == 0:
                    require(profile['fact_cache_hits'] == 0, 'first request was secretly primed')
                else:
                    require(profile['fact_cache_hits'] < profile['indexed_files'],
                            'edited file reused stale cached facts')
        cache_fields = [name for name in FIELDS if name.startswith('project_cache_')]
        require(all(type(metrics[name]) is int for name in cache_fields), 'invalid project cache inventory')
        if report['project_cache'] == 'disabled':
            require(all(metrics[name] == 0 for name in cache_fields), 'disabled project cache wrote files')
        else:
            require(metrics['project_cache_files'] > 0 and metrics['project_cache_bytes'] > 0,
                    'enabled project cache stored nothing')
            for unit in ('files', 'bytes'):
                total, added = metrics['project_cache_' + unit], metrics['project_cache_added_' + unit]
                previous = [other for other in rows if (other['case'], other['strategy'], other['repetition'],
                            other['stage']) == (case, strategy, repetition, STAGES[stage - 1][0])]
                require(total == added + (previous[0]['metrics']['project_cache_' + unit] if stage else 0),
                        'project cache population was excluded or inventory changed')
        require(metrics['worker_seconds'] <= metrics['elapsed_seconds'] <= 30, 'worker timer changed')
        require(metrics['native_seconds'] + metrics['store_seconds'] <= metrics['worker_seconds'],
                'component time exceeds worker time')
        require(metrics['analysis_steps'] <= CASES[case]['steps'], 'analysis exceeded its budget')
        if row['answer_reused']:
            require(metrics['analysis_steps'] == 0, 'saved whole answer repeated function analysis')
        if strategy == 'fresh':
            require(all(metrics[name] == 0 for name in FIELDS if name.startswith(('store_', 'stored_', 'added_'))),
                    'fresh baseline used saved storage')
    require(used_answers == set(report['answers']), 'unreferenced answers')
    for case in CASES:
        for repetition in range(repetitions):
            for name, _, _ in STAGES:
                selected = [row for row in rows if (row['case'], row['repetition'], row['stage']) == (case, repetition, name)]
                require(len({row['answer_digest'] for row in selected}) == 1,
                        'saved answer differs from direct fresh analysis')
    observed = report['observations']
    require(len(observed) == len(CASES) * len(STAGES), 'missing Python observations')
    require(len({(item['case'], item['stage']) for item in observed}) == len(observed), 'duplicate Python observation')
    for item in observed:
        stage = [entry[0] for entry in STAGES].index(item['stage'])
        require(item['result'] == expected(item['case'], stage)['python'], 'ordinary Python result changed')
        require(type(item['elapsed_seconds']) in (int, float) and math.isfinite(item['elapsed_seconds'])
                and 0 <= item['elapsed_seconds'] <= 20, 'invalid Python duration')


def audit_set(directory):
    reports = {}
    for engine in ENGINES:
        for mode in CACHE_MODES:
            report = json.loads((directory / f'{engine}-{mode}' / 'result.json').read_bytes())
            audit(report)
            require((report['engine'], report['project_cache']) == (engine, mode), 'condition mislabeled')
            require(report['repetitions'] == (5 if engine == 'cli' else 3), 'collection repetitions changed')
            reports[engine, mode] = report
    baseline = reports['cli', 'disabled']
    for (engine, _), report in reports.items():
        require(report['answers'] == baseline['answers'], 'cache mode or profiler changed answers')
        require(report['environment'] == baseline['environment'], 'conditions used different environments')
        require(report['binary_sha256'] == reports[engine, 'disabled']['binary_sha256'],
                'cache conditions used different binaries')
    print('checked optimized CLI and separate profiles with project caching off and on; all answers agree')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument('--output', type=Path)
    mode.add_argument('--audit', type=Path)
    mode.add_argument('--audit-set', type=Path)
    mode.add_argument('--worker', action='store_true')
    parser.add_argument('--fr', type=Path)
    parser.add_argument('--profiler', type=Path)
    parser.add_argument('--repetitions', type=int)
    parser.add_argument('--profile-repetitions', type=int)
    parser.add_argument('--project', type=Path)
    parser.add_argument('--work', type=Path)
    parser.add_argument('--case', choices=CASES)
    parser.add_argument('--function', choices=('forwards', 'stopped'))
    parser.add_argument('--strategy', choices=STRATEGIES)
    parser.add_argument('--engine', choices=ENGINES)
    parser.add_argument('--project-cache', choices=CACHE_MODES)
    args = parser.parse_args()
    if args.audit_set:
        audit_set(args.audit_set)
    elif args.audit:
        audit(json.loads((args.audit / 'result.json').read_bytes()))
        print('checked every request, fresh-answer agreement, reuse decision, budget and Python result')
    elif args.worker:
        require(args.engine is not None and args.project_cache is not None, 'supply --engine and --project-cache')
        print(encode(worker(args)).decode())
    else:
        require(args.fr is not None and args.profiler is not None and args.repetitions is not None
                and args.profile_repetitions is not None,
                'supply --fr, --profiler, --repetitions and --profile-repetitions')
        measure(args)


if __name__ == '__main__':
    main()
