#!/usr/bin/env python3
"""Measure fresh questions, saved Python preparation and saved whole answers."""
from __future__ import annotations

import argparse
import copy
import hashlib
import json
import math
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
from fr_ir.context import DirectoryObjectStore
from fr_ir.flow import FlowAnalysis, FlowCache
from fr_ir.flow_startup import StartupCache
from fr_ir.runtime import FrClient

BINDINGS = ('tools/python-startup-cost.py', 'tools/test-python-startup-cost.py',
            'tools/evidence_basis.py', 'tests/agent-eval/python-startup-cost/corpus.py',
            '.github/workflows/python-startup-cost.yml')
FIELDS = ('elapsed_seconds', 'worker_seconds', 'native_seconds', 'store_seconds',
          'native_calls', 'canonical_reply_bytes', 'store_gets', 'store_puts',
          'store_read_bytes', 'store_offered_bytes', 'added_files', 'added_bytes',
          'stored_files', 'stored_bytes', 'analysis_steps',
          'worker_peak_rss_bytes', 'native_peak_rss_bytes')
SCOPE = {
    'elapsed_seconds': 'Parent timer around a new Python worker through parsed JSON reply. Includes Python/SDK startup, finding the function, native process startup, file checks, import resolution, parsing, analysis or validation, saved-record reads/writes and manifest pointer I/O.',
    'excluded': 'Checkout, compilation, dependency installation, fixture edits, ordinary Python observation, store inventory, report comparison and audit. Worker reply serialization/transfer, shared measurement imports and instrumentation are included. Native project caching is explicitly disabled for every call. OS file caches are neither flushed nor controlled.',
    'attribution': 'Native time includes all native work and client reply parsing. No separate parser, file-check or import-resolution timers exist; their individual costs cannot be inferred. Native calls count process starts, including find and inputs-only.',
    'bytes': 'Canonical JSON reply bytes, not exact wire bytes. Store reads and offered writes count repeated object operations. Added bytes count unique object files; offered writes can verify existing objects. Manifest pointer bytes are outside object totals but their I/O is timed. Temporary native reuse files are timed but not retained in the object inventory.',
    'memory': 'Separate worker and largest-child lifetime RSS high-water marks, not sampled aggregate RSS. Python runtime and fr are separate processes.',
    'python': 'Ordinary Python executes the same selected function with source returning 17 and sink returning its argument. It checks one concrete result, not all input-flow paths. Its elapsed time is contextual, not an equivalent analysis baseline.',
    'claims': 'Disclosed synthetic development examples only. No unfamiliar package, agent task, paid study, full-package acceptance or general speedup claim.',
}


def encode(value):
    return json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(',', ':'), allow_nan=False).encode()


def digest(value):
    return hashlib.sha256(encode(value)).hexdigest()


def bindings():
    sdk = [str(path.relative_to(ROOT)) for path in sorted((ROOT / 'sdk/python/src').rglob('*.py'))]
    return native_bindings(ROOT, [*BINDINGS, *sdk])


def require(condition, message):
    if not condition:
        raise ValueError(message)


def evidence(value):
    answer = copy.deepcopy(value)
    del answer['execution']
    answer.pop('context_basis', None)
    return answer


def inventory(root):
    files = list((root / 'objects').rglob('*.json'))
    return len(files), sum(path.stat().st_size for path in files)


class MeasuredStore:
    def __init__(self, root):
        self.store = DirectoryObjectStore(root)
        self.gets = self.puts = self.read_bytes = self.offered_bytes = 0
        self.seconds = 0.0

    def get(self, key):
        self.gets += 1
        started = time.perf_counter()
        try:
            value = self.store.get(key)
            if value is not None:
                self.read_bytes += len(encode(value))
            return value
        finally:
            self.seconds += time.perf_counter() - started

    def put(self, key, value):
        self.puts += 1
        started = time.perf_counter()
        try:
            self.offered_bytes += len(encode(value))
            self.store.put(key, value)
        finally:
            self.seconds += time.perf_counter() - started


class MeasuredClient(FrClient):
    def __init__(self, project, executable):
        super().__init__(project, executable=executable, timeout=20, max_output_bytes=1_048_576)
        self.calls = self.reply_bytes = 0
        self.seconds = 0.0

    def project(self, *arguments):
        started = time.perf_counter()
        report = self.call('--no-cache', 'project', *arguments)
        self.seconds += time.perf_counter() - started
        self.calls += 1
        self.reply_bytes += len(encode(report.to_data()))
        return report


