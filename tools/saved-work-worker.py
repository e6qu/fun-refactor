"""Answer one measured project question in a fresh Python process."""
import argparse
from dataclasses import replace
import hashlib
import json
import os
from pathlib import Path
import resource
import sys
import time

BOOT = time.monotonic()
ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'sdk/python/src'))
from fr_ir.change_scope import ChangeScope
from fr_ir.context import DirectoryObjectStore, restore_stored_value, store_merkle_value
from fr_ir.investigation import Dependency, DependencyKind, Evidence, EvidenceKind, TaskPlan, TaskStep
from fr_ir.runtime import FrClient


def encode(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':'), allow_nan=False).encode()


def digest(value):
    return hashlib.sha256(encode(value)).hexdigest()


def save(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(encode(value) + b'\n')


def files(root):
    return {str(path.relative_to(root)): hashlib.sha256(path.read_bytes()).hexdigest()
            for path in sorted(root.rglob('*')) if path.is_file()
            and path.relative_to(root).parts[0] != '.fr'}


def size(root):
    return sum(path.stat().st_size for path in root.rglob('*') if path.is_file()) if root.exists() else 0


class MeasuredClient(FrClient):
    def __init__(self, project, executable, output):
        super().__init__(project, executable=executable, timeout=20, max_output_bytes=2097152)
        self.output, self.calls = output, []

    def call(self, *arguments, input_bytes=None):
        started = time.monotonic()
        report = super().call(*arguments, input_bytes=input_bytes)
        elapsed = time.monotonic() - started
        data = report.to_data()
        name = 'calls/' + str(len(self.calls)) + '.json'
        save(self.output / name, data)
        self.calls.append({'arguments': list(arguments), 'seconds': elapsed, 'report': name,
                           'sha256': digest(data), 'canonical_report_bytes': len(encode(data))})
        return report


def discover(client, request):
    limits = request['scope_limits']
    found = client.project('find', request['name'], '--source', '--bytes', str(limits['find_bytes']))
    value = found.to_data()
    assert value['page']['next'] is None, 'find requires another page'
    rows = [dict(zip(value['columns'], row)) for row in value['rows']]
    assert len(rows) <= 16, 'too many declarations for this bounded question'
    scopes = [ChangeScope.inspect(client, [row['handle']], depth=limits['depth'],
        nodes=limits['nodes'], references=limits['references'], max_bytes=limits['max_bytes']).report.to_data()
        for row in rows]
    return {'find': value, 'scopes': scopes}


def satisfy(plan, client, reference):
    started = plan.resume(client, transition='selected:start')
    evidence = Evidence('answer', EvidenceKind.OBSERVATION, started.input_digests['selected'], True, reference)
    attached = replace(started.plan, steps=(replace(started.plan.steps[0], evidence=(evidence,)),))
    finished = attached.resume(client, transition='selected:satisfy')
    assert finished.complete
    return finished.plan


def measure(args):
    os.environ['FUN_REFACTOR_CACHE'] = str(args.cache)
    request = json.loads(args.request.read_text())
    client = MeasuredClient(args.project, args.fr, args.output)
    objects = args.state / 'objects'
    before = {'native_state': size(args.project / '.fr'), 'saved_state': size(args.state)}
    phases, invalidated, retained, validation = {}, [], False, None
    stage_start = time.monotonic()
    if args.condition == 'saved':
        store = DirectoryObjectStore(objects)
        if args.stage == 'first':
            plan = TaskPlan(request['question'], ('current discovery',), (TaskStep('selected', request['name'], (
                Dependency(DependencyKind.WORKSPACE, 'project'),
                Dependency(DependencyKind.LOOKUP, request['name']),
                Dependency(DependencyKind.ANALYZER, 'native-implementation')),
                satisfies=('current discovery',)),))
        else:
            receipt = json.loads((args.state / 'receipt.json').read_text())
            plan = TaskPlan.restore(store, receipt['plan'])
            phases['restore_seconds'] = time.monotonic() - stage_start
            checked = time.monotonic()
            resumed = plan.resume(client)
            phases['validation_seconds'] = time.monotonic() - checked
            invalidated = list(resumed.invalidated)
            validation = resumed.report.to_data()
            if args.stage == 'reuse':
                assert resumed.complete and not invalidated
                retained = True
                answer = restore_stored_value(store, receipt['answer'])
                assert plan.steps[0].evidence[0].reference == 'sha256:' + digest(answer)
            else:
                assert invalidated == ['selected'] and not resumed.complete
                plan = resumed.plan.resume(client, transition='selected:reset').plan
    if not retained:
        started = time.monotonic()
        answer = discover(client, request)
        phases['discovery_seconds'] = time.monotonic() - started
        if args.condition == 'saved':
            started = time.monotonic()
            answer_root = store_merkle_value(store, answer).digest
            plan = satisfy(plan, client, 'sha256:' + digest(answer))
            save(args.state / 'receipt.json', {'plan': plan.store(store), 'answer': answer_root})
            phases['persist_seconds'] = time.monotonic() - started
    result = {'condition': args.condition, 'stage': args.stage, 'answer': answer,
        'answer_digest': digest(answer), 'answer_bytes': len(encode(answer)) + 1, 'calls': client.calls,
        'invalidated': invalidated, 'retained': retained, 'validation': validation, 'phases': phases,
        'source_files': files(args.project), 'storage_before': before,
        'storage_after': {'native_state': size(args.project / '.fr'), 'saved_state': size(args.state)},
        'worker_seconds': time.monotonic() - BOOT,
        'worker_cpu_seconds': sum(getattr(resource.getrusage(kind), field)
            for kind in (resource.RUSAGE_SELF, resource.RUSAGE_CHILDREN) for field in ('ru_utime', 'ru_stime'))}
    save(args.output / 'result.json', result)
    sys.stdout.buffer.write(encode(answer) + b'\n')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--project', type=Path, required=True)
    parser.add_argument('--fr', type=Path, required=True)
    parser.add_argument('--request', type=Path, required=True)
    parser.add_argument('--state', type=Path, required=True)
    parser.add_argument('--cache', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--condition', choices=('saved', 'fresh'), required=True)
    parser.add_argument('--stage', choices=('first', 'reuse', 'edit'), required=True)
    measure(parser.parse_args())
