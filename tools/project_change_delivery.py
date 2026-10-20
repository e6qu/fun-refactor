"""Use fr discovery, reviewed changes and history on the original selected projects."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

from fr_ir.change_scope import ChangeScope
from fr_ir.context import DirectoryObjectStore
from fr_ir.investigation import TaskPlan, TaskStep
from fr_ir.investigation_delivery import run_delivery
from fr_ir.investigation_checks import run_checks
from fr_ir.ir import TaskChange, TaskDelivery
from fr_ir.runtime import FrClient
from fr_ir.structural_change import RefactorRequest
from project_change_sources import save, digest

ROOT = Path(__file__).resolve().parents[1]
ORACLE = ROOT / 'tests/agent-eval/project-changes/oracle.py'


def run(argv, root, output, *, success):
    result = subprocess.run(argv, cwd=root, capture_output=True, text=True, timeout=120,
        env={**os.environ, 'PYTHONDONTWRITEBYTECODE': '1'})
    save(output, {'argv': argv, 'returncode': result.returncode,
                  'stdout': result.stdout, 'stderr': result.stderr})
    assert (result.returncode == 0) == success, output
    return result


def rows(report):
    data = report.to_data()
    assert data['page']['next'] is None, 'discovery is unfinished'
    return [dict(zip(data['columns'], row)) for row in data['rows']]


def target(client, name, language):
    report = client.project('find', name, '--source', '--bytes', '65536')
    extension = '.rs' if language == 'Rust' else '.py'
    found = [row for row in rows(report) if row['path'].endswith(extension)]
    assert len(found) == 1, [(r['path'], r['name']) for r in found]
    assert found[0]['source']['next_offset'] is None
    return found[0], report.to_data()


def measure(task, original, output, binary):
    started = time.monotonic()
    output.mkdir(parents=True, exist_ok=False)
    project = output / 'working'
    shutil.copytree(original, project)
    before = {str(p.relative_to(project)): digest(p) for p in project.rglob('*') if p.is_file()}
    oracle = [sys.executable, '-B', str(ORACLE), 'behavior', task['id'], str(project), '--name']
    baseline = run(oracle + [task['function']], project, output / 'before.json', success=True)
    run(oracle + [task['renamed']], project, output / 'requested-before.json', success=False)
    run(['git', 'init', '-q'], project, output / 'git-init.json', success=True)
    run(['git', 'add', '.'], project, output / 'git-index.json', success=True)
    (project / '.fr').mkdir(exist_ok=True)
    (project / 'artifacts').mkdir()
    shutil.copyfile(ORACLE, project / '.fr/oracle.py')
    checks = [{'name': 'behavior', 'argv': [sys.executable, '-B', '.fr/oracle.py', 'behavior', task['id'], '.', '--name', task['renamed']],
        'cwd': '.', 'timeout_seconds': 30, 'identity_files': ['.fr/oracle.py'], 'covers': [json.dumps(task['behavior'])]}]
    if task['id'] == 'boltons':
        run([sys.executable, '-B', str(ORACLE), 'documentation', task['id'], str(project)],
            project, output / 'documentation-before.json', success=True)
        checks.append({'name': 'upstream', 'argv': [sys.executable, '-B', '-m', 'pytest', '-q', '-p', 'no:cacheprovider', 'tests/test_mathutils.py'],
            'cwd': '.', 'timeout_seconds': 120, 'covers': ['original tests/test_mathutils.py']})
        run(checks[-1]['argv'], project, output / 'upstream-before.json', success=True)
    syntax = (['rustc', '--edition=2021', '--crate-name', 'glob', '--crate-type', 'rlib',
        '--emit=metadata', 'src/lib.rs', '-o', '.fr/library.rmeta'] if task['id'] == 'glob' else
        [sys.executable, '-B', '-c', 'import ast,pathlib;[ast.parse(pathlib.Path(p).read_text()) for p in '
         '["boltons/mathutils.py","tests/test_mathutils.py"]]'])
    checks.append({'name': 'syntax', 'argv': syntax, 'cwd': '.', 'timeout_seconds': 30,
        'covers': ['Compile the complete library' if task['id'] == 'glob' else 'Parse the selected module and its original tests']})
    save(project / '.fr/checks.json', {'schema': 1, 'checks': checks})
    client = FrClient(project, executable=binary, timeout=120, max_output_bytes=2097152)
    selected, report = target(client, task['function'], task['language'])
    save(output / 'discovery.json', report)
    position = selected['location']['name']['range']['start']
    selector = f"{selected['path']}:{position['line']}:{position['col']}"
    references = client.call('refs', selector, '--include-unresolved').to_data()
    usages = client.call('usages', selector, '--include-unresolved').to_data()
    save(output / 'references.json', references)
    save(output / 'usages.json', usages)
    changed_paths = sorted({selected['path']} | {str(Path(row['file']).relative_to(project))
        for row in references['references'] if row['rewritable']} | {
        str(Path(row['file']).relative_to(project)) for row in usages['in_text']})
    initial = ChangeScope.inspect(client, [selected['handle']], depth=4,
        nodes=128, references=512, max_bytes=262144)
    save(output / 'scope-before-mapping.json', initial.report.to_data())
    paths = initial.report.at('/affected_paths')
    assert 0 < len(paths) <= 64
    save(project / '.fr/check-scopes.json', {'schema': 1, 'checks': [
        {'name': 'syntax', 'paths': paths}]})
    scope = ChangeScope.inspect(client, [selected['handle']], depth=4,
        nodes=128, references=512, max_bytes=262144)
    save(output / 'scope.json', scope.report.to_data())
    assert scope.ready, 'Discovery is not ready; inspect the retained scope.json for gaps and weak references'
    store = DirectoryObjectStore(output / 'objects')
    scope_root = scope.persist(store)
    names = [check['name'] for check in checks if check['name'] != 'syntax']
    change = scope.bind(TaskChange([], [RefactorRequest.rename(task['renamed'], documentation='update')
        .target('rename', selected['handle'])], {'files-changed': len(changed_paths),
        'paths-changed': changed_paths}, ['syntax'],
        TaskDelivery(patch='artifacts/change.patch'), acceptance_checks=names))
    review = client.review(change)
    save(output / 'review.json', review.to_data())
    plan = TaskPlan(task['delivery'], ('behavior preserved',), (TaskStep.checked('outcome',
        'Does the requested public name preserve the declared behavior?', checks=tuple(names),
        satisfies=('behavior preserved',)),))
    delivered = run_delivery(plan, client, 'outcome', review, store)
    save(output / 'delivery.json', delivered.receipt.result.to_data())
    assert delivered.passed and delivered.resumed.complete
    code_patch = output / 'code.patch'
    shutil.copyfile(project / 'artifacts/change.patch', code_patch)
    history = client.call('history').to_data()
    save(output / 'history-code.json', history)
    transactions = [history['applied'][-1]]
    patch = output / 'change.patch'
    patch.write_bytes(code_patch.read_bytes())
    if task['id'] == 'boltons':
        doc_command = [sys.executable, '-B', str(ORACLE), 'documentation', task['id'], str(project)]
        run(doc_command, project, output / 'documentation-before-repair.json', success=False)
        preview = client.call('file', 'replace', 'docs/mathutils.rst',
            '--before', '.. autofunction:: boltons.mathutils.' + task['function'],
            '--after', '.. autofunction:: boltons.mathutils.' + task['renamed'],
            '--matches', '1', '--save-plan').to_data()
        save(output / 'documentation-review.json', preview)
        assert preview['saved'] is True and preview['applied'] is False and preview['changed'] == 1
        transaction = preview['transaction']
        save(output / 'documentation-patch.json', client.call('history', 'patch', str(transaction),
            '--output', 'artifacts/documentation.patch').to_data())
        documentation_patch = (project / 'artifacts/documentation.patch').read_bytes()
        assert b'--- a/docs/mathutils.rst\n' in documentation_patch
        assert ('-.. autofunction:: boltons.mathutils.' + task['function'] + '\n').encode() in documentation_patch
        assert ('+.. autofunction:: boltons.mathutils.' + task['renamed'] + '\n').encode() in documentation_patch
        save(output / 'documentation-apply.json',
            client.call('history', 'apply', str(transaction), '--write').to_data())
        transactions.append(transaction)
        patch.write_bytes(code_patch.read_bytes() + documentation_patch)
        run(doc_command, project, output / 'documentation-after.json', success=True)
        checks.append({'name': 'documentation',
            'argv': [sys.executable, '-B', '.fr/oracle.py', 'documentation', task['id'], '.'],
            'cwd': '.', 'timeout_seconds': 30, 'identity_files': ['.fr/oracle.py', 'docs/mathutils.rst'],
            'covers': ['Resolve every autodoc object in original docs/mathutils.rst']})
        save(project / '.fr/checks.json', {'schema': 1, 'checks': checks})
    complete_plan = TaskPlan(task['delivery'], ('complete change checked',), (TaskStep.checked('complete',
        'Do the code, original tests and documentation work together?',
        checks=tuple(check['name'] for check in checks), satisfies=('complete change checked',)),))
    checked = run_checks(complete_plan, client, 'complete', client.call('checks', '--toolchain'),
        store, output_bytes=8192)
    assert checked.passed and checked.resumed.complete
    save(output / 'complete-checks.json', checked.checks.to_data())
    after = run(oracle + [task['renamed']], project, output / 'after.json', success=True)
    assert json.loads(after.stdout) == json.loads(baseline.stdout)
    save(output / 'history.json', client.call('history').to_data())
    for transaction in reversed(transactions):
        save(output / f'undo-{transaction}.json', client.call('history', 'undo', str(transaction), '--write').to_data())
    assert all(digest(project / name) == value for name, value in before.items())
    run(oracle + [task['function']], project, output / 'after-undo.json', success=True)
    if task['id'] == 'boltons':
        run(doc_command, project, output / 'documentation-after-undo.json', success=True)
    for transaction in transactions:
        save(output / f'redo-{transaction}.json', client.call('history', 'redo', str(transaction), '--write').to_data())
    run(oracle + [task['renamed']], project, output / 'after-redo.json', success=True)
    if task['id'] == 'boltons':
        run(doc_command, project, output / 'documentation-after-redo.json', success=True)
    receiver = output / 'receiver'
    shutil.copytree(original, receiver)
    run(['git', 'apply', '--check', str(patch.resolve())], receiver, output / 'receiver-check.json', success=True)
    run(['git', 'apply', str(patch.resolve())], receiver, output / 'receiver-apply.json', success=True)
    run([sys.executable, '-B', str(ORACLE), 'behavior', task['id'], str(receiver), '--name', task['renamed']],
        receiver, output / 'receiver-behavior.json', success=True)
    if task['id'] == 'boltons':
        run([sys.executable, '-B', str(ORACLE), 'documentation', task['id'], str(receiver)],
            receiver, output / 'receiver-documentation.json', success=True)
        upstream = next(check for check in checks if check['name'] == 'upstream')
        run(upstream['argv'], receiver, output / 'receiver-upstream.json', success=True)
    elapsed = time.monotonic() - started
    assert elapsed <= 180, 'original project delivery exceeds its task budget'
    result = {'passed': True, 'project': task['id'], 'seconds': elapsed, 'scope_root': scope_root,
        'receipt_root': delivered.receipt_root, 'plan_root': delivered.plan_root,
        'complete_plan_root': checked.resumed.plan.store(store), 'complete_checks_root': checked.report_root,
        'transactions': transactions, 'code_patch_sha256': digest(code_patch),
        'patch_sha256': digest(patch), 'before': json.loads(baseline.stdout), 'after': json.loads(after.stdout)}
    save(output / 'result.json', result)
    shutil.rmtree(project)
    shutil.rmtree(receiver)
    return result
