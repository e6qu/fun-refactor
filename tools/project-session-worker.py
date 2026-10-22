"""Keep the existing Python client alive across explicitly submitted questions."""
import argparse
from dataclasses import replace
import hashlib
import json
import os
from pathlib import Path
import sys
import time


def encode(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':'), allow_nan=False).encode()


def digest(value):
    return hashlib.sha256(encode(value)).hexdigest()


def save(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(encode(value) + b'\n')


def retain(reports, value):
    data = encode(value) + b'\n'
    identity = digest(value)
    path = reports / (identity + '.json')
    if path.exists():
        if path.read_bytes() != data:
            raise ValueError('Conflicting recorded report')
    else:
        path.write_bytes(data)
    return identity


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--sdk', type=Path, required=True)
    parser.add_argument('--project', type=Path, required=True)
    parser.add_argument('--fr', type=Path, required=True)
    parser.add_argument('--state', type=Path, required=True)
    parser.add_argument('--request', type=Path, required=True)
    parser.add_argument('--reports', type=Path, required=True)
    parser.add_argument('--condition', choices=('fresh', 'saved'), required=True)
    args = parser.parse_args()
    sys.path.insert(0, str(args.sdk.resolve()))
    from fr_ir.change_scope import ChangeScope
    from fr_ir.context import DirectoryObjectStore, restore_stored_value, store_merkle_value
    from fr_ir.investigation import Dependency, DependencyKind, Evidence, EvidenceKind, TaskPlan, TaskStep
    from fr_ir.runtime import FrClient
    import fr_ir.runtime
    if not Path(fr_ir.runtime.__file__).resolve().is_relative_to(args.sdk.resolve()):
        raise ValueError('Loaded a different SDK')
    os.environ['FUN_REFACTOR_CACHE'] = str(args.project / '.fr/cost-cache')
    request = json.loads(args.request.read_text())

    class MeasuredClient(FrClient):
        def __init__(self):
            super().__init__(args.project, executable=args.fr, timeout=20, max_output_bytes=2097152)
            self.calls = []

        def call(self, *arguments, input_bytes=None):
            started = time.monotonic()
            report = super().call(*arguments, input_bytes=input_bytes)
            elapsed = time.monotonic() - started
            value = report.to_data()
            name = retain(args.reports, value)
            self.calls.append({'arguments': list(arguments), 'seconds': elapsed,
                'report': name, 'sha256': digest(value), 'canonical_report_bytes': len(encode(value))})
            return report

    client = MeasuredClient()
    print(json.dumps({'ready': True, 'sdk_runtime': str(Path(fr_ir.runtime.__file__).resolve())}), flush=True)
    for line in sys.stdin:
        submitted = json.loads(line)
        if submitted == {'stop': True}:
            print('{"stopped":true}', flush=True)
            return
        if set(submitted) != {'stage', 'output'} or submitted['stage'] not in ('first', 'reuse', 'edit'):
            raise ValueError('Submit an explicit stage and output directory')
        stage, output = submitted['stage'], Path(submitted['output'])
        output.mkdir(parents=True, exist_ok=False)
        client.calls = []
        phases, invalidated, retained, validation = {}, [], False, None
        started = time.monotonic()
        if args.condition == 'saved':
            store = DirectoryObjectStore(args.state / 'objects')
            if stage == 'first':
                plan = TaskPlan(request['question'], ('current discovery',), (TaskStep('selected', request['name'], (
                    Dependency(DependencyKind.WORKSPACE, 'project'),
                    Dependency(DependencyKind.LOOKUP, request['name']),
                    Dependency(DependencyKind.ANALYZER, 'native-implementation')),
                    satisfies=('current discovery',)),))
            else:
                receipt = json.loads((args.state / 'receipt.json').read_text())
                plan = TaskPlan.restore(store, receipt['plan'])
                phases['restore_seconds'] = time.monotonic() - started
                checked = time.monotonic()
                resumed = plan.resume(client)
                phases['validation_seconds'] = time.monotonic() - checked
                invalidated, validation = list(resumed.invalidated), resumed.report.to_data()
                if stage == 'reuse':
                    if not resumed.complete or invalidated:
                        raise ValueError('An unchanged answer was invalidated')
                    retained = True
                    answer = restore_stored_value(store, receipt['answer'])
                    if plan.steps[0].evidence[0].reference != 'sha256:' + digest(answer):
                        raise ValueError('Saved answer differs from its checked reference')
                else:
                    if invalidated != ['selected'] or resumed.complete:
                        raise ValueError('The edited answer was not invalidated')
                    plan = resumed.plan.resume(client, transition='selected:reset').plan
        if not retained:
            started = time.monotonic()
            limits = request['scope_limits']
            found = client.project('find', request['name'], '--source', '--bytes', str(limits['find_bytes']),
                '--limit', str(limits['find_limit']), '--in', limits['find_scope']).to_data()
            if found['page']['next'] is not None:
                raise ValueError('The question requires another find page')
            rows = [dict(zip(found['columns'], row)) for row in found['rows']]
            scopes = [ChangeScope.inspect(client, [row['handle']], depth=limits['depth'], nodes=limits['nodes'],
                references=limits['references'], max_bytes=limits['max_bytes']).report.to_data() for row in rows]
            answer = {'find': found, 'scopes': scopes}
            phases['discovery_seconds'] = time.monotonic() - started
            if args.condition == 'saved':
                started = time.monotonic()
                answer_root = store_merkle_value(store, answer).digest
                running = plan.resume(client, transition='selected:start')
                evidence = Evidence('answer', EvidenceKind.OBSERVATION, running.input_digests['selected'],
                                    True, 'sha256:' + digest(answer))
                attached = replace(running.plan, steps=(replace(running.plan.steps[0], evidence=(evidence,)),))
                finished = attached.resume(client, transition='selected:satisfy')
                if not finished.complete:
                    raise ValueError('The saved task did not complete')
                save(args.state / 'receipt.json', {'plan': finished.plan.store(store), 'answer': answer_root})
                phases['persist_seconds'] = time.monotonic() - started
        save(output / 'answer.json', {'report': retain(args.reports, answer)})
        if args.condition == 'saved':
            save(output / 'receipt.json', json.loads((args.state / 'receipt.json').read_text()))
        value = {'condition': args.condition, 'stage': stage, 'answer_digest': digest(answer),
            'answer_bytes': len(encode(answer)) + 1, 'calls': client.calls, 'invalidated': invalidated,
            'retained': retained, 'validation': validation, 'phases': phases}
        save(output / 'result.json', value)
        print(json.dumps({'stage': stage, 'answer_digest': value['answer_digest']}), flush=True)
    raise ValueError('The client closed without an explicit stop')


if __name__ == '__main__':
    main()
