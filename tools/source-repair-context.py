#!/usr/bin/env python3
"""Compare repair-on-applied-source with undo-and-correct using existing fr routes."""
import argparse
import copy
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('recovery', ROOT / 'tools/recovery-delivery-context.py')
recovery = importlib.util.module_from_spec(spec)
spec.loader.exec_module(recovery)
bodies, routes = recovery.bodies, recovery.routes
ARMS = ('repair-applied', 'undo-correct')
FAULTS = (None, 'wrong-repair', 'source', 'checks', 'later-edit')


def bindings():
    return {**recovery.bindings(), 'tools/source-repair-context.py': routes.digest(Path(__file__).read_bytes())}


def wrong(case, again=False):
    value = copy.deepcopy(case)
    for edit in value['edits']:
        if edit['path'].endswith('.py'):
            edit['new'] = "return 'still wrong'" if again else "return ''"
            edit['body'] = edit['new'] + '\n'
        else:
            edit['new'] = '1' if again else '0'
            edit['body'] = ('{\n    return ' + edit['new'] + ';\n}\n' if case['id'] == 'rust-body'
                            else '{ ' + edit['new'] + ' }\n')
    return value


def body_program(case, repair=False):
    source = bodies.program(case, 'builder').partition('submission = ')[0]
    if repair:
        source = source.replace("TaskDelivery(patch='artifacts/change.patch', check_output_bytes=512)",
            'TaskDelivery(check_original=False, exercise_reversal=False, check_output_bytes=512)')
    return source


def program(case, arm, fault):
    source = '''import hashlib
from pathlib import Path
from fr_ir.runtime import FrClient
client = FrClient('.')
failed_record = client.call('history', 'show', str(transaction)).to_data()['records'][0]
assert failed_record['id'] == transaction and failed_record['status'] == 'applied'
required = failed_record['required_checks']
assert required['checks'] == ['behavior']
'''
    if arm == 'undo-correct':
        source += '''undo_review = client.call('history', 'undo', str(transaction)).to_data()
client.call('history', 'undo', str(transaction), '--write', '--no-diff')
'''
    source += body_program(wrong(case, again=True) if fault == 'wrong-repair' else case, repair=True)
    source += '\n# delivery boundary\n'
    source += "chain = " + ("[transaction, result['transaction']]" if arm == 'repair-applied' else "[result['transaction']]") + '\n'
    source += '''assert result['passed'] is True
transitions = []
for current in reversed(chain):
    review = client.call('history', 'undo', str(current)).to_data()
    changed = client.call('history', 'undo', str(current), '--write', '--no-diff').to_data()
    transitions.append({'action': 'undo', 'transaction': current, 'review': review, 'result': changed})
restored = client.call('checks', '--run', 'behavior', '--basis', required['configuration_basis']).to_data()
assert restored['passed'] is True
for current in chain:
    review = client.call('history', 'redo', str(current)).to_data()
    changed = client.call('history', 'redo', str(current), '--write').to_data()
    transitions.append({'action': 'redo', 'transaction': current, 'review': review, 'result': changed})
rechecked = client.call('checks', '--run', 'behavior', '--basis', required['configuration_basis'],
    '--record-for', str(result['transaction'])).to_data()
assert rechecked['passed'] is True
delivered = client.call('history', 'show', str(result['transaction'])).to_data()
assert delivered['records'][0]['required_checks'] == required
parts = [client.call('history', 'patch', str(current)).to_data() for current in chain]
assert all(part['id'] == current and part['status'] == 'applied' and not part['reverse']
    for current, part in zip(chain, parts))
patch = ''.join(part['patch'] for part in parts).encode()
assert 0 < len(patch) <= 65536
with (Path(client.root) / 'artifacts/change.patch').open('xb') as destination:
    destination.write(patch)
submission = {'passed': True, 'transactions': chain, 'patch_bytes': len(patch),
    'patch_sha256': hashlib.sha256(patch).hexdigest(),
    'check_receipts': [e['receipt'] for e in delivered['records'][0]['check_evidence']]}
'''
    return source


