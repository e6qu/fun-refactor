#!/usr/bin/env python3
"""Pinned upstream tasks with reviewed delivery and independent receiver replay.

Generation is a GitHub-runner workload. Audit replays the public behavior oracle,
checks immutable receipts, and verifies every retained artifact and source binding.
This is deterministic acceptance, not a live agent or population comparison.
"""
from __future__ import annotations

import argparse
import ast
from dataclasses import replace
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import subprocess
import sys
import tarfile
import tempfile
import textwrap
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'sdk/python/src'))
from evidence_basis import file_digest
from fr_ir.context import DirectoryObjectStore
from fr_ir.investigation import Dependency, DependencyKind, Evidence, EvidenceKind, TaskPlan, TaskStep
from fr_ir.investigation_delivery import DeliveryReceipt, run_delivery
from fr_ir.ir import TaskChange, TaskDelivery, TaskTarget
from fr_ir.runtime import FrClient, FrRuntimeError

FIXTURE = ROOT / 'tests/agent-eval/python-repositories'
TASKS = json.loads((FIXTURE / 'task.json').read_text())
TAKE_TAIL_BODY = '''"""Return the first n items, or the last abs(n) items for negative n.

Negative counts require a finite iterable and retain at most abs(n) items.
None consumes all items; nonintegral counts raise TypeError.
"""
from operator import index
if n is None:
    return list(iterable)
n = index(n)
if n < 0:
    return list(deque(iterable, maxlen=-n))
return list(islice(iterable, n))
'''