def worker(args):
    started = time.perf_counter()
    client = MeasuredClient(args.project, str(args.fr.resolve()))
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
            prepared = cache.analyze(client, target, **options)
            result = prepared.answer
            startup_checks, startup_reused = prepared.checks, prepared.reused
        else:
            result = cache.analyze(client, target, summaries=True, **options)
            answer_reused = result.reused
        pointer.write_text(cache.persist())
    value = result.report.to_data()
    elapsed = time.perf_counter() - started
    scale = 1 if sys.platform == 'darwin' else 1024
    return {'answer': evidence(value), 'startup_reused': startup_reused,
            'answer_reused': answer_reused, 'startup_checks': startup_checks,
            'metrics': {'worker_seconds': elapsed, 'native_seconds': client.seconds,
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
    command = [sys.executable, '-B', str(Path(__file__).resolve()), '--worker', '--fr', str(args.fr.resolve()),
               '--project', str(project), '--work', str(work), '--case', case, '--function', function,
               '--strategy', strategy]
    started = time.perf_counter()
    process = subprocess.run(command, capture_output=True, text=True, timeout=30)
    require(process.returncode == 0, f'worker failed ({case}/{function}/{strategy}): {process.stderr}')
    result = json.loads(process.stdout)
    elapsed = time.perf_counter() - started
    after = inventory(work)
    result['metrics'].update(elapsed_seconds=elapsed, stored_files=after[0], stored_bytes=after[1],
                             added_files=after[0] - before[0], added_bytes=after[1] - before[1])
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
    require(3 <= args.repetitions <= 9, 'use 3 to 9 explicit repetitions')
    before = bindings()
    args.output.mkdir(parents=True, exist_ok=False)
    (args.output / 'attempt.json').write_bytes(encode({'bindings': before, 'repetitions': args.repetitions,
                                                     'scope': SCOPE}) + b'\n')
    rows, answers, observations = [], {}, []
    with tempfile.TemporaryDirectory(prefix='fr-startup-cost-') as directory:
        base = Path(directory)
        project = base / 'project'
        for case in CASES:
            for repetition in range(args.repetitions):
                offset = repetition % len(STRATEGIES)
                order = STRATEGIES[offset:] + STRATEGIES[:offset]
                for position, strategy in enumerate(order):
                    for stage, (name, function, state) in enumerate(STAGES):
                        # Remove the previous case's imported helper before installing this case.
                        for path in project.glob('*.py'):
                            path.unlink()
                        install(project, case, state)
                        result = invoke(args, project, base / case / str(repetition) / strategy,
                                        case, function, strategy)
                        answer = result.pop('answer')
                        key = digest(answer)
                        answers[key] = answer
                        row = {'case': case, 'repetition': repetition, 'position': position,
                               'stage': name, 'strategy': strategy, 'answer_digest': key, **result}
                        rows.append(row)
                        with (args.output / 'requests.jsonl').open('ab') as stream:
                            stream.write(encode({'request': row, 'answer': answer}) + b'\n')
                print(f'measured {case} repetition {repetition + 1}', file=sys.stderr, flush=True)
            for name, function, state in STAGES:
                for path in project.glob('*.py'):
                    path.unlink()
                install(project, case, state)
                observations.append({'case': case, 'stage': name, **observe(project, function)})
    require(before == bindings(), 'measurement sources changed')
    report = {'schema': 'fr-python-startup-cost', 'bindings': before,
              'binary_sha256': hashlib.sha256(args.fr.read_bytes()).hexdigest(),
              'environment': {'platform': platform.platform(), 'python': sys.version,
                              'build': 'cargo build --locked --bin fr; dev profile without debug symbols'},
              'repetitions': args.repetitions, 'scope': SCOPE, 'cases': CASES,
              'stages': STAGES, 'strategies': STRATEGIES,
              'samples': rows, 'answers': answers, 'observations': observations,
              'summary': summarize(rows), 'sequence_totals': sequence_totals(rows, args.repetitions)}
    audit(report)
    (args.output / 'result.json').write_bytes(encode(report) + b'\n')


def audit(report):
    require(report['schema'] == 'fr-python-startup-cost', 'unexpected measurement schema')
    require(report['bindings'] == bindings(), 'measurement source changed; collect again')
    require(report['scope'] == SCOPE and report['cases'] == CASES, 'measurement scope changed')
    require(report['stages'] == [list(stage) for stage in STAGES] or report['stages'] == STAGES,
            'question sequence changed')
    require(list(report['strategies']) == list(STRATEGIES), 'strategies changed')
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
        require(metrics['native_calls'] == (3 if strategy == 'saved-answer' else 2), 'native process count changed')
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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument('--output', type=Path)
    mode.add_argument('--audit', type=Path)
    mode.add_argument('--worker', action='store_true')
    parser.add_argument('--fr', type=Path)
    parser.add_argument('--repetitions', type=int)
    parser.add_argument('--project', type=Path)
    parser.add_argument('--work', type=Path)
    parser.add_argument('--case', choices=CASES)
    parser.add_argument('--function', choices=('forwards', 'stopped'))
    parser.add_argument('--strategy', choices=STRATEGIES)
    args = parser.parse_args()
    if args.audit:
        audit(json.loads((args.audit / 'result.json').read_bytes()))
        print('checked every request, fresh-answer agreement, reuse decision, budget and Python result')
    elif args.worker:
        print(encode(worker(args)).decode())
    else:
        require(args.fr is not None and args.repetitions is not None, 'supply --fr and --repetitions')
        measure(args)


if __name__ == '__main__':
    main()