def changed_source(source):
    value = dict(source)
    path = next(iter(value))
    value[path] += '\n# later edit\n' if path.endswith('.py') else '\n// later edit\n'
    return value


def receiver(root, case, parts):
    patch = ''.join(p['patch'] for p in parts)
    result = recovery.replay(root, case, patch)
    project = root.parent / 'receiver'
    artifact = root.parent / 'delivery.patch'
    # A real failed apply must preserve the receiver, including on multi-file patches.
    result['conflict_apply'] = recovery.run_command(['git', 'apply', str(artifact)], project)
    result['conflict_after'] = routes.source_state(project, case)
    controls = []
    if len(parts) == 2:
        for name, payload in (('repair-only', parts[1]['patch']),
                              ('reversed-order', parts[1]['patch'] + parts[0]['patch'])):
            for path, text in case['files'].items():
                (project / path).write_text(text)
            artifact.write_text(payload)
            before = routes.source_state(project, case)
            applied = recovery.run_command(['git', 'apply', str(artifact)], project)
            controls.append({'name': name, 'patch': payload, 'before': before,
                             'apply': applied, 'after': routes.source_state(project, case)})
    result['incomplete_or_reordered'] = controls
    return result


def execute(root, case, arm, fault, executable):
    if root.parent.exists():
        shutil.rmtree(root.parent)
    routes.prepare(root, case)
    # Pure behavior checks: no repaired external marker can make the wrong body pass.
    oracle = "import pathlib, sys\nsys.path.insert(0, str(pathlib.Path.cwd()))\n" + bodies.checker(case)
    (root / '.fr/check.py').write_text(oracle)
    initial = routes.RecordedClient(root, executable=executable, timeout=60)
    initial_program = body_program(wrong(case))
    initial_scope = {}
    initial_error = recovery.attempt(initial_program, initial_scope, initial)
    if not initial_error or initial_error['report'].get('passed') is not False:
        raise ValueError('incorrect initial body did not fail an applied check')
    failed_source = routes.source_state(root, case)
    transaction = initial_error['report']['transaction']
    path = root / 'artifacts/change.patch'
    if path.exists():
        raise ValueError('initial failure delivered a patch')
    reopened = routes.RecordedClient(root, executable=executable, timeout=60)
    source = program(case, arm, fault)
    edit, _, delivery = source.partition('# delivery boundary\n')
    preview, marker, write = edit.partition('result = client.execute_guide')
    scope = {'transaction': transaction}
    recovery.run_code(preview, scope, reopened)
    execution_start = len(reopened.events)
    if fault == 'source':
        for name, text in changed_source(routes.source_state(root, case)).items():
            (root / name).write_text(text)
    elif fault == 'checks':
        checks = root / '.fr/checks.json'
        declaration = json.loads(checks.read_text())
        declaration['checks'][0]['covers'].append('changed after review')
        checks.write_bytes(routes.canonical(declaration))
    before = routes.source_state(root, case)
    error = recovery.attempt(marker + write, scope, reopened)
    delivery_start = len(reopened.events)
    if not error:
        if fault == 'later-edit':
            for name, text in changed_source(routes.source_state(root, case)).items():
                (root / name).write_text(text)
        error = recovery.attempt(delivery, scope, reopened)
    patch = path.read_text() if path.exists() else None
    submission = scope.get('submission')
    inputs = {'transaction': transaction}
    caller_bytes = len(source.encode()) + len(routes.canonical(inputs)) + len(routes.canonical(submission))
    return {'case': case['id'], 'arm': arm, 'fault': fault, 'check_program': oracle,
            'initial_program': initial_program, 'initial_events': initial.events,
            'initial_error': initial_error, 'failed_source': failed_source,
            'initial_metrics': routes.metrics(initial.events), 'program': source, 'input': inputs,
            'execution_start': execution_start, 'delivery_start': delivery_start,
            'events': reopened.events, 'metrics': routes.metrics(reopened.events),
            'total_metrics': routes.metrics(initial.events + reopened.events),
            'caller_bytes': caller_bytes, 'total_caller_bytes': len(initial_program.encode()) + caller_bytes,
            'before': before, 'source': routes.source_state(root, case),
            'result': scope.get('result'), 'error': error, 'patch': patch,
            'submission': submission, 'transitions': scope.get('transitions'),
            'restored': scope.get('restored'), 'rechecked': scope.get('rechecked'),
            'history': scope.get('delivered'), 'parts': scope.get('parts'),
            'receiver': receiver(root, case, scope['parts']) if patch is not None else None}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def response(event, report):
    return event['response'] == {'exit_code': 0, 'report': report}


