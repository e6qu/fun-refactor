#!/usr/bin/env python3
"""Compare reviewed resume with undo-and-retry; collect only on GitHub runners."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("body_cases", ROOT / "tools/guide-body-context.py")
bodies = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bodies)
routes = bodies.routes
runtime = bodies.runtime
ARMS = ("resume", "undo-retry")
FAULTS = (None, "check", "source", "checks")


def bindings():
    paths = ("tools/recovery-delivery-context.py", "src/cli.rs", "src/history/patch.rs")
    return {**bodies.bindings(), **{p: routes.digest((ROOT / p).read_bytes()) for p in paths}}


def program(case, arm):
    if arm == "resume":
        source = '''import json
from pathlib import Path
from fr_ir.runtime import FrClient
client = FrClient('.')
shown = client.call('history', 'show', str(transaction)).to_data()
record = shown['records'][0]
required = record['required_checks']
manifest = {'schema': 1, 'transaction': transaction,
    'transaction-context-basis': record['context_basis'], 'resume-applied': True,
    'checks': {'basis': required['configuration_basis'], 'names': required['checks']},
    'exercise-reversal': True, 'patch': {'output': 'artifacts/change.patch'},
    'check-output-bytes': 512}
(Path(client.root) / '.fr-resume').write_text(json.dumps(manifest))
review = client.call('workflow', '--from', '.fr-resume').to_data()
result = client.call('workflow', '--from', '.fr-resume', '--write',
    '--basis', review['workflow_basis']).to_data()
'''
    else:
        source = ("from fr_ir.runtime import FrClient\nclient = FrClient('.')\n"
                  "undo_review = client.call('history', 'undo', str(transaction)).to_data()\n"
                  "client.call('history', 'undo', str(transaction), '--write', '--no-diff')\n"
                  + bodies.program(case, "builder").partition("submission = ")[0])
    return source + '''delivered = client.call('history', 'show', str(result['transaction'])).to_data()
submission = {'passed': result['passed'], 'transaction': result['transaction'],
    'check_receipts': [e['receipt'] for e in delivered['records'][0]['check_evidence']]}
'''


def run_code(source, scope, client):
    original = runtime.FrClient
    runtime.FrClient = lambda *args, **kwargs: client
    try:
        exec(compile(source, "<recovery-program>", "exec"), scope)
    finally:
        runtime.FrClient = original


def attempt(source, scope, client):
    try:
        run_code(source, scope, client)
    except runtime.FrRuntimeError as error:
        return {"exit_code": error.exit_code, "message": str(error), "report": error.report}
    return None


def run_command(argv, cwd):
    result = subprocess.run(argv, cwd=cwd, capture_output=True, timeout=60)
    return {"argv": argv, "exit_code": result.returncode,
            "stdout": result.stdout.decode(), "stderr": result.stderr.decode()}


def replay(root, case, patch):
    receiver = root.parent / "receiver"
    receiver.mkdir()
    for name, text in case['files'].items():
        path = receiver / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
    artifact = receiver.parent / "delivery.patch"
    artifact.write_text(patch)
    before = routes.source_state(receiver, case)
    checked = run_command(["git", "apply", "--check", str(artifact)], receiver)
    applied = run_command(["git", "apply", str(artifact)], receiver)
    source = routes.source_state(receiver, case)
    oracle = bodies.checker(case, final=True)
    behavior = run_command([sys.executable, "-B", "-c", oracle], receiver)
    conflict = receiver / next(iter(case['files']))
    conflict.write_text("# receiver-owned edit\n" if conflict.suffix == '.py' else "// receiver-owned edit\n")
    conflict_before = routes.source_state(receiver, case)
    refused = run_command(["git", "apply", "--check", str(artifact)], receiver)
    return {"before": before, "check": checked, "apply": applied, "source": source,
            "oracle": oracle, "behavior": behavior, "conflict_before": conflict_before,
            "conflict_check": refused, "conflict_after": routes.source_state(receiver, case)}


def execute(root, case, arm, fault, executable):
    if root.parent.exists():
        shutil.rmtree(root.parent)
    routes.prepare(root, case)
    (root / '.fr/check.py').write_text(
        "import pathlib, sys\nsys.path.insert(0, str(pathlib.Path.cwd()))\n" + bodies.checker(case) +
        "\nif pathlib.Path('../control/fail-applied').exists():\n"
        f"    assert {{p: pathlib.Path(p).read_text() for p in {list(case['files'])!r}}} == {case['files']!r}\n")
    initial = routes.RecordedClient(root, executable=executable, timeout=60)
    initial_program = bodies.program(case, 'builder')
    prefix, marker, suffix = initial_program.partition('result = ')
    scope = {}
    run_code(prefix, scope, initial)
    failure_control = root.parent / 'control/fail-applied'
    failure_control.touch()
    initial_error = attempt(marker + suffix, scope, initial)
    if not initial_error:
        raise ValueError('initial applied-state check did not fail')
    failed_source = routes.source_state(root, case)
    patch_path = root / 'artifacts/change.patch'
    if patch_path.exists():
        raise ValueError('initial failure delivered a patch')
    transaction = initial_error['report']['transaction']
    failure_control.unlink()
    reopened = routes.RecordedClient(root, executable=executable, timeout=60)
    source = program(case, arm)
    prefix, marker, suffix = source.partition('result = ')
    scope = {'transaction': transaction}
    run_code(prefix, scope, reopened)
    boundary = len(reopened.events)
    if fault == 'check':
        failure_control.touch()
    elif fault == 'source':
        path = root / next(iter(case['files']))
        path.write_text(path.read_text() + ('\n# later edit\n' if path.suffix == '.py' else '\n// later edit\n'))
    elif fault == 'checks':
        path = root / '.fr/checks.json'
        declaration = json.loads(path.read_text())
        declaration['checks'][0]['covers'].append('changed after recovery review')
        path.write_bytes(routes.canonical(declaration))
    before = routes.source_state(root, case)
    error = attempt(marker + suffix, scope, reopened)
    result = scope.get('result')
    patch = patch_path.read_text() if patch_path.exists() else None
    receiver = replay(root, case, patch) if patch is not None else None
    submission = scope.get('submission')
    return {'case': case['id'], 'arm': arm, 'fault': fault,
            'initial_program': initial_program, 'initial_events': initial.events,
            'initial_metrics': routes.metrics(initial.events),
            'total_metrics': routes.metrics(initial.events + reopened.events),
            'initial_error': initial_error, 'failed_source': failed_source,
            'program': source, 'input': {'transaction': transaction},
            'execution_start': boundary, 'events': reopened.events,
            'metrics': routes.metrics(reopened.events), 'before': before,
            'source': routes.source_state(root, case), 'result': result, 'error': error,
            'submission': submission, 'history': scope.get('delivered'), 'patch': patch,
            'receiver': receiver, 'caller_bytes': len(source.encode()) +
                len(routes.canonical({'transaction': transaction})) + len(routes.canonical(submission))}


def audit(value):
    if value.get('schema') != 'fr-recovery-delivery-1' or value.get('bindings') != bindings():
        raise ValueError('recovery schema or source bindings changed')
    rows = {(r['case'], r['arm'], r['fault']): r for r in value['runs']}
    expected = {(c['id'], a, f) for c in bodies.CASES for a in ARMS for f in FAULTS}
    if set(rows) != expected or len(rows) != len(value['runs']):
        raise ValueError('missing or duplicate recovery cells')
    for (case_id, arm, fault), row in rows.items():
        case = next(c for c in bodies.CASES if c['id'] == case_id)
        source = program(case, arm)
        if (row['program'] != source or row['initial_program'] != bodies.program(case, 'builder')
                or row['metrics'] != routes.metrics(row['events'])
                or row['initial_metrics'] != routes.metrics(row['initial_events'])
                or row['total_metrics'] != routes.metrics(row['initial_events'] + row['events'])
                or row['caller_bytes'] != len(source.encode()) + len(routes.canonical(row['input']))
                    + len(routes.canonical(row['submission']))):
            raise ValueError('recovery caller or traffic accounting changed')
        failed = row['initial_error']['report']
        initial_error = row['initial_error']
        if (failed['passed'] is not False or failed['workflow']['transaction_status'] != 'applied'
                or len(row['initial_events']) != 3
                or row['initial_events'][-1]['response'] != {
                    'exit_code': initial_error['exit_code'], 'error': initial_error['message'], 'report': failed}
                or row['failed_source'] != bodies.expected_source(case)
                or row['input'] != {'transaction': failed['transaction']}
                or [(s['stage'], s['status']) for s in failed['workflow']['stages']] !=
                    [(s, 'passed' if i < 2 else 'failed' if i == 2 else 'pending')
                     for i, s in enumerate(routes.STAGES)]):
            raise ValueError('initial applied failure or transaction lost')
        events = row['events']
        boundary = 2 if arm == 'resume' else 4
        if row['execution_start'] != boundary or len(events) != boundary + (2 if fault is None else 1):
            raise ValueError('recovery call sequence changed')
        execution = events[boundary]['response']
        if row['error']:
            error = row['error']
            if execution != {'exit_code': error['exit_code'], 'error': error['message'], 'report': error['report']}:
                raise ValueError('recovery error differs from recorded response')
        elif execution != {'exit_code': 0, 'report': row['result']}:
            raise ValueError('recovery result differs from recorded response')
        if fault is not None:
            expected_source = bodies.expected_source(case) if fault == 'check' else row['before']
            if (not row['error'] or row['result'] is not None or row['patch'] is not None
                    or row['receiver'] is not None or row['source'] != expected_source):
                raise ValueError('failed recovery changed source or delivered a patch')
            if fault == 'check':
                failure = row['error']['report']
                workflow = failure.get('workflow', failure)
                stages = routes.STAGES if arm == 'undo-retry' else routes.STAGES[2:]
                failed_at = 2 if arm == 'undo-retry' else 0
                if (workflow['transaction_status'] != 'applied' or workflow['passed'] is not False
                        or [(s['stage'], s['status']) for s in workflow['stages']] !=
                            [(s, 'passed' if i < failed_at else 'failed' if i == failed_at else 'pending')
                             for i, s in enumerate(stages)]):
                    raise ValueError('repeated check failure lost its applied-state boundary')
            continue
        result = row['result']
        workflow = result.get('workflow', result)
        stages = routes.STAGES if arm == 'undo-retry' else routes.STAGES[2:]
        record = row['history']['records'][0]
        if (row['error'] or not result['passed'] or row['source'] != bodies.expected_source(case)
                or [(s['stage'], s['status']) for s in workflow['stages']] != [(s, 'passed') for s in stages]
                or workflow['transaction_status'] != 'applied' or not row['patch']
                or record['id'] != result['transaction'] or record['status'] != 'applied'
                or (arm == 'resume') != (result['transaction'] == failed['transaction'])):
            raise ValueError('recovery lost the reviewed lifecycle or applied twice')
        evidence = record['check_evidence']
        required = record['required_checks']
        if (not evidence or any(e['configuration_basis'] != required['configuration_basis']
                                or e['checks'] != required['checks'] for e in evidence)
                or row['submission'] != {'passed': True, 'transaction': result['transaction'],
                                        'check_receipts': [e['receipt'] for e in evidence]}
                or events[-1]['response'] != {'exit_code': 0, 'report': row['history']}):
            raise ValueError('recovery lost required check receipts')
        for stage in workflow['stages']:
            if stage['stage'] == 'check-applied':
                check = stage['result']
                if (check['passed'] is not True or check['recorded_evidence']['transaction'] != record['id']
                        or not any(e['receipt'] == check['recorded_evidence']['receipt']
                                   and e['source_revision'] == check['source_revision'] for e in evidence)):
                    raise ValueError('applied check receipt lost its source binding')
        delivery = workflow['stages'][-1]['result']
        if (delivery['sha256'] != routes.digest(row['patch'].encode())
                or delivery['bytes'] != len(row['patch'].encode())):
            raise ValueError('patch differs from delivery receipt')
        receiver = row['receiver']
        if (receiver['before'] != case['files'] or receiver['source'] != row['source']
                or receiver['check']['exit_code'] != 0 or receiver['apply']['exit_code'] != 0
                or receiver['behavior']['exit_code'] != 0 or receiver['oracle'] != bodies.checker(case, final=True)
                or receiver['conflict_check']['exit_code'] == 0
                or receiver['conflict_before'] != receiver['conflict_after']):
            raise ValueError('receiver replay, behavior or conflict preservation failed')
    for case in bodies.CASES:
        if rows[(case['id'], 'resume', None)]['patch'] != rows[(case['id'], 'undo-retry', None)]['patch']:
            raise ValueError('recovery arms delivered different patches')
    return {'cells': len(rows), 'successful_deliveries': len(bodies.CASES) * len(ARMS),
            'comparison': {case['id']: {arm: {key: rows[(case['id'], arm, None)][key]
                                              for key in ('caller_bytes', 'metrics', 'total_metrics')} for arm in ARMS}
                           for case in bodies.CASES}}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--fr', type=Path, default=ROOT / 'target/debug/fr')
    parser.add_argument('--output', type=Path)
    parser.add_argument('--audit', type=Path)
    args = parser.parse_args()
    if args.audit:
        value = json.loads(args.audit.read_text())
    else:
        if os.environ.get('GITHUB_ACTIONS') != 'true':
            raise RuntimeError('Collect on GitHub Actions; local collection is disabled')
        with tempfile.TemporaryDirectory(prefix='fr-recovery-delivery-') as directory:
            value = {'schema': 'fr-recovery-delivery-1', 'bindings': bindings(),
                     'source_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT).decode().strip(),
                     'binary_sha256': routes.digest(args.fr.read_bytes()),
                     'limits': 'Prescribed bodies and repaired external check prerequisite. Source repairs need a new '
                               'transaction. No model, token, time or provider-cost comparison. Initial failed work '
                               'is retained separately; caller bytes include the complete recovery program, input '
                               'and canonical submission. Fixture setup, instrumentation and receiver oracle are excluded.',
                     'runs': []}
            for case in bodies.CASES:
                for arm in ARMS:
                    for fault in FAULTS:
                        value['runs'].append(execute(Path(directory) / 'cell/project', case, arm, fault, str(args.fr.resolve())))
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n')
    print(json.dumps(audit(value), indent=2))


if __name__ == '__main__':
    main()