def save(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    text = json.dumps(value, indent=2, sort_keys=True) + '\n'
    assert len(text.encode()) <= TASKS['budgets']['report_bytes'], 'retained report exceeds the declared byte budget'
    path.write_text(text)


def run(argv, cwd, check=True):
    result = subprocess.run(argv, cwd=cwd, capture_output=True, text=True, timeout=120,
                            env={**os.environ, 'PYTHONDONTWRITEBYTECODE': '1'})
    if check and result.returncode:
        raise AssertionError(f'{argv}: {result.stdout[-4096:]} {result.stderr[-4096:]}')
    return result


def unpack(task, root):
    """Reject links, traversal, duplicate paths and expanded/archive budget excess."""
    archive = FIXTURE / task['archive']
    limits = TASKS['budgets']
    assert archive.stat().st_size <= limits['archive_bytes']
    assert hashlib.sha256(archive.read_bytes()).hexdigest() == task['sha256']
    root.mkdir(parents=True)
    with tarfile.open(archive) as stream:
        members = stream.getmembers()
        assert len(members) <= limits['archive_members']
        assert sum(m.size for m in members) <= limits['expanded_bytes']
        prefix = PurePosixPath(members[0].name).parts[0]
        seen = set()
        for member in members:
            parts = PurePosixPath(member.name).parts
            assert parts and parts[0] == prefix and not PurePosixPath(member.name).is_absolute()
            assert '..' not in parts and not member.issym() and not member.islnk()
            assert member.isdir() or member.isfile()
            relative = Path(*parts[1:])
            if relative == Path('.'):
                assert member.isdir()
                continue
            assert relative not in seen
            seen.add(relative)
            destination = root / relative
            if member.isdir():
                destination.mkdir(parents=True, exist_ok=True)
            else:
                destination.parent.mkdir(parents=True, exist_ok=True)
                with stream.extractfile(member) as source:
                    destination.write_bytes(source.read())


def oracle(root, name):
    result = run([sys.executable, '-B', str(FIXTURE / 'oracle.py'), name, str(root)], root, False)
    value = json.loads(result.stdout)
    assert value['passed'] == (result.returncode == 0)
    return value


def rows(report):
    value = report.to_data()
    assert value['page']['next'] is None, 'discovery must not silently drop candidates'
    return [dict(zip(value['columns'], row)) for row in value['rows']]


def find(client, name, path=None):
    args = ['find', name, '--source', '--bytes', str(TASKS['budgets']['source_reveal_bytes'])]
    if path:
        args += ['--in', path]
    report = client.project(*args)
    candidates = rows(report)
    runtime = [r for r in candidates if r['path'].endswith('.py')]
    assert len(runtime) == 1, (name, [(r['path'], r['name']) for r in candidates])
    selected = runtime[0]
    assert selected['source']['next_offset'] is None
    return selected, report.to_data()


def body(row):
    # Python methods need their declaration's indentation removed before parsing.
    source = textwrap.dedent(row['source']['text'])
    node = ast.parse(source).body[0]
    lines = source.splitlines(keepends=True)
    return textwrap.dedent(''.join(lines[node.body[0].lineno - 1:]))


def satisfy(plan, client, name):
    started = plan.resume(client, transition=f'{name}:start')
    observation = Evidence('read', EvidenceKind.OBSERVATION, started.input_digests[name], True,
                           'bounded repository discovery')
    changed = replace(started.plan, steps=tuple(replace(step, evidence=(observation,))
                     if step.id == name else step for step in started.plan.steps))
    return changed.resume(client, transition=f'{name}:satisfy').plan


def perturb(root, row, replacement):
    """Intentional external edit, outside fr, exercises stale-observation admission."""
    path = root / row['path']
    raw = path.read_bytes()
    span = row['location']['definition']['span']
    source = raw[span['start']:span['end']].decode()
    header = source.split(':\n', 1)[0] + ':\n'
    indent = len(source.splitlines()[1]) - len(source.splitlines()[1].lstrip())
    new = header + textwrap.indent(replacement, ' ' * indent)
    path.write_bytes(raw[:span['start']] + new.encode() + raw[span['end']:])


def configuration(root, name, task):
    (root / '.fr').mkdir(exist_ok=True)
    (root / 'artifacts').mkdir()
    shutil.copyfile(FIXTURE / 'oracle.py', root / '.fr/oracle.py')
    base = [sys.executable, '-B', '.fr/oracle.py', name, '.']
    save(root / '.fr/checks.json', {'schema': 1, 'checks': [
        {'name': 'syntax', 'argv': base + ['--syntax'], 'cwd': '.', 'timeout_seconds': 60,
         'identity_files': ['.fr/oracle.py'], 'covers': ['Parse original, changed, reversed and redone sources']},
        {'name': 'behavior', 'argv': base, 'cwd': '.', 'timeout_seconds': 30,
         'identity_files': ['.fr/oracle.py'], 'covers': [task['oracle']]},
        {'name': 'upstream', 'argv': [sys.executable, '-B', '-m', 'pytest', '-q', task['upstream_tests']],
         'cwd': '.', 'timeout_seconds': 120, 'covers': [task['upstream_tests']]},
    ]})
    run(['git', 'init', '-q'], root)


def proposal(client, changes):
    targets = []
    paths = set()
    for index, change in enumerate(changes):
        selected, _ = find(client, change['name'], change['path'])
        paths.add(selected['path'])
        targets.append(TaskTarget(f'edit-{index}', selected['handle'], 'replace-body', fragment=change['body']))
    return client.review(TaskChange([], targets, {'files-changed': len(paths), 'paths-changed': sorted(paths)},
        ['syntax'], TaskDelivery(patch='artifacts/change.patch', check_output_bytes=8192),
        acceptance_checks=['behavior', 'upstream']))


def reopen(binary, root, objects, plan_root):
    client = FrClient(root, executable=str(binary), timeout=120, max_output_bytes=2_097_152)
    plan = TaskPlan.restore(DirectoryObjectStore(objects), plan_root)
    return plan.resume(client).report.to_data()


def flow_probe(client, selected, root):
    try:
        report = client.project('dataflow', selected['handle'], '--summaries', '--imports', '--rules',
                                str(root / '.fr/rules.json'), '--steps', '4096', '--bytes', '1048576').to_data()
    except FrRuntimeError as error:
        # A runtime/stub ambiguity may refuse imported entry selection entirely.
        # Keep that boundary separate from a successfully returned incomplete analysis.
        if 'imported flow requires' not in str(error):
            raise
        return {'status': 'refused', 'refusal': str(error), 'report': error.report}
    return {'status': 'complete' if report['complete'] else 'incomplete', 'report': report, 'refusal': None}


def measure_task(binary, name, task, output):
    start = time.monotonic()
    with tempfile.TemporaryDirectory(prefix='fr-upstream-task-') as temporary:
        root = Path(temporary) / 'project'
        unpack(task, root)
        configuration(root, name, task)
        baseline = oracle(root, name)
        assert not baseline['passed']
        original_tests = run([sys.executable, '-B', '-m', 'pytest', '-q', task['upstream_tests']], root)
        client = FrClient(root, executable=str(binary), timeout=120, max_output_bytes=2_097_152)
        symbol = 'singularize' if task['kind'] == 'bug' else 'take'
        selected, discovery = find(client, symbol)
        discoveries = {'entry': discovery}
        original_body = body(selected)
        changes = []
        if task['kind'] == 'bug':
            tree = ast.parse(textwrap.dedent(selected['source']['text']))
            calls = sorted({n.func.id for n in ast.walk(tree) if isinstance(n, ast.Call)
                            and isinstance(n.func, ast.Name) and n.func.id.startswith('_')})
            assert len(calls) == 1
            dependent, discoveries['helper'] = find(client, calls[0], selected['path'])
            needle = "else:\n    singular = word[:-1]"
            assert original_body.count(needle) == 1
            repaired = original_body.replace(needle, "elif word.endswith('ss'):\n    return orig_word\n" + needle)
            changes = [{'name': symbol, 'path': selected['path'], 'body': repaired},
                       {'name': dependent['name'], 'path': dependent['path'], 'body': body(dependent)}]
            replacement = 'return disciple'
        else:
            dependent = selected
            test, discoveries['test'] = find(client, 'test_negative_take')
            assert test['path'] == task['upstream_tests']
            repaired = TAKE_TAIL_BODY
            changes = [{'name': symbol, 'path': selected['path'], 'body': repaired},
                       {'name': test['name'], 'path': test['path'], 'body':
                        'self.assertEqual(mi.take(-3, range(10)), [7, 8, 9])\n'
                        'self.assertEqual(mi.take(-20, range(3)), [0, 1, 2])\n'
                        'self.assertEqual(mi.take(-3, iter(())), [])\n'}]
            replacement = 'return []'
        # Full dataflow evidence stays explicit even when these real programs exceed the scalar subset.
        save(root / '.fr/rules.json', {'version': 'repository-probe-1', 'sources': ['fr_probe_source'], 'sinks': ['fr_probe_sink']})
        selected, _ = find(client, symbol, selected['path'])
        analysis = flow_probe(client, selected, root)
        save(output / 'analysis-probe.json', analysis)
        refusal = None
        if task['kind'] == 'feature':
            public = Path(selected['path']).parent / '__init__.py'
            public_report = client.project('map', str(public), '--depth', '0', '--fields', 'handle,kind').to_data()
            discoveries['public_consumer'] = public_report
            try:
                client.review(TaskChange([], [TaskTarget('new-api', public_report['rows'][0][0],
                    'insert-declaration', fragment='def take_exact(n, iterable):\n    return list(iterable)\n')],
                    {'files-changed': 1}, ['syntax']))
            except FrRuntimeError as error:
                refusal = str(error)
                assert 'wildcard' in refusal.lower(), refusal
            else:
                raise AssertionError('wildcard-import insertion unexpectedly admitted')
        paths = sorted({c['path'] for c in changes})
        note = next(p.name for p in sorted(root.glob('README*')) if p.is_file())
        plan = TaskPlan(task['requirement'], ('delivered',), (
            TaskStep('analysis', 'Which sources must change?', tuple(Dependency(DependencyKind.SOURCE, p) for p in paths)),
            TaskStep('independent', 'Keep independent upstream evidence', (Dependency(DependencyKind.SOURCE, note),)),
            TaskStep.checked('outcome', 'Do independent behavior and upstream checks pass?',
                             checks=('behavior', 'upstream'), satisfies=('delivered',)),
        ))
        independent_refusal = None
        try:
            plan.resume(client)
        except FrRuntimeError as error:
            if task['kind'] != 'feature' or 'dependency exists outside the indexed snapshot' not in str(error):
                raise
            independent_refusal = str(error)
            note = str(public)
            plan = replace(plan, steps=tuple(replace(step, inputs=(Dependency(DependencyKind.SOURCE, note),))
                           if step.id == 'independent' else step for step in plan.steps))
        for step in ('analysis', 'independent'):
            plan = satisfy(plan, client, step)
        objects = output / 'objects'
        store = DirectoryObjectStore(objects)
        plan_root = plan.store(store)
        stale_review = proposal(client, changes)
        assert stale_review.at('/ready')
        # Rediscover because configuration/analysis inputs may have changed the project revision.
        dependent, _ = find(client, dependent['name'], dependent['path'])
        perturb(root, dependent, replacement)
        interrupted_sources = {p: (root / p).read_text() for p in paths}
        reopened = json.loads(run([sys.executable, '-B', str(Path(__file__).resolve()), '--reopen',
            '--fr', str(binary), '--project', str(root), '--objects', str(objects), '--plan', plan_root], ROOT).stdout)
        assert 'analysis' in reopened['invalidated']
        assert next(s for s in reopened['plan']['steps'] if s['id'] == 'independent')['state'] == 'satisfied'
        try:
            client.execute(stale_review)
        except FrRuntimeError as error:
            stale_refusal = str(error)
        else:
            raise AssertionError('stale reviewed mutation admitted')
        plan = TaskPlan.restore(store, plan_root).resume(client).plan
        plan = plan.resume(client, transition='analysis:reset').plan
        plan = satisfy(plan, client, 'analysis')
        plan = plan.resume(client, transition='outcome:reset').plan
        review = proposal(client, changes)
        assert review.at('/ready')
        delivered = run_delivery(plan, client, 'outcome', review, store)
        assert delivered.passed, (delivered.attachment_error, delivered.receipt.result.to_data())
        # Source edits invalidate the earlier diagnosis; retain a fresh post-change observation.
        plan = delivered.resumed.plan.resume(client, transition='analysis:reset').plan
        plan = satisfy(plan, client, 'analysis')
        final = plan.resume(client)
        assert final.complete
        final_root = final.plan.store(store)
        accepted = oracle(root, name)
        assert accepted['passed']
        patch = (root / 'artifacts/change.patch').read_text()
        record = {'repository': task['repository'], 'revision': task['revision'], 'archive_sha256': task['sha256'],
            'baseline': baseline, 'original_upstream': {'exit_code': original_tests.returncode, 'stdout': original_tests.stdout},
            'discovery': discoveries, 'analysis': analysis, 'wildcard_insertion_refusal': refusal,
            'independent_path': note, 'unindexed_document_refusal': independent_refusal,
            'interruption': {'sources': interrupted_sources, 'reopened': reopened, 'stale_review_refusal': stale_refusal},
            'review': review.to_data(), 'delivery': delivered.receipt.result.to_data(),
            'receipt_root': delivered.receipt_root, 'initial_plan_root': plan_root, 'final_plan_root': final_root,
            'complete': final.complete, 'oracle': accepted, 'patch': patch,
            'seconds': time.monotonic() - start}
        assert record['seconds'] <= TASKS['budgets']['task_seconds']
        save(output / 'result.json', record)
        return record


def replay(name, task, record):
    with tempfile.TemporaryDirectory(prefix='fr-upstream-replay-') as temporary:
        root = Path(temporary) / 'receiver'
        unpack(task, root)
        assert oracle(root, name) == record['baseline']
        for path, source in record['interruption']['sources'].items():
            relative = PurePosixPath(path)
            assert not relative.is_absolute() and '..' not in relative.parts
            assert (root / path).is_file()
            (root / path).write_text(source)
        before = {str(p.relative_to(root)): p.read_bytes() for p in root.rglob('*.py')}
        run(['git', 'init', '-q'], root)
        patch = Path(temporary) / 'change.patch'
        patch.write_text(record['patch'])
        for reverse in (False, True, False):
            args = ['git', 'apply'] + (['--reverse'] if reverse else [])
            run(args + ['--check', str(patch)], root)
            run(args + [str(patch)], root)
            if reverse:
                assert before == {str(p.relative_to(root)): p.read_bytes() for p in root.rglob('*.py')}
            else:
                assert oracle(root, name) == record['oracle']


def bindings():
    paths = [*ROOT.glob('src/**/*.rs'), *ROOT.glob('crates/**/*.rs'), *ROOT.glob('sdk/python/src/fr_ir/*.py'),
             *FIXTURE.iterdir(), ROOT / 'Cargo.lock', ROOT / 'tools/evidence_basis.py', Path(__file__).resolve()]
    return {str(p.relative_to(ROOT)): file_digest(p) for p in sorted(paths) if p.is_file()}


def audit(output):
    manifest = json.loads((output / 'result.json').read_text())
    assert manifest['schema'] == 'fr-python-repository-acceptance-1'
    assert manifest['execution'] == TASKS['execution']
    assert manifest['source_bindings'] == bindings(), 'stale repository acceptance source bindings'
    expected = {str(p.relative_to(output)): hashlib.sha256(p.read_bytes()).hexdigest()
                for p in sorted(output.rglob('*')) if p.is_file() and p != output / 'result.json'}
    assert manifest['artifacts'] == expected, 'missing, extra or tampered acceptance artifact'
    assert set(manifest['tasks']) == set(TASKS['tasks'])
    for name, task in TASKS['tasks'].items():
        record = json.loads((output / name / 'result.json').read_text())
        assert manifest['tasks'][name] == {'complete': True, 'oracle_cases': record['oracle']['cases']}
        assert record['repository'] == task['repository'] and record['revision'] == task['revision']
        assert record['archive_sha256'] == task['sha256']
        assert not record['baseline']['passed'] and record['oracle']['passed'] and record['complete']
        assert record['original_upstream']['exit_code'] == 0
        assert record['interruption']['stale_review_refusal']
        assert 'analysis' in record['interruption']['reopened']['invalidated']
        probe = record['analysis']
        if probe['status'] == 'refused':
            assert 'imported flow requires' in probe['refusal']
        else:
            assert probe['report']['semantics'].startswith('python-scalar')
            assert probe['status'] == ('complete' if probe['report']['complete'] else 'incomplete')
        if task['kind'] == 'feature':
            assert 'wildcard' in record['wildcard_insertion_refusal'].lower()
            assert 'dependency exists outside the indexed snapshot' in record['unindexed_document_refusal']
        store = DirectoryObjectStore(output / name / 'objects')
        receipt = DeliveryReceipt.restore(store, record['receipt_root'])
        assert receipt.passed and receipt.result.to_data() == record['delivery']
        assert receipt.manifest['acceptance_checks'] == ['behavior', 'upstream']
        assert record['review']['task_change_basis'] == receipt.result.task_change_basis
        exported = next(s['result'] for s in record['delivery']['workflow']['stages'] if s['stage'] == 'deliver-patch')
        assert exported['sha256'] == hashlib.sha256(record['patch'].encode()).hexdigest()
        assert exported['bytes'] == len(record['patch'].encode())
        assert next(s for s in record['interruption']['reopened']['plan']['steps'] if s['id'] == 'independent')['state'] == 'satisfied'
        initial = TaskPlan.restore(store, record['initial_plan_root'])
        final = TaskPlan.restore(store, record['final_plan_root'])
        assert initial.steps[1].state.value == 'satisfied'
        assert initial.steps[1].inputs[0].key == final.steps[1].inputs[0].key == record['independent_path']
        assert all(step.state.value == 'satisfied' for step in final.steps)
        replay(name, task, record)
    return manifest


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--fr', type=Path, default=ROOT / 'target/debug/fr')
    parser.add_argument('--output', type=Path)
    parser.add_argument('--audit', type=Path)
    parser.add_argument('--reopen', action='store_true')
    parser.add_argument('--project', type=Path)
    parser.add_argument('--objects', type=Path)
    parser.add_argument('--plan')
    args = parser.parse_args()
    if args.reopen:
        print(json.dumps(reopen(args.fr, args.project, args.objects, args.plan)))
        return
    if args.audit:
        value = audit(args.audit.parent if args.audit.is_file() else args.audit)
        print(json.dumps({'passed': True, 'tasks': value['tasks']}))
        return
    if os.environ.get('GITHUB_ACTIONS') != 'true':
        raise SystemExit('Generate repository acceptance on GitHub Actions; local regeneration is disabled.')
    assert args.output and not args.output.exists()
    args.output.mkdir(parents=True)
    initial_bindings = bindings()
    records = {name: measure_task(args.fr.resolve(), name, task, args.output / name)
               for name, task in TASKS['tasks'].items()}
    assert initial_bindings == bindings(), 'acceptance inputs changed during execution'
    manifest = {'schema': 'fr-python-repository-acceptance-1', 'execution': TASKS['execution'],
        'runner': {'run_id': os.environ['GITHUB_RUN_ID'], 'revision': os.environ['GITHUB_SHA'],
                   'python': sys.version, 'repository': os.environ['GITHUB_REPOSITORY']},
        'source_bindings': initial_bindings, 'binary_sha256': hashlib.sha256(args.fr.read_bytes()).hexdigest(),
        'tasks': {name: {'complete': row['complete'], 'oracle_cases': row['oracle']['cases']} for name, row in records.items()},
        'artifacts': {str(p.relative_to(args.output)): hashlib.sha256(p.read_bytes()).hexdigest()
                      for p in sorted(args.output.rglob('*')) if p.is_file()}}
    save(args.output / 'result.json', manifest)
    assert (args.output / 'result.json').stat().st_size <= TASKS['budgets']['report_bytes']
    audit(args.output)
    print(json.dumps({'passed': True, 'tasks': manifest['tasks']}))


if __name__ == '__main__':
    main()