def check_receipt(check, basis):
    require(check['passed'] is True and check['basis'] == basis and check['source_snapshot_stable'] is True
            and [r['name'] for r in check['results']] == ['behavior']
            and all(r['passed'] is True and r['exit_code'] == 0 and r['source_snapshot_stable'] is True
                    for r in check['results']), 'required behavior check did not pass on stable source')


def audit(value):
    require(value.get('schema') == 'fr-source-repair-1' and value.get('bindings') == bindings(),
            'source repair schema or bindings changed')
    rows = {(r['case'], r['arm'], r['fault']): r for r in value['runs']}
    expected = {(c['id'], a, f) for c in bodies.CASES for a in ARMS for f in FAULTS}
    require(set(rows) == expected and len(rows) == len(value['runs']), 'missing or duplicate source repair cells')
    for (case_id, arm, fault), row in rows.items():
        case = next(c for c in bodies.CASES if c['id'] == case_id)
        source = program(case, arm, fault)
        initial = body_program(wrong(case))
        caller = len(source.encode()) + len(routes.canonical(row['input'])) + len(routes.canonical(row['submission']))
        events = row['events']
        require(row['program'] == source and row['initial_program'] == initial
                and row['check_program'] == "import pathlib, sys\nsys.path.insert(0, str(pathlib.Path.cwd()))\n" + bodies.checker(case)
                and row['caller_bytes'] == caller and row['total_caller_bytes'] == len(initial.encode()) + caller
                and row['metrics'] == routes.metrics(events)
                and row['initial_metrics'] == routes.metrics(row['initial_events'])
                and row['total_metrics'] == routes.metrics(row['initial_events'] + events),
                'source repair program or traffic accounting changed')
        failed = row['initial_error']['report']
        failed_check = failed['workflow']['stages'][2]['result']
        tx = failed['transaction']
        require(len(row['initial_events']) == 3 and row['initial_events'][-1]['response'] == {
                    'exit_code': row['initial_error']['exit_code'], 'error': row['initial_error']['message'], 'report': failed}
                and failed['passed'] is False and failed['workflow']['transaction_status'] == 'applied'
                and [(s['stage'], s['status']) for s in failed['workflow']['stages']] ==
                    [(s, 'passed' if i < 2 else 'failed' if i == 2 else 'pending') for i, s in enumerate(routes.STAGES)]
                and failed_check['passed'] is False and failed_check['results'][0]['exit_code'] != 0
                and row['failed_source'] == bodies.expected_source(wrong(case))
                and row['input'] == {'transaction': tx}, 'incorrect initial body or failed check was lost')
        required = {'checks': ['behavior'], 'configuration_basis': failed_check['basis']}
        shown = events[0]['response']['report']['records'][0]
        require(events[0]['request']['arguments'] == ['history', 'show', str(tx)]
                and shown['id'] == tx and shown['status'] == 'applied' and shown['required_checks'] == required,
                'repair did not retain the failed transaction and required checks')
        boundary = 3 if arm == 'repair-applied' else 5
        require(row['execution_start'] == boundary and row['delivery_start'] == boundary + 1,
                'repair review boundary changed')
        if arm == 'undo-correct':
            require(events[1]['request']['arguments'] == ['history', 'undo', str(tx)]
                    and events[2]['request']['arguments'] == ['history', 'undo', str(tx), '--write', '--no-diff']
                    and events[2]['response']['report']['applied'] is True, 'undo-and-correct lost its initial reversal')
        goal = json.loads(events[boundary - 2]['request']['stdin'])
        policy = goal['delivery']
        require(goal['checks'] == ['behavior'] and policy == {
                    'check-original': False, 'compact-success': True, 'exercise-reversal': False,
                    'patch': None, 'check-output-bytes': 512}, 'repair policy changed')
        start = bodies.expected_source(wrong(case)) if arm == 'repair-applied' else case['files']
        require(row['before'] == (changed_source(start) if fault == 'source' else start),
                'repair starting source changed')
        if row['result'] is not None:
            require(response(events[boundary], row['result']), 'repair result differs from recorded response')
        if fault is not None:
            error = row['error']
            require(error is not None and events[-1]['response'] == {
                        'exit_code': error['exit_code'], 'error': error['message'], 'report': error['report']}
                    and row['patch'] is None and row['parts'] is None and row['submission'] is None
                    and row['receiver'] is None, 'failed repair delivered output or lost its refusal')
            expected_source = (bodies.expected_source(wrong(case, again=True)) if fault == 'wrong-repair' else
                               changed_source(bodies.expected_source(case)) if fault == 'later-edit' else row['before'])
            require(row['source'] == expected_source, 'failed repair overwrote or lost source')
            if fault == 'later-edit':
                require(row['result']['passed'] is True and len(events) == boundary + 2
                        and events[-1]['request']['arguments'] == ['history', 'undo', str(row['result']['transaction'])],
                        'later edit was not refused before delivery reversal')
            else:
                require(row['result'] is None and len(events) == boundary + 1, 'failed edit continued into delivery')
                if fault == 'wrong-repair':
                    workflow = error['report']['workflow']
                    require(workflow['transaction_status'] == 'applied' and workflow['passed'] is False
                            and [(s['stage'], s['status']) for s in workflow['stages']] ==
                                [('apply', 'passed'), ('check-applied', 'failed')], 'wrong repair lost its failed check')
            continue
        result = row['result']
        chain = [tx, result['transaction']] if arm == 'repair-applied' else [result['transaction']]
        require(result['passed'] is True and result['transaction'] != tx and row['error'] is None
                and row['source'] == bodies.expected_source(case)
                and [(s['stage'], s['status']) for s in result['workflow']['stages']] ==
                    [('apply', 'passed'), ('check-applied', 'passed')], 'corrected edit lost its checked lifecycle')
        transitions = [('undo', t) for t in reversed(chain)] + [('redo', t) for t in chain]
        require([(t['action'], t['transaction']) for t in row['transitions']] == transitions,
                'combined reversal sequence changed')
        position = boundary + 1
        for index, (action, current) in enumerate(transitions):
            if index == len(chain):
                require(events[position]['request']['arguments'] ==
                            ['checks', '--run', 'behavior', '--basis', required['configuration_basis']]
                        and response(events[position], row['restored']), 'restored source check was lost')
                check_receipt(row['restored'], required['configuration_basis'])
                require(row['restored']['source_revision'] == failed['workflow']['stages'][0]['result']['source_revision'],
                        'combined reversal did not restore the original source revision')
                position += 1
            transition = row['transitions'][index]
            written = ['history', action, str(current), '--write'] + (['--no-diff'] if action == 'undo' else [])
            require(events[position]['request']['arguments'] == ['history', action, str(current)]
                    and events[position + 1]['request']['arguments'] == written
                    and response(events[position], transition['review'])
                    and response(events[position + 1], transition['result'])
                    and transition['result']['applied'] is True, 'combined reversal evidence changed')
            position += 2
        require(events[position]['request']['arguments'] ==
                    ['checks', '--run', 'behavior', '--basis', required['configuration_basis'], '--record-for', str(result['transaction'])]
                and response(events[position], row['rechecked'])
                and events[position + 1]['request']['arguments'] == ['history', 'show', str(result['transaction'])]
                and response(events[position + 1], row['history']), 'final source check or history receipt was lost')
        check_receipt(row['rechecked'], required['configuration_basis'])
        record = row['history']['records'][0]
        receipt = row['rechecked']['recorded_evidence']
        applied = result['workflow']['stages'][1]['result']
        require(record['id'] == result['transaction'] and record['status'] == 'applied'
                and record['required_checks'] == required and record['check_evidence']
                and receipt['transaction'] == record['id']
                and row['rechecked']['source_revision'] == applied['source_revision']
                and any(e['receipt'] == receipt['receipt'] for e in record['check_evidence'])
                and all(e['configuration_basis'] == required['configuration_basis'] and e['checks'] == ['behavior']
                        and e['source_revision'] == applied['source_revision'] for e in record['check_evidence']),
                'required source-bound check receipts changed')
        position += 2
        require(len(row['parts']) == len(chain) and len(events) == position + len(chain), 'patch chain length changed')
        for index, (current, part) in enumerate(zip(chain, row['parts'])):
            require(events[position + index]['request']['arguments'] == ['history', 'patch', str(current)]
                    and response(events[position + index], part) and part['id'] == current
                    and part['status'] == 'applied' and part['reverse'] is False,
                    'exported transaction order or patch receipt changed')
        patch = ''.join(p['patch'] for p in row['parts'])
        require(row['patch'] == patch and 0 < len(patch.encode()) <= 65536
                and row['submission'] == {'passed': True, 'transactions': chain,
                    'patch_bytes': len(patch.encode()), 'patch_sha256': routes.digest(patch.encode()),
                    'check_receipts': [e['receipt'] for e in record['check_evidence']]}, 'combined patch bytes or submission changed')
        replay = row['receiver']
        require(replay['before'] == case['files'] and replay['source'] == row['source']
                and all(replay[n]['exit_code'] == 0 for n in ('check', 'apply', 'behavior'))
                and replay['oracle'] == bodies.checker(case, final=True)
                and replay['conflict_check']['exit_code'] != 0 and replay['conflict_apply']['exit_code'] != 0
                and replay['conflict_before'] == replay['conflict_after'], 'receiver replay or conflict preservation failed')
        controls = replay['incomplete_or_reordered']
        require([c['name'] for c in controls] == (['repair-only', 'reversed-order'] if len(chain) == 2 else []),
                'missing incomplete patch controls')
        for control, payload in zip(controls, (row['parts'][-1]['patch'], ''.join(p['patch'] for p in reversed(row['parts'])))):
            require(control['patch'] == payload and control['before'] == case['files']
                    and control['after'] == control['before'] and control['apply']['exit_code'] != 0,
                    'incomplete or reordered patch was accepted or changed source')
    return {'cells': len(rows), 'successful_deliveries': len(bodies.CASES) * len(ARMS),
            'comparison': {c['id']: {a: {k: rows[(c['id'], a, None)][k] for k in
                ('caller_bytes', 'total_caller_bytes', 'metrics', 'total_metrics')} for a in ARMS} for c in bodies.CASES}}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--fr', type=Path, default=ROOT / 'target/debug/fr')
    parser.add_argument('--output', type=Path)
    parser.add_argument('--audit', type=Path)
    args = parser.parse_args()
    if args.audit:
        value = json.loads(args.audit.read_text())
    else:
        require(os.environ.get('GITHUB_ACTIONS') == 'true', 'collect only on GitHub runners')
        with tempfile.TemporaryDirectory(prefix='fr-source-repair-') as directory:
            value = {'schema': 'fr-source-repair-1', 'bindings': bindings(),
                     'source_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT).decode().strip(),
                     'binary_sha256': routes.digest(args.fr.read_bytes()),
                     'limits': 'Prescribed incorrect and corrected bodies, not independent agent work. No model, token, '
                               'time or billing comparison. Programs, transaction input, submission and all initial failed '
                               'and recovery fr traffic are counted. Common setup, recording and receiver oracles are excluded. '
                               'Concatenated transaction patches preserve intermediate changes; they are not a minimal diff. '
                               'Intermediate incorrect source is not required to pass; original and final states are checked.',
                     'runs': []}
            for case in bodies.CASES:
                for arm in ARMS:
                    for fault in FAULTS:
                        row = execute(Path(directory) / 'cell/project', case, arm, fault, str(args.fr.resolve()))
                        value['runs'].append(row)
                        print(f"{case['id']}/{arm}/{fault}: captured", file=sys.stderr, flush=True)
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n')
    print(json.dumps(audit(value), indent=2))


if __name__ == '__main__':
    main()
